//! A small bounded queue with std `mpsc` semantics (FIFO, `send` blocks when full, a dropped end disconnects the other).
//! `mpsc` instantiates its list, array and zero flavors for every message type (about 5 KB each, measured);
//! this is one `Vec` behind a mutex, so each message type costs a few hundred bytes.
pub use std::sync::mpsc::{TryRecvError, TrySendError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

struct State<T> {
    queue: Vec<T>,
    cap: usize,
    senders: usize,
    receiver: bool,
}
struct Shared<T> {
    state: Mutex<State<T>>,
    // One condvar for "not empty" and "not full": the queues are short and the waiters few.
    changed: Condvar,
}
impl<T> Shared<T> {
    fn lock(&self) -> MutexGuard<'_, State<T>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Debug, PartialEq)]
pub struct Closed;
pub struct Sender<T>(Arc<Shared<T>>);
pub struct Receiver<T>(Arc<Shared<T>>);

/// A queue holding at most `cap` messages (`usize::MAX` for no limit).
pub fn channel<T>(cap: usize) -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            queue: Vec::new(),
            cap,
            senders: 1,
            receiver: true,
        }),
        changed: Condvar::new(),
    });
    (Sender(Arc::clone(&shared)), Receiver(shared))
}

impl<T> Sender<T> {
    /// Waits for room; `Err` if the receiver is gone.
    pub fn send(&self, message: T) -> Result<(), Closed> {
        let mut state = self.0.lock();
        loop {
            if !state.receiver {
                return Err(Closed);
            }
            if state.queue.len() < state.cap {
                state.queue.push(message);
                self.0.changed.notify_all();
                return Ok(());
            }
            state = self
                .0
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
    pub fn try_send(&self, message: T) -> Result<(), TrySendError<T>> {
        let mut state = self.0.lock();
        if !state.receiver {
            Err(TrySendError::Disconnected(message))
        } else if state.queue.len() >= state.cap {
            Err(TrySendError::Full(message))
        } else {
            state.queue.push(message);
            self.0.changed.notify_all();
            Ok(())
        }
    }
}
impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.0.lock().senders += 1;
        Self(Arc::clone(&self.0))
    }
}
impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut state = self.0.lock();
        state.senders -= 1;
        if state.senders == 0 {
            self.0.changed.notify_all();
        }
    }
}

impl<T> Receiver<T> {
    /// Waits for a message; `Err` once the queue is empty and every sender is gone.
    pub fn recv(&self) -> Result<T, ()> {
        let mut state = self.0.lock();
        loop {
            match Self::take(&mut state) {
                Ok(message) => {
                    self.0.changed.notify_all();
                    return Ok(message);
                }
                Err(TryRecvError::Disconnected) => return Err(()),
                Err(TryRecvError::Empty) => {
                    state = self
                        .0
                        .changed
                        .wait(state)
                        .unwrap_or_else(PoisonError::into_inner);
                }
            }
        }
    }
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        let message = Self::take(&mut self.0.lock());
        if message.is_ok() {
            self.0.changed.notify_all();
        }
        message
    }
    fn take(state: &mut State<T>) -> Result<T, TryRecvError> {
        if !state.queue.is_empty() {
            Ok(state.queue.remove(0))
        } else if state.senders == 0 {
            Err(TryRecvError::Disconnected)
        } else {
            Err(TryRecvError::Empty)
        }
    }
    #[cfg(test)]
    pub fn try_iter(&self) -> impl Iterator<Item = T> + '_ {
        std::iter::from_fn(|| self.try_recv().ok())
    }
    #[cfg(test)]
    pub fn recv_timeout(&self, wait: std::time::Duration) -> Result<T, ()> {
        let deadline = std::time::Instant::now() + wait;
        loop {
            match self.try_recv() {
                Ok(message) => return Ok(message),
                Err(TryRecvError::Disconnected) => return Err(()),
                Err(TryRecvError::Empty) if std::time::Instant::now() >= deadline => return Err(()),
                Err(TryRecvError::Empty) => std::thread::sleep(std::time::Duration::from_millis(1)),
            }
        }
    }
}
impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut state = self.0.lock();
        state.receiver = false;
        let queued = std::mem::take(&mut state.queue);
        drop(state);
        drop(queued); // a message's own drop runs outside the lock
        self.0.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_fifo_and_disconnect() {
        let (tx, rx) = channel(2);
        assert!(tx.try_send(1).is_ok() && tx.try_send(2).is_ok());
        assert!(matches!(tx.try_send(3), Err(TrySendError::Full(3))));
        assert_eq!((rx.try_recv(), rx.recv()), (Ok(1), Ok(2)));
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
        let more = tx.clone();
        tx.send(4).unwrap();
        drop(tx);
        assert_eq!(rx.try_recv(), Ok(4));
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty)); // `more` still holds the queue open
        drop(more);
        assert_eq!(
            (rx.try_recv(), rx.recv()),
            (Err(TryRecvError::Disconnected), Err(()))
        );
        let (tx, rx) = channel(1);
        drop(rx);
        assert_eq!(tx.send(5), Err(Closed));
        assert!(matches!(tx.try_send(6), Err(TrySendError::Disconnected(6))));
    }

    #[test]
    fn blocked_send_and_recv_wake_across_threads() {
        let (tx, rx) = channel(1);
        let producer = std::thread::spawn(move || (0..50).for_each(|n| tx.send(n).unwrap()));
        assert_eq!((0..50).map(|_| rx.recv().unwrap()).sum::<i32>(), 1225);
        producer.join().unwrap();
        assert_eq!(rx.recv(), Err(())); // the producer's sender is gone
    }
}

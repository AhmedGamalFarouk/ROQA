//! External-monitor brightness over DDC/CI (dxva2), for the left-edge slider.
//!
//! The calls take tens to hundreds of milliseconds and may hang on a flaky monitor, so they run on one
//! lazily started worker thread behind a small bounded queue; the host never waits for them. The worker
//! opens the monitor's physical handles for each job and destroys them before the next, so none is held.
//! Laptop panels (WMI `WmiMonitorBrightnessMethods`) are deferred: they report "not supported" here.
use std::sync::mpsc::{Receiver, Sender, SyncSender, channel, sync_channel};
use windows::Win32::{
    Devices::Display::{
        DestroyPhysicalMonitors, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR,
        GetPhysicalMonitorsFromHMONITOR, PHYSICAL_MONITOR, SetMonitorBrightness,
    },
    Foundation::{LPARAM, POINT, WPARAM},
    Graphics::Gdi::{MONITOR_DEFAULTTONULL, MonitorFromPoint},
    UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP},
};

/// Thread message that only wakes the host loop; results wait in the channel.
const WAKE: u32 = WM_APP + 52;
/// Jobs queued at most; a full queue drops the job (the next throttled write resends the latest).
const QUEUE: usize = 8;

/// A monitor, found again on the worker by a point inside it (an HMONITOR does not outlive a topology change).
pub type Target = (i32, i32);

enum Job {
    Read(Target),
    Write(Target, u8),
}

/// What the worker found for `target`: its brightness percent, or `None` without DDC/CI.
pub struct Reading {
    pub target: Target,
    pub percent: Option<u8>,
}

/// The host's end: queues jobs and collects readings. Dropping it lets the worker finish and exit; it is
/// not joined, so a hung monitor call never delays shutdown (the process exit reaps the thread).
pub struct Brightness {
    jobs: SyncSender<Job>,
    readings: Receiver<Reading>,
}
impl Brightness {
    /// Starts the worker; `None` if the thread cannot be created.
    pub fn start(host: u32) -> Option<Self> {
        let (jobs, queue) = sync_channel(QUEUE);
        let (done, readings) = channel();
        std::thread::Builder::new()
            .name("roqa-brightness".into())
            .spawn(move || work(&queue, &done, host))
            .ok()?;
        Some(Self { jobs, readings })
    }
    /// Asks for the current level; it arrives through `poll`.
    pub fn read(&self, target: Target) {
        let _ = self.jobs.try_send(Job::Read(target));
    }
    /// Asks for a new level, 0 to 100.
    pub fn write(&self, target: Target, percent: u8) {
        let _ = self.jobs.try_send(Job::Write(target, percent));
    }
    pub fn poll(&self) -> Option<Reading> {
        self.readings.try_recv().ok()
    }
}

fn work(queue: &Receiver<Job>, done: &Sender<Reading>, host: u32) {
    // The raw (min, max) of each monitor read so far, keyed by its target.
    let mut ranges: Vec<(Target, (u32, u32))> = Vec::new();
    while let Ok(first) = queue.recv() {
        let mut batch = vec![first];
        while let Ok(job) = queue.try_recv() {
            batch.push(job);
        }
        for job in coalesce(batch) {
            match job {
                Job::Read(target) => {
                    let found = read_raw(target);
                    if let Some((_, min, max)) = found {
                        ranges.retain(|(known, _)| *known != target);
                        ranges.push((target, (min, max)));
                    }
                    let percent = found.map(|(current, min, max)| percent_of(current, min, max));
                    if done.send(Reading { target, percent }).is_err() {
                        return; // the host is gone
                    }
                    // SAFETY: a scalar thread message with no pointers; a lost post only delays the poll.
                    let _ = unsafe { PostThreadMessageW(host, WAKE, WPARAM(0), LPARAM(0)) };
                }
                Job::Write(target, percent) => {
                    let range = ranges
                        .iter()
                        .find(|(known, _)| *known == target)
                        .map(|r| r.1);
                    let range = range.or_else(|| read_raw(target).map(|(_, min, max)| (min, max)));
                    if let Some((min, max)) = range {
                        write_raw(target, scale(percent, min, max));
                    }
                }
            }
        }
    }
}

/// Reads keep their place; of the writes to one monitor only the last survives, in the place it arrived.
fn coalesce(batch: Vec<Job>) -> Vec<Job> {
    let last = |index: usize, target: Target| {
        batch[index + 1..]
            .iter()
            .all(|job| !matches!(job, Job::Write(other, _) if *other == target))
    };
    let keep: Vec<bool> = batch
        .iter()
        .enumerate()
        .map(|(index, job)| match job {
            Job::Write(target, _) => last(index, *target),
            Job::Read(_) => true,
        })
        .collect();
    batch
        .into_iter()
        .zip(keep)
        .filter_map(|(job, keep)| keep.then_some(job))
        .collect()
}

/// A raw DDC value as a whole percent of the monitor's range.
fn percent_of(current: u32, min: u32, max: u32) -> u8 {
    if max <= min {
        return 0;
    }
    let part = u64::from(current.clamp(min, max) - min) * 100;
    (part + u64::from(max - min) / 2).div_euclid(u64::from(max - min)) as u8
}

/// A percent as a raw DDC value in `min..=max`.
fn scale(percent: u8, min: u32, max: u32) -> u32 {
    let span = u64::from(max.saturating_sub(min));
    min + ((span * u64::from(percent.min(100)) + 50) / 100) as u32
}

/// Runs `action` on the first physical monitor of the display containing `target`, then releases them all.
fn with_monitor<T>(
    target: Target,
    action: impl FnOnce(&PHYSICAL_MONITOR) -> Option<T>,
) -> Option<T> {
    // SAFETY: documented dxva2 calls on a monitor handle looked up just now; the physical monitors are
    // destroyed before returning, and the array outlives `action`.
    unsafe {
        let monitor = MonitorFromPoint(
            POINT {
                x: target.0,
                y: target.1,
            },
            MONITOR_DEFAULTTONULL,
        );
        if monitor.is_invalid() {
            return None;
        }
        let mut count = 0;
        GetNumberOfPhysicalMonitorsFromHMONITOR(monitor, &mut count).ok()?;
        if count == 0 {
            return None;
        }
        let mut physical = vec![PHYSICAL_MONITOR::default(); count as usize];
        GetPhysicalMonitorsFromHMONITOR(monitor, &mut physical).ok()?;
        let result = action(&physical[0]);
        if let Err(error) = DestroyPhysicalMonitors(&physical) {
            eprintln!("Brightness: release: {error}");
        }
        result
    }
}

/// `(current, min, max)` raw values, or `None` when the monitor does not answer DDC/CI.
fn read_raw(target: Target) -> Option<(u32, u32, u32)> {
    with_monitor(target, |monitor| {
        let (mut min, mut current, mut max) = (0, 0, 0);
        // SAFETY: a valid physical-monitor handle and three local outputs.
        let ok = unsafe {
            GetMonitorBrightness(monitor.hPhysicalMonitor, &mut min, &mut current, &mut max)
        };
        (ok != 0 && max > min).then_some((current, min, max))
    })
}

fn write_raw(target: Target, value: u32) {
    let written = with_monitor(target, |monitor| {
        // SAFETY: a valid physical-monitor handle and a scalar.
        (unsafe { SetMonitorBrightness(monitor.hPhysicalMonitor, value) } != 0).then_some(())
    });
    if written.is_none() {
        eprintln!("Brightness: write failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ddc_values_map_to_whole_percents_and_back() {
        // The usual 0..=100 range maps to itself; odd ranges round to the nearest percent.
        assert_eq!(
            (
                percent_of(0, 0, 100),
                percent_of(37, 0, 100),
                percent_of(100, 0, 100)
            ),
            (0, 37, 100)
        );
        assert_eq!(percent_of(5, 0, 10), 50);
        assert_eq!(percent_of(15, 10, 20), 50);
        assert_eq!(
            percent_of(999, 0, 100),
            100,
            "an out-of-range reading clamps"
        );
        assert_eq!(percent_of(3, 5, 5), 0, "an empty range is not divided by");
        assert_eq!(
            (scale(0, 10, 20), scale(50, 10, 20), scale(100, 10, 20)),
            (10, 15, 20)
        );
        assert_eq!(scale(200, 0, 255), 255, "a percent above 100 clamps");
        for percent in 0..=100u8 {
            assert_eq!(percent_of(scale(percent, 0, 100), 0, 100), percent);
        }
    }

    #[test]
    fn a_batch_keeps_reads_and_only_the_last_write_per_monitor() {
        let (a, b) = ((0, 0), (1920, 0));
        let batch = vec![
            Job::Write(a, 10),
            Job::Read(b),
            Job::Write(b, 40),
            Job::Write(a, 20),
            Job::Write(a, 30),
        ];
        let kept: Vec<(char, Target, u8)> = coalesce(batch)
            .into_iter()
            .map(|job| match job {
                Job::Read(target) => ('r', target, 0),
                Job::Write(target, percent) => ('w', target, percent),
            })
            .collect();
        assert_eq!(kept, [('r', b, 0), ('w', b, 40), ('w', a, 30)]);
        assert!(coalesce(Vec::new()).is_empty());
    }
}

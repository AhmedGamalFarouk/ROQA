//! The screenshot pill (design `cook/design/moon-and-screenshot-pill.md`, part 2): shots from the clipboard
//! worker join the stack, the stack takes the notice row when it is free, and a shot is dragged out to an app
//! or the Shelf. One `impl Desktop` block; the methods are `pub(super)` so the host reaches them.

use super::*;
use crate::{shot_os, shots::Shot};
use std::rc::Rc;

/// A press on the chip (or on a kept shot on the Shelf) that becomes a drag once the pointer moves.
pub(super) struct ShotPress {
    x: i32,
    y: i32,
    /// The kept shot under the press; `None` is the pill's newest.
    kept: Option<Rc<Shot>>,
}

impl Desktop {
    /// The worker's finished shots join the stack (newest first); the next pass shows it when the row is free.
    pub(super) fn take_shots(&mut self) {
        let now = Instant::now();
        let mut added = false;
        for shot in shot_os::take() {
            self.shelf.shots.add(Rc::new(shot), now);
            added = true;
        }
        if added {
            self.request_notches(Repaint::Full, |_| true);
        }
    }
    /// The pill takes a free notice row, or gives it back: the row's owner is told to `Power` so a plain notice
    /// waits for the pill and a held pill keeps the row. True when the pill came or went.
    pub(super) fn sync_shots(&mut self, now: Instant) -> bool {
        let shots = &mut self.shelf.shots;
        let over = shots.over(now);
        let changed = shots.sync(self.power.text().is_none(), now) || over;
        self.power.set_shot(shots.showing().then(|| shots.held()));
        changed
    }
    /// Pointer input on a notch for the pill: hover on the chip, and a press that moves far enough starts a drag
    /// (from the chip, or from a kept shot on the Shelf). Never consumes the message.
    pub(super) fn shot_input(&mut self, notch: usize, message: &MSG) {
        let surface = &self.surfaces[notch];
        let (x, y) = point(message.lParam);
        let now = Instant::now();
        let media = self.pages.track(&self.media).is_some();
        let on_chip = self.shelf.shots.showing()
            && self.power.text().is_none()
            && layout::shot_chip_at(
                surface.bounds.width,
                surface.dpi,
                surface.motion.value(now),
                media,
                (x, y),
            );
        match message.message {
            WM_MOUSEMOVE => {
                if self.shelf.shots.point(on_chip, now) {
                    self.request_notches(Repaint::Full, |_| true);
                    self.surfaces
                        .iter_mut()
                        .filter(|s| s.kind() == SurfaceKind::Notch)
                        .for_each(|s| s.live = true);
                }
                let dpi = self.surfaces[notch].dpi;
                let moved = self.shot_press.as_ref().is_some_and(|press| {
                    dock::drag_started(press.x, x, dpi) || dock::drag_started(press.y, y, dpi)
                });
                if message.wParam.0 & MK_LBUTTON == 0 {
                    self.shot_press = None;
                } else if moved && let Some(press) = self.shot_press.take() {
                    let shot = press
                        .kept
                        .or_else(|| self.shelf.shots.list().first().cloned());
                    if let Some(shot) = shot {
                        self.drag_shot(notch, &shot);
                    }
                }
            }
            WM_LBUTTONDOWN => {
                let kept = self
                    .shelf_slot_at(notch, message.lParam)
                    .and_then(|slot| self.shelf.shelf.absolute(slot))
                    .and_then(|index| self.shelf.shelf.entries().get(index))
                    .and_then(|entry| entry.shot.as_ref())
                    .map(|kept| Rc::clone(&kept.0));
                self.shot_press = (on_chip || kept.is_some()).then_some(ShotPress { x, y, kept });
            }
            WM_LBUTTONUP => self.shot_press = None,
            WM_MOUSELEAVE => {
                if self.shelf.shots.point(false, now) {
                    self.request_notches(Repaint::Full, |_| true);
                }
            }
            _ => {}
        }
    }
    /// Drags `shot` out in the system's drag loop. The page shows the Shelf first, painted at rest, since no frame
    /// runs inside the loop. A drop on the Shelf keeps the shot there; a drop anywhere takes it off the pill.
    fn drag_shot(&mut self, notch: usize, shot: &Rc<Shot>) {
        let now = Instant::now();
        let from_pill = self.shelf.shots.list().iter().any(|s| Rc::ptr_eq(s, shot));
        let shelf_on = self.pages.tabs.on(Tab::Shelf);
        self.shelf.shots.dragging = from_pill;
        self.shelf.shots.hold(true, now);
        self.power
            .set_shot(self.shelf.shots.showing().then_some(true));
        if shelf_on {
            self.shelf.copied = false;
            self.select_tab(Tab::Shelf);
        }
        // Two paints: the first starts the page switch, the second shows where it lands.
        for _ in 0..2 {
            self.request_notches(Repaint::Full, |_| true);
            let painted = self.paint_requested();
            self.recover(painted, "Shot drag paint");
            for surface in &mut self.surfaces {
                surface.page_fx = Tween::rest(1.0);
                surface.page_from = None;
            }
        }
        let (hwnd, dpi) = (self.surfaces[notch].window.0, self.surfaces[notch].dpi);
        let dropped = shot_os::drag(hwnd, shot, dpi);
        let mut kept = false;
        self.drops.borrow_mut().retain(|event| match event {
            DropEvent::Shot(id) if *id == shot.id => {
                kept = true;
                false
            }
            _ => true,
        });
        let now = Instant::now();
        self.shelf.shots.dragging = false;
        self.shelf.shots.point(false, now);
        self.shelf.shots.hold(false, now);
        match dropped {
            Ok(true) if kept && !shelf_on => {}
            Ok(true) => {
                if kept {
                    self.shelf.shelf.add_shot(Rc::clone(shot));
                }
                if from_pill {
                    self.shelf.shots.remove(shot.id);
                }
            }
            Ok(false) => {}
            Err(error) => {
                eprintln!("Shot drag: {error}");
                self.show_notice("Couldn't drag that.".into(), now);
            }
        }
        self.request_notches(Repaint::Full, |_| true);
    }
}

/// What the chip draws this frame: the newest shot and the one under it, while the pill holds the row.
pub(super) fn scene(
    shots: &crate::shots::Shots,
    on: bool,
    now: Instant,
) -> Option<crate::graphics::ShotScene<'_>> {
    let list = shots.list();
    list.first()
        .filter(|_| on)
        .map(|top| crate::graphics::ShotScene {
            top: (top.id, &top.thumb),
            under: list.get(1).map(|shot| (shot.id, &shot.thumb)),
            count: list.len(),
            land: shots.land.value(now),
            hover: shots.hover.value(now),
            dragging: shots.dragging,
        })
}

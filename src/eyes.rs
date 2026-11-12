//! Mascot eyes (features tabs-eyes, design round 2 variant C "Squash"): the pure mood and pose rules. Eyes are
//! two capsules in the pill. A blink or a mood change is a short stepped sequence (at most three poses) that
//! only a trigger starts: the once-a-minute clock repaint, a mood change or the pointer entering the notch.
//! There is no free-running timer, so `due` is `None` whenever no sequence runs.

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Idle,
    /// Music is playing.
    Happy,
    /// Long idle, or late at night.
    Sleepy,
    /// A timer finished or a power notice shows.
    Surprised,
}

/// Squash and stretch: the shape the eye holds for one step of a sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pose {
    Rest,
    Squash,
    Stretch,
}

/// No input for this long counts as long idle.
pub const IDLE_AFTER: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Copy, Debug, Default)]
pub struct Facts {
    pub playing: bool,
    pub alert: bool,
    pub idle: bool,
    /// Local hour, 0 to 23.
    pub hour: u8,
}

/// Late means 23:00 to 04:59.
pub fn late(hour: u8) -> bool {
    !(5..23).contains(&hour)
}

/// The mood the facts call for; a fresh alert outranks music, music outranks sleep.
pub fn mood(facts: Facts) -> Mood {
    if facts.alert {
        Mood::Surprised
    } else if facts.playing {
        Mood::Happy
    } else if facts.idle || late(facts.hour) {
        Mood::Sleepy
    } else {
        Mood::Idle
    }
}

/// One eye in DIPs: size, vertical offset from the lane's middle, and the fractions of its height cut off by
/// a flat top and bottom lid. Both eyes share it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Eye {
    pub w: f32,
    pub h: f32,
    pub dy: f32,
    pub top: f32,
    pub bottom: f32,
}

/// Distance between the two eyes' centres, and the base capsule.
pub const GAP: f32 = 17.0;
const BASE: (f32, f32) = (5.5, 11.0);

/// Design C's shapes: (width factor, height factor, dy, top lid, bottom lid) per mood, then the pose's
/// factors on top (`blsq`: wide and flat at the squash, tall and thin at the stretch).
pub fn eye(mood: Mood, pose: Pose) -> Eye {
    let (sx, sy, dy, top, bottom) = match mood {
        Mood::Idle => (1.0, 1.0, 0.0, 0.0, 0.0),
        Mood::Happy => (1.22, 0.78, -0.6, 0.0, 0.35),
        Mood::Sleepy => (1.2, 0.5, 2.0, 0.2, 0.0),
        Mood::Surprised => (0.85, 1.45, -1.0, 0.0, 0.0),
    };
    let (px, py) = match pose {
        Pose::Rest => (1.0, 1.0),
        Pose::Squash => (1.3, 0.1),
        Pose::Stretch => (0.86, 1.14),
    };
    Eye {
        w: BASE.0 * sx * px,
        h: BASE.1 * sy * py,
        dy,
        top,
        bottom,
    }
}

/// The poses held before the eye returns to rest, with how long each lasts: a blink squashes and opens; a mood
/// change squashes, overshoots tall, then settles.
const BLINK: [(Pose, Duration); 1] = [(Pose::Squash, Duration::from_millis(130))];
const CHANGE: [(Pose, Duration); 2] = [
    (Pose::Squash, Duration::from_millis(120)),
    (Pose::Stretch, Duration::from_millis(120)),
];

#[derive(Clone, Copy, Debug)]
struct Run {
    steps: &'static [(Pose, Duration)],
    at: usize,
    next: Instant,
}

#[derive(Clone, Copy, Debug)]
pub struct Eyes {
    mood: Mood,
    run: Option<Run>,
}

impl Default for Eyes {
    fn default() -> Self {
        Self {
            mood: Mood::Idle,
            run: None,
        }
    }
}

impl Eyes {
    pub fn mood(&self) -> Mood {
        self.mood
    }
    pub fn pose(&self) -> Pose {
        self.run.map_or(Pose::Rest, |run| run.steps[run.at].0)
    }
    fn start(&mut self, steps: &'static [(Pose, Duration)], now: Instant) {
        self.run = Some(Run {
            steps,
            at: 0,
            next: now + steps[0].1,
        });
    }
    /// A new mood squashes and settles (a repaint is needed). The same mood does nothing.
    pub fn set_mood(&mut self, mood: Mood, now: Instant) -> bool {
        if mood == self.mood {
            return false;
        }
        self.mood = mood;
        self.start(&CHANGE, now);
        true
    }
    /// A blink, started by the clock repaint or the pointer. A sequence already running keeps going.
    /// Returns whether the pose changed (the caller paints in the same pass).
    pub fn blink(&mut self, now: Instant) -> bool {
        if self.run.is_some() {
            return false;
        }
        self.start(&BLINK, now);
        true
    }
    /// Moves to the step now due; returns whether the pose changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        let Some(mut run) = self.run else {
            return false;
        };
        if now < run.next {
            return false;
        }
        let before = run.steps[run.at].0;
        run.at += 1;
        self.run = if run.at < run.steps.len() {
            run.next = now + run.steps[run.at].1;
            Some(run)
        } else {
            None
        };
        self.pose() != before
    }
    /// The next step's time; `None` at rest, the common case.
    pub fn due(&self) -> Option<Instant> {
        self.run.map(|run| run.next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(playing: bool, alert: bool, idle: bool, hour: u8) -> Facts {
        Facts {
            playing,
            alert,
            idle,
            hour,
        }
    }

    #[test]
    fn events_pick_the_mood_and_alerts_outrank_music_and_sleep() {
        assert_eq!(mood(facts(false, false, false, 14)), Mood::Idle);
        assert_eq!(mood(facts(true, false, false, 14)), Mood::Happy);
        assert_eq!(mood(facts(false, true, false, 14)), Mood::Surprised);
        assert_eq!(mood(facts(true, true, true, 3)), Mood::Surprised);
        assert_eq!(
            mood(facts(true, false, true, 3)),
            Mood::Happy,
            "music keeps it awake"
        );
        assert_eq!(
            mood(facts(false, false, true, 14)),
            Mood::Sleepy,
            "long idle"
        );
        assert_eq!(mood(facts(false, false, false, 2)), Mood::Sleepy, "late");
        assert!(late(23) && late(0) && late(4) && !late(5) && !late(22));
    }

    #[test]
    fn poses_squash_wide_and_flat_and_stretch_tall() {
        let rest = eye(Mood::Idle, Pose::Rest);
        assert_eq!((rest.w, rest.h), (5.5, 11.0));
        let squash = eye(Mood::Idle, Pose::Squash);
        assert!(squash.w > rest.w && squash.h < rest.h / 5.0);
        let stretch = eye(Mood::Idle, Pose::Stretch);
        assert!(stretch.w < rest.w && stretch.h > rest.h);
        assert!(eye(Mood::Surprised, Pose::Rest).h > rest.h);
        assert!(eye(Mood::Happy, Pose::Rest).bottom > 0.0);
        assert!(eye(Mood::Sleepy, Pose::Rest).top > 0.0);
    }

    #[test]
    fn every_eye_fits_the_30_dip_pill_lane() {
        for mood in [Mood::Idle, Mood::Happy, Mood::Sleepy, Mood::Surprised] {
            for pose in [Pose::Rest, Pose::Squash, Pose::Stretch] {
                let e = eye(mood, pose);
                assert!(e.w > 0.0 && e.h > 0.0);
                assert!(
                    15.0 + e.dy - e.h / 2.0 >= 0.0 && 15.0 + e.dy + e.h / 2.0 <= 30.0,
                    "{mood:?} {pose:?}"
                );
                // Two of them, centred GAP apart, stay inside the 80 DIP lane.
                assert!(GAP + e.w < 80.0);
            }
        }
    }

    #[test]
    fn a_blink_is_never_scheduled_on_its_own() {
        let t0 = Instant::now();
        let mut eyes = Eyes::default();
        assert_eq!(eyes.due(), None);
        // Time passing, ticks and the same mood again start nothing.
        for minutes in [1, 60, 600] {
            assert!(!eyes.tick(t0 + Duration::from_secs(60 * minutes)));
            assert!(!eyes.set_mood(Mood::Idle, t0));
            assert_eq!((eyes.due(), eyes.pose()), (None, Pose::Rest));
        }
    }

    #[test]
    fn a_clock_blink_squashes_then_opens_and_then_asks_for_nothing() {
        let t0 = Instant::now();
        let mut eyes = Eyes::default();
        assert!(eyes.blink(t0), "painted with the clock in the same pass");
        assert_eq!(eyes.pose(), Pose::Squash);
        assert!(!eyes.blink(t0), "one at a time");
        let due = eyes.due().unwrap();
        assert_eq!(due, t0 + Duration::from_millis(130));
        assert!(!eyes.tick(due - Duration::from_millis(1)));
        assert!(eyes.tick(due));
        assert_eq!((eyes.pose(), eyes.due()), (Pose::Rest, None));
    }

    #[test]
    fn a_mood_change_squashes_overshoots_and_settles_on_the_new_mood() {
        let t0 = Instant::now();
        let mut eyes = Eyes::default();
        assert!(eyes.set_mood(Mood::Happy, t0));
        assert_eq!((eyes.mood(), eyes.pose()), (Mood::Happy, Pose::Squash));
        let mut now = t0;
        let mut poses = vec![eyes.pose()];
        while let Some(due) = eyes.due() {
            now = due;
            if eyes.tick(now) {
                poses.push(eyes.pose());
            }
        }
        assert_eq!(poses, [Pose::Squash, Pose::Stretch, Pose::Rest]);
        assert_eq!(now, t0 + Duration::from_millis(240));
        assert_eq!(eyes.mood(), Mood::Happy);
    }
}

//! Mascot eyes (features tabs-eyes, design round 2 variant C "Squash"): the pure mood and pose rules. Eyes are
//! two capsules that are NOT shown by default: the pill shows the time. Every minute or so (random-ish, one
//! deadline) they fade in, blink, look around or smile, and fade out; a happy or surprised mood does the same
//! once. Between shows nothing runs, so the only wait is the next show's deadline and a paint happens only
//! for the steps of a show.

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
    /// A prayer reminder is active: half-lidded, with a stepped glow.
    Adhan,
    /// The Weather page is open and it rains or storms: droopy.
    Rain,
}

/// Squash and stretch: the shape the eye holds for one step of a show.
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
    /// A prayer reminder is active.
    pub adhan: bool,
    /// The Weather page shows a rain or storm condition.
    pub rain: bool,
    /// Local hour, 0 to 23.
    pub hour: u8,
}

/// Late means 23:00 to 04:59.
pub fn late(hour: u8) -> bool {
    !(5..23).contains(&hour)
}

/// The mood the facts call for: a prayer reminder, then a fresh alert, outrank the rain page, which outranks
/// music; music outranks sleep.
pub fn mood(facts: Facts) -> Mood {
    if facts.adhan {
        Mood::Adhan
    } else if facts.alert {
        Mood::Surprised
    } else if facts.rain {
        Mood::Rain
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
        Mood::Adhan => (1.2, 0.9, 0.0, 0.25, 0.25),
        Mood::Rain => (0.9, 1.0, 2.0, 0.4, 0.0),
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

/// One step of a show: the eyes' opacity, horizontal look (DIPs), whether they smile, the squash pose and how
/// long it holds (ms). Stepped opacity is the fade; a show is about a dozen paints.
type Step = (f32, f32, bool, Pose, u64);
const R: Pose = Pose::Rest;
const SQ: Pose = Pose::Squash;
const ST: Pose = Pose::Stretch;
/// The last step of every show is the resting one: it ends the show.
const BLINK: [Step; 8] = [
    (0.3, 0.0, false, R, 60),
    (0.7, 0.0, false, R, 60),
    (1.0, 0.0, false, R, 450),
    (1.0, 0.0, false, SQ, 120),
    (1.0, 0.0, false, R, 450),
    (0.7, 0.0, false, R, 60),
    (0.3, 0.0, false, R, 60),
    (0.0, 0.0, false, R, 0),
];
const LOOK: [Step; 9] = [
    (0.3, 0.0, false, R, 60),
    (0.7, 0.0, false, R, 60),
    (1.0, -3.0, false, R, 350),
    (1.0, 3.0, false, R, 350),
    (1.0, 0.0, false, SQ, 120),
    (1.0, 0.0, false, R, 250),
    (0.6, 0.0, false, R, 60),
    (0.3, 0.0, false, R, 60),
    (0.0, 0.0, false, R, 0),
];
const SMILE: [Step; 7] = [
    (0.3, 0.0, false, R, 60),
    (0.7, 0.0, false, R, 60),
    (1.0, 0.0, true, SQ, 100),
    (1.0, 0.0, true, R, 900),
    (0.6, 0.0, true, R, 60),
    (0.3, 0.0, true, R, 60),
    (0.0, 0.0, false, R, 0),
];
/// A mood event (happy or surprised): fade in squashing, overshoot, hold, fade out.
const EVENT: [Step; 8] = [
    (0.4, 0.0, false, SQ, 80),
    (1.0, 0.0, false, SQ, 120),
    (1.0, 0.0, false, ST, 120),
    (1.0, 0.0, false, R, 1500),
    (0.7, 0.0, false, R, 60),
    (0.4, 0.0, false, R, 60),
    (0.15, 0.0, false, R, 60),
    (0.0, 0.0, false, R, 0),
];

/// The adhan: fade in, three slow glow pulses (stepped opacity), fade out; 11 paints in about 4 s.
const ADHAN: [Step; 11] = [
    (0.4, 0.0, false, R, 80),
    (0.7, 0.0, false, R, 80),
    (1.0, 0.0, false, R, 700),
    (0.65, 0.0, false, R, 700),
    (1.0, 0.0, false, R, 700),
    (0.65, 0.0, false, R, 700),
    (1.0, 0.0, false, R, 700),
    (0.65, 0.0, false, R, 300),
    (0.35, 0.0, false, R, 80),
    (0.15, 0.0, false, R, 80),
    (0.0, 0.0, false, R, 0),
];
/// Rain: the droopy eyes fade in, hold and fade out.
const RAIN: [Step; 6] = [
    (0.4, 0.0, false, R, 80),
    (0.8, 0.0, false, R, 80),
    (1.0, 0.0, false, R, 1800),
    (0.6, 0.0, false, R, 60),
    (0.3, 0.0, false, R, 60),
    (0.0, 0.0, false, R, 0),
];

/// Waking up (start-up, resume, back from sleepy): the eyes open from a squash, stretch, smile and fade.
const WAKE: [Step; 9] = [
    (0.4, 0.0, false, SQ, 80),
    (1.0, 0.0, false, SQ, 120),
    (1.0, 0.0, false, ST, 120),
    (1.0, 0.0, false, R, 250),
    (1.0, 0.0, true, SQ, 100),
    (1.0, 0.0, true, R, 800),
    (0.6, 0.0, true, R, 60),
    (0.3, 0.0, true, R, 60),
    (0.0, 0.0, false, R, 0),
];

/// The gap before the next idle show: `MIN_GAP` plus up to `SPREAD_MS` more.
pub const MIN_GAP: Duration = Duration::from_secs(40);
const SPREAD_MS: u32 = 60_000;

/// What the pill draws now: the mood's shape (or a smile), the pose, the opacity and the look offset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub mood: Mood,
    pub pose: Pose,
    pub alpha: f32,
    pub look: f32,
}

#[derive(Clone, Copy, Debug)]
struct Run {
    steps: &'static [Step],
    at: usize,
    next: Instant,
}

#[derive(Clone, Copy, Debug)]
pub struct Eyes {
    mood: Mood,
    run: Option<Run>,
    next_show: Instant,
    seed: u32,
}

impl Default for Eyes {
    fn default() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |since| since.subsec_nanos());
        Self::new(Instant::now(), seed)
    }
}

impl Eyes {
    /// Resting, with the first show a random gap after `now`.
    pub fn new(now: Instant, seed: u32) -> Self {
        let mut eyes = Self {
            mood: Mood::Idle,
            run: None,
            next_show: now,
            seed: seed | 1,
        };
        eyes.next_show = now + eyes.gap();
        eyes
    }
    #[cfg(test)]
    pub fn mood(&self) -> Mood {
        self.mood
    }
    /// xorshift32: cheap, and the shows only need to look unpredictable.
    fn roll(&mut self) -> u32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        self.seed
    }
    fn gap(&mut self) -> Duration {
        MIN_GAP + Duration::from_millis(u64::from(self.roll() % SPREAD_MS))
    }
    /// `None` at rest, when the pill shows only the time.
    pub fn frame(&self) -> Option<Frame> {
        let run = self.run?;
        let (alpha, look, smile, pose, _) = run.steps[run.at];
        Some(Frame {
            mood: if smile { Mood::Happy } else { self.mood },
            pose,
            alpha,
            look,
        })
    }
    fn start(&mut self, steps: &'static [Step], now: Instant) {
        self.run = Some(Run {
            steps,
            at: 0,
            next: now + Duration::from_millis(steps[0].4),
        });
    }
    /// A new happy, surprised, adhan or rain mood shows the eyes once; any other change is silent. Returns whether a show
    /// started (a repaint is needed).
    pub fn set_mood(&mut self, mood: Mood, now: Instant) -> bool {
        if mood == self.mood {
            return false;
        }
        let was = std::mem::replace(&mut self.mood, mood);
        if was == Mood::Sleepy && mood == Mood::Idle {
            return self.wake(now);
        }
        let show: &'static [Step] = match mood {
            Mood::Happy | Mood::Surprised => &EVENT,
            Mood::Adhan => &ADHAN,
            Mood::Rain => &RAIN,
            _ => return false,
        };
        self.start(show, now);
        true
    }
    /// Plays the wake animation now, replacing any show; true (a repaint is needed).
    pub fn wake(&mut self, now: Instant) -> bool {
        self.start(&WAKE, now);
        true
    }
    /// Moves to the step now due, or starts the idle show whose deadline passed; returns whether the frame
    /// changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        let before = self.frame();
        let Some(mut run) = self.run else {
            if now >= self.next_show {
                self.next_show = now + self.gap();
                let shows: [&'static [Step]; 3] = [&BLINK, &LOOK, &SMILE];
                let pick = shows[self.roll() as usize % shows.len()];
                self.start(pick, now);
            }
            return self.frame() != before;
        };
        if now >= run.next {
            run.at += 1;
            self.run = if run.at + 1 < run.steps.len() {
                run.next = now + Duration::from_millis(run.steps[run.at].4);
                Some(run)
            } else {
                None
            };
        }
        self.frame() != before
    }
    /// The next step's time while a show runs, else the next show's: the only wait the eyes ask for.
    pub fn due(&self) -> Instant {
        self.run.map_or(self.next_show, |run| run.next)
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
            ..Facts::default()
        }
    }

    #[test]
    fn adhan_outranks_everything_and_rain_outranks_music_and_sleep() {
        let base = facts(true, true, true, 3);
        assert_eq!(
            mood(Facts {
                adhan: true,
                rain: true,
                ..base
            }),
            Mood::Adhan
        );
        assert_eq!(
            mood(Facts { rain: true, ..base }),
            Mood::Surprised,
            "alerts first"
        );
        let calm = facts(true, false, true, 3);
        assert_eq!(mood(Facts { rain: true, ..calm }), Mood::Rain);
        assert_eq!(
            mood(Facts {
                adhan: true,
                ..facts(false, false, false, 14)
            }),
            Mood::Adhan
        );
        assert_eq!(
            mood(Facts {
                rain: false,
                ..calm
            }),
            Mood::Happy
        );
    }

    #[test]
    fn adhan_and_rain_show_once_with_their_own_shapes() {
        let t0 = Instant::now();
        let mut eyes = Eyes::new(t0, 9);
        assert!(eyes.set_mood(Mood::Adhan, t0));
        let (frames, end) = play(&mut eyes, t0);
        assert!(frames.iter().all(|f| f.mood == Mood::Adhan));
        // Stepped glow: opacity returns to full more than once, and the show is short.
        assert!(frames.iter().filter(|f| f.alpha == 1.0).count() >= 3);
        assert!(frames.len() <= 12 && end - t0 < Duration::from_secs(5));
        assert!(
            !eyes.set_mood(Mood::Adhan, t0),
            "the same mood shows nothing"
        );
        assert!(!eyes.set_mood(Mood::Idle, t0), "leaving it is silent");
        assert!(eyes.set_mood(Mood::Rain, t0));
        let (frames, _) = play(&mut eyes, t0);
        assert!(frames.iter().all(|f| f.mood == Mood::Rain) && frames.len() <= 6);
        let (half, droop) = (eye(Mood::Adhan, Pose::Rest), eye(Mood::Rain, Pose::Rest));
        assert!(half.top > 0.0 && half.bottom > 0.0, "half-lidded");
        assert!(
            droop.top > 0.0 && droop.dy > 0.0,
            "droopy: lid down, sitting low"
        );
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
        for mood in [
            Mood::Idle,
            Mood::Happy,
            Mood::Sleepy,
            Mood::Surprised,
            Mood::Adhan,
            Mood::Rain,
        ] {
            for pose in [Pose::Rest, Pose::Squash, Pose::Stretch] {
                let e = eye(mood, pose);
                assert!(e.w > 0.0 && e.h > 0.0);
                assert!(
                    15.0 + e.dy - e.h / 2.0 >= 0.0 && 15.0 + e.dy + e.h / 2.0 <= 30.0,
                    "{mood:?} {pose:?}"
                );
                // Two of them, centred GAP apart and looking 4 aside, stay inside the 80 DIP lane.
                assert!(GAP + e.w + 8.0 < 80.0);
            }
        }
    }

    /// Runs the show now under way to its end; returns the frames painted and the time it ended.
    fn play(eyes: &mut Eyes, mut now: Instant) -> (Vec<Frame>, Instant) {
        let mut seen = Vec::new();
        while eyes.frame().is_some() {
            now = eyes.due();
            if eyes.tick(now)
                && let Some(frame) = eyes.frame()
            {
                seen.push(frame);
            }
        }
        (seen, now)
    }

    #[test]
    fn at_rest_nothing_is_drawn_and_the_only_wait_is_the_next_show() {
        let t0 = Instant::now();
        let mut eyes = Eyes::new(t0, 7);
        assert!(eyes.frame().is_none());
        let due = eyes.due();
        assert!(due >= t0 + MIN_GAP && due < t0 + MIN_GAP + Duration::from_millis(60_000));
        // Time before the deadline, the same mood and a quiet mood start nothing.
        assert!(!eyes.tick(due - Duration::from_millis(1)));
        assert!(!eyes.set_mood(Mood::Idle, t0));
        assert!(!eyes.set_mood(Mood::Sleepy, t0), "sleep is silent");
        assert!(eyes.frame().is_none() && eyes.mood() == Mood::Sleepy);
    }

    #[test]
    fn every_idle_show_fades_in_does_its_trick_fades_out_and_reschedules() {
        let t0 = Instant::now();
        let mut kinds = std::collections::HashSet::new();
        for seed in 1..40 {
            let mut eyes = Eyes::new(t0, seed);
            let mut now = eyes.due();
            for _ in 0..3 {
                assert!(eyes.tick(now), "the show's first frame is painted");
                let first = eyes.frame().unwrap();
                assert!(first.alpha > 0.0 && first.alpha < 1.0, "fades in");
                let (frames, end) = play(&mut eyes, now);
                assert!(frames.iter().any(|f| f.alpha == 1.0));
                assert!(frames.len() <= 12, "a handful of paints: {}", frames.len());
                assert!(end - now < Duration::from_secs(3), "short");
                assert!(frames.iter().all(|f| f.look.abs() <= 4.0));
                kinds.insert((
                    frames.iter().any(|f| f.look != 0.0),
                    frames.iter().any(|f| f.mood == Mood::Happy),
                ));
                assert!(eyes.frame().is_none(), "back to the time");
                assert!(eyes.due() >= end + MIN_GAP - Duration::from_secs(3));
                now = eyes.due();
            }
        }
        assert!(
            kinds.len() >= 3,
            "blink, look and smile all occur: {kinds:?}"
        );
    }

    #[test]
    fn happy_and_surprised_show_once_and_other_moods_stay_hidden() {
        let t0 = Instant::now();
        let mut eyes = Eyes::new(t0, 3);
        assert!(eyes.set_mood(Mood::Happy, t0));
        assert_eq!(eyes.frame().unwrap().mood, Mood::Happy);
        let (frames, _) = play(&mut eyes, t0);
        assert!(frames.iter().any(|f| f.pose == Pose::Stretch));
        assert!(eyes.frame().is_none());
        assert!(
            !eyes.set_mood(Mood::Happy, t0),
            "the same mood shows nothing"
        );
        assert!(eyes.set_mood(Mood::Surprised, t0));
        assert!(!eyes.set_mood(Mood::Idle, t0));
    }

    #[test]
    fn waking_plays_open_smile_fade_and_waking_from_sleepy_wakes() {
        let t0 = Instant::now();
        let mut eyes = Eyes::new(t0, 5);
        assert!(eyes.wake(t0));
        let (frames, end) = play(&mut eyes, t0);
        assert!(frames.iter().any(|f| f.pose == Pose::Stretch));
        assert!(frames.iter().any(|f| f.mood == Mood::Happy), "smiles");
        assert!(end - t0 < Duration::from_secs(3) && eyes.frame().is_none());
        // Falling asleep is silent; coming back to idle wakes; the idle gap is not touched.
        let gap = eyes.due();
        assert!(!eyes.set_mood(Mood::Sleepy, t0));
        assert!(eyes.set_mood(Mood::Idle, t0));
        assert!(eyes.frame().is_some());
        play(&mut eyes, t0);
        assert_eq!(eyes.due(), gap);
    }
}

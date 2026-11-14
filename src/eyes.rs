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
    /// The pointer arrived: wide and a little flat, sitting low (brand "squash on hover").
    Bump,
    /// Lids sliding down for a doze.
    Doze,
    /// The right eye hops and tilts (drawn by `tab_art::draw_eyes`); the shape is the rest one.
    Wave,
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
pub const GAP: f32 = 25.0;
const BASE: (f32, f32) = (9.5, 18.0);

/// Design C's shapes: (width factor, height factor, dy, top lid, bottom lid) per mood, then the pose's
/// factors on top (`blsq`: wide and flat at the squash, tall and thin at the stretch).
pub fn eye(mood: Mood, pose: Pose) -> Eye {
    let (sx, sy, dy, top, bottom) = match mood {
        Mood::Idle => (1.0, 1.0, 0.0, 0.0, 0.0),
        Mood::Happy => (1.3, 0.72, -0.6, 0.0, 0.4),
        Mood::Sleepy => (1.2, 0.5, 2.0, 0.2, 0.0),
        Mood::Surprised => (0.85, 1.3, -1.0, 0.0, 0.0),
        Mood::Adhan => (1.2, 0.9, 0.0, 0.25, 0.25),
        Mood::Rain => (0.9, 1.0, 2.0, 0.4, 0.0),
    };
    // Pose factors: width, height, extra dy and extra top lid.
    let (px, py, pdy, ptop) = match pose {
        Pose::Rest | Pose::Wave => (1.0, 1.0, 0.0, 0.0),
        Pose::Squash => (1.1, 0.12, 0.0, 0.0),
        Pose::Stretch => (0.86, 1.14, 0.0, 0.0),
        Pose::Bump => (1.28, 0.82, 2.0, 0.0),
        Pose::Doze => (1.15, 0.5, 3.0, 0.2),
    };
    Eye {
        w: BASE.0 * sx * px,
        h: BASE.1 * sy * py,
        dy: dy + pdy,
        top: top + ptop,
        bottom,
    }
}

/// One step of a show: the eyes' opacity, horizontal look (DIPs), whether they smile, the squash pose and how
/// long it holds (ms). Stepped opacity is the fade; a show is about a dozen paints.
type Step = (f32, f32, bool, Pose, u64);
const R: Pose = Pose::Rest;
const SQ: Pose = Pose::Squash;
const ST: Pose = Pose::Stretch;
const BP: Pose = Pose::Bump;
const DZ: Pose = Pose::Doze;
const WV: Pose = Pose::Wave;
/// A blink anticipates with a stretch, squashes and overshoots back (U3). The last step of every show is the
/// resting one: it ends the show.
const BLINK: [Step; 10] = [
    (0.3, 0.0, false, R, 60),
    (0.7, 0.0, false, R, 60),
    (1.0, 0.0, false, R, 450),
    (1.0, 0.0, false, ST, 50),
    (1.0, 0.0, false, SQ, 90),
    (1.0, 0.0, false, ST, 60),
    (1.0, 0.0, false, R, 450),
    (0.7, 0.0, false, R, 60),
    (0.3, 0.0, false, R, 60),
    (0.0, 0.0, false, R, 0),
];
const LOOK: [Step; 11] = [
    (0.3, 0.0, false, R, 60),
    (0.7, 0.0, false, R, 60),
    (1.0, -4.0, false, R, 350),
    (1.0, 4.0, false, R, 350),
    (1.0, 0.0, false, ST, 50),
    (1.0, 0.0, false, SQ, 90),
    (1.0, 0.0, false, ST, 60),
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
/// Doze: the eyes open, the lids slide down and hold, then they fade. No loop.
const DOZE: [Step; 6] = [
    (0.3, 0.0, false, R, 60),
    (0.7, 0.0, false, R, 60),
    (1.0, 0.0, false, R, 500),
    (1.0, 0.0, false, DZ, 1400),
    (0.5, 0.0, false, DZ, 60),
    (0.0, 0.0, false, R, 0),
];
/// The pointer arrived while the eyes show: a quick squash, then back with the pop curve's overshoot (a
/// stretch step), rest and out.
const BUMP: [Step; 6] = [
    (1.0, 0.0, false, BP, 90),
    (1.0, 0.0, false, ST, 70),
    (1.0, 0.0, false, R, 700),
    (0.6, 0.0, false, R, 60),
    (0.3, 0.0, false, R, 60),
    (0.0, 0.0, false, R, 0),
];
/// The pointer arrived while the eyes were hidden: they pop in squashed, overshoot, settle and fade.
const HELLO: [Step; 7] = [
    (0.5, 0.0, false, BP, 80),
    (1.0, 0.0, false, BP, 90),
    (1.0, 0.0, false, ST, 70),
    (1.0, 0.0, false, R, 600),
    (0.6, 0.0, false, R, 60),
    (0.3, 0.0, false, R, 60),
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
/// then wave: the right eye hops twice.
const WAKE: [Step; 11] = [
    (0.4, 0.0, false, SQ, 80),
    (1.0, 0.0, false, SQ, 120),
    (1.0, 0.0, false, ST, 120),
    (1.0, 0.0, false, R, 250),
    (1.0, 0.0, true, WV, 140),
    (1.0, 0.0, true, R, 140),
    (1.0, 0.0, true, WV, 140),
    (1.0, 0.0, true, R, 300),
    (0.6, 0.0, true, R, 60),
    (0.3, 0.0, true, R, 60),
    (0.0, 0.0, false, R, 0),
];

/// No pointer on the notch for this long and the eyes doze once (the lids slide, then they fade); the next
/// pointer move wakes them with a wave. The deadline joins `due`, so no timer runs for it.
pub const DOZE_AFTER: Duration = Duration::from_secs(20);

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
    /// Horizontal lean (DIPs) the eyes hold while shown: towards a running timer (+) or the media tile (-).
    lean: f32,
    /// When the pointer's quiet becomes a doze (`None` once it has, until the next pointer move), and whether
    /// the last show was that doze.
    doze_due: Option<Instant>,
    dozed: bool,
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
            lean: 0.0,
            doze_due: Some(now + DOZE_AFTER),
            dozed: false,
        };
        eyes.next_show = now + eyes.gap();
        eyes
    }
    #[cfg(test)]
    pub fn mood(&self) -> Mood {
        self.mood
    }
    /// What the pill draws now, or at rest an invisible frame (alpha 0): the open panel still draws its small
    /// resting eyes from it, and the pill draws nothing for it.
    pub fn scene(&self) -> Frame {
        self.frame().unwrap_or(Frame {
            mood: self.mood,
            pose: Pose::Rest,
            alpha: 0.0,
            look: 0.0,
        })
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
            look: look + self.lean,
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
    /// The eyes lean `dx` DIPs (a timer runs: +7, media plays: -8, else 0); takes effect on the next frame
    /// painted, never starts one.
    pub fn set_lean(&mut self, dx: f32) {
        self.lean = dx;
    }
    /// The pointer entered the pill: fully shown eyes squash and settle, hidden ones pop in; mid-fade or an
    /// adhan nothing starts. True when it started (a repaint is needed).
    pub fn poke(&mut self, now: Instant) -> bool {
        if self.mood == Mood::Adhan {
            return false;
        }
        match self.run {
            None => self.start(&HELLO, now),
            Some(run) if run.steps[run.at].0 >= 1.0 => self.start(&BUMP, now),
            _ => return false,
        }
        true
    }
    /// The pointer moved on the notch: the doze deadline restarts, and eyes that dozed wake with a wave when the
    /// pill is `visible` (closed). True when that wake started (a repaint is needed).
    pub fn touch(&mut self, now: Instant, visible: bool) -> bool {
        self.doze_due = Some(now + DOZE_AFTER);
        std::mem::take(&mut self.dozed) && visible && self.wake(now)
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
            if self.doze_due.is_some_and(|due| now >= due) {
                (self.doze_due, self.dozed) = (None, true);
                self.start(&DOZE, now);
            } else if now >= self.next_show {
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
    /// The next step's time while a show runs, else the sooner of the next show and the doze: the only wait the
    /// eyes ask for.
    pub fn due(&self) -> Instant {
        self.run.map_or_else(
            || {
                self.doze_due
                    .map_or(self.next_show, |due| due.min(self.next_show))
            },
            |run| run.next,
        )
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
        assert_eq!((rest.w, rest.h), (9.5, 18.0));
        let squash = eye(Mood::Idle, Pose::Squash);
        assert!(squash.w > rest.w && squash.h < rest.h / 5.0);
        let stretch = eye(Mood::Idle, Pose::Stretch);
        assert!(stretch.w < rest.w && stretch.h > rest.h);
        assert!(eye(Mood::Surprised, Pose::Rest).h > rest.h);
        assert!(eye(Mood::Happy, Pose::Rest).bottom > 0.0);
        assert!(eye(Mood::Sleepy, Pose::Rest).top > 0.0);
    }

    #[test]
    fn hover_squashes_only_shown_eyes_and_lean_shifts_the_look() {
        let t0 = Instant::now();
        let mut eyes = Eyes::new(t0, 11);
        assert!(eyes.poke(t0), "hidden eyes pop in");
        assert_eq!(eyes.frame().unwrap().pose, Pose::Bump);
        eyes.run = None;
        eyes.start(&WAKE, t0);
        assert!(!eyes.poke(t0), "mid fade-in does not react");
        eyes.tick(eyes.due());
        assert!(eyes.poke(t0));
        assert_eq!(eyes.frame().unwrap().pose, Pose::Bump);
        eyes.set_lean(7.0);
        assert_eq!(eyes.frame().unwrap().look, 7.0);
        let bump = eye(Mood::Idle, Pose::Bump);
        assert!(bump.w > 9.5 * 1.2 && bump.h < 18.0 * 0.9 && bump.dy > 0.0);
        let doze = eye(Mood::Idle, Pose::Doze);
        assert!(doze.h <= 9.0 && doze.top > 0.0 && doze.dy > 0.0);
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
            for pose in [
                Pose::Rest,
                Pose::Squash,
                Pose::Stretch,
                Pose::Bump,
                Pose::Doze,
                Pose::Wave,
            ] {
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
        eyes.doze_due = None;
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
            eyes.doze_due = None;
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
    fn a_quiet_pointer_dozes_the_eyes_once_and_the_next_move_wakes_them() {
        let t0 = Instant::now();
        let mut eyes = Eyes::new(t0, 13);
        let doze_at = t0 + DOZE_AFTER;
        // The deadline is the only wait; a move before it pushes it back.
        assert_eq!(eyes.due(), doze_at);
        assert!(!eyes.touch(t0 + Duration::from_secs(5), true));
        let later = t0 + Duration::from_secs(5) + DOZE_AFTER;
        assert_eq!(eyes.due(), later);
        eyes.next_show = later + Duration::from_secs(300);
        assert!(eyes.tick(later) && eyes.dozed && eyes.doze_due.is_none());
        let (frames, end) = play(&mut eyes, later);
        assert!(frames.iter().any(|f| f.pose == Pose::Doze));
        // Dozing is not repeated while the pointer stays away.
        assert!(eyes.due() > end + Duration::from_secs(60));
        assert!(!eyes.tick(end + Duration::from_secs(60)));
        // A move while the pill is not visible only re-arms; on the pill it waves.
        let mut hidden = Eyes::new(t0, 17);
        hidden.dozed = true;
        assert!(!hidden.touch(t0, false) && !hidden.dozed);
        assert!(eyes.touch(end, true));
        assert!(eyes.doze_due.is_some() && eyes.frame().unwrap().pose == Pose::Squash);
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

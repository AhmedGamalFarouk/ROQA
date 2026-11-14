use std::{
    sync::atomic::{AtomicBool, Ordering::Relaxed},
    time::{Duration, Instant},
};

/// The Settings choices that shape every animation and the way the notch opens (set by the host, one
/// writer, read on the UI thread).
static FAST: AtomicBool = AtomicBool::new(false);
static CLICK: AtomicBool = AtomicBool::new(false);
/// Fast animations take this share of the normal time.
const FAST_SCALE: f32 = 0.6;
pub fn configure(fast: bool, click: bool) {
    FAST.store(fast, Relaxed);
    CLICK.store(click, Relaxed);
}
/// True when the notch opens on a click instead of after hovering.
pub fn click_to_open() -> bool {
    CLICK.load(Relaxed)
}
/// `duration` and `curve` at the chosen speed: a spring's frequency rises as its time shrinks, so the shape
/// is the same, only quicker.
#[inline(never)]
fn paced(fast: bool, duration: Duration, curve: Curve) -> (Duration, Curve) {
    if !fast {
        return (duration, curve);
    }
    let curve = match curve {
        Curve::Spring { damping, frequency } => Curve::Spring {
            damping,
            frequency: frequency / FAST_SCALE,
        },
        // Reduced motion: no overshoot.
        Curve::Bezier(x1, y1, x2, y2) => Curve::Bezier(x1, y1.min(1.0), x2, y2.min(1.0)),
    };
    (duration.mul_f32(FAST_SCALE), curve)
}
/// Reduced motion (the Settings "fast" choice): no looping or overshooting extras.
pub fn calm() -> bool {
    FAST.load(Relaxed)
}
/// How often the collapsed pill's timer ring is repainted while it breathes; the ring is a function of the
/// wall clock (`breath`), so nothing runs between repaints and none are asked for when it is hidden.
pub const BREATH_STEP: Duration = Duration::from_millis(250);
/// The timer ring's opacity: 0.35 to 0.75 over 3.2 s, a steady 0.6 under reduced motion.
pub fn breath() -> f32 {
    if calm() {
        return 0.6;
    }
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() % 3200);
    0.55 + 0.2 * (since as f32 / 3200.0 * std::f32::consts::TAU).sin()
}
/// A deadline length at the chosen speed (the hover intent).
pub fn time(duration: Duration) -> Duration {
    paced(FAST.load(Relaxed), duration, SETTLE).0
}

/// Easing from 0 to 1. Values may leave 0..1 to overshoot; a tween always lands exactly on its target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    /// CSS-style cubic bézier through (0,0), (x1,y1), (x2,y2), (1,1), over the tween's duration.
    Bezier(f32, f32, f32, f32),
    /// Damped spring evaluated in seconds; the duration only bounds its invisible tail.
    Spring { damping: f32, frequency: f32 },
}

/// Notch open, tuned to the live Nusa capture: ~7 % overshoot peaking near 230 ms (docs/design).
pub const OPEN: Curve = Curve::Spring {
    damping: 0.66,
    frequency: 19.0,
};
/// Notch close: a spring that dips ~7 % below the pill before settling (U3: calmer than the observed 9 %).
pub const CLOSE: Curve = Curve::Spring {
    damping: 0.64,
    frequency: 17.0,
};
/// settle: fades, hover out, value changes (brand curve 1, 260 ms).
pub const SETTLE: Curve = Curve::Bezier(0.2, 0.9, 0.3, 1.0);
/// pop: hover grow, squash recover, greet (brand curve 2, 360 ms): about 6 % overshoot.
pub const POP: Curve = Curve::Bezier(0.34, 1.45, 0.5, 1.0);
/// pop's time.
pub const POP_TIME: Duration = Duration::from_millis(360);
/// slide: tab and page switches, notices (brand curve 3, `PAGE_TIME`).
pub const SLIDE: Curve = Curve::Bezier(0.3, 0.0, 0.15, 1.0);
/// A tab switch (slide and fade) and a notice's entry.
pub const PAGE_TIME: Duration = Duration::from_millis(240);
pub const OPEN_TIME: Duration = Duration::from_millis(500);
pub const CLOSE_TIME: Duration = Duration::from_millis(560);
/// Hover intent: the pill swells slightly at once, then opens only if the pointer stays this long.
pub const INTENT_TIME: Duration = Duration::from_millis(320);
pub const HOVER_IN_TIME: Duration = Duration::from_millis(260);
pub const HOVER_OUT_TIME: Duration = Duration::from_millis(220);

/// The slide direction (+1 towards a later tab, -1 an earlier one) of a switch to `page` from `last` while
/// the panel is `open`; none when the page is unchanged or the panel is not open, so an idle notch starts no
/// frames.
pub fn page_switch(last: usize, page: usize, open: bool) -> Option<f32> {
    (open && page != last).then(|| if page > last { 1.0 } else { -1.0 })
}

/// Smoothstep of `t` clamped to 0..1: soft start and end for progress-derived fades and reaches.
pub fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Fade of the panel's secondary content (the date, the artist and the media controls) at an open
/// progress: it starts once the body is about a third open and completes before it lands.
pub fn reveal(progress: f32) -> f32 {
    smooth((progress - 0.35) / 0.45)
}

impl Curve {
    /// `t` is the elapsed fraction of `duration` (both known by the caller).
    fn ease(self, t: f32, duration: Duration) -> f32 {
        match self {
            Curve::Bezier(x1, y1, x2, y2) => {
                let bezier = |a: f32, b: f32, s: f32| {
                    let r = 1.0 - s;
                    3.0 * r * r * s * a + 3.0 * r * s * s * b + s * s * s
                };
                let t = t.clamp(0.0, 1.0);
                // x(s) is monotonic for x1, x2 in 0..1, so bisection converges; 24 halvings exceed f32 precision.
                let (mut low, mut high) = (0.0f32, 1.0f32);
                for _ in 0..24 {
                    let middle = (low + high) / 2.0;
                    if bezier(x1, x2, middle) < t {
                        low = middle;
                    } else {
                        high = middle;
                    }
                }
                bezier(y1, y2, (low + high) / 2.0)
            }
            Curve::Spring { damping, frequency } => {
                // Underdamped step response x(s) = 1 - e^(-ζωs) (cos ω_d s + ζω/ω_d sin ω_d s).
                let seconds = t * duration.as_secs_f32();
                let decay = damping * frequency;
                let damped = frequency * (1.0 - damping * damping).sqrt();
                1.0 - (-decay * seconds).exp()
                    * ((damped * seconds).cos() + decay / damped * (damped * seconds).sin())
            }
        }
    }
}

/// One animated scalar. Re-targeting starts from the current value, so interruptions never jump.
#[derive(Clone, Copy, Debug)]
pub struct Tween {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
    curve: Curve,
}

impl Tween {
    pub fn rest(value: f32) -> Self {
        Self {
            from: value,
            to: value,
            start: Instant::now(),
            duration: Duration::ZERO,
            curve: SETTLE,
        }
    }
    /// Always animates, like Nusa: by user decision (2026-11-09) the Windows "Animation effects"
    /// setting is not consulted, so there is no reduced-motion path.
    #[inline(never)]
    pub fn retarget(&mut self, to: f32, duration: Duration, curve: Curve) {
        if to == self.to {
            return;
        }
        let now = Instant::now();
        let (duration, curve) = paced(FAST.load(Relaxed), duration, curve);
        *self = Self {
            from: self.value(now),
            to,
            start: now,
            duration,
            curve,
        };
    }
    /// Starts the run's clock at `now`: the first painted frame of a run that began earlier (the retarget came
    /// from a mouse message, the frame is a few ms later) then shows the rest value, not a step into the curve.
    /// A tween that has already settled is left alone.
    pub fn restart(&mut self, now: Instant) {
        if self.running(now) {
            self.start = now;
        }
    }
    pub fn value(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.start);
        if elapsed >= self.duration {
            return self.to;
        }
        let t = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        self.from + (self.to - self.from) * self.curve.ease(t, self.duration)
    }
    /// Where the tween lands; the notch publishes it to UI Automation before the motion ends.
    pub fn target(&self) -> f32 {
        self.to
    }
    pub fn running(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) < self.duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_shortens_time_and_keeps_the_spring_shape() {
        let (d, c) = paced(true, OPEN_TIME, OPEN);
        assert_eq!(d, OPEN_TIME.mul_f32(0.6));
        // The same fraction of the run gives the same value: frequency x seconds is unchanged.
        let normal = OPEN.ease(0.5, OPEN_TIME);
        assert!((c.ease(0.5, d) - normal).abs() < 1e-4);
        assert_eq!(paced(false, OPEN_TIME, OPEN), (OPEN_TIME, OPEN));
        assert_eq!(paced(true, HOVER_IN_TIME, SETTLE).1, SETTLE);
    }

    fn samples(curve: Curve, duration: Duration) -> Vec<f32> {
        (0..=200)
            .map(|i| curve.ease(i as f32 / 200.0, duration))
            .collect()
    }

    #[test]
    fn curves_match_the_observed_motion_and_end_without_a_visible_jump() {
        for (curve, duration) in [
            (OPEN, OPEN_TIME),
            (CLOSE, CLOSE_TIME),
            (SETTLE, HOVER_IN_TIME),
        ] {
            let values = samples(curve, duration);
            assert!(values[0].abs() < 1e-4);
            // Tween snaps to 1 at the end: the remaining error must be under 1 % (< 2 DIP of notch travel).
            assert!(
                (values[200] - 1.0).abs() < 0.01,
                "{curve:?} ends at {}",
                values[200]
            );
        }
        let peak = |curve, duration| samples(curve, duration).into_iter().fold(0.0, f32::max);
        let open = peak(OPEN, OPEN_TIME);
        assert!(open > 1.05 && open < 1.09, "open overshoot {open}");
        let close = peak(CLOSE, CLOSE_TIME);
        assert!(close > 1.05 && close < 1.10, "close undershoot {close}");
        let pop = peak(POP, POP_TIME);
        assert!(pop > 1.03 && pop < 1.10, "pop overshoot {pop}");
        assert!((samples(POP, POP_TIME)[200] - 1.0).abs() < 0.01);
        // Reduced motion keeps the shape but never overshoots.
        let (_, calm) = paced(true, POP_TIME, POP);
        assert!(samples(calm, POP_TIME).into_iter().all(|v| v <= 1.0 + 1e-4));
        assert!(samples(SLIDE, PAGE_TIME)[200] > 0.99);
        let ease = samples(SETTLE, HOVER_IN_TIME);
        assert!(ease.windows(2).all(|pair| pair[1] >= pair[0] - 1e-5) && ease[100] > 0.8);
    }

    #[test]
    fn the_ring_breathes_between_a_third_and_three_quarters_opacity() {
        let range = (0..3200)
            .step_by(50)
            .map(|ms| 0.55 + 0.2 * (ms as f32 / 3200.0 * std::f32::consts::TAU).sin());
        assert!(
            range
                .clone()
                .all(|v| (0.35 - 1e-4..=0.75 + 1e-4).contains(&v))
        );
        assert!(range.fold(0.0f32, f32::max) > 0.74);
        assert!((0.35..=0.75).contains(&breath()));
    }

    #[test]
    fn a_restarted_tween_shows_its_rest_value_on_the_first_frame() {
        let started = Instant::now();
        let mut tween = Tween::rest(0.0);
        tween.retarget(1.0, POP_TIME, POP);
        // The first frame lands 40 ms after the retarget: without the restart it is already into the curve.
        let late = started + Duration::from_millis(40);
        assert!(tween.value(late) > 0.0);
        tween.restart(late);
        assert!(tween.value(late).abs() < 1e-4);
        assert!(tween.value(late + Duration::from_millis(100)) > 0.1);
        assert_eq!(tween.value(late + POP_TIME), 1.0);
    }

    #[test]
    fn an_idle_notch_starts_no_frames() {
        // Same page, or a closed panel: no switch, so nothing sets a surface live.
        assert_eq!(page_switch(2, 2, true), None);
        assert_eq!(page_switch(0, 3, false), None);
        assert_eq!(
            (page_switch(1, 3, true), page_switch(3, 1, true)),
            (Some(1.0), Some(-1.0))
        );
        // A settled tween is not running and a switch's tween stops at its end.
        let mut tween = Tween::rest(1.0);
        assert!(!tween.running(Instant::now()));
        tween.retarget(0.0, Duration::ZERO, SETTLE);
        assert!(!tween.running(Instant::now()));
    }

    #[test]
    fn tween_interrupts_from_current_value_and_lands_on_target() {
        let mut tween = Tween::rest(0.0);
        tween.retarget(1.0, Duration::from_secs(10), SETTLE);
        assert!(tween.running(Instant::now()));
        let midway = tween.value(Instant::now());
        tween.retarget(0.0, Duration::from_secs(10), SETTLE);
        assert!((tween.value(tween.start) - midway).abs() < 0.05);
        tween.retarget(1.0, Duration::ZERO, SETTLE);
        assert!(!tween.running(Instant::now()));
        assert_eq!(tween.value(Instant::now()), 1.0);
    }
}

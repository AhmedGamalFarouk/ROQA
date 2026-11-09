use std::time::{Duration, Instant};

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
    damping: 0.65,
    frequency: 18.0,
};
/// Notch close: quicker spring that dips ~9 % below the pill before settling, as observed.
pub const CLOSE: Curve = Curve::Spring {
    damping: 0.61,
    frequency: 16.0,
};
/// Small state feedback (dock hover, hover-intent swell).
pub const EASE_OUT: Curve = Curve::Bezier(0.2, 0.9, 0.3, 1.0);
pub const OPEN_TIME: Duration = Duration::from_millis(500);
pub const CLOSE_TIME: Duration = Duration::from_millis(560);
/// Hover intent: the pill swells slightly at once, then opens only if the pointer stays this long.
pub const INTENT_TIME: Duration = Duration::from_millis(320);
pub const HOVER_IN_TIME: Duration = Duration::from_millis(160);
pub const HOVER_OUT_TIME: Duration = Duration::from_millis(180);

/// Smoothstep of `t` clamped to 0..1: soft start and end for progress-derived fades and reaches.
pub fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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
            curve: EASE_OUT,
        }
    }
    /// Always animates, like Nusa: by user decision (2026-11-09) the Windows "Animation effects"
    /// setting is not consulted, so there is no reduced-motion path.
    pub fn retarget(&mut self, to: f32, duration: Duration, curve: Curve) {
        if to == self.to {
            return;
        }
        let now = Instant::now();
        *self = Self {
            from: self.value(now),
            to,
            start: now,
            duration,
            curve,
        };
    }
    pub fn value(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.start);
        if elapsed >= self.duration {
            return self.to;
        }
        let t = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        self.from + (self.to - self.from) * self.curve.ease(t, self.duration)
    }
    pub fn running(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) < self.duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            (EASE_OUT, HOVER_IN_TIME),
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
        assert!(close > 1.07 && close < 1.11, "close undershoot {close}");
        let ease = samples(EASE_OUT, HOVER_IN_TIME);
        assert!(ease.windows(2).all(|pair| pair[1] >= pair[0] - 1e-5) && ease[100] > 0.8);
    }

    #[test]
    fn tween_interrupts_from_current_value_and_lands_on_target() {
        let mut tween = Tween::rest(0.0);
        tween.retarget(1.0, Duration::from_secs(10), EASE_OUT);
        assert!(tween.running(Instant::now()));
        let midway = tween.value(Instant::now());
        tween.retarget(0.0, Duration::from_secs(10), EASE_OUT);
        assert!((tween.value(tween.start) - midway).abs() < 0.05);
        tween.retarget(1.0, Duration::ZERO, EASE_OUT);
        assert!(!tween.running(Instant::now()));
        assert_eq!(tween.value(Instant::now()), 1.0);
    }
}

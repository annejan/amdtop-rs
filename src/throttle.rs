//! Throttle residency: the share of each sample interval that a power or
//! thermal limiter held the chip back.
//!
//! APU `gpu_metrics` (v3.0) carries one monotonic accumulator per limiter.
//! Each counts milliseconds spent throttled: measured on a Krackan Point
//! laptop under an all-core load, a pegged limiter adds ~1000 per second.

use std::time::Duration;

/// Limiter names in `gpu_metrics` field order.
pub const NAMES: [&str; 7] = [
    "prochot", "spl", "fppt", "sppt", "thm_core", "thm_gfx", "thm_soc",
];

pub type Counters = [Option<u32>; 7];

#[derive(Debug, Default)]
pub struct ThrottleTracker {
    prev: Option<Counters>,
    pub percent: [Option<f64>; 7],
}

impl ThrottleTracker {
    pub fn update(&mut self, current: Counters, elapsed: Duration) {
        self.percent = match self.prev {
            Some(prev) => std::array::from_fn(|i| residency_percent(prev[i], current[i], elapsed)),
            None => [None; 7],
        };
        self.prev = Some(current);
    }
}

/// Percent of `elapsed` a limiter was engaged. `None` when either sample is
/// missing, the counter went backwards (reset), or no time passed; small
/// timing jitter above 100% is clamped.
pub fn residency_percent(prev: Option<u32>, cur: Option<u32>, elapsed: Duration) -> Option<f64> {
    let delta_ms = cur?.checked_sub(prev?)?;
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    (elapsed_ms > 0.0).then(|| (f64::from(delta_ms) / elapsed_ms * 100.0).min(100.0))
}

/// One-line summary: engaged limiters (>= 0.5%) in field order, `no
/// throttling` when every supported limiter is idle, `None` when the device
/// reports no residency counters at all.
pub fn summary(percent: &[Option<f64>; 7]) -> Option<String> {
    if percent.iter().all(Option::is_none) {
        return None;
    }
    let engaged: Vec<String> = NAMES
        .iter()
        .zip(percent)
        .filter_map(|(name, p)| p.filter(|&p| p >= 0.5).map(|p| format!("{name} {p:.0}%")))
        .collect();
    Some(if engaged.is_empty() {
        "no throttling".to_string()
    } else {
        format!("throttle {}", engaged.join("  "))
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{ThrottleTracker, residency_percent, summary};

    const SEC: Duration = Duration::from_secs(1);

    #[test]
    fn residency_is_throttled_milliseconds_over_elapsed_time() {
        assert_eq!(residency_percent(Some(0), Some(1000), SEC), Some(100.0));
        assert_eq!(residency_percent(Some(100), Some(350), SEC), Some(25.0));
        assert_eq!(
            residency_percent(Some(0), Some(250), Duration::from_millis(500)),
            Some(50.0)
        );
    }

    #[test]
    fn residency_clamps_jitter_and_rejects_unusable_samples() {
        // Firmware and wall-clock sampling drift slightly apart.
        assert_eq!(residency_percent(Some(0), Some(1003), SEC), Some(100.0));
        // Counter reset (e.g. after resume), missing values, zero interval.
        assert_eq!(residency_percent(Some(500), Some(10), SEC), None);
        assert_eq!(residency_percent(None, Some(10), SEC), None);
        assert_eq!(residency_percent(Some(0), None, SEC), None);
        assert_eq!(residency_percent(Some(0), Some(10), Duration::ZERO), None);
    }

    #[test]
    fn tracker_needs_two_samples() {
        let mut t = ThrottleTracker::default();
        t.update([Some(0); 7], SEC);
        assert_eq!(t.percent, [None; 7]);

        let mut cur = [Some(0); 7];
        cur[2] = Some(230); // fppt
        cur[1] = Some(1001); // spl
        t.update(cur, SEC);
        assert_eq!(t.percent[2], Some(23.0));
        assert_eq!(t.percent[1], Some(100.0));
        assert_eq!(t.percent[0], Some(0.0));
    }

    #[test]
    fn summary_lists_only_engaged_limiters_in_field_order() {
        let mut p = [Some(0.0); 7];
        p[2] = Some(23.4); // fppt
        p[1] = Some(100.0); // spl
        p[5] = Some(0.2); // thm_gfx below display threshold
        assert_eq!(summary(&p).as_deref(), Some("throttle spl 100%  fppt 23%"));
    }

    #[test]
    fn summary_distinguishes_idle_from_unsupported() {
        assert_eq!(summary(&[Some(0.0); 7]).as_deref(), Some("no throttling"));
        assert_eq!(summary(&[None; 7]), None);
    }
}

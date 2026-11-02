//! Per-device trend histories (sclk, temperature, power) for the GPU band.
//!
//! [`History`] stores 0..=100, so each value is scaled first: against a fixed
//! maximum when the device reports one (max sclk, a power cap), otherwise
//! against the highest value seen this session.

use crate::history::History;

pub struct Trend {
    pub hist: History,
    pub current: Option<f64>,
    pub peak: Option<f64>,
}

impl Trend {
    pub fn new(capacity: usize) -> Self {
        Self {
            hist: History::new(capacity),
            current: None,
            peak: None,
        }
    }

    /// Record `value`; `scale_max` fixes the 100% point when known.
    pub fn record(&mut self, value: Option<f64>, scale_max: Option<f64>) {
        self.current = value;
        let Some(value) = value.filter(|v| v.is_finite() && *v >= 0.0) else {
            return;
        };
        let peak = self.peak.map_or(value, |p| p.max(value));
        self.peak = Some(peak);
        let scale = scale_max.filter(|m| *m > 0.0).unwrap_or(peak);
        let percent = if scale > 0.0 {
            (value / scale * 100.0).round()
        } else {
            0.0
        };
        self.hist.push(percent as u64);
    }
}

/// Trends cost three rows per device, so show them for APUs (whose GPU
/// shares one power budget with the CPU and NPU) and for single-GPU systems.
pub fn show_trends(is_apu: bool, gpu_count: usize) -> bool {
    is_apu || gpu_count == 1
}

/// Rows of one GPU band in the GPU section.
pub fn band_height(trends: bool) -> u16 {
    // identity/gauges/history (6) + trend graphs (2) + labels (1)
    if trends { 9 } else { 6 }
}

#[cfg(test)]
mod tests {
    use super::{Trend, band_height, show_trends};

    #[test]
    fn fixed_scale_maps_values_to_percent() {
        let mut t = Trend::new(8);
        t.record(Some(1450.0), Some(2900.0));
        assert_eq!(t.hist.latest(), 50);
        assert_eq!(t.current, Some(1450.0));
        assert_eq!(t.peak, Some(1450.0));
    }

    #[test]
    fn without_a_fixed_scale_the_session_peak_is_100_percent() {
        let mut t = Trend::new(8);
        t.record(Some(19.0), None);
        assert_eq!(t.hist.latest(), 100);
        t.record(Some(9.5), None);
        assert_eq!(t.hist.latest(), 50);
        assert_eq!(t.peak, Some(19.0));
    }

    #[test]
    fn missing_or_unusable_samples_leave_the_history_alone() {
        let mut t = Trend::new(8);
        t.record(Some(40.0), Some(100.0));
        t.record(None, Some(100.0));
        assert_eq!(t.hist.latest(), 40);
        assert_eq!(t.current, None);
        // A zero scale (e.g. unknown max clock reported as 0) is ignored.
        t.record(Some(10.0), Some(0.0));
        assert_eq!(t.hist.latest(), 25); // falls back to the session peak (40)
    }

    #[test]
    fn trends_show_for_apus_and_single_gpu_systems() {
        assert!(show_trends(true, 3));
        assert!(show_trends(false, 1));
        assert!(!show_trends(false, 2));
        assert_eq!(band_height(false), 6);
        assert_eq!(band_height(true), 9);
    }
}

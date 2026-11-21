use std::time::SystemTime;

use wayward_core::Level;

use crate::{Meter, Usage};

pub const WARN_BELOW: f32 = 0.20;
pub const URGENT_BELOW: f32 = 0.0;

/// Percent used, clamped to `0..=1`. A non-positive limit yields `1` when
/// `used > 0`, otherwise `0`.
pub fn percent_used(used: f64, limit: f64) -> f32 {
    if limit <= 0.0 {
        return if used > 0.0 { 1.0 } else { 0.0 };
    }
    let ratio = used / limit;
    if ratio.is_nan() {
        return 0.0;
    }
    ratio.clamp(0.0, 1.0) as f32
}

/// Level for a single meter from its remaining fraction.
pub fn meter_level(m: &Meter, warn_below: f32, urgent_below: f32) -> Level {
    let remaining = 1.0 - percent_used(m.used, m.limit);

    if m.limit > 0.0 && m.used >= m.limit {
        return Level::Urgent;
    }
    if remaining <= urgent_below {
        return Level::Urgent;
    }
    if remaining <= warn_below {
        return Level::Warn;
    }
    Level::Normal
}

/// Worst of the given levels; empty input yields [`Level::Normal`].
///
/// `wayward_core::Level` does not implement `Ord`, so the `Normal < Warn < Urgent`
/// ordering is encoded locally rather than relying on a trait the core crate lacks.
pub fn worst_level(levels: impl IntoIterator<Item = Level>) -> Level {
    fn rank(level: Level) -> u8 {
        match level {
            Level::Normal => 0,
            Level::Warn => 1,
            Level::Urgent => 2,
        }
    }

    levels.into_iter().fold(Level::Normal, |worst, level| {
        if rank(level) > rank(worst) {
            level
        } else {
            worst
        }
    })
}

/// Worst level across all meters of a usage snapshot.
pub fn aggregate_level(usage: &Usage, warn_below: f32, urgent_below: f32) -> Level {
    worst_level(
        usage
            .meters
            .iter()
            .map(|m| meter_level(m, warn_below, urgent_below)),
    )
}

/// Human-readable time until reset: `1h51m`, `2d18h`, etc. Empty when absent or past.
pub fn format_remaining(reset_at: Option<SystemTime>, now: SystemTime) -> String {
    let Some(reset_at) = reset_at else {
        return String::new();
    };
    let Ok(until_reset) = reset_at.duration_since(now) else {
        return String::new();
    };

    let total_secs = until_reset.as_secs();
    let days = total_secs / 86_400;
    let hours = (total_secs % 86_400) / 3_600;
    let minutes = (total_secs % 3_600) / 60;
    let seconds = total_secs % 60;

    if days > 0 {
        format!("{days}d{hours}h")
    } else if hours > 0 {
        format!("{hours}h{minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m{seconds}s")
    } else if seconds > 0 {
        format!("{seconds}s")
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn meter(used: f64, limit: f64) -> Meter {
        Meter {
            label: "test".to_owned(),
            used,
            limit,
            reset_at: None,
        }
    }

    fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
    }

    #[test]
    fn percent_used_half() {
        assert_eq!(percent_used(50.0, 100.0), 0.5);
    }

    #[test]
    fn percent_used_clamps_over_limit() {
        assert_eq!(percent_used(150.0, 100.0), 1.0);
    }

    #[test]
    fn percent_used_zero_limit_with_used() {
        assert_eq!(percent_used(5.0, 0.0), 1.0);
    }

    #[test]
    fn percent_used_zero_limit_without_used() {
        assert_eq!(percent_used(0.0, 0.0), 0.0);
    }

    #[test]
    fn percent_used_negative_used() {
        assert_eq!(percent_used(-10.0, 100.0), 0.0);
    }

    #[test]
    fn meter_level_warn_at_15_percent_left() {
        assert_eq!(
            meter_level(&meter(85.0, 100.0), WARN_BELOW, URGENT_BELOW),
            Level::Warn
        );
    }

    #[test]
    fn meter_level_urgent_at_limit() {
        assert_eq!(
            meter_level(&meter(100.0, 100.0), WARN_BELOW, URGENT_BELOW),
            Level::Urgent
        );
    }

    #[test]
    fn meter_level_urgent_when_remaining_below_urgent() {
        assert_eq!(
            meter_level(&meter(99.0, 100.0), WARN_BELOW, 0.02),
            Level::Urgent
        );
    }

    #[test]
    fn meter_level_normal_at_62_percent() {
        assert_eq!(
            meter_level(&meter(62.0, 100.0), WARN_BELOW, URGENT_BELOW),
            Level::Normal
        );
    }

    #[test]
    fn worst_level_empty_is_normal() {
        assert_eq!(worst_level(Vec::<Level>::new()), Level::Normal);
    }

    #[test]
    fn worst_level_picks_urgent() {
        assert_eq!(
            worst_level([Level::Normal, Level::Urgent, Level::Warn]),
            Level::Urgent
        );
    }

    #[test]
    fn aggregate_level_worst_across_meters() {
        let usage = Usage {
            plan: Some("MAX".to_owned()),
            meters: vec![meter(62.0, 100.0), meter(85.0, 100.0), meter(100.0, 100.0)],
            spend: None,
        };
        assert_eq!(
            aggregate_level(&usage, WARN_BELOW, URGENT_BELOW),
            Level::Urgent
        );
    }

    #[test]
    fn aggregate_level_empty_is_normal() {
        let usage = Usage {
            plan: None,
            meters: Vec::new(),
            spend: None,
        };
        assert_eq!(
            aggregate_level(&usage, WARN_BELOW, URGENT_BELOW),
            Level::Normal
        );
    }

    #[test]
    fn format_remaining_hours_minutes() {
        assert_eq!(
            format_remaining(Some(now() + Duration::from_secs(6_660)), now()),
            "1h51m"
        );
    }

    #[test]
    fn format_remaining_days_hours() {
        assert_eq!(
            format_remaining(Some(now() + Duration::from_secs(237_600)), now()),
            "2d18h"
        );
    }

    #[test]
    fn format_remaining_past_is_empty() {
        assert_eq!(
            format_remaining(Some(now() - Duration::from_secs(60)), now()),
            ""
        );
    }

    #[test]
    fn format_remaining_none_is_empty() {
        assert_eq!(format_remaining(None, now()), "");
    }

    #[test]
    fn format_remaining_seconds() {
        assert_eq!(
            format_remaining(Some(now() + Duration::from_secs(45)), now()),
            "45s"
        );
    }

    #[test]
    fn format_remaining_minutes_seconds() {
        assert_eq!(
            format_remaining(Some(now() + Duration::from_secs(125)), now()),
            "2m5s"
        );
    }
}

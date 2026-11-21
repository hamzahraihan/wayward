use std::time::{Duration, SystemTime};

use crate::Meter;

pub fn next_reset(anchor: Option<u64>, reset_after_secs: u64, now: SystemTime) -> SystemTime {
    let window = reset_after_secs.max(1);
    let rolling = || {
        now.checked_add(Duration::from_secs(window))
            .unwrap_or(SystemTime::UNIX_EPOCH)
    };
    let Some(anchor_secs) = anchor.filter(|anchor| *anchor > 0) else {
        return rolling();
    };
    let Ok(now_duration) = now.duration_since(SystemTime::UNIX_EPOCH) else {
        return rolling();
    };
    let now_secs = now_duration.as_secs();
    let boundary = if now_secs < anchor_secs {
        anchor_secs
    } else {
        let periods = (now_secs - anchor_secs) / window + 1;
        let target = match periods
            .checked_mul(window)
            .and_then(|offset| anchor_secs.checked_add(offset))
        {
            Some(value) => value,
            None => now_secs.saturating_add(window),
        };
        if target <= now_secs {
            target.saturating_add(window)
        } else {
            target
        }
    };
    SystemTime::UNIX_EPOCH
        .checked_add(Duration::from_secs(boundary))
        .unwrap_or_else(rolling)
}

pub fn window_meter(anchor: Option<u64>, reset_after_secs: u64, limit: f64, used: f64) -> Meter {
    Meter {
        label: "usage window".into(),
        used,
        limit,
        reset_at: Some(next_reset(anchor, reset_after_secs, SystemTime::now())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64, nanos: u32) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::new(secs, nanos)
    }

    #[test]
    fn no_anchor_gives_rolling_window() {
        let now = at(1_000, 0);
        assert_eq!(next_reset(None, 100, now), at(1_100, 0));
        assert_eq!(next_reset(Some(0), 100, now), at(1_100, 0));
    }

    #[test]
    fn past_anchor_gives_next_strict_boundary() {
        assert_eq!(next_reset(Some(100), 100, at(1_050, 0)), at(1_100, 0));
        assert_eq!(
            next_reset(Some(100), 100, at(1_000, 500_000_000)),
            at(1_100, 0)
        );
        assert_eq!(
            next_reset(Some(100), 100, at(1_099, 999_999_999)),
            at(1_100, 0)
        );
    }

    #[test]
    fn future_anchor_gives_anchor_itself() {
        assert_eq!(next_reset(Some(2_000), 100, at(1_050, 0)), at(2_000, 0));
    }

    #[test]
    fn zero_window_is_treated_as_one_second() {
        assert_eq!(next_reset(Some(100), 0, at(1_050, 0)), at(1_051, 0));
    }

    #[test]
    fn window_meter_uses_the_glossary_label_and_anchor_boundary() {
        let before = SystemTime::now();
        let meter = window_meter(Some(100), 100, 500.0, 42.0);
        assert_eq!(meter.label, "usage window");
        assert_eq!(meter.used, 42.0);
        assert_eq!(meter.limit, 500.0);
        assert!(meter.reset_at.expect("meter has reset time") > before);
    }

    #[test]
    fn overflow_falls_back_without_panicking() {
        let now = at(1_000, 0);
        assert_eq!(next_reset(Some(u64::MAX), 100, now), at(1_100, 0));
    }
}

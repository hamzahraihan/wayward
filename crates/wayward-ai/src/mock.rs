use std::time::{Duration, SystemTime};

use async_trait::async_trait;

use crate::{Meter, Usage, UsageProvider};

/// Deterministic usage provider for the popup and level logic.
///
/// Each call to [`MockProvider::next_usage`] advances a fixed three-step
/// cycle: normal, warn, urgent. Reset times are derived from
/// [`SystemTime::now`] with fixed offsets, so no clock, network, key, or
/// randomness is involved.
pub struct MockProvider {
    index: usize,
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockProvider {
    pub fn new() -> Self {
        Self { index: 0 }
    }

    /// Cycle three scenarios deterministically: normal, warn, urgent.
    pub fn next_usage(&mut self) -> Usage {
        let now = SystemTime::now();
        let scenario = self.index % 3;
        self.index = self.index.wrapping_add(1);

        match scenario {
            0 => Usage {
                plan: Some("MAX".to_owned()),
                meters: vec![
                    meter("Claude Code session", 62.0, 100.0, 6_660, now),
                    meter("Claude Code weekly", 36.0, 100.0, 237_600, now),
                    // AGENTS.md §7 shows "CODEX PRO 200"; limit 200 keeps this
                    // meter Normal under WARN_BELOW (88/100 would be Warn).
                    meter("Codex session", 88.0, 200.0, 13_200, now),
                ],
                spend: None,
            },
            1 => Usage {
                plan: Some("MAX".to_owned()),
                meters: vec![meter("Claude Code session", 85.0, 100.0, 6_660, now)],
                spend: None,
            },
            _ => Usage {
                plan: Some("MAX".to_owned()),
                meters: vec![meter("Claude Code session", 100.0, 100.0, 6_660, now)],
                spend: None,
            },
        }
    }
}

/// Build a meter whose reset time is `reset_after_secs` from `now`.
fn meter(label: &str, used: f64, limit: f64, reset_after_secs: u64, now: SystemTime) -> Meter {
    Meter {
        label: label.to_owned(),
        used,
        limit,
        reset_at: Some(now + Duration::from_secs(reset_after_secs)),
    }
}

#[async_trait]
impl UsageProvider for MockProvider {
    fn id(&self) -> &'static str {
        "mock"
    }

    async fn fetch(&mut self) -> anyhow::Result<Usage> {
        Ok(self.next_usage())
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use wayward_core::Level;

    use super::*;
    use crate::level::{aggregate_level, URGENT_BELOW, WARN_BELOW};

    #[test]
    fn scenarios_cycle_normal_warn_urgent() {
        let mut provider = MockProvider::new();

        let normal = provider.next_usage();
        assert_eq!(normal.plan.as_deref(), Some("MAX"));
        assert_eq!(normal.meters.len(), 3);
        assert!(normal.spend.is_none());
        assert_eq!(
            aggregate_level(&normal, WARN_BELOW, URGENT_BELOW),
            Level::Normal
        );

        let warn = provider.next_usage();
        assert_eq!(warn.plan.as_deref(), Some("MAX"));
        assert_eq!(warn.meters.len(), 1);
        assert_eq!(
            aggregate_level(&warn, WARN_BELOW, URGENT_BELOW),
            Level::Warn
        );

        let urgent = provider.next_usage();
        assert_eq!(urgent.plan.as_deref(), Some("MAX"));
        assert_eq!(urgent.meters.len(), 1);
        assert_eq!(
            aggregate_level(&urgent, WARN_BELOW, URGENT_BELOW),
            Level::Urgent
        );
    }

    #[test]
    fn cycle_wraps_to_normal() {
        let mut provider = MockProvider::new();
        for _ in 0..3 {
            let _ = provider.next_usage();
        }

        let wrapped = provider.next_usage();
        assert_eq!(wrapped.meters.len(), 3);
        assert_eq!(
            aggregate_level(&wrapped, WARN_BELOW, URGENT_BELOW),
            Level::Normal
        );
    }

    #[test]
    fn reset_times_are_in_the_future() {
        let before = SystemTime::now();
        let usage = MockProvider::new().next_usage();
        for m in &usage.meters {
            let reset_at = m.reset_at.expect("mock meters have a reset time");
            assert!(reset_at > before);
        }
    }

    #[test]
    fn id_is_mock() {
        assert_eq!(MockProvider::new().id(), "mock");
    }
}

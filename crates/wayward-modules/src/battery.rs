use std::time::Duration;

use wayward_core::{Level, Module, ModuleState};
use wayward_platform::power_status;

/// Pure decision helper: a missing battery or unknown charge level yields the
/// failed state (`--`); otherwise the charge is rendered as `BAT {percent}%`.
pub fn battery_state(present: bool, life_percent: Option<u8>) -> ModuleState {
    if !present {
        tracing::warn!("battery unavailable: no battery present");
        return ModuleState::failed();
    }

    match life_percent {
        None => {
            tracing::warn!("battery unavailable: charge level unknown");
            ModuleState::failed()
        }
        Some(percent) => ModuleState {
            text: format!("BAT {percent}%"),
            icon: String::new(),
            level: Level::Normal,
        },
    }
}

pub struct BatteryModule {
    interval_ms: u64,
}

impl BatteryModule {
    pub fn new(interval_ms: u64) -> Self {
        Self { interval_ms }
    }
}

impl Module for BatteryModule {
    fn id(&self) -> &'static str {
        "battery"
    }

    fn interval(&self) -> Option<Duration> {
        Some(Duration::from_millis(self.interval_ms.clamp(2000, 5000)))
    }

    fn update(&mut self) -> ModuleState {
        match power_status() {
            Ok(status) => battery_state(status.battery_present, status.life_percent),
            Err(error) => {
                tracing::warn!("battery unavailable: {error}");
                ModuleState::failed()
            }
        }
    }
}

// --- tests ---

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_battery_is_failed_state() {
        assert_eq!(battery_state(false, Some(50)), ModuleState::failed());
    }

    #[test]
    fn unknown_percent_is_failed_state() {
        assert_eq!(battery_state(true, None), ModuleState::failed());
    }

    #[test]
    fn half_charge_text() {
        assert_eq!(battery_state(true, Some(50)).text, "BAT 50%");
    }

    #[test]
    fn full_charge_text() {
        assert_eq!(battery_state(true, Some(100)).text, "BAT 100%");
    }

    #[test]
    fn empty_charge_text() {
        assert_eq!(battery_state(true, Some(0)).text, "BAT 0%");
    }

    #[test]
    fn healthy_state_is_normal_level_with_empty_icon() {
        let state = battery_state(true, Some(50));
        assert_eq!(state.level, Level::Normal);
        assert!(state.icon.is_empty());
    }

    #[test]
    fn module_id_is_battery() {
        assert_eq!(BatteryModule::new(2000).id(), "battery");
    }

    #[test]
    fn interval_clamps_low_value_to_2000ms() {
        assert_eq!(
            BatteryModule::new(100).interval(),
            Some(Duration::from_millis(2000))
        );
    }

    #[test]
    fn interval_clamps_high_value_to_5000ms() {
        assert_eq!(
            BatteryModule::new(9000).interval(),
            Some(Duration::from_millis(5000))
        );
    }
}

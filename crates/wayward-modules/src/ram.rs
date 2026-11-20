use std::time::Duration;

use sysinfo::{MemoryRefreshKind, RefreshKind, System};
use wayward_core::{Level, Module, ModuleState};

/// Pure percentage math: RAM percent = used/total*100; None when total==0 or used>total.
pub fn used_percent(used: u64, total: u64) -> Option<f32> {
    if total == 0 || used > total {
        return None;
    }
    Some(used as f32 / total as f32 * 100.0)
}

/// Pure formatting: rounds and clamps to 0..=100, e.g. 62.4 -> "RAM 62%".
pub fn format_ram(percent: f32) -> String {
    format!("RAM {}%", percent.clamp(0.0, 100.0).round())
}

pub struct RamModule {
    system: Option<System>,
    interval_ms: u64,
}

impl RamModule {
    /// Infallible: on error warn + store None.
    pub fn new(interval_ms: u64) -> Self {
        let refresh = RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram());
        let system = match System::new_with_specifics(refresh) {
            Ok(system) => Some(system),
            Err(error) => {
                tracing::warn!(%error, "failed to initialize RAM system");
                None
            }
        };
        Self {
            system,
            interval_ms,
        }
    }
}

impl Module for RamModule {
    fn id(&self) -> &'static str {
        "ram"
    }

    fn interval(&self) -> Option<Duration> {
        Some(Duration::from_millis(self.interval_ms.clamp(2000, 5000)))
    }

    fn update(&mut self) -> ModuleState {
        let Some(system) = self.system.as_mut() else {
            tracing::warn!("RAM system unavailable");
            return ModuleState::failed();
        };

        system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        let used = system.used_memory();
        let total = system.total_memory();

        match used_percent(used, total) {
            Some(percent) => ModuleState {
                text: format_ram(percent),
                icon: String::new(),
                level: Level::Normal,
            },
            None => {
                tracing::warn!(used, total, "invalid RAM reading");
                ModuleState::failed()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use wayward_core::{Module, ModuleState};

    use super::{format_ram, used_percent, RamModule};

    #[test]
    fn used_percent_half() {
        assert_eq!(used_percent(50, 100), Some(50.0));
    }

    #[test]
    fn used_percent_zero() {
        assert_eq!(used_percent(0, 100), Some(0.0));
    }

    #[test]
    fn used_percent_full() {
        assert_eq!(used_percent(100, 100), Some(100.0));
    }

    #[test]
    fn used_percent_zero_total_is_none() {
        assert_eq!(used_percent(5, 0), None);
    }

    #[test]
    fn used_percent_used_exceeds_total_is_none() {
        assert_eq!(used_percent(110, 100), None);
    }

    #[test]
    fn format_ram_rounds_down() {
        assert_eq!(format_ram(62.4), "RAM 62%");
    }

    #[test]
    fn format_ram_zero() {
        assert_eq!(format_ram(0.0), "RAM 0%");
    }

    #[test]
    fn format_ram_clamps_above_hundred() {
        assert_eq!(format_ram(150.0), "RAM 100%");
    }

    #[test]
    fn ram_module_id_is_ram() {
        let module = RamModule::new(3000);
        assert_eq!(module.id(), "ram");
    }

    #[test]
    fn interval_clamps_below_minimum_to_two_seconds() {
        let module = RamModule::new(100);
        assert_eq!(module.interval(), Some(Duration::from_millis(2000)));
    }

    #[test]
    fn interval_clamps_above_maximum_to_five_seconds() {
        let module = RamModule::new(9000);
        assert_eq!(module.interval(), Some(Duration::from_millis(5000)));
    }

    #[test]
    fn update_without_system_returns_failed_state() {
        let mut module = RamModule {
            system: None,
            interval_ms: 3000,
        };
        assert_eq!(module.update(), ModuleState::failed());
    }
}

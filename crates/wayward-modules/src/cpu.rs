use std::time::Duration;

use sysinfo::{CpuRefreshKind, RefreshKind, System, MINIMUM_CPU_UPDATE_INTERVAL};
use wayward_core::{Level, Module, ModuleState};

/// Format a CPU usage percentage as `CPU N%`, clamped to `0..=100` and rounded to an integer.
pub fn format_usage_percent(percent: f32) -> String {
    format!("CPU {:.0}%", percent.clamp(0.0, 100.0).round())
}

pub struct CpuModule {
    system: Option<System>,
    interval_ms: u64,
}

impl CpuModule {
    /// Infallible: logs and stores `None` on `System` construction failure.
    /// Primes the usage differential once with a 200ms sleep, never per tick.
    pub fn new(interval_ms: u64) -> Self {
        let refresh_kind = RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing().with_usage());
        let mut system = match System::new_with_specifics(refresh_kind) {
            Ok(system) => Some(system),
            Err(error) => {
                tracing::warn!(%error, "failed to create sysinfo System for CPU module");
                None
            }
        };
        if let Some(system) = system.as_mut() {
            std::thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL);
            system.refresh_cpu_usage();
        }
        Self {
            system,
            interval_ms,
        }
    }
}

impl Module for CpuModule {
    fn id(&self) -> &'static str {
        "cpu"
    }

    fn interval(&self) -> Option<Duration> {
        Some(Duration::from_millis(self.interval_ms.clamp(2000, 5000)))
    }

    fn update(&mut self) -> ModuleState {
        let Some(system) = self.system.as_mut() else {
            tracing::warn!("CPU module has no System; reporting failed state");
            return ModuleState::failed();
        };
        system.refresh_cpu_usage();
        let percent = system.global_cpu_usage();
        if percent.is_nan() {
            tracing::warn!("CPU usage is NaN; reporting failed state");
            return ModuleState::failed();
        }
        ModuleState {
            text: format_usage_percent(percent),
            icon: String::new(),
            level: Level::Normal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_usage_percent_rounds_to_nearest_integer() {
        assert_eq!(format_usage_percent(42.6), "CPU 43%");
    }

    #[test]
    fn format_usage_percent_zero() {
        assert_eq!(format_usage_percent(0.0), "CPU 0%");
    }

    #[test]
    fn format_usage_percent_full() {
        assert_eq!(format_usage_percent(100.0), "CPU 100%");
    }

    #[test]
    fn format_usage_percent_clamps_above_100() {
        assert_eq!(format_usage_percent(150.0), "CPU 100%");
    }

    #[test]
    fn format_usage_percent_clamps_below_zero() {
        assert_eq!(format_usage_percent(-5.0), "CPU 0%");
    }

    #[test]
    fn id_is_cpu() {
        let module = CpuModule {
            system: None,
            interval_ms: 2000,
        };
        assert_eq!(module.id(), "cpu");
    }

    #[test]
    fn interval_clamps_low_value() {
        let module = CpuModule {
            system: None,
            interval_ms: 100,
        };
        assert_eq!(module.interval(), Some(Duration::from_millis(2000)));
    }

    #[test]
    fn interval_clamps_high_value() {
        let module = CpuModule {
            system: None,
            interval_ms: 9000,
        };
        assert_eq!(module.interval(), Some(Duration::from_millis(5000)));
    }

    #[test]
    fn update_failed_when_system_unavailable() {
        let mut module = CpuModule {
            system: None,
            interval_ms: 2000,
        };
        assert_eq!(module.update(), ModuleState::failed());
    }
}

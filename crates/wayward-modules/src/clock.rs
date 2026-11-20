use std::time::Duration;

use jiff::ZonedDateTime as Zoned;
use wayward_core::{Level, Module, ModuleState};

/// Format a zoned timestamp with a strtime pattern, or `--` when the pattern is invalid.
pub fn format_clock(zoned: &Zoned, pattern: &str) -> String {
    match jiff::fmt::strtime::format(pattern, zoned) {
        Ok(s) => s,
        Err(_) => "--".to_string(),
    }
}

/// Current time in the local zone.
pub fn now_zoned() -> Zoned {
    Zoned::now()
}

pub struct ClockModule {
    format: String,
    interval_ms: u64,
}

impl ClockModule {
    pub fn new(format: String, interval_ms: u64) -> Self {
        Self {
            format,
            interval_ms,
        }
    }
}

impl Module for ClockModule {
    fn id(&self) -> &'static str {
        "clock"
    }

    fn interval(&self) -> Option<Duration> {
        Some(Duration::from_millis(self.interval_ms.max(1000)))
    }

    fn update(&mut self) -> ModuleState {
        let text = format_clock(&now_zoned(), &self.format);
        if text == "--" {
            tracing::warn!(format = %self.format, "failed to format clock");
            ModuleState::failed()
        } else {
            ModuleState {
                text,
                icon: String::new(),
                level: Level::Normal,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use jiff::civil;

    use super::{format_clock, ClockModule, Module, Zoned};

    fn fixed_zoned() -> Zoned {
        civil::date(2024, 7, 15)
            .at(16, 24, 59, 0)
            .in_tz("UTC")
            .expect("fixed tz")
    }

    #[test]
    fn formats_time_pattern() {
        assert_eq!(format_clock(&fixed_zoned(), "%H:%M:%S"), "16:24:59");
    }

    #[test]
    fn formats_date_pattern() {
        assert_eq!(format_clock(&fixed_zoned(), "%Y-%m-%d"), "2024-07-15");
    }

    #[test]
    fn invalid_pattern_yields_placeholder() {
        assert_eq!(format_clock(&fixed_zoned(), "%J"), "--");
    }

    #[test]
    fn clock_module_id_is_clock() {
        let module = ClockModule::new("%H:%M:%S".to_owned(), 1000);
        assert_eq!(module.id(), "clock");
    }

    #[test]
    fn interval_clamps_subsecond_config_to_one_second() {
        let module = ClockModule::new("%H:%M:%S".to_owned(), 250);
        assert_eq!(module.interval(), Some(Duration::from_millis(1000)));
    }

    #[test]
    fn update_with_invalid_format_returns_failed_state() {
        // `%Q` is a valid jiff specifier (IANA zone name), so use `%J`, which jiff rejects.
        let mut module = ClockModule::new("%J".to_owned(), 1000);
        let state = module.update();
        assert_eq!(state.text, "--");
        assert_eq!(state.icon, "");
        assert_eq!(state.level, wayward_core::Level::Normal);
    }
}

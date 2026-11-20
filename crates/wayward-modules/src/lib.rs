pub mod battery;
pub mod clock;
pub mod cpu;
pub mod ram;

use std::time::Duration;

use wayward_core::{Config, Module};

/// Clock ticks at most once per second (AGENTS.md §6).
pub fn clock_interval(interval_ms: u64) -> Duration {
    Duration::from_millis(interval_ms.max(1000))
}

/// Stats poll between 2 and 5 seconds.
pub fn stat_interval(interval_ms: u64) -> Duration {
    Duration::from_millis(interval_ms.clamp(2000, 5000))
}

/// Construct a module by config id. Unknown ids return `None` (app logs a warning).
pub fn create(id: &str, config: &Config) -> Option<Box<dyn Module>> {
    match id {
        "clock" => Some(Box::new(clock::ClockModule::new(
            config.modules.clock.format.clone(),
            config.modules.clock.interval_ms,
        ))),
        "cpu" => Some(Box::new(cpu::CpuModule::new(
            config.modules.cpu.interval_ms,
        ))),
        "ram" => Some(Box::new(ram::RamModule::new(
            config.modules.ram.interval_ms,
        ))),
        "battery" => Some(Box::new(battery::BatteryModule::new(
            config.modules.battery.interval_ms,
        ))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use wayward_core::Config;

    use super::{clock_interval, create, stat_interval};

    #[test]
    fn clock_interval_floors_subsecond_to_one_second() {
        assert_eq!(clock_interval(250), Duration::from_millis(1000));
    }

    #[test]
    fn clock_interval_preserves_value_above_minimum() {
        assert_eq!(clock_interval(5000), Duration::from_millis(5000));
    }

    #[test]
    fn stat_interval_floors_below_two_seconds() {
        assert_eq!(stat_interval(100), Duration::from_millis(2000));
    }

    #[test]
    fn stat_interval_ceils_above_five_seconds() {
        assert_eq!(stat_interval(9000), Duration::from_millis(5000));
    }

    #[test]
    fn stat_interval_preserves_in_range_value() {
        assert_eq!(stat_interval(3000), Duration::from_millis(3000));
    }

    #[test]
    fn create_clock_module() {
        let config = Config::default();
        let module = create("clock", &config).expect("clock module should be created");
        assert_eq!(module.id(), "clock");
    }

    #[test]
    fn create_cpu_module() {
        let config = Config::default();
        let module = create("cpu", &config).expect("cpu module should be created");
        assert_eq!(module.id(), "cpu");
    }

    #[test]
    fn create_ram_module() {
        let config = Config::default();
        let module = create("ram", &config).expect("ram module should be created");
        assert_eq!(module.id(), "ram");
    }

    #[test]
    fn create_battery_module() {
        let config = Config::default();
        let module = create("battery", &config).expect("battery module should be created");
        assert_eq!(module.id(), "battery");
    }

    #[test]
    fn create_unknown_id_returns_none() {
        let config = Config::default();
        assert!(create("nope", &config).is_none());
    }
}

use std::time::Duration;

use wayward_core::{Level, Module, ModuleState};
use wayward_platform::NetworkState;

/// Format a connectivity state as `NET up` or `NET down`. Pure: no Win32 calls.
pub fn format_network(connected: bool) -> String {
    if connected {
        "NET up".to_owned()
    } else {
        "NET down".to_owned()
    }
}

/// Map a platform `NetworkState` to a `ModuleState`. Pure: no Win32 calls.
pub fn network_state(state: NetworkState) -> ModuleState {
    let level = if state.connected {
        Level::Normal
    } else {
        Level::Warn
    };
    ModuleState {
        text: format_network(state.connected),
        icon: String::new(),
        level,
    }
}

pub struct NetworkModule {
    interval_ms: u64,
}

impl NetworkModule {
    pub fn new(interval_ms: u64) -> Self {
        Self { interval_ms }
    }
}

impl Module for NetworkModule {
    fn id(&self) -> &'static str {
        "network"
    }

    fn interval(&self) -> Option<Duration> {
        Some(Duration::from_millis(self.interval_ms.clamp(2000, 5000)))
    }

    fn update(&mut self) -> ModuleState {
        match wayward_platform::network_state() {
            Ok(state) => network_state(state),
            Err(error) => {
                tracing::warn!("network unavailable: {error}");
                ModuleState::failed()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_connected() {
        assert_eq!(format_network(true), "NET up");
    }

    #[test]
    fn format_disconnected() {
        assert_eq!(format_network(false), "NET down");
    }

    #[test]
    fn connected_state_is_normal_level() {
        let state = network_state(NetworkState { connected: true });
        assert_eq!(state.level, Level::Normal);
        assert_eq!(state.text, "NET up");
        assert!(state.icon.is_empty());
    }

    #[test]
    fn disconnected_state_is_warn_level() {
        let state = network_state(NetworkState { connected: false });
        assert_eq!(state.level, Level::Warn);
        assert_eq!(state.text, "NET down");
    }

    #[test]
    fn module_id_is_network() {
        assert_eq!(NetworkModule::new(5000).id(), "network");
    }

    #[test]
    fn interval_clamps_low_value() {
        assert_eq!(
            NetworkModule::new(100).interval(),
            Some(Duration::from_millis(2000))
        );
    }

    #[test]
    fn interval_clamps_high_value() {
        assert_eq!(
            NetworkModule::new(9000).interval(),
            Some(Duration::from_millis(5000))
        );
    }
}

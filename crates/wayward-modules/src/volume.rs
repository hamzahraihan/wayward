use std::time::Duration;

use wayward_core::{Level, Module, ModuleState};
use wayward_platform::AudioState;

/// Format a volume state as `VOL N%` or `VOL muted`. Pure: no Win32 calls.
pub fn format_volume(volume: f32, muted: bool) -> String {
    if muted {
        return "VOL muted".to_owned();
    }
    let percent = (volume.clamp(0.0, 1.0) * 100.0).round() as u32;
    format!("VOL {percent}%")
}

/// Map a platform `AudioState` to a `ModuleState`. Pure: no Win32 calls.
pub fn volume_state(audio: AudioState) -> ModuleState {
    ModuleState {
        text: format_volume(audio.volume, audio.muted),
        icon: String::new(),
        level: Level::Normal,
    }
}

pub struct VolumeModule {
    interval_ms: u64,
}

impl VolumeModule {
    pub fn new(interval_ms: u64) -> Self {
        Self { interval_ms }
    }
}

impl Module for VolumeModule {
    fn id(&self) -> &'static str {
        "volume"
    }

    fn interval(&self) -> Option<Duration> {
        Some(Duration::from_millis(self.interval_ms.clamp(2000, 5000)))
    }

    fn update(&mut self) -> ModuleState {
        match wayward_platform::audio_state() {
            Ok(state) => volume_state(state),
            Err(error) => {
                tracing::warn!("volume unavailable: {error}");
                ModuleState::failed()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_zero_volume() {
        assert_eq!(format_volume(0.0, false), "VOL 0%");
    }

    #[test]
    fn format_full_volume() {
        assert_eq!(format_volume(1.0, false), "VOL 100%");
    }

    #[test]
    fn format_rounds_to_nearest_percent() {
        assert_eq!(format_volume(0.426, false), "VOL 43%");
    }

    #[test]
    fn format_clamps_above_one() {
        assert_eq!(format_volume(1.5, false), "VOL 100%");
    }

    #[test]
    fn format_clamps_below_zero() {
        assert_eq!(format_volume(-0.5, false), "VOL 0%");
    }

    #[test]
    fn muted_renders_distinct_label() {
        assert_eq!(format_volume(0.5, true), "VOL muted");
    }

    #[test]
    fn muted_wins_over_volume_value() {
        assert_eq!(format_volume(1.0, true), "VOL muted");
    }

    #[test]
    fn state_carries_normal_level() {
        let state = volume_state(AudioState {
            volume: 0.5,
            muted: false,
        });
        assert_eq!(state.level, Level::Normal);
        assert!(state.icon.is_empty());
        assert_eq!(state.text, "VOL 50%");
    }

    #[test]
    fn module_id_is_volume() {
        assert_eq!(VolumeModule::new(2000).id(), "volume");
    }

    #[test]
    fn interval_clamps_low_value() {
        assert_eq!(
            VolumeModule::new(100).interval(),
            Some(Duration::from_millis(2000))
        );
    }

    #[test]
    fn interval_clamps_high_value() {
        assert_eq!(
            VolumeModule::new(9000).interval(),
            Some(Duration::from_millis(5000))
        );
    }
}

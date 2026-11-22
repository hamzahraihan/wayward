//! Safe wrapper over the Windows audio endpoint volume API.
//!
//! All `unsafe` and COM calls live here; callers in `wayward-modules` receive
//! plain data (`AudioState`) with no Win32 types.

use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};

/// Plain, Win32-free snapshot of the default render endpoint volume.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioState {
    /// 0.0..=1.0 scalar volume level.
    pub volume: f32,
    pub muted: bool,
}

/// Build an `AudioState` from raw scalar + mute flag. Pure: no COM calls.
///
/// The scalar is clamped to `0.0..=1.0` so a misbehaving driver cannot
/// produce out-of-range percentages in the UI.
pub fn state_from(volume: f32, muted: bool) -> AudioState {
    AudioState {
        volume: volume.clamp(0.0, 1.0),
        muted,
    }
}

/// Read the current default render endpoint volume and mute state.
///
/// One-shot COM apartment setup per call: the audio API is a system boundary
/// and is only queried every few seconds (per `wayward-modules::volume`),
/// so the setup cost is negligible and keeps the wrapper stateless.
pub fn audio_state() -> Result<AudioState, String> {
    // SAFETY: CoInitializeEx with COINIT_MULTITHREADED is the standard
    // apartment model for a short-lived worker call. S_FALSE (already
    // initialized) is a success code; failure HRESULTs are surfaced as errors.
    let coinit = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if coinit.is_err() {
        return Err(format!("CoInitializeEx failed: {coinit:?}"));
    }

    let result = read_audio_state();
    // SAFETY: pairs with the successful CoInitializeEx above; no COM objects
    // outlive this scope.
    unsafe { windows::Win32::System::Com::CoUninitialize() };
    result
}

fn read_audio_state() -> Result<AudioState, String> {
    // SAFETY: CoCreateInstance with a NULL outer pointer and CLSCTX_ALL is the
    // documented in-proc activation pattern for MMDeviceEnumerator. The
    // returned interface is owned and released on drop by windows-rs.
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
            .map_err(|error| format!("CoCreateInstance(MMDeviceEnumerator) failed: {error}"))?;

    // SAFETY: GetDefaultAudioEndpoint is a pure query; eRender/eConsole are
    // valid constants selecting the default playback endpoint.
    let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }
        .map_err(|error| format!("GetDefaultAudioEndpoint failed: {error}"))?;

    // SAFETY: Activate<IAudioEndpointVolume> is the documented pattern; the
    // returned interface is owned by `endpoint`.
    let endpoint: IAudioEndpointVolume = unsafe { device.Activate(CLSCTX_ALL, None) }
        .map_err(|error| format!("Activate(IAudioEndpointVolume) failed: {error}"))?;

    // SAFETY: GetMasterVolumeLevelScalar returns a scalar by value.
    let scalar = unsafe { endpoint.GetMasterVolumeLevelScalar() }
        .map_err(|error| format!("volume read failed: {error}"))?;

    // SAFETY: GetMute returns a BOOL by value.
    let muted = unsafe { endpoint.GetMute() }
        .map_err(|error| format!("mute read failed: {error}"))?
        .as_bool();

    Ok(state_from(scalar, muted))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_from_clamps_high() {
        assert_eq!(state_from(1.5, false).volume, 1.0);
    }

    #[test]
    fn state_from_clamps_low() {
        assert_eq!(state_from(-0.5, false).volume, 0.0);
    }

    #[test]
    fn state_from_preserves_in_range() {
        assert_eq!(state_from(0.42, false).volume, 0.42);
    }

    #[test]
    fn state_from_carries_mute_flag() {
        assert!(state_from(0.42, true).muted);
        assert!(!state_from(0.42, false).muted);
    }
}

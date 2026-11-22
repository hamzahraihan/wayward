//! Safe wrapper over the Windows `INetworkListManager` connectivity API.
//!
//! All COM calls live here; callers in `wayward-modules` receive plain data
//! (`NetworkState`) with no Win32 types.

use windows::Win32::Networking::NetworkListManager::{
    INetworkListManager, NetworkListManager, NLM_CONNECTIVITY_IPV4_INTERNET,
    NLM_CONNECTIVITY_IPV6_INTERNET,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};

/// Plain, Win32-free connectivity snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkState {
    /// True when either IPv4 or IPv6 internet connectivity is reported.
    pub connected: bool,
}

/// Build a `NetworkState` from a raw `NLM_CONNECTIVITY` bitfield. Pure.
pub fn state_from(connectivity: u32) -> NetworkState {
    let internet_mask =
        (NLM_CONNECTIVITY_IPV4_INTERNET.0 | NLM_CONNECTIVITY_IPV6_INTERNET.0) as u32;
    NetworkState {
        connected: (connectivity & internet_mask) != 0,
    }
}

/// Query current internet connectivity.
///
/// One-shot COM apartment setup per call; the network module polls slowly
/// (default 5s) and the cost is negligible.
pub fn network_state() -> Result<NetworkState, String> {
    // SAFETY: standard in-proc COM initialization for a short-lived call.
    let coinit = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if coinit.is_err() {
        return Err(format!("CoInitializeEx failed: {coinit:?}"));
    }
    let result = read_network_state();
    // SAFETY: pairs with the successful CoInitializeEx above.
    unsafe { windows::Win32::System::Com::CoUninitialize() };
    result
}

fn read_network_state() -> Result<NetworkState, String> {
    // SAFETY: CoCreateInstance with NULL outer + CLSCTX_ALL is the documented
    // pattern for NetworkListManager. Returned interface is owned by windows-rs.
    let manager: INetworkListManager =
        unsafe { CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL) }
            .map_err(|error| format!("CoCreateInstance(NetworkListManager) failed: {error}"))?;

    // SAFETY: GetConnectivity is a pure query returning NLM_CONNECTIVITY by value.
    let connectivity = unsafe { manager.GetConnectivity() }
        .map_err(|error| format!("GetConnectivity failed: {error}"))?;

    Ok(state_from(connectivity.0 as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    const I4: u32 = NLM_CONNECTIVITY_IPV4_INTERNET.0 as u32;
    const I6: u32 = NLM_CONNECTIVITY_IPV6_INTERNET.0 as u32;

    #[test]
    fn disconnected_when_no_internet_bits() {
        assert!(!state_from(0).connected);
    }

    #[test]
    fn connected_with_ipv4() {
        assert!(state_from(I4).connected);
    }

    #[test]
    fn connected_with_ipv6() {
        assert!(state_from(I6).connected);
    }

    #[test]
    fn connected_with_both() {
        assert!(state_from(I4 | I6).connected);
    }

    #[test]
    fn lan_only_is_not_internet() {
        // NLM_CONNECTIVITY_LAN = 0x1; we only treat IPv4/IPv6 internet bits as connected.
        assert!(!state_from(0x1).connected);
    }
}

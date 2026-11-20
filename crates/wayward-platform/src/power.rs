use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

/// Safe, plain-data view of the system power state. No Win32 types escape this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerStatus {
    pub ac_online: bool,
    pub battery_present: bool,
    pub life_percent: Option<u8>,
    pub charging: bool,
}

/// Pure helper for testing without Win32 calls.
pub fn from_fields(ac_line_status: u8, battery_flag: u8, life_percent: u8) -> PowerStatus {
    PowerStatus {
        ac_online: ac_line_status == 1,
        battery_present: battery_flag & 0x80 == 0,
        life_percent: if life_percent == 255 {
            None
        } else {
            Some(life_percent)
        },
        charging: battery_flag & 0x08 != 0,
    }
}

/// One GetSystemPowerStatus syscall. All unsafe lives here in wayward-platform.
pub fn power_status() -> Result<PowerStatus, String> {
    let mut raw = SYSTEM_POWER_STATUS::default();
    // SAFETY: `raw` is a local, exclusively owned, zero-initialized
    // SYSTEM_POWER_STATUS. `&mut raw` is a valid, properly aligned pointer to a
    // struct of the exact type the API expects; GetSystemPowerStatus only writes
    // this struct and retains no pointer. The call is stateless and thread-safe.
    unsafe { GetSystemPowerStatus(&mut raw) }
        .map_err(|error| format!("GetSystemPowerStatus failed: {error}"))?;
    Ok(from_fields(
        raw.ACLineStatus,
        raw.BatteryFlag,
        raw.BatteryLifePercent,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_battery_flag_means_battery_absent() {
        let status = from_fields(1, 0x80, 50);
        assert!(!status.battery_present);
    }

    #[test]
    fn percent_255_is_unknown() {
        let status = from_fields(1, 0, 255);
        assert_eq!(status.life_percent, None);
    }

    #[test]
    fn charging_flag_sets_charging() {
        let status = from_fields(1, 0x08, 50);
        assert!(status.charging);
    }

    #[test]
    fn ac_line_1_is_online() {
        let status = from_fields(1, 0, 50);
        assert!(status.ac_online);
    }

    #[test]
    fn known_percent_is_some() {
        let status = from_fields(1, 0, 62);
        assert_eq!(status.life_percent, Some(62));
    }

    #[test]
    fn ac_line_0_is_offline() {
        let status = from_fields(0, 0, 50);
        assert!(!status.ac_online);
    }

    #[test]
    fn battery_present_without_no_battery_flag() {
        let status = from_fields(1, 0, 50);
        assert!(status.battery_present);
    }

    #[test]
    fn absent_charging_flag_is_not_charging() {
        let status = from_fields(1, 0, 50);
        assert!(!status.charging);
    }
}

/// Logical (device-independent pixel) size of the primary monitor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorSize {
    pub width_logical: f32,
    pub height_logical: f32,
}

/// Convert a physical-pixel measurement to logical (DIP) pixels.
///
/// A `dpi` of 0 is treated as 96 (100% scaling). The same formula applies to
/// every input: a `physical` value of 0 yields `0.0`, and a negative
/// `physical` value yields a negative result (no clamping is performed).
pub fn logical_from_physical(physical: i32, dpi: u32) -> f32 {
    let effective_dpi = if dpi == 0 { 96 } else { dpi };
    physical as f32 * 96.0 / effective_dpi as f32
}

/// Returns the logical (DIP) size of the primary monitor.
///
/// `GetSystemMetrics` reports physical pixels, so each axis is converted with
/// [`logical_from_physical`] using the system DPI.
pub fn primary_monitor_logical_size() -> MonitorSize {
    // SAFETY: stateless Win32 query with no raw pointers, aliasing, or
    // lifetime-bearing arguments; `SM_CXSCREEN` is a valid index constant and
    // the `i32` result is returned by value. Safe from any thread once per
    // window setup.
    let physical_x = unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
            windows::Win32::UI::WindowsAndMessaging::SM_CXSCREEN,
        )
    };

    // SAFETY: stateless Win32 query with no raw pointers, aliasing, or
    // lifetime-bearing arguments; `SM_CYSCREEN` is a valid index constant and
    // the `i32` result is returned by value. Safe from any thread once per
    // window setup.
    let physical_y = unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
            windows::Win32::UI::WindowsAndMessaging::SM_CYSCREEN,
        )
    };

    // SAFETY: stateless Win32 query with no arguments, raw pointers, aliasing,
    // or lifetime-bearing values; the `u32` result is returned by value. Safe
    // from any thread once per window setup.
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForSystem() };

    MonitorSize {
        width_logical: logical_from_physical(physical_x, dpi),
        height_logical: logical_from_physical(physical_y, dpi),
    }
}

#[cfg(test)]
mod tests {
    use super::logical_from_physical;

    #[test]
    fn converts_at_96_dpi() {
        assert_eq!(logical_from_physical(1920, 96), 1920.0);
    }

    #[test]
    fn converts_at_192_dpi() {
        assert_eq!(logical_from_physical(1920, 192), 960.0);
    }

    #[test]
    fn treats_zero_dpi_as_96() {
        assert_eq!(logical_from_physical(1920, 0), 1920.0);
    }

    #[test]
    fn zero_physical_is_zero() {
        assert_eq!(logical_from_physical(0, 96), 0.0);
    }

    #[test]
    fn negative_physical_stays_negative() {
        assert_eq!(logical_from_physical(-100, 96), -100.0);
    }
}

/// Logical (device-independent pixel) size of the primary monitor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorSize {
    pub width_logical: f32,
    pub height_logical: f32,
}

/// Logical (device-independent pixel) primary work area: the screen region
/// excluding the taskbar. All fields are logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkArea {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
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

/// Query the primary work area via `SPI_GETWORKAREA` and convert its physical
/// `RECT` to logical pixels using the current system DPI.
pub fn primary_work_area_logical() -> WorkArea {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::HiDpi::GetDpiForSystem;
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };

    let mut raw = RECT::default();

    // SAFETY: `SPI_GETWORKAREA` writes exactly one `RECT` into `pvparam`. `raw`
    // is a live, writable, correctly aligned `RECT` whose address is passed as
    // `Some(*mut c_void)`; it outlives the synchronous call. `uiparam` is
    // ignored for this action and `fwinini` is 0 (no profile write or
    // broadcast), both valid. No aliasing or lifetime-bearing value escapes.
    let _ = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut raw as *mut RECT as *mut core::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };

    // SAFETY: stateless Win32 query with no arguments, raw pointers, aliasing,
    // or lifetime-bearing values; the `u32` result is returned by value. Safe
    // from any thread once per window setup.
    let dpi = unsafe { GetDpiForSystem() };

    WorkArea {
        left: logical_from_physical(raw.left, dpi),
        top: logical_from_physical(raw.top, dpi),
        right: logical_from_physical(raw.right, dpi),
        bottom: logical_from_physical(raw.bottom, dpi),
    }
}

/// Clamp a preferred `(x, y)` so a `w` x `h` box stays inside `area`.
///
/// Pure: no Win32 calls. If the box is larger than the area on an axis, that
/// axis is pinned to the area's origin (`left` / `top`).
pub fn clamp_work_area(area: WorkArea, w: f32, h: f32, preferred: (f32, f32)) -> (f32, f32) {
    let max_x = area.right - w;
    let max_y = area.bottom - h;
    let x = if max_x < area.left {
        area.left
    } else {
        preferred.0.clamp(area.left, max_x)
    };
    let y = if max_y < area.top {
        area.top
    } else {
        preferred.1.clamp(area.top, max_y)
    };
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::{clamp_work_area, logical_from_physical, WorkArea};

    fn area() -> WorkArea {
        WorkArea {
            left: 0.0,
            top: 0.0,
            right: 1920.0,
            bottom: 1080.0,
        }
    }

    #[test]
    fn clamp_inside_is_unchanged() {
        assert_eq!(
            clamp_work_area(area(), 100.0, 40.0, (500.0, 300.0)),
            (500.0, 300.0)
        );
    }

    #[test]
    fn clamp_left_edge() {
        assert_eq!(
            clamp_work_area(area(), 100.0, 40.0, (-10.0, 300.0)),
            (0.0, 300.0)
        );
    }

    #[test]
    fn clamp_right_edge() {
        assert_eq!(
            clamp_work_area(area(), 100.0, 40.0, (2000.0, 300.0)),
            (1820.0, 300.0)
        );
    }

    #[test]
    fn clamp_top_edge() {
        assert_eq!(
            clamp_work_area(area(), 100.0, 40.0, (500.0, -10.0)),
            (500.0, 0.0)
        );
    }

    #[test]
    fn clamp_bottom_edge() {
        assert_eq!(
            clamp_work_area(area(), 100.0, 40.0, (500.0, 2000.0)),
            (500.0, 1040.0)
        );
    }

    #[test]
    fn oversized_box_returns_origin() {
        assert_eq!(
            clamp_work_area(area(), 2000.0, 1200.0, (500.0, 300.0)),
            (0.0, 0.0)
        );
    }

    #[test]
    fn nonzero_origin_area() {
        let a = WorkArea {
            left: 100.0,
            top: 50.0,
            right: 500.0,
            bottom: 350.0,
        };
        assert_eq!(
            clamp_work_area(a, 100.0, 40.0, (250.0, 150.0)),
            (250.0, 150.0)
        );
        assert_eq!(clamp_work_area(a, 100.0, 40.0, (0.0, 0.0)), (100.0, 50.0));
        assert_eq!(
            clamp_work_area(a, 100.0, 40.0, (999.0, 999.0)),
            (400.0, 310.0)
        );
    }

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

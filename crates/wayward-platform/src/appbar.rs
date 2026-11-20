use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::Shell::{
    SHAppBarMessage, ABE_BOTTOM, ABE_TOP, ABM_ACTIVATE, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE,
    ABM_SETPOS, ABM_WINDOWPOSCHANGED, ABN_FULLSCREENAPP, ABN_POSCHANGED, APPBARDATA,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, RegisterWindowMessageW, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    GWL_EXSTYLE, HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE,
    SW_SHOWNA, WS_EX_TOOLWINDOW,
};

/// Opaque, `Copy + Send` reference to a Win32 window. No raw pointer escapes the platform crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowHandle(pub isize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppBarEdge {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// logical pixels -> physical pixels. `dpi == 0` means 96 (100%).
pub fn physical_height(logical_height: u32, dpi: u32) -> i32 {
    let effective = if dpi == 0 { 96 } else { dpi };
    let scaled = (logical_height as f64) * (effective as f64) / 96.0;
    scaled.round() as i32
}

/// `ABM_QUERYPOS` only adjusts the requested edge. Restore the opposite edge
/// using our physical height; leave left/right untouched.
pub fn query_rect(edge: AppBarEdge, approved: Rect, height_px: i32) -> Rect {
    match edge {
        AppBarEdge::Top => Rect {
            bottom: approved.top + height_px,
            ..approved
        },
        AppBarEdge::Bottom => Rect {
            top: approved.bottom - height_px,
            ..approved
        },
    }
}

/// Single-shot flag claim used for reentrancy and fullscreen idempotency.
pub fn claim_once(flag: &AtomicBool) -> bool {
    flag.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

pub fn release_claim(flag: &AtomicBool) {
    flag.store(false, Ordering::SeqCst);
}

/// Returns true only on the TRUE -> FALSE transition (state actually changed).
pub fn set_fullscreen_hidden(flag: &AtomicBool, hidden: bool) -> bool {
    if hidden {
        claim_once(flag)
    } else {
        if flag.load(Ordering::SeqCst) {
            flag.store(false, Ordering::SeqCst);
            true
        } else {
            false
        }
    }
}

/// Register a unique callback message id for AppBar notifications.
pub fn register_callback_message() -> Result<u32, String> {
    // SAFETY: RegisterWindowMessageW takes a nul-terminated wide string literal with
    // static lifetime; it returns a u32 by value and has no pointer/lifetime arguments.
    let id = unsafe { RegisterWindowMessageW(windows::core::w!("WaywardAppBarMessage")) };
    if id == 0 {
        Err("RegisterWindowMessageW failed".to_owned())
    } else {
        Ok(id)
    }
}

/// Owns a registered AppBar. `Drop` always sends `ABM_REMOVE`.
pub struct AppBar {
    hwnd: HWND,
    callback_msg: u32,
    edge: AppBarEdge,
    logical_height: u32,
    registered: bool,
    repositioning: AtomicBool,
    fullscreen_hidden: AtomicBool,
}

impl AppBar {
    pub fn register(
        hwnd: WindowHandle,
        edge: AppBarEdge,
        logical_height: u32,
    ) -> Result<AppBar, String> {
        let hwnd = HWND(hwnd.0 as *mut core::ffi::c_void);
        let callback_msg = register_callback_message()?;
        let mut bar = AppBar {
            hwnd,
            callback_msg,
            edge,
            logical_height,
            registered: false,
            repositioning: AtomicBool::new(false),
            fullscreen_hidden: AtomicBool::new(false),
        };

        // SAFETY: `bar` is a local, exclusively owned APPBARDATA. Every field is a
        // plain value (HWND copied by value, RECT is POD); `cbSize` is set correctly.
        // SHAppBarMessage only reads pdata for these messages and returns a usize.
        let mut data = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: hwnd,
            uCallbackMessage: callback_msg,
            uEdge: match edge {
                AppBarEdge::Top => ABE_TOP,
                AppBarEdge::Bottom => ABE_BOTTOM,
            },
            rc: RECT::default(),
            ..Default::default()
        };
        // SAFETY: see above — valid, correctly sized APPBARDATA for ABM_NEW.
        let ok = unsafe { SHAppBarMessage(ABM_NEW, &mut data) };
        if ok == 0 {
            return Err("ABM_NEW failed".to_owned());
        }
        bar.registered = true;

        // Ensure WS_EX_TOOLWINDOW so the bar has no taskbar button / Alt+Tab entry.
        // SAFETY: GetWindowLongPtrW/SetWindowLongPtrW operate on our own HWND with a
        // valid index (GWL_EXSTYLE) and flag value; no pointers are dereferenced.
        let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
        // SAFETY: same as above; SetWindowLongPtrW returns the previous value by value.
        unsafe {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style | (WS_EX_TOOLWINDOW.0 as isize));
        }
        // SAFETY: SetWindowPos with null insert-after and SWP flags only changes style;
        // all arguments are values, no pointer dereference.
        let _ = unsafe {
            SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        };

        bar.apply_geometry();
        Ok(bar)
    }

    pub fn callback_message(&self) -> u32 {
        self.callback_msg
    }

    fn apply_geometry(&mut self) {
        let dpi = self.current_dpi();
        let height_px = physical_height(self.logical_height, dpi);
        let mut data = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: self.hwnd,
            uCallbackMessage: self.callback_msg,
            uEdge: match self.edge {
                AppBarEdge::Top => ABE_TOP,
                AppBarEdge::Bottom => ABE_BOTTOM,
            },
            ..Default::default()
        };

        let proposed = self.proposed_rect(height_px);
        data.rc.left = proposed.left;
        data.rc.top = proposed.top;
        data.rc.right = proposed.right;
        data.rc.bottom = proposed.bottom;

        // SAFETY: valid APPBARDATA; ABM_QUERYPOS fills `rc` with the system-approved
        // rectangle for our HWND and edge. No caller-owned pointers are retained.
        unsafe {
            SHAppBarMessage(ABM_QUERYPOS, &mut data);
        }
        let approved = Rect {
            left: data.rc.left,
            top: data.rc.top,
            right: data.rc.right,
            bottom: data.rc.bottom,
        };
        let final_rect = query_rect(self.edge, approved, height_px);
        data.rc = RECT {
            left: final_rect.left,
            top: final_rect.top,
            right: final_rect.right,
            bottom: final_rect.bottom,
        };
        // SAFETY: valid APPBARDATA with the corrected rectangle; ABM_SETPOS commits it.
        unsafe {
            SHAppBarMessage(ABM_SETPOS, &mut data);
        }
        // SAFETY: MoveWindow takes plain integer coordinates for our own HWND.
        let _ = unsafe {
            windows::Win32::UI::WindowsAndMessaging::MoveWindow(
                self.hwnd,
                data.rc.left,
                data.rc.top,
                data.rc.right - data.rc.left,
                data.rc.bottom - data.rc.top,
                true,
            )
        };
    }

    /// Primary monitor size in PHYSICAL pixels.
    fn primary_monitor_physical(&self) -> (i32, i32) {
        // SAFETY: GetSystemMetrics takes an index constant and returns i32 by value.
        unsafe {
            (
                windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
                    windows::Win32::UI::WindowsAndMessaging::SM_CXSCREEN,
                ),
                windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
                    windows::Win32::UI::WindowsAndMessaging::SM_CYSCREEN,
                ),
            )
        }
    }

    fn proposed_rect(&self, height_px: i32) -> Rect {
        let (width, height) = self.primary_monitor_physical();
        match self.edge {
            AppBarEdge::Top => Rect {
                left: 0,
                top: 0,
                right: width,
                bottom: height_px,
            },
            AppBarEdge::Bottom => Rect {
                left: 0,
                top: height - height_px,
                right: width,
                bottom: height,
            },
        }
    }

    fn current_dpi(&self) -> u32 {
        // SAFETY: GetDpiForSystem takes no arguments and returns a u32 by value.
        let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForSystem() };
        if dpi == 0 {
            96
        } else {
            dpi
        }
    }

    /// Re-run QUERYPOS/SETPOS after DPI/display changes. Guarded against the
    /// ABN_POSCHANGED feedback loop.
    pub fn reposition(&mut self, dpi: u32) {
        if !self.registered {
            return;
        }
        if !claim_once(&self.repositioning) {
            return;
        }
        let height_px = physical_height(self.logical_height, dpi);
        let mut data = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: self.hwnd,
            uCallbackMessage: self.callback_msg,
            uEdge: match self.edge {
                AppBarEdge::Top => ABE_TOP,
                AppBarEdge::Bottom => ABE_BOTTOM,
            },
            ..Default::default()
        };
        let proposed = self.proposed_rect(height_px);
        data.rc.left = proposed.left;
        data.rc.top = proposed.top;
        data.rc.right = proposed.right;
        data.rc.bottom = proposed.bottom;

        // SAFETY: valid APPBARDATA; see apply_geometry().
        unsafe {
            SHAppBarMessage(ABM_QUERYPOS, &mut data);
        }
        let approved = Rect {
            left: data.rc.left,
            top: data.rc.top,
            right: data.rc.right,
            bottom: data.rc.bottom,
        };
        let final_rect = query_rect(self.edge, approved, height_px);
        data.rc = RECT {
            left: final_rect.left,
            top: final_rect.top,
            right: final_rect.right,
            bottom: final_rect.bottom,
        };
        // SAFETY: valid APPBARDATA; see apply_geometry().
        unsafe {
            SHAppBarMessage(ABM_SETPOS, &mut data);
        }
        release_claim(&self.repositioning);
    }

    pub fn activate(&self) {
        let mut data = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: self.hwnd,
            ..Default::default()
        };
        // SAFETY: valid APPBARDATA; ABM_ACTIVATE reads hWnd/cbSize only.
        unsafe {
            SHAppBarMessage(ABM_ACTIVATE, &mut data);
        }
    }

    pub fn window_pos_changed(&self) {
        let mut data = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: self.hwnd,
            ..Default::default()
        };
        // SAFETY: valid APPBARDATA; ABM_WINDOWPOSCHANGED reads hWnd/cbSize only.
        unsafe {
            SHAppBarMessage(ABM_WINDOWPOSCHANGED, &mut data);
        }
    }

    /// Dispatch ABN_* notifications received on `uCallbackMessage`.
    pub fn on_appbar_message(&mut self, wparam: usize, lparam: isize) {
        if !self.registered {
            return;
        }
        match wparam as u32 {
            ABN_POSCHANGED => self.reposition(self.current_dpi()),
            ABN_FULLSCREENAPP if set_fullscreen_hidden(&self.fullscreen_hidden, lparam != 0) => {
                {
                    if lparam != 0 {
                        // SAFETY: ShowWindow takes our HWND and a plain command value.
                        unsafe {
                            let _ = ShowWindow(self.hwnd, SW_HIDE);
                        }
                    } else {
                        // SAFETY: ShowWindow/SetWindowPos take values and our own HWND.
                        unsafe {
                            let _ = ShowWindow(self.hwnd, SW_SHOWNA);
                            let _ = SetWindowPos(
                                self.hwnd,
                                Some(HWND_TOPMOST),
                                0,
                                0,
                                0,
                                0,
                                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Idempotent ABM_REMOVE.
    pub fn remove(&mut self) {
        if !self.registered {
            return;
        }
        self.registered = false;
        let mut data = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: self.hwnd,
            uCallbackMessage: self.callback_msg,
            uEdge: match self.edge {
                AppBarEdge::Top => ABE_TOP,
                AppBarEdge::Bottom => ABE_BOTTOM,
            },
            ..Default::default()
        };
        // SAFETY: valid APPBARDATA matching registration; ABM_REMOVE unregisters our appbar.
        let _remove_result = unsafe { SHAppBarMessage(ABM_REMOVE, &mut data) };
    }
}

impl Drop for AppBar {
    fn drop(&mut self) {
        self.remove();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::{
        claim_once, physical_height, query_rect, release_claim, set_fullscreen_hidden, AppBarEdge,
        Rect,
    };

    #[test]
    fn physical_height_at_96_dpi() {
        assert_eq!(physical_height(28, 96), 28);
    }

    #[test]
    fn physical_height_at_120_dpi() {
        assert_eq!(physical_height(28, 120), 35);
    }

    #[test]
    fn physical_height_at_144_dpi() {
        assert_eq!(physical_height(28, 144), 42);
    }

    #[test]
    fn physical_height_at_192_dpi() {
        assert_eq!(physical_height(28, 192), 56);
    }

    #[test]
    fn physical_height_zero_dpi_defaults_to_96() {
        assert_eq!(physical_height(28, 0), 28);
    }

    #[test]
    fn query_rect_top_restores_bottom_edge() {
        let approved = Rect {
            left: 0,
            top: 30,
            right: 1920,
            bottom: 30,
        };
        assert_eq!(
            query_rect(AppBarEdge::Top, approved, 28),
            Rect {
                left: 0,
                top: 30,
                right: 1920,
                bottom: 58
            }
        );
    }

    #[test]
    fn query_rect_bottom_restores_top_edge() {
        let approved = Rect {
            left: 0,
            top: 1000,
            right: 1920,
            bottom: 1052,
        };
        assert_eq!(
            query_rect(AppBarEdge::Bottom, approved, 28),
            Rect {
                left: 0,
                top: 1024,
                right: 1920,
                bottom: 1052
            }
        );
    }

    #[test]
    fn claim_once_is_single_shot() {
        let flag = AtomicBool::new(false);
        assert!(claim_once(&flag));
        assert!(!claim_once(&flag));
    }

    #[test]
    fn claim_once_can_be_released_and_reclaimed() {
        let flag = AtomicBool::new(false);
        assert!(claim_once(&flag));
        release_claim(&flag);
        assert!(claim_once(&flag));
    }

    #[test]
    fn fullscreen_transition_is_idempotent() {
        let hidden = AtomicBool::new(false);
        assert!(set_fullscreen_hidden(&hidden, true));
        assert!(!set_fullscreen_hidden(&hidden, true));
        assert!(set_fullscreen_hidden(&hidden, false));
        assert!(!set_fullscreen_hidden(&hidden, false));
    }
}

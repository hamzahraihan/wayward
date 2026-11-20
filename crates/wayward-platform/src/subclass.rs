use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    WM_ACTIVATE, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_NCDESTROY, WM_SETTINGCHANGE,
    WM_WINDOWPOSCHANGED,
};

use crate::appbar::{AppBar, WindowHandle};

/// Idempotent guard for an installed window subclass.
pub struct SubclassGuard {
    hwnd: HWND,
    installed: bool,
}

impl SubclassGuard {
    pub fn remove(&mut self) {
        if !self.installed {
            return;
        }
        self.installed = false;
        // SAFETY: `subclass_proc` is a static, long-lived function pointer; the HWND
        // is our own window and we are on its UI thread. RemoveWindowSubclass takes
        // plain values and unregisters the callback installed by `install`.
        unsafe {
            let _ = RemoveWindowSubclass(self.hwnd, Some(subclass_proc), SUBCLASS_ID);
        }
    }
}

impl Drop for SubclassGuard {
    fn drop(&mut self) {
        self.remove();
    }
}

const SUBCLASS_ID: usize = 1;

/// Install the subclass. MUST be called on the UI thread (iced's `window::run` closure).
pub fn install(hwnd: WindowHandle, appbar: *mut AppBar) -> Result<SubclassGuard, String> {
    let hwnd = HWND(hwnd.0 as *mut core::ffi::c_void);
    // SAFETY: `subclass_proc` has static lifetime and correct SUBCLASSPROC signature.
    // `appbar` is passed as dwRefData; it is owned by the thread-local PlatformGuard in
    // lib.rs, remains valid until shutdown(), and is only touched on this UI thread.
    // SetWindowSubclass takes plain values and returns BOOL; no pointer is dereferenced here.
    let ok = unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, appbar as usize) };
    if !ok.as_bool() {
        return Err("SetWindowSubclass failed".to_owned());
    }
    Ok(SubclassGuard {
        hwnd,
        installed: true,
    })
}

/// SAFETY: `data` is the `appbar` pointer supplied at install time. It points to the
/// AppBar owned by the thread-local PlatformGuard; valid until shutdown() removes
/// the subclass first. All access happens on the single UI thread. Every Win32 call
/// uses plain values or our own HWND. The unconditional exit path calls DefSubclassProc.
unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    let appbar = data as *mut AppBar;
    if appbar.is_null() {
        // SAFETY: forwarding is always safe; the subclass chain remains intact.
        return DefSubclassProc(hwnd, msg, wparam, lparam);
    }

    // SAFETY: see function-level SAFETY. `&mut *appbar` is unique on this thread.
    let bar = &mut *appbar;

    if msg == bar.callback_message() {
        bar.on_appbar_message(wparam.0, lparam.0);
    } else if msg == WM_DPICHANGED || msg == WM_DISPLAYCHANGE || msg == WM_SETTINGCHANGE {
        // SAFETY: GetDpiForWindow takes our HWND and returns u32 by value.
        let dpi = {
            let d = GetDpiForWindow(hwnd);
            if d == 0 {
                96
            } else {
                d
            }
        };
        bar.reposition(dpi);
    } else if msg == WM_ACTIVATE {
        bar.activate();
    } else if msg == WM_WINDOWPOSCHANGED {
        bar.window_pos_changed();
    } else if msg == WM_NCDESTROY {
        // SAFETY: RemoveWindowSubclass takes plain values; this removes our own callback.
        let _ = RemoveWindowSubclass(hwnd, Some(subclass_proc), id);
    }

    // SAFETY: DefSubclassProc forwards to the next handler with the original arguments.
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

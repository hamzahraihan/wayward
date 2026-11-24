use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use std::io::{self, BufRead, Write};
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};

use windows::Win32::System::Console::{
    AttachConsole, GetConsoleMode, GetStdHandle, SetConsoleCtrlHandler, SetConsoleMode,
    ATTACH_PARENT_PROCESS, CTRL_BREAK_EVENT, CTRL_C_EVENT, ENABLE_ECHO_INPUT, STD_INPUT_HANDLE,
};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

static HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);
static CTRL_CLAIMED: AtomicBool = AtomicBool::new(false);
static TARGET_HWND: AtomicIsize = AtomicIsize::new(0);

/// Read one line from stdin with console echo disabled, so a secret typed at
/// an interactive prompt is not displayed. Piped stdin needs no echo change.
pub fn read_secret() -> Result<String, String> {
    // SAFETY: GetStdHandle takes a standard-device constant and returns a
    // handle by value. An error means stdin is not a console (e.g. piped
    // input), in which case there is no echo to suppress.
    let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) }.ok();
    let mut original_mode = None;

    if let Some(handle) = handle {
        // SAFETY: GetConsoleMode writes one value into a live, correctly typed
        // local and returns a Result by value. The handle is the console stdin.
        let mut mode = windows::Win32::System::Console::CONSOLE_MODE(0);
        if unsafe { GetConsoleMode(handle, &mut mode) }.is_ok() {
            let no_echo =
                windows::Win32::System::Console::CONSOLE_MODE(mode.0 & !ENABLE_ECHO_INPUT.0);
            // SAFETY: SetConsoleMode takes a console handle and a mode value;
            // it changes only this process's console input mode.
            if unsafe { SetConsoleMode(handle, no_echo) }.is_ok() {
                original_mode = Some((handle, mode));
            }
        }
    }

    let mut line = String::new();
    let result = io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|error| format!("reading secret from stdin: {error}"));
    let _ = io::stderr().flush();

    if let Some((handle, mode)) = original_mode {
        // SAFETY: restores the exact mode read above; the handle remains valid
        // for the lifetime of this process.
        unsafe {
            let _ = SetConsoleMode(handle, mode);
        }
    }

    result?;
    Ok(line.trim_end_matches(['\r', '\n']).to_owned())
}

/// Pure single-shot claim used by the console handler.
pub fn claim_ctrl_event(claimed: &AtomicBool) -> bool {
    claimed
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

pub struct ConsoleGuard {
    registered: bool,
}

impl ConsoleGuard {
    pub fn unregister(&mut self) {
        if !self.registered {
            return;
        }
        self.registered = false;
        // SAFETY: `console_handler` is a static, long-lived function pointer with the
        // correct signature; SetConsoleCtrlHandler with add=false removes our handler.
        unsafe {
            let _ = SetConsoleCtrlHandler(Some(console_handler), false);
        }
        HANDLER_INSTALLED.store(false, Ordering::SeqCst);
        TARGET_HWND.store(0, Ordering::SeqCst);
        CTRL_CLAIMED.store(false, Ordering::SeqCst);
    }
}

impl Drop for ConsoleGuard {
    fn drop(&mut self) {
        self.unregister();
    }
}

/// Attach to the parent console (failure tolerated) and install the control handler.
pub fn install(hwnd: isize) -> Result<ConsoleGuard, String> {
    // SAFETY: AttachConsole takes a u32 value (ATTACH_PARENT_PROCESS) and returns BOOL.
    // It has no pointer/lifetime arguments; failure simply means no parent console.
    let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
    if attached.is_err() {
        eprintln!("wayward: no parent console; control events unavailable");
    }

    if HANDLER_INSTALLED.load(Ordering::SeqCst) {
        return Err("console handler already installed".to_owned());
    }

    TARGET_HWND.store(hwnd, Ordering::SeqCst);
    CTRL_CLAIMED.store(false, Ordering::SeqCst);

    // SAFETY: `console_handler` has static lifetime and matches PHANDLER_ROUTINE.
    // SetConsoleCtrlHandler takes the function pointer and a bool by value.
    let result = unsafe { SetConsoleCtrlHandler(Some(console_handler), true) };
    if let Err(error) = result {
        TARGET_HWND.store(0, Ordering::SeqCst);
        return Err(format!("SetConsoleCtrlHandler failed: {error:?}"));
    }

    HANDLER_INSTALLED.store(true, Ordering::SeqCst);
    Ok(ConsoleGuard { registered: true })
}

/// SAFETY: runs on an OS-created thread. Uses only atomics and PostMessageW with
/// plain values. No locks, no logging, no UI state access. Handler posts WM_CLOSE
/// once and returns TRUE so the process exits through iced's graceful close path.
unsafe extern "system" fn console_handler(ctrl_type: u32) -> BOOL {
    if ctrl_type != CTRL_C_EVENT && ctrl_type != CTRL_BREAK_EVENT {
        return BOOL(0);
    }
    if !claim_ctrl_event(&CTRL_CLAIMED) {
        return BOOL(1);
    }

    let hwnd = TARGET_HWND.load(Ordering::SeqCst);
    if hwnd == 0 {
        return BOOL(1);
    }

    // SAFETY: PostMessageW takes an HWND value, a message id and two by-value
    // parameters; it posts asynchronously without dereferencing caller memory.
    unsafe {
        let _ = PostMessageW(
            Some(HWND(hwnd as *mut core::ffi::c_void)),
            WM_CLOSE,
            WPARAM(0),
            LPARAM(0),
        );
    }
    BOOL(1)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::claim_ctrl_event;

    #[test]
    fn ctrl_event_is_claimed_once() {
        let flag = AtomicBool::new(false);
        assert!(claim_ctrl_event(&flag));
        assert!(!claim_ctrl_event(&flag));
    }
}

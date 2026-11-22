//! Active-window tracking via `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)`.
//!
//! All `unsafe` and Win32 calls live here. A single hook thread runs a Win32
//! message pump and calls `win_event_proc` on each `EVENT_SYSTEM_FOREGROUND`.
//! The callback reads the new foreground title via `GetForegroundWindow` +
//! `GetWindowTextW` and sends it over a std channel. The iced subscription
//! drains that channel as a stream, so idle CPU is effectively zero between
//! foreground changes (the thread blocks in `GetMessageW`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowTextW, PostThreadMessageW,
    TranslateMessage, EVENT_SYSTEM_FOREGROUND, MSG, WINEVENT_OUTOFCONTEXT, WM_QUIT,
};

const TITLE_BUFFER: usize = 512;

/// Plain, Win32-free payload delivered on every foreground change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundEvent {
    pub title: String,
}

/// Truncate `title` to `max` chars, replacing the last kept char with an
/// ellipsis when the input is longer. Pure: no Win32 calls.
pub fn truncate_title(title: &str, max: usize) -> String {
    let count = title.chars().count();
    if count <= max {
        return title.to_owned();
    }
    let keep = max.saturating_sub(1);
    let mut out: String = title.chars().take(keep).collect();
    out.push('\u{2026}');
    out
}

/// Read the current foreground window title, if any.
pub fn foreground_title() -> Option<String> {
    // SAFETY: GetForegroundWindow takes no arguments and returns an HWND by
    // value; it never dereferences caller memory.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return None;
    }
    read_title(hwnd)
}

fn read_title(hwnd: HWND) -> Option<String> {
    let mut buf = [0u16; TITLE_BUFFER];
    // SAFETY: GetWindowTextW copies at most `buf.len()` UTF-16 units into `buf`
    // (including a null terminator when it fits) and returns the copied length.
    // `buf` is a live, writable, correctly aligned array that outlives the call.
    let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if len <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

/// Handle to the running hook thread. Dropping it signals the pump to exit
/// and joins the thread so the hook is unhooked cleanly.
pub struct ForegroundGuard {
    stop: Arc<AtomicBool>,
    thread_id: u32,
    handle: Option<JoinHandle<()>>,
}

impl ForegroundGuard {
    /// Stop the pump thread and wait for it to exit.
    pub fn stop(&mut self) {
        if self.handle.is_none() {
            return;
        }
        self.stop.store(true, Ordering::SeqCst);
        // SAFETY: PostThreadMessageW posts WM_QUIT to the hook thread's queue,
        // breaking its blocking GetMessageW call. `thread_id` is the hook
        // thread's id captured inside hook_thread via GetCurrentThreadId.
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for ForegroundGuard {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Install the foreground hook on a dedicated thread. Returns a receiver that
/// yields one event per foreground change (plus an initial event for the
/// current foreground), and a guard that unhooks on drop.
pub fn install() -> Result<(Receiver<ForegroundEvent>, ForegroundGuard), String> {
    let (tx, rx) = channel::<ForegroundEvent>();
    let (ready_tx, ready_rx) = channel::<Result<u32, String>>();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_thread = Arc::clone(&stop);

    let handle = thread::Builder::new()
        .name("wayward-foreground".to_owned())
        .spawn(move || hook_thread(tx, ready_tx, stop_for_thread))
        .map_err(|error| format!("spawn foreground thread: {error}"))?;

    match ready_rx.recv_timeout(Duration::from_secs(2)) {
        Ok(Ok(thread_id)) => Ok((
            rx,
            ForegroundGuard {
                stop,
                thread_id,
                handle: Some(handle),
            },
        )),
        Ok(Err(message)) => Err(message),
        Err(_) => Err("foreground hook setup timed out".to_owned()),
    }
}

fn hook_thread(
    tx: Sender<ForegroundEvent>,
    ready: Sender<Result<u32, String>>,
    stop: Arc<AtomicBool>,
) {
    // SAFETY: GetCurrentThreadId takes no arguments and returns a thread id by
    // value; used only to address PostThreadMessageW(WM_QUIT) later.
    let thread_id = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };

    // SAFETY: SetWinEventHook with WINEVENT_OUTOFCONTEXT installs a thread-local
    // hook that calls back via the message pump below. `win_event_proc` has the
    // correct WINEVENTPROC signature and static lifetime. idprocess=0 and
    // idthread=0 means "all processes on this thread".
    let hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(win_event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    if hook.0.is_null() {
        let _ = ready.send(Err("SetWinEventHook failed".to_owned()));
        return;
    }

    // Make this thread's sender reachable from win_event_proc.
    SENDER.with(|cell| *cell.borrow_mut() = Some(tx.clone()));

    // Emit an initial event so the bar shows the current foreground immediately.
    if let Some(title) = foreground_title() {
        let _ = tx.send(ForegroundEvent { title });
    }
    if ready.send(Ok(thread_id)).is_err() {
        // Receiver dropped (install timed out); unhook and exit.
        // SAFETY: UnhookWinEvent takes the handle returned above.
        unsafe {
            let _ = UnhookWinEvent(hook);
        }
        return;
    }

    // SAFETY: standard Win32 message pump. GetMessageW blocks until a message
    // arrives; WM_QUIT (posted by Drop) breaks the loop. The hook's
    // win_event_proc runs inline during DispatchMessageW.
    while !stop.load(Ordering::SeqCst) {
        let mut msg = MSG::default();
        // SAFETY: GetMessageW fills `msg`; hWnd=None means "any window owned by
        // this thread". Returns false on WM_QUIT or error, true otherwise.
        let result = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if !result.as_bool() {
            break;
        }
        // SAFETY: TranslateMessage/DispatchMessageW dispatch the pump message;
        // win_event_proc runs inline and does the channel send.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    // SAFETY: UnhookWinEvent takes the handle returned by SetWinEventHook above.
    unsafe {
        let _ = UnhookWinEvent(hook);
    }
    SENDER.with(|cell| *cell.borrow_mut() = None);
}

/// SAFETY: invoked by Windows on the hook thread with valid arguments. Reads
/// the current foreground title (a stateless Win32 query) and sends it over a
/// thread-local sender. No caller memory is retained; no locks; no logging.
unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _idobject: i32,
    _idchild: i32,
    _thread: u32,
    _time: u32,
) {
    if let Some(title) = foreground_title() {
        if let Some(tx) = current_sender() {
            let _ = tx.send(ForegroundEvent { title });
        }
    }
}

thread_local! {
    static SENDER: std::cell::RefCell<Option<Sender<ForegroundEvent>>> =
        const { std::cell::RefCell::new(None) };
}

fn current_sender() -> Option<Sender<ForegroundEvent>> {
    SENDER.with(|cell| cell.borrow().clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_title_unchanged() {
        assert_eq!(truncate_title("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_fit_unchanged() {
        assert_eq!(truncate_title("hello", 5), "hello");
    }

    #[test]
    fn truncate_long_title_ellipsis() {
        assert_eq!(truncate_title("hello world", 5), "hell\u{2026}");
    }

    #[test]
    fn truncate_zero_max_is_ellipsis_only() {
        assert_eq!(truncate_title("hello", 0), "\u{2026}");
    }

    #[test]
    fn truncate_counts_chars_not_bytes() {
        // "é" is 2 bytes but 1 char; truncating by chars must keep it intact.
        assert_eq!(truncate_title("ééé", 2), "é\u{2026}");
    }

    #[test]
    fn truncate_empty_title() {
        assert_eq!(truncate_title("", 5), "");
    }
}

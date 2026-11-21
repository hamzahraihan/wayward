pub mod appbar;
pub mod console;
pub mod power;
pub mod screen;
pub mod shell;
pub mod subclass;

pub use appbar::*;

pub use console::{claim_ctrl_event, ConsoleGuard};
pub use power::*;
pub use screen::*;
pub use shell::*;
pub use subclass::SubclassGuard;

use std::cell::RefCell;

use crate::appbar::AppBar as AppBarImpl;

/// Options supplied by the iced app at attach time.
#[derive(Debug, Clone, Copy)]
pub struct PlatformOptions {
    pub edge: AppBarEdge,
    pub logical_height: u32,
}

struct PlatformGuard {
    appbar: Box<AppBarImpl>,
    subclass: SubclassGuard,
    console: Option<ConsoleGuard>,
}

thread_local! {
    static ACTIVE: RefCell<Option<PlatformGuard>> = const { RefCell::new(None) };
}

/// Attach the AppBar to a window. SAFE at the call site: every unsafe operation is
/// confined to wayward-platform. MUST be called on the UI thread (iced's
/// `window::run` closure) because subclass installation is thread-affine.
pub fn attach(hwnd: WindowHandle, options: PlatformOptions) -> Result<(), String> {
    let mut appbar = Box::new(AppBar::register(
        hwnd,
        options.edge,
        options.logical_height,
    )?);
    let appbar_ptr = &mut *appbar as *mut AppBarImpl;

    let subclass = subclass::install(hwnd, appbar_ptr)?;
    let console = match console::install(hwnd.0) {
        Ok(guard) => Some(guard),
        Err(error) => {
            eprintln!("wayward: control-event handler unavailable: {error}");
            None
        }
    };
    ACTIVE.with(|slot| {
        *slot.borrow_mut() = Some(PlatformGuard {
            appbar,
            subclass,
            console,
        });
    });
    Ok(())
}

pub fn reconfigure(options: PlatformOptions) -> Result<(), String> {
    ACTIVE.with(|slot| {
        let mut guard = slot.borrow_mut();
        let guard = guard
            .as_mut()
            .ok_or_else(|| "AppBar is not attached".to_owned())?;
        guard
            .appbar
            .reconfigure(options.edge, options.logical_height);
        Ok(())
    })
}

/// Deterministic shutdown: ABM_REMOVE + subclass removal + console unregister.
/// Idempotent. Called from the iced close path.
pub fn shutdown() {
    ACTIVE.with(|slot| {
        if let Some(mut guard) = slot.borrow_mut().take() {
            if let Some(console) = guard.console.as_mut() {
                console.unregister();
            }
            guard.subclass.remove();
            guard.appbar.remove();
        }
    });
}

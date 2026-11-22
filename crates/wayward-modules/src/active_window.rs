use std::time::Duration;

use wayward_core::{Level, Module, ModuleState};
use wayward_platform::truncate_title;

/// Maximum number of chars rendered for the active-window cell.
pub const MAX_TITLE_CHARS: usize = 32;

/// Build a `ModuleState` for an active-window title, truncating to the bar.
/// Pure: no Win32 calls.
pub fn active_window_state(title: &str) -> ModuleState {
    if title.is_empty() {
        return ModuleState::failed();
    }
    ModuleState {
        text: truncate_title(title, MAX_TITLE_CHARS),
        icon: String::new(),
        level: Level::Normal,
    }
}

pub struct ActiveWindowModule {
    latest: Option<String>,
}

impl ActiveWindowModule {
    pub fn new() -> Self {
        Self { latest: None }
    }
}

impl Default for ActiveWindowModule {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for ActiveWindowModule {
    fn id(&self) -> &'static str {
        "active_window"
    }

    fn interval(&self) -> Option<Duration> {
        None
    }

    fn update(&mut self) -> ModuleState {
        match self.latest.as_deref() {
            Some(title) => active_window_state(title),
            None => ModuleState::failed(),
        }
    }

    fn push(&mut self, payload: &str) {
        self.latest = Some(payload.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_title_unchanged() {
        let state = active_window_state("Untitled - Editor");
        assert_eq!(state.text, "Untitled - Editor");
        assert_eq!(state.level, Level::Normal);
    }

    #[test]
    fn empty_title_is_failed() {
        assert_eq!(active_window_state("").text, "--");
    }

    #[test]
    fn long_title_truncated() {
        let title = "a".repeat(MAX_TITLE_CHARS + 10);
        let state = active_window_state(&title);
        assert_eq!(state.text.chars().count(), MAX_TITLE_CHARS);
        assert!(state.text.ends_with('\u{2026}'));
    }

    #[test]
    fn title_at_limit_is_not_truncated() {
        let title = "a".repeat(MAX_TITLE_CHARS);
        let state = active_window_state(&title);
        assert_eq!(state.text.chars().count(), MAX_TITLE_CHARS);
        assert!(!state.text.ends_with('\u{2026}'));
    }

    #[test]
    fn module_id_is_active_window() {
        assert_eq!(ActiveWindowModule::new().id(), "active_window");
    }

    #[test]
    fn interval_is_none_for_event_driven_module() {
        assert_eq!(ActiveWindowModule::new().interval(), None);
    }

    #[test]
    fn update_is_failed_before_first_event() {
        let mut module = ActiveWindowModule::new();
        assert_eq!(module.update().text, "--");
    }

    #[test]
    fn push_updates_state() {
        let mut module = ActiveWindowModule::new();
        module.push("Hello");
        assert_eq!(module.update().text, "Hello");
    }

    #[test]
    fn push_trims_long_titles_on_update() {
        let mut module = ActiveWindowModule::new();
        module.push(&"a".repeat(MAX_TITLE_CHARS + 5));
        assert_eq!(module.update().text.chars().count(), MAX_TITLE_CHARS);
    }

    #[test]
    fn latest_push_wins() {
        let mut module = ActiveWindowModule::new();
        module.push("first");
        module.push("second");
        assert_eq!(module.update().text, "second");
    }
}

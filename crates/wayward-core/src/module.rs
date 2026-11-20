use std::time::Duration;

/// Mouse button, mirrored from iced at the app boundary because core must not depend on iced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level {
    #[default]
    Normal,
    Warn,
    Urgent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleState {
    pub text: String,
    pub icon: String,
    pub level: Level,
}

impl ModuleState {
    /// Failure placeholder per AGENTS.md §6: failing module shows `--` and logs a warning.
    pub fn failed() -> Self {
        Self {
            text: "--".to_owned(),
            icon: String::new(),
            level: Level::Normal,
        }
    }
}

pub trait Module: Send + 'static {
    fn id(&self) -> &'static str;
    fn interval(&self) -> Option<Duration>;
    fn update(&mut self) -> ModuleState;
    fn on_click(&mut self, _button: MouseButton) {}
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn failed_state_uses_placeholder_text() {
        assert_eq!(ModuleState::failed().text, "--");
    }

    #[test]
    fn failed_state_has_empty_icon() {
        assert_eq!(ModuleState::failed().icon, "");
    }

    #[test]
    fn failed_state_is_normal_level() {
        assert_eq!(ModuleState::failed().level, Level::Normal);
    }

    #[test]
    fn level_defaults_to_normal() {
        assert_eq!(Level::default(), Level::Normal);
    }

    struct StubModule;

    impl Module for StubModule {
        fn id(&self) -> &'static str {
            "stub"
        }

        fn interval(&self) -> Option<Duration> {
            None
        }

        fn update(&mut self) -> ModuleState {
            ModuleState::failed()
        }
    }

    #[test]
    fn module_on_click_default_is_noop() {
        let mut module = StubModule;
        module.on_click(MouseButton::Left);
        module.on_click(MouseButton::Other(7));
    }
}

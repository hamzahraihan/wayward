pub const DEFAULT_CONFIG: &str = include_str!("../../../config/wayward.toml");
pub const DEFAULT_THEME_NAME: &str = wayward_core::DEFAULT_THEME_NAME;
pub const DEFAULT_THEME: &str = include_str!("../../../config/themes/catppuccin-mocha.toml");

#[cfg(test)]
mod tests {
    use super::DEFAULT_CONFIG;

    #[test]
    fn bundled_template_has_catppuccin_mocha_defaults() {
        assert!(DEFAULT_CONFIG.contains("name = \"catppuccin-mocha\""));
        assert!(DEFAULT_CONFIG.contains("background = \"#1e1e2e\""));
    }

    #[test]
    fn bundled_template_deserializes_with_module_defaults() {
        let config: wayward_core::Config =
            toml::from_str(DEFAULT_CONFIG).expect("bundled template should deserialize");

        assert_eq!(config.modules.clock.interval_ms, 1000);
        assert_eq!(config.modules.cpu.interval_ms, 2000);
        assert_eq!(config.modules.ram.interval_ms, 3000);
        assert_eq!(config.modules.battery.interval_ms, 5000);
        assert_eq!(config.layout.left, ["clock", "cpu", "ram", "battery"]);
    }

    #[test]
    fn bundled_template_deserializes_with_ai_defaults() {
        let config: wayward_core::Config =
            toml::from_str(DEFAULT_CONFIG).expect("bundled template should deserialize");

        assert!(!config.ai.enabled);
        assert_eq!(config.ai.mode, wayward_core::AiMode::Mock);
        assert_eq!(config.ai.interval_ms, 300000);
    }

    #[test]
    fn bundled_template_documents_providers_without_enabling_them() {
        let config: wayward_core::Config =
            toml::from_str(DEFAULT_CONFIG).expect("bundled template should deserialize");

        assert!(!config.ai.experimental);
        assert!(config.ai.providers.is_empty());
        assert!(DEFAULT_CONFIG.contains("# experimental = false"));
        assert!(DEFAULT_CONFIG.contains("# [[ai.providers]]"));
        assert!(DEFAULT_CONFIG.contains("# id = \"claude-code\""));
        assert!(DEFAULT_CONFIG.contains("# id = \"opencode\""));
    }

    #[test]
    fn bundled_theme_deserializes_with_exact_palette() {
        use super::{DEFAULT_THEME, DEFAULT_THEME_NAME};

        let theme: wayward_core::ThemeFile =
            toml::from_str(DEFAULT_THEME).expect("bundled theme should deserialize");

        assert_eq!(DEFAULT_THEME_NAME, "catppuccin-mocha");
        assert_eq!(theme.name.as_deref(), Some("catppuccin-mocha"));
        assert_eq!(theme.background.as_deref(), Some("#1e1e2e"));
        assert_eq!(theme.foreground.as_deref(), Some("#cdd6f4"));
        assert_eq!(theme.accent.as_deref(), Some("#cba6f7"));
        assert_eq!(theme.muted.as_deref(), Some("#6c7086"));
        assert_eq!(theme.warn.as_deref(), Some("#f9e2af"));
        assert_eq!(theme.urgent.as_deref(), Some("#f38ba8"));
        assert_eq!(theme.surface.as_deref(), Some("#313244"));
        assert_eq!(theme.success.as_deref(), Some("#a6e3a1"));
    }
}

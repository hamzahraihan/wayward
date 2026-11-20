use crate::config::{Config, ThemeConfig};

pub const BACKGROUND: &str = "#1e1e2e";
pub const FOREGROUND: &str = "#cdd6f4";
pub const ACCENT: &str = "#cba6f7";
pub const MUTED: &str = "#6c7086";
pub const WARN: &str = "#f9e2af";
pub const URGENT: &str = "#f38ba8";
pub const SURFACE: &str = "#313244";
pub const SUCCESS: &str = "#a6e3a1";

pub fn catppuccin_mocha() -> ThemeConfig {
    ThemeConfig {
        name: "catppuccin-mocha".to_owned(),
        background: BACKGROUND.to_owned(),
        foreground: FOREGROUND.to_owned(),
        accent: ACCENT.to_owned(),
        muted: MUTED.to_owned(),
        warn: WARN.to_owned(),
        urgent: URGENT.to_owned(),
        surface: SURFACE.to_owned(),
        success: SUCCESS.to_owned(),
    }
}

pub fn parse_hex_rgb(s: &str) -> Option<(f32, f32, f32)> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }

    let channel = |start: usize| u8::from_str_radix(&hex[start..start + 2], 16).ok();
    let (r, g, b) = (channel(0)?, channel(2)?, channel(4)?);
    Some((
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    ))
}

pub fn validate(config: &Config) -> Vec<String> {
    let theme = &config.theme;
    let colors = [
        ("theme.background", theme.background.as_str()),
        ("theme.foreground", theme.foreground.as_str()),
        ("theme.accent", theme.accent.as_str()),
        ("theme.muted", theme.muted.as_str()),
        ("theme.warn", theme.warn.as_str()),
        ("theme.urgent", theme.urgent.as_str()),
        ("theme.surface", theme.surface.as_str()),
        ("theme.success", theme.success.as_str()),
    ];

    let mut errors: Vec<String> = colors
        .iter()
        .filter(|(_, value)| parse_hex_rgb(value).is_none())
        .map(|(name, value)| format!("{name} must be a #RRGGBB hex color, got {value:?}"))
        .collect();

    if !(16..=128).contains(&config.bar.height) {
        errors.push(format!(
            "bar.height must be in 16..=128, got {}",
            config.bar.height
        ));
    }
    if !(8.0..=24.0).contains(&config.font.size) {
        errors.push(format!(
            "font.size must be in 8.0..=24.0, got {}",
            config.font.size
        ));
    }
    if config.modules.clock.interval_ms < 250 {
        errors.push(format!(
            "clock.interval_ms must be at least 250, got {}",
            config.modules.clock.interval_ms
        ));
    }

    errors
}

pub fn effective_theme(config: &Config) -> ThemeConfig {
    let fallback = catppuccin_mocha();
    let pick = |value: &str, default: &str| -> String {
        if parse_hex_rgb(value).is_some() {
            value.to_owned()
        } else {
            default.to_owned()
        }
    };

    ThemeConfig {
        name: if config.theme.name.is_empty() {
            fallback.name.clone()
        } else {
            config.theme.name.clone()
        },
        background: pick(&config.theme.background, &fallback.background),
        foreground: pick(&config.theme.foreground, &fallback.foreground),
        accent: pick(&config.theme.accent, &fallback.accent),
        muted: pick(&config.theme.muted, &fallback.muted),
        warn: pick(&config.theme.warn, &fallback.warn),
        urgent: pick(&config.theme.urgent, &fallback.urgent),
        surface: pick(&config.theme.surface, &fallback.surface),
        success: pick(&config.theme.success, &fallback.success),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn catppuccin_mocha_has_exact_palette() {
        let theme = catppuccin_mocha();

        assert_eq!(theme.name, "catppuccin-mocha");
        assert_eq!(theme.background, "#1e1e2e");
        assert_eq!(theme.foreground, "#cdd6f4");
        assert_eq!(theme.accent, "#cba6f7");
        assert_eq!(theme.muted, "#6c7086");
        assert_eq!(theme.warn, "#f9e2af");
        assert_eq!(theme.urgent, "#f38ba8");
        assert_eq!(theme.surface, "#313244");
        assert_eq!(theme.success, "#a6e3a1");

        assert_eq!(BACKGROUND, "#1e1e2e");
        assert_eq!(FOREGROUND, "#cdd6f4");
        assert_eq!(ACCENT, "#cba6f7");
        assert_eq!(MUTED, "#6c7086");
        assert_eq!(WARN, "#f9e2af");
        assert_eq!(URGENT, "#f38ba8");
        assert_eq!(SURFACE, "#313244");
        assert_eq!(SUCCESS, "#a6e3a1");
    }

    #[test]
    fn parse_hex_rgb_parses_background() {
        assert_eq!(
            parse_hex_rgb(BACKGROUND),
            Some((30.0 / 255.0, 30.0 / 255.0, 46.0 / 255.0))
        );
    }

    #[test]
    fn parse_hex_rgb_accepts_uppercase_and_mixed_case() {
        assert_eq!(parse_hex_rgb("#FF00AA"), Some((1.0, 0.0, 170.0 / 255.0)));
        assert_eq!(
            parse_hex_rgb("#aBcDeF"),
            Some((171.0 / 255.0, 205.0 / 255.0, 239.0 / 255.0))
        );
    }

    #[test]
    fn parse_hex_rgb_rejects_invalid_input() {
        for input in [
            "#fff", "red", "", "#gggggg", "#12345", "#1234567", "123456", "#12 456",
        ] {
            assert_eq!(parse_hex_rgb(input), None, "expected None for {input:?}");
        }
    }

    #[test]
    fn validate_accepts_default_config() {
        assert!(validate(&Config::default()).is_empty());
    }

    #[test]
    fn validate_flags_out_of_range_values_and_bad_hex() {
        let mut config = Config::default();
        config.bar.height = 4;
        config.font.size = 2.0;
        config.modules.clock.interval_ms = 100;
        config.theme.background = "red".to_owned();

        let errors = validate(&config);

        assert_eq!(errors.len(), 4, "unexpected errors: {errors:?}");
        assert!(errors.iter().any(|e| e.contains("height")), "{errors:?}");
        assert!(errors.iter().any(|e| e.contains("size")), "{errors:?}");
        assert!(
            errors.iter().any(|e| e.contains("interval_ms")),
            "{errors:?}"
        );
        assert!(
            errors.iter().any(|e| e.contains("background")),
            "{errors:?}"
        );
    }

    #[test]
    fn validate_flags_every_invalid_theme_field() {
        let mut config = Config::default();
        config.theme.foreground = String::new();
        config.theme.accent = "#fff".to_owned();
        config.theme.muted = "nope".to_owned();
        config.theme.warn = "#1234567".to_owned();
        config.theme.urgent = String::new();
        config.theme.surface = "blue".to_owned();
        config.theme.success = "#GGGGGG".to_owned();

        let errors = validate(&config);

        assert_eq!(errors.len(), 7, "unexpected errors: {errors:?}");
        for field in [
            "theme.foreground",
            "theme.accent",
            "theme.muted",
            "theme.warn",
            "theme.urgent",
            "theme.surface",
            "theme.success",
        ] {
            assert!(
                errors.iter().any(|e| e.contains(field)),
                "missing {field}: {errors:?}"
            );
        }
    }

    #[test]
    fn effective_theme_falls_back_field_by_field() {
        let mut config = Config::default();
        config.theme.name = "custom".to_owned();
        config.theme.background = "#010203".to_owned();
        config.theme.foreground = String::new();
        config.theme.accent = "not-a-color".to_owned();
        config.theme.muted = "#aabbcc".to_owned();
        config.theme.warn = String::new();
        config.theme.urgent = "#ff0000".to_owned();
        config.theme.surface = "#12345".to_owned();
        config.theme.success = "#00ff00".to_owned();

        let theme = effective_theme(&config);

        assert_eq!(theme.name, "custom");
        assert_eq!(theme.background, "#010203");
        assert_eq!(theme.foreground, FOREGROUND);
        assert_eq!(theme.accent, ACCENT);
        assert_eq!(theme.muted, "#aabbcc");
        assert_eq!(theme.warn, WARN);
        assert_eq!(theme.urgent, "#ff0000");
        assert_eq!(theme.surface, SURFACE);
        assert_eq!(theme.success, "#00ff00");
    }

    #[test]
    fn effective_theme_restores_empty_name() {
        let mut config = Config::default();
        config.theme.name = String::new();

        assert_eq!(effective_theme(&config).name, "catppuccin-mocha");
    }
}

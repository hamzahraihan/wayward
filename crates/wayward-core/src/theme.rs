use crate::config::{AiMode, Config, ThemeConfig};
use serde::Deserialize;

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

/// A named theme file. Every field is optional: absent keys fall back to the
/// inline config value, then to Catppuccin Mocha (AGENTS.md §5.1).
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct ThemeFile {
    pub name: Option<String>,
    pub background: Option<String>,
    pub foreground: Option<String>,
    pub accent: Option<String>,
    pub muted: Option<String>,
    pub warn: Option<String>,
    pub urgent: Option<String>,
    pub surface: Option<String>,
    pub success: Option<String>,
}

/// Named-file value (valid hex) > inline value (valid hex) > Catppuccin constant.
/// name: file non-empty > inline non-empty > "catppuccin-mocha".
pub fn resolve_theme(inline: &ThemeConfig, file: Option<&ThemeFile>) -> ThemeConfig {
    let fallback = catppuccin_mocha();

    let pick = |file_value: Option<&str>, inline_value: &str, default: &str| -> String {
        match file_value {
            Some(value) if parse_hex_rgb(value).is_some() => value.to_owned(),
            _ if parse_hex_rgb(inline_value).is_some() => inline_value.to_owned(),
            _ => default.to_owned(),
        }
    };

    let name = match file.and_then(|f| f.name.as_deref()) {
        Some(value) if !value.is_empty() => value.to_owned(),
        _ if !inline.name.is_empty() => inline.name.clone(),
        _ => fallback.name.clone(),
    };

    ThemeConfig {
        name,
        background: pick(
            file.and_then(|f| f.background.as_deref()),
            &inline.background,
            &fallback.background,
        ),
        foreground: pick(
            file.and_then(|f| f.foreground.as_deref()),
            &inline.foreground,
            &fallback.foreground,
        ),
        accent: pick(
            file.and_then(|f| f.accent.as_deref()),
            &inline.accent,
            &fallback.accent,
        ),
        muted: pick(
            file.and_then(|f| f.muted.as_deref()),
            &inline.muted,
            &fallback.muted,
        ),
        warn: pick(
            file.and_then(|f| f.warn.as_deref()),
            &inline.warn,
            &fallback.warn,
        ),
        urgent: pick(
            file.and_then(|f| f.urgent.as_deref()),
            &inline.urgent,
            &fallback.urgent,
        ),
        surface: pick(
            file.and_then(|f| f.surface.as_deref()),
            &inline.surface,
            &fallback.surface,
        ),
        success: pick(
            file.and_then(|f| f.success.as_deref()),
            &inline.success,
            &fallback.success,
        ),
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

    for (name, interval_ms) in [
        ("cpu.interval_ms", config.modules.cpu.interval_ms),
        ("ram.interval_ms", config.modules.ram.interval_ms),
        ("battery.interval_ms", config.modules.battery.interval_ms),
    ] {
        if !(2000..=5000).contains(&interval_ms) {
            errors.push(format!("{name} must be in 2000..=5000, got {interval_ms}"));
        }
    }

    if config.ai.enabled {
        let ai = &config.ai;
        if ai.mode == AiMode::Unsupported {
            errors.push(
                "ai.mode is reserved for a later milestone; only \"mock\" is supported".to_owned(),
            );
        }
        if ai.interval_ms < 30_000 {
            errors.push(format!(
                "ai.interval_ms must be at least 30000, got {}",
                ai.interval_ms
            ));
        }
        if !(0.0..=1.0).contains(&ai.warn_below) {
            errors.push(format!(
                "ai.warn_below must be in 0.0..=1.0, got {}",
                ai.warn_below
            ));
        }
        if !(0.0..=1.0).contains(&ai.urgent_below) {
            errors.push(format!(
                "ai.urgent_below must be in 0.0..=1.0, got {}",
                ai.urgent_below
            ));
        }
        if ai.warn_below < ai.urgent_below {
            errors.push(format!(
                "ai.warn_below must be >= ai.urgent_below, got {} < {}",
                ai.warn_below, ai.urgent_below
            ));
        }
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
    fn validate_flags_out_of_range_stat_interval() {
        let mut config = Config::default();
        config.modules.cpu.interval_ms = 1000;

        let errors = validate(&config);

        assert_eq!(errors.len(), 1, "unexpected errors: {errors:?}");
        assert!(
            errors
                .iter()
                .any(|e| e.contains("cpu.interval_ms") && e.contains("2000..=5000")),
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

    #[test]
    fn resolve_theme_file_hex_overrides_inline() {
        let inline = ThemeConfig {
            background: "#111111".to_owned(),
            foreground: "#222222".to_owned(),
            ..ThemeConfig::default()
        };

        let file = ThemeFile {
            background: Some("#aaaaaa".to_owned()),
            foreground: Some("#bbbbbb".to_owned()),
            ..ThemeFile::default()
        };

        let resolved = resolve_theme(&inline, Some(&file));
        assert_eq!(resolved.background, "#aaaaaa");
        assert_eq!(resolved.foreground, "#bbbbbb");
    }

    #[test]
    fn resolve_theme_invalid_file_hex_falls_back_to_inline() {
        let inline = ThemeConfig {
            background: "#123456".to_owned(),
            ..ThemeConfig::default()
        };

        let file = ThemeFile {
            background: Some("red".to_owned()),
            ..ThemeFile::default()
        };

        let resolved = resolve_theme(&inline, Some(&file));
        assert_eq!(resolved.background, "#123456");
    }

    #[test]
    fn resolve_theme_missing_file_field_falls_back_to_inline() {
        let inline = ThemeConfig {
            accent: "#abcdef".to_owned(),
            ..ThemeConfig::default()
        };

        let resolved = resolve_theme(&inline, Some(&ThemeFile::default()));
        assert_eq!(resolved.accent, "#abcdef");
    }

    #[test]
    fn resolve_theme_invalid_inline_falls_back_to_catppuccin() {
        let inline = ThemeConfig {
            background: "not-a-color".to_owned(),
            accent: "#fff".to_owned(),
            ..ThemeConfig::default()
        };

        let resolved = resolve_theme(&inline, None);
        assert_eq!(resolved.background, BACKGROUND);
        assert_eq!(resolved.accent, ACCENT);
    }

    #[test]
    fn resolve_theme_name_precedence() {
        let inline = ThemeConfig {
            name: "inline-name".to_owned(),
            ..ThemeConfig::default()
        };

        let file_named = ThemeFile {
            name: Some("file-name".to_owned()),
            ..ThemeFile::default()
        };
        assert_eq!(resolve_theme(&inline, Some(&file_named)).name, "file-name");

        let file_empty = ThemeFile {
            name: Some(String::new()),
            ..ThemeFile::default()
        };
        assert_eq!(
            resolve_theme(&inline, Some(&file_empty)).name,
            "inline-name"
        );
        assert_eq!(resolve_theme(&inline, None).name, "inline-name");

        let inline_empty = ThemeConfig {
            name: String::new(),
            ..ThemeConfig::default()
        };
        assert_eq!(resolve_theme(&inline_empty, None).name, "catppuccin-mocha");
        assert_eq!(
            resolve_theme(&inline_empty, Some(&file_empty)).name,
            "catppuccin-mocha"
        );
    }

    #[test]
    fn theme_file_deserializes_partial_toml() {
        let file: ThemeFile = toml::from_str("name = \"tokyo-night\"\nbackground = \"#010203\"")
            .expect("partial theme TOML should deserialize");

        assert_eq!(file.name.as_deref(), Some("tokyo-night"));
        assert_eq!(file.background.as_deref(), Some("#010203"));
        assert_eq!(file.foreground, None);
        assert_eq!(file.accent, None);
        assert_eq!(file.muted, None);
        assert_eq!(file.warn, None);
        assert_eq!(file.urgent, None);
        assert_eq!(file.surface, None);
        assert_eq!(file.success, None);
    }

    #[test]
    fn validate_ai_unsupported_mode_errors() {
        use crate::config::{AiConfig, AiMode, Config};
        let config = Config {
            ai: AiConfig {
                enabled: true,
                mode: AiMode::Unsupported,
                ..AiConfig::default()
            },
            ..Config::default()
        };
        let errors = validate(&config);
        assert!(
            errors.iter().any(|e| e
                .contains("ai.mode is reserved for a later milestone; only \"mock\" is supported")),
            "{errors:?}"
        );
    }

    #[test]
    fn validate_ai_interval_too_low_errors() {
        use crate::config::{AiConfig, Config};
        let config = Config {
            ai: AiConfig {
                enabled: true,
                interval_ms: 1000,
                ..AiConfig::default()
            },
            ..Config::default()
        };
        let errors = validate(&config);
        assert!(errors.iter().any(|e| e.contains("interval")), "{errors:?}");
    }

    #[test]
    fn validate_ai_warn_below_out_of_range_errors() {
        use crate::config::{AiConfig, Config};
        let config = Config {
            ai: AiConfig {
                enabled: true,
                warn_below: 1.5,
                ..AiConfig::default()
            },
            ..Config::default()
        };
        let errors = validate(&config);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("warn_below") || e.contains("ai")),
            "{errors:?}"
        );
    }

    #[test]
    fn validate_default_has_no_ai_errors() {
        let errors = validate(&Config::default());

        assert!(errors.iter().all(|e| !e.contains("ai.")), "{errors:?}");
    }

    #[test]
    fn validate_rejects_unsupported_ai_mode() {
        let config: Config = toml::from_str("[ai]\nenabled = true\nmode = \"real\"")
            .expect("unknown ai mode should deserialize");

        let errors = validate(&config);

        assert!(
            errors.iter().any(|e| e
                .contains("ai.mode is reserved for a later milestone; only \"mock\" is supported")),
            "{errors:?}"
        );
    }

    #[test]
    fn validate_rejects_too_short_ai_interval() {
        let config: Config = toml::from_str("[ai]\nenabled = true\ninterval_ms = 1000")
            .expect("ai TOML should deserialize");

        let errors = validate(&config);

        assert!(
            errors.iter().any(|e| e.contains("ai.interval_ms")),
            "{errors:?}"
        );
    }

    #[test]
    fn validate_rejects_out_of_range_ai_warn_below() {
        let config: Config = toml::from_str("[ai]\nenabled = true\nwarn_below = 1.5")
            .expect("ai TOML should deserialize");

        let errors = validate(&config);

        assert!(
            errors.iter().any(|e| e.contains("ai.warn_below")),
            "{errors:?}"
        );
    }

    #[test]
    fn validate_rejects_warn_below_under_urgent_below() {
        let config: Config =
            toml::from_str("[ai]\nenabled = true\nwarn_below = 0.1\nurgent_below = 0.5")
                .expect("ai TOML should deserialize");

        let errors = validate(&config);

        assert!(
            errors.iter().any(|e| e.contains("ai.warn_below")),
            "{errors:?}"
        );
    }

    #[test]
    fn validate_skips_ai_checks_when_disabled() {
        let config: Config = toml::from_str(
            "[ai]\nmode = \"real\"\ninterval_ms = 1000\nwarn_below = 1.5\nurgent_below = 2.0",
        )
        .expect("ai TOML should deserialize");

        let errors = validate(&config);

        assert!(errors.iter().all(|e| !e.contains("ai.")), "{errors:?}");
    }
}

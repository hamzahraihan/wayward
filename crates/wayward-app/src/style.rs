use iced::Color;
use wayward_core::{Config, Level};

/// All colors used by the views, sourced only from the theme (AGENTS.md §5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colors {
    pub background: Color,
    pub foreground: Color,
    pub warn: Color,
    pub urgent: Color,
    pub accent: Color,
    pub muted: Color,
    pub surface: Color,
    pub success: Color,
}

pub fn to_color(rgb: (f32, f32, f32)) -> Color {
    Color::from_rgb(rgb.0, rgb.1, rgb.2)
}

pub fn theme_colors(config: &Config) -> Colors {
    let theme = wayward_core::effective_theme(config);
    let fallback = wayward_core::catppuccin_mocha();

    let color = |value: &str, default: &str| -> Color {
        to_color(
            wayward_core::parse_hex_rgb(value)
                .or_else(|| wayward_core::parse_hex_rgb(default))
                .unwrap_or_else(|| {
                    wayward_core::parse_hex_rgb(default)
                        .expect("catppuccin_mocha fallback must be valid hex")
                }),
        )
    };

    Colors {
        background: color(&theme.background, &fallback.background),
        foreground: color(&theme.foreground, &fallback.foreground),
        warn: color(&theme.warn, &fallback.warn),
        urgent: color(&theme.urgent, &fallback.urgent),
        accent: color(&theme.accent, &fallback.accent),
        muted: color(&theme.muted, &fallback.muted),
        surface: color(&theme.surface, &fallback.surface),
        success: color(&theme.success, &fallback.success),
    }
}

/// Level -> theme color. No thresholds are invented in M3; all successes are Normal.
pub fn level_color(level: Level, colors: &Colors) -> Color {
    match level {
        Level::Normal => colors.foreground,
        Level::Warn => colors.warn,
        Level::Urgent => colors.urgent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_colors_uses_catppuccin_mocha_for_default_config() {
        let colors = theme_colors(&Config::default());

        let expected_background =
            wayward_core::parse_hex_rgb(wayward_core::BACKGROUND).expect("valid constant");
        let expected_foreground =
            wayward_core::parse_hex_rgb(wayward_core::FOREGROUND).expect("valid constant");
        let expected_warn =
            wayward_core::parse_hex_rgb(wayward_core::WARN).expect("valid constant");
        let expected_urgent =
            wayward_core::parse_hex_rgb(wayward_core::URGENT).expect("valid constant");
        let expected_accent =
            wayward_core::parse_hex_rgb(wayward_core::ACCENT).expect("valid constant");
        let expected_muted =
            wayward_core::parse_hex_rgb(wayward_core::MUTED).expect("valid constant");
        let expected_surface =
            wayward_core::parse_hex_rgb(wayward_core::SURFACE).expect("valid constant");
        let expected_success =
            wayward_core::parse_hex_rgb(wayward_core::SUCCESS).expect("valid constant");

        assert_eq!(
            (
                colors.background.r,
                colors.background.g,
                colors.background.b
            ),
            expected_background
        );
        assert_eq!(
            (
                colors.foreground.r,
                colors.foreground.g,
                colors.foreground.b
            ),
            expected_foreground
        );
        assert_eq!((colors.warn.r, colors.warn.g, colors.warn.b), expected_warn);
        assert_eq!(
            (colors.urgent.r, colors.urgent.g, colors.urgent.b),
            expected_urgent
        );
        assert_eq!(
            (colors.accent.r, colors.accent.g, colors.accent.b),
            expected_accent
        );
        assert_eq!(
            (colors.muted.r, colors.muted.g, colors.muted.b),
            expected_muted
        );
        assert_eq!(
            (colors.surface.r, colors.surface.g, colors.surface.b),
            expected_surface
        );
        assert_eq!(
            (colors.success.r, colors.success.g, colors.success.b),
            expected_success
        );
    }

    #[test]
    fn theme_colors_falls_back_for_invalid_theme() {
        let mut config = Config::default();
        config.theme.background.clear();
        config.theme.foreground.clear();
        config.theme.warn.clear();
        config.theme.urgent.clear();
        config.theme.accent.clear();
        config.theme.muted.clear();
        config.theme.surface.clear();
        config.theme.success.clear();

        let colors = theme_colors(&config);
        let defaults = theme_colors(&Config::default());

        assert_eq!(colors, defaults);
    }

    #[test]
    fn level_color_maps_to_theme_colors() {
        let colors = theme_colors(&Config::default());
        assert_eq!(level_color(Level::Normal, &colors), colors.foreground);
        assert_eq!(level_color(Level::Warn, &colors), colors.warn);
        assert_eq!(level_color(Level::Urgent, &colors), colors.urgent);
    }
}

use iced::Color;
use wayward_core::{Config, Level};

/// All colors used by the views, sourced only from the theme (AGENTS.md §5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colors {
    pub background: Color,
    pub foreground: Color,
    pub warn: Color,
    pub urgent: Color,
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
    }

    #[test]
    fn level_color_maps_to_theme_colors() {
        let colors = theme_colors(&Config::default());
        assert_eq!(level_color(Level::Normal, &colors), colors.foreground);
        assert_eq!(level_color(Level::Warn, &colors), colors.warn);
        assert_eq!(level_color(Level::Urgent, &colors), colors.urgent);
    }
}

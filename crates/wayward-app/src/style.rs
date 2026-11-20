use iced::Color;
use wayward_core::Config;

pub fn to_color(rgb: (f32, f32, f32)) -> Color {
    Color::from_rgb(rgb.0, rgb.1, rgb.2)
}

pub fn theme_colors(config: &Config) -> (Color, Color) {
    let theme = wayward_core::effective_theme(config);
    let fallback = wayward_core::catppuccin_mocha();

    let background = wayward_core::parse_hex_rgb(&theme.background)
        .or_else(|| wayward_core::parse_hex_rgb(&fallback.background))
        .expect("catppuccin_mocha background must be valid hex");
    let foreground = wayward_core::parse_hex_rgb(&theme.foreground)
        .or_else(|| wayward_core::parse_hex_rgb(&fallback.foreground))
        .expect("catppuccin_mocha foreground must be valid hex");

    (to_color(background), to_color(foreground))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_colors_uses_catppuccin_mocha_for_default_config() {
        let (background, foreground) = theme_colors(&Config::default());

        let expected_background =
            wayward_core::parse_hex_rgb(wayward_core::BACKGROUND).expect("valid constant");
        let expected_foreground =
            wayward_core::parse_hex_rgb(wayward_core::FOREGROUND).expect("valid constant");

        assert_eq!(
            (background.r, background.g, background.b),
            expected_background
        );
        assert_eq!(
            (foreground.r, foreground.g, foreground.b),
            expected_foreground
        );
    }
}

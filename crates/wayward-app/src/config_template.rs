pub const DEFAULT_CONFIG: &str = include_str!("../../../config/wayward.toml");

#[cfg(test)]
mod tests {
    use super::DEFAULT_CONFIG;

    #[test]
    fn bundled_template_has_catppuccin_mocha_defaults() {
        assert!(DEFAULT_CONFIG.contains("name = \"catppuccin-mocha\""));
        assert!(DEFAULT_CONFIG.contains("background = \"#1e1e2e\""));
    }
}

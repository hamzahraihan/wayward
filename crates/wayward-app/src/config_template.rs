pub const DEFAULT_CONFIG: &str = include_str!("../../../config/wayward.toml");

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
}

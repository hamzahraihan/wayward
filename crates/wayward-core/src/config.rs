use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BarPosition {
    #[default]
    Top,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct BarConfig {
    pub position: BarPosition,
    pub height: u32,
    pub padding: u16,
    pub monitor: String,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            position: BarPosition::Top,
            height: 28,
            padding: 8,
            monitor: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct FontConfig {
    pub family: String,
    pub size: f32,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: "Cascadia Mono".to_owned(),
            size: 12.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub name: String,
    pub background: String,
    pub foreground: String,
    pub accent: String,
    pub muted: String,
    pub warn: String,
    pub urgent: String,
    pub surface: String,
    pub success: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "catppuccin-mocha".to_owned(),
            background: "#1e1e2e".to_owned(),
            foreground: "#cdd6f4".to_owned(),
            accent: "#cba6f7".to_owned(),
            muted: "#6c7086".to_owned(),
            warn: "#f9e2af".to_owned(),
            urgent: "#f38ba8".to_owned(),
            surface: "#313244".to_owned(),
            success: "#a6e3a1".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct LayoutConfig {
    pub left: Vec<String>,
    pub center: Vec<String>,
    pub right: Vec<String>,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            left: vec!["clock".to_owned()],
            center: Vec::new(),
            right: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct ClockConfig {
    pub format: String,
    pub interval_ms: u64,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            format: "%H:%M:%S".to_owned(),
            interval_ms: 1000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct CpuConfig {
    pub interval_ms: u64,
}

impl Default for CpuConfig {
    fn default() -> Self {
        Self { interval_ms: 2000 }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct RamConfig {
    pub interval_ms: u64,
}

impl Default for RamConfig {
    fn default() -> Self {
        Self { interval_ms: 3000 }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct BatteryConfig {
    pub interval_ms: u64,
}

impl Default for BatteryConfig {
    fn default() -> Self {
        Self { interval_ms: 5000 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiMode {
    #[default]
    Mock,
    #[serde(other)]
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    pub enabled: bool,
    pub mode: AiMode,
    pub interval_ms: u64,
    pub warn_below: f32,
    pub urgent_below: f32,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: AiMode::Mock,
            interval_ms: 300_000,
            warn_below: 0.20,
            urgent_below: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct ModulesConfig {
    pub clock: ClockConfig,
    pub cpu: CpuConfig,
    pub ram: RamConfig,
    pub battery: BatteryConfig,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub bar: BarConfig,
    pub font: FontConfig,
    pub theme: ThemeConfig,
    pub layout: LayoutConfig,
    pub modules: ModulesConfig,
    pub ai: AiConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_toml_yields_default_config() {
        let parsed: Config = toml::from_str("").expect("empty TOML should deserialize");
        assert_eq!(parsed, Config::default());
        assert_eq!(parsed.bar.height, 28);
        assert_eq!(parsed.font.size, 12.0);
        assert_eq!(parsed.theme.background, "#1e1e2e");
    }

    #[test]
    fn partial_toml_preserves_other_defaults() {
        let defaults = Config::default();
        let parsed: Config =
            toml::from_str("[bar]\nheight = 40").expect("partial TOML should deserialize");

        assert_eq!(parsed.bar.height, 40);
        assert_eq!(parsed.bar.position, defaults.bar.position);
        assert_eq!(parsed.font.size, defaults.font.size);
        assert_eq!(parsed.theme.background, defaults.theme.background);
        assert_eq!(parsed.layout.left, defaults.layout.left);

        assert_eq!(parsed.bar.position, BarPosition::Top);
        assert_eq!(parsed.font.size, 12.0);
        assert_eq!(parsed.theme.background, "#1e1e2e");
        assert_eq!(parsed.layout.left, vec!["clock"]);

        assert_eq!(parsed.modules.clock.interval_ms, 1000);
        assert_eq!(parsed.modules.cpu.interval_ms, 2000);
        assert_eq!(parsed.modules.ram.interval_ms, 3000);
        assert_eq!(parsed.modules.battery.interval_ms, 5000);
    }

    #[test]
    fn partial_cpu_toml_preserves_other_stat_defaults() {
        let parsed: Config = toml::from_str("[modules.cpu]\ninterval_ms = 4000")
            .expect("partial TOML should deserialize");

        assert_eq!(parsed.modules.cpu.interval_ms, 4000);
        assert_eq!(parsed.modules.ram.interval_ms, 3000);
        assert_eq!(parsed.modules.battery.interval_ms, 5000);
        assert_eq!(parsed.modules.clock.interval_ms, 1000);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let parsed: Config =
            toml::from_str("[bar]\nheight = 30\nunknown_key = 1\n[unknown_section]\nx = true")
                .expect("unknown keys should be ignored");

        assert_eq!(parsed.bar.height, 30);
        assert_eq!(parsed.bar.position, BarPosition::Top);
    }

    #[test]
    fn empty_toml_ai_defaults() {
        let parsed: Config = toml::from_str("").expect("empty TOML should deserialize");
        assert!(!parsed.ai.enabled);
        assert_eq!(parsed.ai.mode, AiMode::Mock);
    }

    #[test]
    fn partial_ai_toml_preserves_defaults() {
        let parsed: Config = toml::from_str("[ai]\nenabled = true\ninterval_ms = 60000")
            .expect("partial ai TOML should deserialize");
        assert!(parsed.ai.enabled);
        assert_eq!(parsed.ai.interval_ms, 60000);
        assert_eq!(parsed.ai.warn_below, 0.20);
    }

    #[test]
    fn empty_toml_yields_disabled_ai_defaults() {
        let parsed: Config = toml::from_str("").expect("empty TOML should deserialize");

        assert!(!parsed.ai.enabled);
        assert_eq!(parsed.ai.mode, AiMode::Mock);
        assert_eq!(parsed.ai.interval_ms, 300_000);
        assert_eq!(parsed.ai.warn_below, 0.20);
        assert_eq!(parsed.ai.urgent_below, 0.0);
    }

    #[test]
    fn ai_toml_overrides_keep_other_ai_defaults() {
        let parsed: Config = toml::from_str("[ai]\nenabled = true\ninterval_ms = 60000")
            .expect("ai TOML should deserialize");

        assert!(parsed.ai.enabled);
        assert_eq!(parsed.ai.interval_ms, 60_000);
        assert_eq!(parsed.ai.mode, AiMode::Mock);
        assert_eq!(parsed.ai.warn_below, 0.20);
        assert_eq!(parsed.ai.urgent_below, 0.0);
    }

    #[test]
    fn unknown_ai_mode_deserializes_to_unsupported() {
        let parsed: Config = toml::from_str("[ai]\nenabled = true\nmode = \"real\"")
            .expect("unknown ai mode should deserialize");

        assert_eq!(parsed.ai.mode, AiMode::Unsupported);
    }
}

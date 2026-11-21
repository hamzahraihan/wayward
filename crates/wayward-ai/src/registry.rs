use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use wayward_core::config::{AiConfig, AiMode};

use crate::claude_code::ClaudeCodeProvider;
use crate::mock::MockProvider;
use crate::opencode::OpenCodeProvider;
use crate::UsageProvider;

pub struct ProviderSpec {
    pub id: String,
    pub interval: Duration,
    pub provider: Box<dyn UsageProvider>,
    rebuild: Arc<dyn Fn() -> Box<dyn UsageProvider> + Send + Sync>,
}

impl fmt::Debug for ProviderSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderSpec")
            .field("id", &self.id)
            .field("interval", &self.interval)
            .finish_non_exhaustive()
    }
}

impl Clone for ProviderSpec {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            interval: self.interval,
            provider: (self.rebuild)(),
            rebuild: Arc::clone(&self.rebuild),
        }
    }
}

pub fn build_providers(config: &AiConfig, home: &Path) -> Vec<ProviderSpec> {
    match config.mode {
        AiMode::Mock => vec![ProviderSpec {
            id: "mock".to_owned(),
            interval: Duration::from_millis(config.interval_ms),
            provider: Box::new(MockProvider::new()),
            rebuild: Arc::new(|| Box::new(MockProvider::new())),
        }],
        AiMode::Real => config
            .providers
            .iter()
            .filter(|entry| entry.enabled)
            .filter_map(|entry| {
                let interval = Duration::from_millis(entry.interval_ms);
                let limit = entry.limit;
                let reset_after_secs = entry.reset_after_secs;
                let reset_anchor_secs = entry.reset_anchor_secs;
                match entry.id.as_str() {
                    "claude-code" => {
                        let root = entry.root.clone().unwrap_or_else(|| home.join(".claude"));
                        Some(ProviderSpec {
                            id: entry.id.clone(),
                            interval,
                            provider: Box::new(ClaudeCodeProvider::new(
                                root.clone(),
                                limit,
                                reset_after_secs,
                                reset_anchor_secs,
                            )),
                            rebuild: Arc::new(move || {
                                Box::new(ClaudeCodeProvider::new(
                                    root.clone(),
                                    limit,
                                    reset_after_secs,
                                    reset_anchor_secs,
                                ))
                            }),
                        })
                    }
                    "opencode" => {
                        let db_path = entry
                            .root
                            .clone()
                            .unwrap_or_else(|| home.join(".local/share/opencode/opencode.db"));
                        Some(ProviderSpec {
                            id: entry.id.clone(),
                            interval,
                            provider: Box::new(OpenCodeProvider::new(
                                db_path.clone(),
                                limit,
                                reset_after_secs,
                                reset_anchor_secs,
                            )),
                            rebuild: Arc::new(move || {
                                Box::new(OpenCodeProvider::new(
                                    db_path.clone(),
                                    limit,
                                    reset_after_secs,
                                    reset_anchor_secs,
                                ))
                            }),
                        })
                    }
                    _ => None,
                }
            })
            .collect(),
        AiMode::Unsupported => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use wayward_core::config::{AiConfig, AiMode, AiProviderConfig};

    use super::*;

    fn real_config(providers: Vec<AiProviderConfig>) -> AiConfig {
        AiConfig {
            enabled: true,
            mode: AiMode::Real,
            interval_ms: 300_000,
            warn_below: 0.2,
            urgent_below: 0.0,
            experimental: false,
            providers,
        }
    }

    fn provider_config(id: &str, enabled: bool, interval_ms: u64) -> AiProviderConfig {
        AiProviderConfig {
            id: id.to_owned(),
            enabled,
            interval_ms,
            root: None,
            limit: 500.0,
            reset_after_secs: 3_600,
            reset_anchor_secs: None,
        }
    }

    #[test]
    fn mock_mode_returns_only_the_mock_provider() {
        let config = AiConfig {
            mode: AiMode::Mock,
            interval_ms: 4_321,
            ..AiConfig::default()
        };

        let specs = build_providers(&config, Path::new("home"));

        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].id, "mock");
        assert_eq!(specs[0].provider.id(), "mock");
        assert_eq!(specs[0].interval, Duration::from_millis(4_321));
    }

    #[test]
    fn real_mode_returns_only_enabled_known_providers() {
        let config = real_config(vec![
            provider_config("claude-code", true, 1_000),
            provider_config("opencode", false, 2_000),
            provider_config("official-api", true, 3_000),
            provider_config("opencode", true, 4_000),
        ]);

        let specs = build_providers(&config, Path::new("home"));

        assert_eq!(
            specs
                .iter()
                .map(|spec| spec.id.as_str())
                .collect::<Vec<_>>(),
            vec!["claude-code", "opencode"]
        );
        assert_eq!(specs[0].provider.id(), "claude-code");
        assert_eq!(specs[0].interval, Duration::from_millis(1_000));
        assert_eq!(specs[1].provider.id(), "opencode");
        assert_eq!(specs[1].interval, Duration::from_millis(4_000));
    }

    #[test]
    fn unsupported_mode_returns_no_providers() {
        let config = AiConfig {
            mode: AiMode::Unsupported,
            ..AiConfig::default()
        };

        assert!(build_providers(&config, Path::new("home")).is_empty());
    }

    #[test]
    fn configured_root_overrides_default_data_location() {
        use std::time::SystemTime;

        use crate::test_support::{block_on, TempPath};

        let dir = TempPath::new("registry-root");
        let projects = dir.path().join("projects");
        std::fs::create_dir_all(&projects).expect("create projects dir");
        let timestamp = jiff::Timestamp::try_from(SystemTime::now())
            .expect("convert now to timestamp")
            .to_string();
        let line = format!(
            r#"{{"type":"assistant","timestamp":"{timestamp}","message":{{"usage":{{"input_tokens":7,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}}}}"#
        );
        std::fs::write(projects.join("session.jsonl"), format!("{line}\n"))
            .expect("write jsonl fixture");

        let mut entry = provider_config("claude-code", true, 60_000);
        entry.root = Some(dir.path().to_path_buf());
        let mut specs = build_providers(&real_config(vec![entry]), Path::new("home"));

        assert_eq!(specs.len(), 1);
        let usage = block_on(specs[0].provider.fetch()).expect("fetch from configured root");
        assert_eq!(usage.meters[0].used, 7.0);
    }
}

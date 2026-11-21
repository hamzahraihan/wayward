use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use wayward_core::{
    config_file, ensure_default_config, ensure_default_theme, resolve_config_dir, resolve_theme,
    theme_file, validate, Config, ThemeFile,
};

#[derive(Debug)]
pub struct LoadedConfig {
    pub dir: PathBuf,
    pub path: PathBuf,
    pub config: Config,
    pub created: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReloadOutcome {
    Unchanged,
    Loaded {
        config: Box<Config>,
        warnings: Vec<String>,
    },
    Invalid {
        error: String,
    },
}

/// FNV-1a: stable, dependency-free content hash for self-write suppression.
fn content_hash(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn known_keys() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "bar",
        "font",
        "theme",
        "layout",
        "modules",
        "position",
        "height",
        "padding",
        "monitor",
        "family",
        "size",
        "name",
        "background",
        "foreground",
        "accent",
        "muted",
        "warn",
        "urgent",
        "surface",
        "success",
        "left",
        "center",
        "right",
        "clock",
        "cpu",
        "ram",
        "battery",
        "format",
        "interval_ms",
        "ai",
        "enabled",
        "mode",
        "warn_below",
        "urgent_below",
        "experimental",
        "providers",
        "id",
        "root",
        "limit",
        "reset_after_secs",
    ])
}

fn unknown_key_warnings(value: &toml::Value) -> Vec<String> {
    let known = known_keys();
    let mut warnings = Vec::new();
    let mut check = |table: &toml::Table, prefix: &str| {
        for key in table.keys() {
            if !known.contains(key.as_str()) {
                warnings.push(format!("unknown config key `{prefix}{key}` (ignored)"));
            }
        }
    };
    let Some(root) = value.as_table() else {
        return warnings;
    };
    check(root, "");
    for section in [
        "bar", "font", "theme", "layout", "modules", "clock", "cpu", "ram", "battery", "ai",
    ] {
        if let Some(table) = root.get(section).and_then(toml::Value::as_table) {
            check(table, &format!("{section}."));
        }
    }
    if let Some(providers) = root
        .get("ai")
        .and_then(toml::Value::as_table)
        .and_then(|ai| ai.get("providers"))
        .and_then(toml::Value::as_array)
    {
        for (index, provider) in providers.iter().enumerate() {
            if let Some(table) = provider.as_table() {
                check(table, &format!("ai.providers[{index}]."));
            }
        }
    }
    warnings
}

/// Resolve the named theme file selected by `config.theme.name`.
/// Returns the raw bytes and parsed file so callers can hash and merge consistently.
fn named_theme_file(config_dir: &Path, config: &Config) -> (Option<Vec<u8>>, Option<ThemeFile>) {
    let name = config.theme.name.as_str();
    if name.is_empty() {
        return (None, None);
    }
    let Some(path) = theme_file(config_dir, name) else {
        return (None, None);
    };
    let Ok(bytes) = fs::read(path) else {
        return (None, None);
    };
    let parsed = std::str::from_utf8(&bytes)
        .ok()
        .and_then(|text| toml::from_str::<ThemeFile>(text).ok());
    (Some(bytes), parsed)
}

/// Holds the last-good config bytes so a broken edit never blanks the bar.
#[derive(Debug)]
pub struct ReloadContext {
    config_path: PathBuf,
    config_hash: Option<u64>,
    theme_hash: Option<u64>,
}

impl ReloadContext {
    pub fn new(config_path: PathBuf) -> Self {
        Self {
            config_path,
            config_hash: None,
            theme_hash: None,
        }
    }

    /// Seed from disk after first-run creation so an identical write is Unchanged.
    pub fn seed_from_disk(&mut self) {
        if let Ok(bytes) = fs::read(&self.config_path) {
            self.config_hash = Some(content_hash(&bytes));
        }
        if let Some(current_theme) = self.current_theme_name_from_disk() {
            let config_dir = self.config_path.parent().unwrap_or(Path::new("."));
            if let Some(path) = theme_file(config_dir, &current_theme) {
                if let Ok(bytes) = fs::read(path) {
                    self.theme_hash = Some(content_hash(&bytes));
                }
            }
        }
    }

    fn current_theme_name_from_disk(&self) -> Option<String> {
        let text = fs::read_to_string(&self.config_path).ok()?;
        let config: Config = toml::from_str(&text).ok()?;
        let name = config.theme.name;
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }

    /// Re-read config + named theme. Invalid input returns Invalid and leaves hashes untouched.
    pub fn reload(&mut self) -> ReloadOutcome {
        let config_bytes = match fs::read(&self.config_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return ReloadOutcome::Invalid {
                    error: format!("cannot read {}: {error}", self.config_path.display()),
                }
            }
        };
        let new_config_hash = content_hash(&config_bytes);

        let text = match String::from_utf8(config_bytes.clone()) {
            Ok(text) => text,
            Err(error) => {
                return ReloadOutcome::Invalid {
                    error: format!("config is not UTF-8: {error}"),
                }
            }
        };
        let config: Config = match toml::from_str(&text) {
            Ok(config) => config,
            Err(error) => {
                return ReloadOutcome::Invalid {
                    error: format!("failed to parse {}: {error}", self.config_path.display()),
                }
            }
        };

        let mut warnings = Vec::new();
        if let Ok(value) = toml::from_str::<toml::Value>(&text) {
            warnings.extend(unknown_key_warnings(&value));
        }
        for message in validate(&config) {
            warnings.push(message);
        }
        if !validate(&config).is_empty() {
            return ReloadOutcome::Invalid {
                error: format!(
                    "invalid config in {}: validation failed",
                    self.config_path.display()
                ),
            };
        }
        let config_dir = self.config_path.parent().unwrap_or(Path::new("."));
        let (theme_bytes, theme_file_parsed) = named_theme_file(config_dir, &config);
        let new_theme_hash = theme_bytes.as_deref().map(content_hash);

        if self.config_hash == Some(new_config_hash) && self.theme_hash == new_theme_hash {
            return ReloadOutcome::Unchanged;
        }

        let mut config = config;
        config.theme = resolve_theme(&config.theme, theme_file_parsed.as_ref());

        self.config_hash = Some(new_config_hash);
        self.theme_hash = new_theme_hash;
        ReloadOutcome::Loaded {
            config: Box::new(config),
            warnings,
        }
    }
}

pub fn load_config_from(
    home: Option<&Path>,
    override_dir: Option<&OsStr>,
    template: &str,
) -> anyhow::Result<LoadedConfig> {
    let dir = resolve_config_dir(home, override_dir).ok_or_else(|| {
        anyhow!(
            "cannot resolve config directory: no home directory and no WAYWARD_CONFIG_DIR override"
        )
    })?;

    let created = ensure_default_config(&dir, template)
        .with_context(|| format!("failed to create default config in {}", dir.display()))?;
    let themes_created = ensure_default_theme(
        &dir,
        crate::config_template::DEFAULT_THEME_NAME,
        crate::config_template::DEFAULT_THEME,
    )
    .with_context(|| format!("failed to create default theme in {}", dir.display()))?;
    let created = created || themes_created;

    let path = config_file(&dir);
    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;

    match toml::from_str::<Config>(&text) {
        Ok(mut config) => {
            let (_, theme_file_parsed) = named_theme_file(&dir, &config);
            config.theme = resolve_theme(&config.theme, theme_file_parsed.as_ref());
            Ok(LoadedConfig {
                dir,
                path,
                config,
                created,
                error: None,
            })
        }
        Err(error) => {
            let message = format!("failed to parse config file {}: {error}", path.display());
            tracing::warn!("{message}");
            Ok(LoadedConfig {
                dir,
                path,
                config: Config::default(),
                created,
                error: Some(message),
            })
        }
    }
}

pub fn load_config() -> anyhow::Result<LoadedConfig> {
    let home = dirs::home_dir();
    let override_dir = std::env::var_os("WAYWARD_CONFIG_DIR");
    load_config_from(
        home.as_deref(),
        override_dir.as_deref(),
        crate::config_template::DEFAULT_CONFIG,
    )
}

pub fn validate_or_warn(config: &Config) {
    for message in validate(config) {
        tracing::warn!("{message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use wayward_core::themes_dir;

    const TEMPLATE: &str = "# wayward config\n[bar]\nheight = 28\n";

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("wayward-app-{label}-{}-{seq}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create unique temp dir");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn context_at(dir: &Path) -> ReloadContext {
        ReloadContext::new(config_file(dir))
    }

    #[test]
    fn override_dir_wins_over_home() {
        let temp = TempDir::new("override");
        let home = Path::new("/nonexistent/wayward-home");
        let loaded = load_config_from(Some(home), Some(temp.0.as_os_str()), TEMPLATE)
            .expect("override dir should resolve");
        assert_eq!(loaded.dir, temp.0);
        assert_eq!(loaded.path, temp.0.join("wayward.toml"));
        assert!(loaded.created);
        assert!(loaded.error.is_none());
    }

    #[test]
    fn first_run_creates_config_and_theme() {
        let temp = TempDir::new("first-run");
        let loaded = load_config_from(None, Some(temp.0.as_os_str()), TEMPLATE)
            .expect("first run should create config");
        assert!(loaded.created);
        assert_eq!(
            fs::read(&loaded.path).expect("read created config"),
            TEMPLATE.as_bytes()
        );
        let theme_path = theme_file(&temp.0, crate::config_template::DEFAULT_THEME_NAME)
            .expect("valid theme name");
        assert_eq!(
            fs::read(&theme_path).expect("read created theme"),
            crate::config_template::DEFAULT_THEME.as_bytes()
        );
    }

    #[test]
    fn existing_config_is_not_overwritten() {
        let temp = TempDir::new("no-overwrite");
        let original = b"# user comment: keep me\n[bar]\nheight = 40\n";
        fs::write(temp.0.join("wayward.toml"), original).expect("write existing config");
        let loaded = load_config_from(None, Some(temp.0.as_os_str()), TEMPLATE)
            .expect("existing config should load");
        assert_eq!(fs::read(&loaded.path).expect("read config"), original);
        assert_eq!(loaded.config.bar.height, 40);
    }

    #[test]
    fn malformed_toml_falls_back_to_default_with_error() {
        let temp = TempDir::new("malformed");
        fs::write(temp.0.join("wayward.toml"), "garbage = [").expect("write malformed config");
        let loaded = load_config_from(None, Some(temp.0.as_os_str()), TEMPLATE)
            .expect("malformed TOML must not fail startup");
        assert_eq!(loaded.config, Config::default());
        let error = loaded.error.expect("parse error should be captured");
        assert!(
            error.contains("wayward.toml"),
            "error should name the file: {error}"
        );
    }

    #[test]
    fn missing_home_and_override_returns_error() {
        let result = load_config_from(None, None, TEMPLATE);
        assert!(result
            .expect_err("expected error")
            .to_string()
            .contains("config"));
    }

    #[test]
    fn identical_bytes_are_unchanged() {
        let temp = TempDir::new("identical");
        fs::write(temp.0.join("wayward.toml"), TEMPLATE).expect("write config");
        let mut context = context_at(&temp.0);
        context.seed_from_disk();
        assert_eq!(context.reload(), ReloadOutcome::Unchanged);
    }

    #[test]
    fn valid_change_is_loaded() {
        let temp = TempDir::new("valid-change");
        fs::write(
            temp.0.join("wayward.toml"),
            "[bar]\nheight = 32\n[modules.clock]\nformat = \"%Y-%m-%d\"\n",
        )
        .expect("write config");
        let mut context = context_at(&temp.0);
        match context.reload() {
            ReloadOutcome::Loaded { config, .. } => {
                assert_eq!(config.bar.height, 32);
                assert_eq!(config.modules.clock.format, "%Y-%m-%d");
            }
            other => panic!("expected Loaded, got {other:?}"),
        }
    }

    #[test]
    fn malformed_edit_is_invalid_and_keeps_last_good() {
        let temp = TempDir::new("bad-edit");
        let path = temp.0.join("wayward.toml");
        fs::write(&path, TEMPLATE).expect("write baseline config");
        let mut context = context_at(&temp.0);
        context.seed_from_disk();
        fs::write(&path, "[bar]\nheight = 32\n").expect("write good config");
        assert!(matches!(context.reload(), ReloadOutcome::Loaded { .. }));
        fs::write(&path, "garbage = [").expect("write bad config");
        assert!(matches!(context.reload(), ReloadOutcome::Invalid { .. }));
        fs::write(&path, "[bar]\nheight = 32\n").expect("restore good config");
        assert_eq!(
            context.reload(),
            ReloadOutcome::Unchanged,
            "restoring the last-good bytes needs no re-apply"
        );
    }

    #[test]
    fn validation_failure_is_invalid() {
        let temp = TempDir::new("validation");
        fs::write(temp.0.join("wayward.toml"), "[bar]\nheight = 4\n").expect("write config");
        let mut context = context_at(&temp.0);
        assert!(matches!(context.reload(), ReloadOutcome::Invalid { .. }));
    }

    #[test]
    fn unknown_key_is_warning_not_error() {
        let temp = TempDir::new("unknown-key");
        fs::write(
            temp.0.join("wayward.toml"),
            "[bar]\nheight = 28\nfuture_flag = true\n",
        )
        .expect("write config");
        let mut context = context_at(&temp.0);
        match context.reload() {
            ReloadOutcome::Loaded { warnings, .. } => {
                assert!(warnings
                    .iter()
                    .any(|warning| warning.contains("future_flag")));
            }
            other => panic!("expected Loaded, got {other:?}"),
        }
    }

    #[test]
    fn named_theme_overrides_inline() {
        let temp = TempDir::new("named-theme");
        fs::write(
            temp.0.join("wayward.toml"),
            "[theme]\nname = \"qa-theme\"\nbackground = \"#111111\"\n",
        )
        .expect("write config");
        let themes = themes_dir(&temp.0);
        fs::create_dir_all(&themes).expect("create themes dir");
        fs::write(
            theme_file(&temp.0, "qa-theme").expect("valid name"),
            "background = \"#010203\"\n",
        )
        .expect("write theme");
        let mut context = context_at(&temp.0);
        match context.reload() {
            ReloadOutcome::Loaded { config, .. } => {
                assert_eq!(config.theme.background, "#010203");
                assert_eq!(config.theme.foreground, wayward_core::FOREGROUND);
            }
            other => panic!("expected Loaded, got {other:?}"),
        }
    }

    #[test]
    fn theme_switch_uses_new_config_theme_name() {
        let temp = TempDir::new("theme-switch");
        let cfg = temp.0.join("wayward.toml");
        let themes = themes_dir(&temp.0);
        fs::create_dir_all(&themes).expect("create themes dir");

        fs::write(&cfg, "[theme]\nname = \"first\"\n").expect("write baseline config");
        fs::write(
            theme_file(&temp.0, "first").expect("valid"),
            "background = \"#111111\"\n",
        )
        .expect("write first theme");
        let mut context = ReloadContext::new(cfg.clone());
        context.seed_from_disk();
        assert_eq!(context.reload(), ReloadOutcome::Unchanged);

        // Switch to a different named theme. reload() must derive the name from
        // the freshly parsed config, not from the previously applied theme.
        fs::write(&cfg, "[theme]\nname = \"second\"\n").expect("write switched config");
        fs::write(
            theme_file(&temp.0, "second").expect("valid"),
            "background = \"#222222\"\n",
        )
        .expect("write second theme");
        match context.reload() {
            ReloadOutcome::Loaded { config, .. } => {
                assert_eq!(config.theme.name, "second");
                assert_eq!(config.theme.background, "#222222");
            }
            other => panic!("expected Loaded, got {other:?}"),
        }
    }

    #[test]
    fn missing_named_theme_falls_back_to_inline() {
        let temp = TempDir::new("missing-theme");
        fs::write(
            temp.0.join("wayward.toml"),
            "[theme]\nname = \"does-not-exist\"\nbackground = \"#111111\"\n",
        )
        .expect("write config");
        let mut context = context_at(&temp.0);
        match context.reload() {
            ReloadOutcome::Loaded { config, .. } => {
                assert_eq!(config.theme.background, "#111111");
            }
            other => panic!("expected Loaded, got {other:?}"),
        }
    }

    #[test]
    fn ai_section_loads_without_unknown_warnings() {
        let temp = TempDir::new("ai-section");
        fs::write(
            temp.0.join("wayward.toml"),
            "[ai]\nenabled = true\nmode = \"mock\"\n",
        )
        .expect("write config");
        let mut context = context_at(&temp.0);
        match context.reload() {
            ReloadOutcome::Loaded { config, warnings } => {
                assert!(config.ai.enabled, "ai.enabled should be true");
                assert_eq!(config.ai.mode, wayward_core::AiMode::Mock);
                assert!(
                    !warnings.iter().any(|w| {
                        w.contains("ai.") || w.contains("enabled") || w.contains("`ai`")
                    }),
                    "unexpected ai warning: {warnings:?}"
                );
            }
            other => panic!("expected Loaded, got {other:?}"),
        }
    }

    #[test]
    fn unknown_ai_key_is_warning() {
        let value: toml::Value = toml::from_str("[ai]\nbogus = 1\n").expect("parse TOML value");
        let warnings = unknown_key_warnings(&value);
        assert!(
            warnings.iter().any(|w| w.contains("ai.bogus")),
            "expected ai.bogus warning, got {warnings:?}"
        );
    }

    #[test]
    fn ai_hot_reload_flip() {
        let temp = TempDir::new("ai-flip");
        let path = temp.0.join("wayward.toml");
        let disabled = b"[ai]\nenabled = false\n";
        let enabled = b"[ai]\nenabled = true\n";
        fs::write(&path, disabled).expect("write disabled config");
        let mut context = context_at(&temp.0);
        context.seed_from_disk();
        assert_eq!(context.reload(), ReloadOutcome::Unchanged);

        fs::write(&path, enabled).expect("write enabled config");
        match context.reload() {
            ReloadOutcome::Loaded { config, .. } => assert!(config.ai.enabled),
            other => panic!("expected Loaded, got {other:?}"),
        }

        fs::write(&path, enabled).expect("rewrite identical bytes");
        assert_eq!(context.reload(), ReloadOutcome::Unchanged);
    }

    #[test]
    fn new_ai_keys_are_not_unknown_warnings() {
        let temp = TempDir::new("ai-new-keys");
        fs::write(
            temp.0.join("wayward.toml"),
            "[ai]\nmode = \"mock\"\nexperimental = true\n\
             [[ai.providers]]\nid = \"claude-code\"\nenabled = false\ninterval_ms = 300000\n\
             root = \"/tmp/claude\"\nlimit = 1000000.0\nreset_after_secs = 18000\n",
        )
        .expect("write config");
        let mut context = context_at(&temp.0);

        match context.reload() {
            ReloadOutcome::Loaded { config, warnings } => {
                assert!(config.ai.experimental);
                assert_eq!(config.ai.providers.len(), 1);
                assert_eq!(config.ai.providers[0].id, "claude-code");
                for key in [
                    "experimental",
                    "providers",
                    "id",
                    "root",
                    "limit",
                    "reset_after_secs",
                ] {
                    assert!(
                        !warnings.iter().any(|warning| warning.contains(key)),
                        "unexpected {key} warning: {warnings:?}"
                    );
                }
            }
            other => panic!("expected Loaded, got {other:?}"),
        }
    }

    #[test]
    fn unknown_provider_key_is_warning() {
        let value: toml::Value =
            toml::from_str("[[ai.providers]]\nid = \"claude-code\"\nbogus = 1\n")
                .expect("parse TOML value");
        let warnings = unknown_key_warnings(&value);
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("ai.providers[0].bogus")),
            "expected provider warning, got {warnings:?}"
        );
    }

    #[test]
    fn malformed_provider_config_keeps_last_good() {
        let temp = TempDir::new("bad-provider");
        let path = temp.0.join("wayward.toml");
        let baseline = b"[ai]\nenabled = false\n";
        fs::write(&path, baseline).expect("write baseline config");
        let mut context = context_at(&temp.0);
        context.seed_from_disk();

        fs::write(
            &path,
            "[ai]\nenabled = false\n[[ai.providers]]\nid = \"claude-code\"\ninterval_ms = \"oops\"\n",
        )
        .expect("write malformed provider config");
        assert!(matches!(context.reload(), ReloadOutcome::Invalid { .. }));

        fs::write(&path, baseline).expect("restore baseline config");
        assert_eq!(
            context.reload(),
            ReloadOutcome::Unchanged,
            "restoring the last-good bytes needs no re-apply"
        );
    }
}

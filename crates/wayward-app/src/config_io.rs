use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use wayward_core::{config_file, ensure_default_config, resolve_config_dir, validate, Config};

#[derive(Debug)]
pub struct LoadedConfig {
    pub dir: PathBuf,
    pub path: PathBuf,
    pub config: Config,
    pub created: bool,
    pub error: Option<String>,
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

    let path = config_file(&dir);
    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;

    match toml::from_str::<Config>(&text) {
        Ok(config) => Ok(LoadedConfig {
            dir,
            path,
            config,
            created,
            error: None,
        }),
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
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

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
    fn first_run_creates_config_with_exact_template_bytes() {
        let temp = TempDir::new("first-run");

        let loaded = load_config_from(None, Some(temp.0.as_os_str()), TEMPLATE)
            .expect("first run should create config");

        assert!(loaded.created);
        assert_eq!(
            fs::read(&loaded.path).expect("read created config"),
            TEMPLATE.as_bytes()
        );
        assert_eq!(loaded.config.bar.height, 28);
        assert!(loaded.error.is_none());
    }

    #[test]
    fn existing_config_is_not_overwritten() {
        let temp = TempDir::new("no-overwrite");
        let original = b"# user comment: keep me\n[bar]\nheight = 40\n";
        fs::write(temp.0.join("wayward.toml"), original).expect("write existing config");

        let loaded = load_config_from(None, Some(temp.0.as_os_str()), TEMPLATE)
            .expect("existing config should load");

        assert!(!loaded.created);
        assert_eq!(fs::read(&loaded.path).expect("read config"), original);
        assert_eq!(loaded.config.bar.height, 40);
        assert!(loaded.error.is_none());
    }

    #[test]
    fn malformed_toml_falls_back_to_default_with_error() {
        let temp = TempDir::new("malformed");
        fs::write(temp.0.join("wayward.toml"), "garbage = [").expect("write malformed config");

        let loaded = load_config_from(None, Some(temp.0.as_os_str()), TEMPLATE)
            .expect("malformed TOML must not fail startup");

        assert!(!loaded.created);
        assert_eq!(loaded.config, Config::default());
        let error = loaded.error.expect("parse error should be captured");
        assert!(
            error.contains("wayward.toml"),
            "error should name the file: {error}"
        );
        assert!(!error.is_empty());
    }

    #[test]
    fn missing_home_and_override_returns_error() {
        let result = load_config_from(None, None, TEMPLATE);

        assert!(result
            .expect_err("expected error")
            .to_string()
            .contains("config"));
    }
}

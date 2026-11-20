use std::ffi::OsStr;
use std::fs::{create_dir_all, OpenOptions};
use std::io::{ErrorKind, Result, Write};
use std::path::{Path, PathBuf};

pub const CONFIG_DIR_SEGMENTS: &str = ".config/.wayward";
pub const CONFIG_FILE_NAME: &str = "wayward.toml";

pub fn resolve_config_dir(home: Option<&Path>, override_dir: Option<&OsStr>) -> Option<PathBuf> {
    match override_dir {
        Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
        _ => home.map(|home| home.join(CONFIG_DIR_SEGMENTS)),
    }
}

pub fn config_file(dir: &Path) -> PathBuf {
    dir.join(CONFIG_FILE_NAME)
}

pub fn ensure_default_config(dir: &Path, template: &str) -> Result<bool> {
    create_dir_all(dir)?;

    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(config_file(dir))
    {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => return Ok(false),
        Err(error) => return Err(error),
    };
    file.write_all(template.as_bytes())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("wayward-core-{label}-{}-{seq}", std::process::id()));
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
    fn resolve_config_dir_override_wins_over_home() {
        let home = Path::new("/home/test-user");
        let override_dir = OsStr::new("/custom/wayward-config");

        assert_eq!(
            resolve_config_dir(Some(home), Some(override_dir)),
            Some(PathBuf::from(override_dir))
        );
        assert_eq!(
            resolve_config_dir(None, Some(override_dir)),
            Some(PathBuf::from(override_dir))
        );
    }

    #[test]
    fn resolve_config_dir_ignores_empty_override() {
        let home = Path::new("/home/test-user");

        assert_eq!(
            resolve_config_dir(Some(home), Some(OsStr::new(""))),
            Some(home.join(CONFIG_DIR_SEGMENTS))
        );
    }

    #[test]
    fn resolve_config_dir_uses_home_when_no_override() {
        let home = Path::new("/home/test-user");

        assert_eq!(
            resolve_config_dir(Some(home), None),
            Some(home.join(".config").join(".wayward"))
        );
    }

    #[test]
    fn resolve_config_dir_returns_none_without_inputs() {
        assert_eq!(resolve_config_dir(None, None), None);
        assert_eq!(resolve_config_dir(None, Some(OsStr::new(""))), None);
    }

    #[test]
    fn config_file_joins_file_name() {
        let dir = Path::new("/some/config/dir");

        assert_eq!(config_file(dir), dir.join("wayward.toml"));
    }

    #[test]
    fn ensure_default_config_creates_file_with_exact_template() {
        let temp = TempDir::new("create");
        let dir = temp.0.join("nested").join("config");
        let template = "# wayward config\n[bar]\nheight = 28\n";

        assert!(ensure_default_config(&dir, template).expect("create config"));
        assert_eq!(
            fs::read(config_file(&dir)).expect("read config"),
            template.as_bytes()
        );
    }

    #[test]
    fn ensure_default_config_does_not_overwrite_existing_file() {
        let temp = TempDir::new("no-overwrite");
        let dir = temp.0.join("config");
        fs::create_dir_all(&dir).expect("create config dir");
        let original = b"# user comment: keep me\n[bar]\nheight = 40\n";
        fs::write(config_file(&dir), original).expect("write existing config");

        assert!(!ensure_default_config(&dir, "# default template\n")
            .expect("existing config is not an error"));
        assert_eq!(fs::read(config_file(&dir)).expect("read config"), original);
    }
}

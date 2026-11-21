use std::ffi::OsStr;
use std::fs::{create_dir_all, OpenOptions};
use std::io::{Error, ErrorKind, Result, Write};
use std::path::{Component, Path, PathBuf};

pub const CONFIG_DIR_SEGMENTS: &str = ".config/.wayward";
pub const CONFIG_FILE_NAME: &str = "wayward.toml";
pub const THEMES_DIR_NAME: &str = "themes";
pub const DEFAULT_THEME_NAME: &str = "catppuccin-mocha";

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

pub fn themes_dir(dir: &Path) -> PathBuf {
    dir.join(THEMES_DIR_NAME)
}

pub fn theme_file(dir: &Path, name: &str) -> Option<PathBuf> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return None;
    }

    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Some(themes_dir(dir).join(format!("{name}.toml"))),
        _ => None,
    }
}

pub fn ensure_default_theme(dir: &Path, name: &str, template: &str) -> Result<bool> {
    create_dir_all(themes_dir(dir))?;

    let path = theme_file(dir, name).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            "theme name is not a single path component",
        )
    })?;

    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
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

    #[test]
    fn themes_dir_joins_themes_name() {
        let dir = Path::new("/some/config/dir");

        assert_eq!(themes_dir(dir), dir.join("themes"));
    }

    #[test]
    fn theme_file_builds_exact_path_for_valid_name() {
        let dir = Path::new("/some/config/dir");

        assert_eq!(
            theme_file(dir, "catppuccin-mocha"),
            Some(dir.join("themes").join("catppuccin-mocha.toml"))
        );
    }

    #[test]
    fn theme_file_rejects_invalid_names() {
        let dir = Path::new("/some/config/dir");

        for name in [
            "",
            "/",
            "\\",
            "..",
            ".",
            "a/b",
            "a\\b",
            "a..b",
            "../secret",
            "foo/../bar",
            "foo\\..\\bar",
        ] {
            assert_eq!(theme_file(dir, name), None, "expected None for {name:?}");
        }
    }

    #[test]
    fn ensure_default_theme_creates_file_with_exact_bytes() {
        let temp = TempDir::new("theme-create");
        let dir = temp.0.join("nested").join("config");
        let template = "# theme\nname = \"x\"\n";

        assert!(ensure_default_theme(&dir, "x", template).expect("create theme"));
        assert_eq!(
            fs::read(theme_file(&dir, "x").expect("valid name")).expect("read theme"),
            template.as_bytes()
        );
    }

    #[test]
    fn ensure_default_theme_does_not_overwrite_existing_file() {
        let temp = TempDir::new("theme-no-overwrite");
        let dir = temp.0.join("config");
        let themes = themes_dir(&dir);
        fs::create_dir_all(&themes).expect("create themes dir");
        let original = b"# user comment: keep me\nname = \"custom\"\n";
        fs::write(themes.join("custom.toml"), original).expect("write existing theme");

        assert!(!ensure_default_theme(&dir, "custom", "# default\n")
            .expect("existing theme is not an error"));
        assert_eq!(
            fs::read(themes.join("custom.toml")).expect("read theme"),
            original
        );
    }

    #[test]
    fn ensure_default_theme_create_new_does_not_truncate() {
        let temp = TempDir::new("theme-no-truncate");
        let dir = temp.0.join("config");
        let themes = themes_dir(&dir);
        fs::create_dir_all(&themes).expect("create themes dir");
        let original = b"# a much longer pre-existing theme file\nname = \"keep\"\nbackground = \"#000000\"\nforeground = \"#ffffff\"\n";
        fs::write(themes.join("keep.toml"), original).expect("write existing theme");

        assert!(!ensure_default_theme(&dir, "keep", "short\n").expect("existing theme"));
        assert_eq!(
            fs::read(themes.join("keep.toml")).expect("read theme"),
            original
        );
    }
}

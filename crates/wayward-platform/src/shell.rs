use std::path::Path;
use std::process::Command;

/// Open `path` in Windows Explorer.
///
/// # Errors
/// Returns the underlying `std::io::Error` if Explorer cannot be spawned.
pub fn open_in_explorer(path: &Path) -> std::io::Result<()> {
    Command::new("explorer").arg(path).spawn().map(|_| ())
}

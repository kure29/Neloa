use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

/// Replaces `path` through a synced temporary file in the same directory, so a
/// crash or power loss leaves either the previous or the new contents, never a
/// truncated file.
pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

/// Moves an unreadable data file aside, keeping it for diagnosis, so the
/// caller can continue with defaults instead of failing on every launch.
pub(crate) fn quarantine_corrupt_file(path: &Path) -> io::Result<PathBuf> {
    let backup = path.with_extension(format!("corrupt-{}.json", uuid::Uuid::new_v4()));
    fs::rename(path, &backup)?;
    Ok(backup)
}

/// Settings are user preferences, not security state: losing them must never
/// stop the app from starting. Keep the bad file for diagnosis and let the
/// caller fall back to defaults; the next save writes a fresh file.
pub(crate) fn set_aside_corrupt_settings(path: &Path, label: &str, error: &str) {
    match quarantine_corrupt_file(path) {
        Ok(backup) => eprintln!(
            "{label}已损坏（{error}），已保留至 {}，将使用默认设置",
            backup.display()
        ),
        Err(rename_error) => {
            eprintln!("{label}已损坏（{error}），且无法移走（{rename_error}），将使用默认设置")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_creates_and_replaces_contents() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested").join("settings.json");
        write_atomically(&path, b"first").unwrap();
        write_atomically(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn quarantine_preserves_the_corrupt_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, b"{truncated").unwrap();
        let backup = quarantine_corrupt_file(&path).unwrap();
        assert!(!path.exists());
        assert!(backup.to_string_lossy().contains("corrupt-"));
        assert_eq!(fs::read(backup).unwrap(), b"{truncated");
    }
}

//! Centralized, crash-safe writer for `config.json5`.
//!
//! Several independent paths mutate the operator config (blacklist edits,
//! player-head version refresh, generated Cofl session). The Discord gateway
//! dispatches each interaction on its own task, so two async writers can run
//! concurrently. Two protections are layered here:
//!
//! 1. Every write lands in a process-unique temp file in the same directory
//!    before an atomic rename, so concurrent writers never share a temp path
//!    and a reader never observes a half-written file.
//! 2. The async writers serialize behind a single process-wide async lock so
//!    a read-modify-write cycle cannot interleave and drop an update.
//!
//! The sync writer runs only during single-threaded startup (before the
//! gateway is live), so it relies on the unique temp file alone.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

/// Process-wide lock guarding async `config.json5` read-modify-write cycles.
fn config_write_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Builds a unique temp path next to `path`, keeping the original extension
/// (so `config.json5` -> `config.json5.<pid>.<seq>.tmp`). Co-locating the temp
/// file guarantees the final rename stays within one filesystem.
fn unique_temp_path(path: &Path) -> PathBuf {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let mut temp = path.to_path_buf();
    let suffix = match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) => format!("{extension}.{pid}.{sequence}.tmp"),
        None => format!("{pid}.{sequence}.tmp"),
    };
    temp.set_extension(suffix);
    temp
}

/// Serialized async config write: acquire the process-wide lock, then atomically
/// replace `path` with `contents` via a unique temp file. Async writers (which
/// can be dispatched concurrently by the gateway) must use this so their
/// read-modify-write cycles cannot interleave.
pub(crate) async fn write_config_atomic(path: &Path, contents: String) -> std::io::Result<()> {
    let _guard = config_write_lock().lock().await;
    let temp = unique_temp_path(path);
    if let Err(error) = tokio::fs::write(&temp, contents).await {
        let _ = tokio::fs::remove_file(&temp).await;
        return Err(error);
    }
    if let Err(error) = tokio::fs::rename(&temp, path).await {
        let _ = tokio::fs::remove_file(&temp).await;
        return Err(error);
    }
    Ok(())
}

/// Atomically replace `path` with `contents` via a unique temp file, without
/// the async lock. Reserved for single-threaded startup paths that cannot await
/// (they do not race the gateway writers); the unique temp file still prevents
/// any shared-temp collision.
pub(crate) fn write_config_atomic_blocking(path: &Path, contents: &str) -> std::io::Result<()> {
    let temp = unique_temp_path(path);
    if let Err(error) = std::fs::write(&temp, contents) {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    if let Err(error) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_temp_paths_do_not_collide_and_keep_directory() {
        let dir = std::path::Path::new("/tmp/saf");
        let path = dir.join("config.json5");
        let first = unique_temp_path(&path);
        let second = unique_temp_path(&path);
        assert_ne!(first, second);
        assert_eq!(first.parent(), Some(dir));
        assert_eq!(second.parent(), Some(dir));
        assert!(
            first
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.ends_with("tmp"))
        );
    }

    #[tokio::test]
    async fn atomic_write_replaces_file_contents() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.json5");
        std::fs::write(&path, "old").unwrap();

        write_config_atomic(&path, "new".to_string()).await.unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        let leftover = std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"));
        assert!(!leftover, "no temp files should remain after atomic write");
    }

    #[test]
    fn blocking_atomic_write_replaces_file_contents() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.json5");
        std::fs::write(&path, "old").unwrap();

        write_config_atomic_blocking(&path, "new").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    }
}

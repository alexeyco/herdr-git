//! Locating the gitui binary.
//!
//! The path is resolved to a canonical absolute one before gitui reaches the
//! pane process: herdr spawns that process with its own `PATH` and working
//! directory, which may differ from the action process's. A candidate that
//! cannot be canonicalized is an error rather than a cwd-relative guess.
//!
//! There is no env override: gitui is always resolved from `PATH`.

use std::path::{Path, PathBuf};

const INSTALL_HINT: &str = "install it from https://github.com/gitui-org/gitui";

/// Finds the first executable `gitui` on `PATH`, canonicalized to an absolute
/// path.
pub(crate) fn find_gitui() -> Result<String, String> {
    let path = std::env::var("PATH").unwrap_or_default();
    resolve_gitui(&path)
}

/// Resolves gitui from `PATH`. Split out so the selection is testable without
/// mutating the process environment.
fn resolve_gitui(path: &str) -> Result<String, String> {
    if let Some(found) = first_in_path(path)? {
        return Ok(found.to_string_lossy().into_owned());
    }
    Err(format!("gitui not found on PATH; {INSTALL_HINT}"))
}

/// Canonical absolute path of the first executable `gitui` among `PATH`
/// entries, in order. Empty entries are skipped.
fn first_in_path(path: &str) -> Result<Option<PathBuf>, String> {
    let Some(candidate) = std::env::split_paths(path)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join("gitui"))
        .find(|candidate| is_executable(candidate))
    else {
        return Ok(None);
    };
    canonical_absolute(candidate).map(Some)
}

/// Canonicalizes a candidate to an absolute path. There is deliberately no
/// working-directory fallback: the action and pane processes can have different
/// cwds, so guessing would break the pane-safety contract.
fn canonical_absolute(candidate: PathBuf) -> Result<PathBuf, String> {
    std::fs::canonicalize(&candidate).map_err(|e| {
        format!(
            "found gitui at {} but cannot resolve it to an absolute path: {e}",
            candidate.display()
        )
    })
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// A canonicalized scratch directory, removed when the test ends, so
    /// comparisons survive symlinked temp roots (e.g. `/var` → `/private/var`
    /// on macOS).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "herdr-gitui-test-{tag}-{}-{nanos}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(std::fs::canonicalize(&dir).unwrap())
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Serializes tests that change the process working directory.
    static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Sets the process cwd for the duration of a test and restores it on drop.
    struct CwdGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        prev: PathBuf,
    }

    impl CwdGuard {
        fn set(dir: &Path) -> Self {
            let lock = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let prev = std::env::current_dir().unwrap();
            std::env::set_current_dir(dir).unwrap();
            Self { _lock: lock, prev }
        }
    }

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.prev);
        }
    }

    fn write_gitui(dir: &Path, executable: bool) {
        let path = dir.join("gitui");
        std::fs::write(&path, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = if executable { 0o755 } else { 0o644 };
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        }
        #[cfg(not(unix))]
        let _ = executable;
    }

    #[test]
    fn empty_path_has_no_gitui() {
        assert!(first_in_path("").unwrap().is_none());
        assert!(
            first_in_path(":/nonexistent-dir-for-herdr-test:")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn first_executable_entry_wins() {
        let first = TempDir::new("first");
        let second = TempDir::new("second");
        write_gitui(first.path(), true);
        write_gitui(second.path(), true);
        let path = std::env::join_paths([first.path(), second.path()])
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            first_in_path(&path).unwrap(),
            Some(first.path().join("gitui"))
        );
    }

    #[test]
    fn non_executable_entry_is_skipped_and_result_is_absolute() {
        let first = TempDir::new("nonexec");
        let second = TempDir::new("exec");
        write_gitui(first.path(), false);
        write_gitui(second.path(), true);
        let path = std::env::join_paths([first.path(), second.path()])
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let found = first_in_path(&path).unwrap().unwrap();
        assert_eq!(found, second.path().join("gitui"));
        assert!(found.is_absolute());
    }

    #[test]
    fn relative_path_entry_resolves_to_absolute() {
        let dir = TempDir::new("relpath");
        write_gitui(dir.path(), true);
        let _cwd = CwdGuard::set(dir.path());
        let found = first_in_path(".").unwrap().unwrap();
        assert!(found.is_absolute());
        assert_eq!(found, dir.path().join("gitui"));
    }

    #[test]
    fn missing_path_entry_errors_with_install_hint() {
        for path in ["", ":"] {
            let err = resolve_gitui(path).unwrap_err();
            assert!(err.contains("gitui not found"), "{err}");
            assert!(err.contains("https://github.com/gitui-org/gitui"), "{err}");
        }
    }
}

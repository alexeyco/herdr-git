//! Locating supported Git TUI binaries.
//!
//! Each path is resolved to a canonical absolute one before the tool reaches the
//! pane process: herdr spawns that process with its own `PATH` and working
//! directory, which may differ from the action process's. A candidate that
//! cannot be canonicalized is an error rather than a cwd-relative guess.
//!
//! There are no binary overrides: both tools are resolved from `PATH`.

use std::path::{Path, PathBuf};

const TOOLS: [&str; 2] = ["gitui", "lazygit"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Tool {
    pub(crate) id: &'static str,
    pub(crate) path: String,
}

/// Finds supported tools on `PATH`, in deterministic gitui/lazygit order.
pub(crate) fn find_tools() -> Result<Vec<Tool>, String> {
    let path = std::env::var("PATH").unwrap_or_default();
    resolve_tools(&path)
}

fn resolve_tools(path: &str) -> Result<Vec<Tool>, String> {
    TOOLS
        .into_iter()
        .filter_map(|id| match first_in_path(path, id) {
            Ok(Some(path)) => Some(Ok(Tool {
                id,
                path: path.to_string_lossy().into_owned(),
            })),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

/// Canonical absolute path of the first executable named `name` among `PATH`
/// entries, in order. Empty entries are skipped.
fn first_in_path(path: &str, name: &str) -> Result<Option<PathBuf>, String> {
    let Some(candidate) = std::env::split_paths(path)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
    else {
        return Ok(None);
    };
    canonical_absolute(candidate, name).map(Some)
}

/// Canonicalizes a candidate to an absolute path. There is deliberately no
/// working-directory fallback: the action and pane processes can have different
/// cwds, so guessing would break the pane-safety contract.
fn canonical_absolute(candidate: PathBuf, name: &str) -> Result<PathBuf, String> {
    std::fs::canonicalize(&candidate).map_err(|e| {
        format!(
            "found {name} at {} but cannot resolve it to an absolute path: {e}",
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

    fn write_tool(dir: &Path, name: &str, executable: bool) {
        let path = dir.join(name);
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
        assert!(first_in_path("", "gitui").unwrap().is_none());
        assert!(
            first_in_path(":/nonexistent-dir-for-herdr-test:", "gitui")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn first_executable_entry_wins() {
        let first = TempDir::new("first");
        let second = TempDir::new("second");
        write_tool(first.path(), "gitui", true);
        write_tool(second.path(), "gitui", true);
        let path = std::env::join_paths([first.path(), second.path()])
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            first_in_path(&path, "gitui").unwrap(),
            Some(first.path().join("gitui"))
        );
    }

    #[test]
    fn non_executable_entry_is_skipped_and_result_is_absolute() {
        let first = TempDir::new("nonexec");
        let second = TempDir::new("exec");
        write_tool(first.path(), "gitui", false);
        write_tool(second.path(), "gitui", true);
        let path = std::env::join_paths([first.path(), second.path()])
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let found = first_in_path(&path, "gitui").unwrap().unwrap();
        assert_eq!(found, second.path().join("gitui"));
        assert!(found.is_absolute());
    }

    #[test]
    fn relative_path_entry_resolves_to_absolute() {
        let dir = TempDir::new("relpath");
        write_tool(dir.path(), "gitui", true);
        let _cwd = CwdGuard::set(dir.path());
        let found = first_in_path(".", "gitui").unwrap().unwrap();
        assert!(found.is_absolute());
        assert_eq!(found, dir.path().join("gitui"));
    }

    #[test]
    fn empty_path_resolves_to_no_tools() {
        assert!(resolve_tools("").unwrap().is_empty());
        assert!(resolve_tools(":").unwrap().is_empty());
    }

    #[test]
    fn each_tool_uses_its_first_executable_path_match() {
        let first = TempDir::new("both-first");
        let second = TempDir::new("both-second");
        write_tool(first.path(), "gitui", true);
        write_tool(first.path(), "lazygit", false);
        write_tool(second.path(), "gitui", true);
        write_tool(second.path(), "lazygit", true);
        let path = std::env::join_paths([first.path(), second.path()])
            .unwrap()
            .to_string_lossy()
            .into_owned();

        let tools = resolve_tools(&path).unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].id, "gitui");
        assert_eq!(tools[0].path, first.path().join("gitui").to_string_lossy());
        assert_eq!(tools[1].id, "lazygit");
        assert_eq!(
            tools[1].path,
            second.path().join("lazygit").to_string_lossy()
        );
        assert!(tools.iter().all(|tool| Path::new(&tool.path).is_absolute()));
    }
}

//! Read-only git probes and snapshot capture.
//!
//! Detection runs in the pane process, in the pane's working directory (the
//! repository), before and after gitui runs. Every probe is read-only: the
//! index and the working tree are never mutated.

use std::path::Path;
use std::process::{Command, Output};

/// Read-only git state captured at a point in time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Snapshot {
    /// Whether the directory is a git work tree.
    pub(crate) in_repo: bool,
    /// Current branch name (`None` for detached HEAD or no commits).
    pub(crate) branch: Option<String>,
    /// Full SHA of HEAD (`None` when no commits).
    pub(crate) head: Option<String>,
    /// Upstream ref, e.g. `origin/main` (`None` when unset).
    pub(crate) upstream: Option<String>,
    /// Number of commits ahead of upstream (`None` when no upstream).
    pub(crate) ahead: Option<u64>,
    /// Count of staged files.
    pub(crate) staged: u64,
    /// Count of unstaged (modified) files.
    pub(crate) unstaged: u64,
    /// Count of untracked files.
    pub(crate) untracked: u64,
}

/// Probes `dir` read-only and returns its git snapshot. On any unexpected git
/// failure the probe degrades gracefully: one warning is written to stderr and
/// a default (non-repo) snapshot is returned.
pub(crate) fn snapshot(dir: &Path) -> Snapshot {
    match try_snapshot(dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "herdr-gitui: warning: git probe failed ({}): {}",
                e.cmd, e.err
            );
            Snapshot::default()
        }
    }
}

/// A failed probe: the command that ran and what went wrong.
struct ProbeError {
    cmd: String,
    err: String,
}

impl ProbeError {
    fn new(cmd: String, err: impl Into<String>) -> Self {
        Self {
            cmd,
            err: err.into(),
        }
    }
}

fn try_snapshot(dir: &Path) -> Result<Snapshot, ProbeError> {
    if !is_repo(dir)? {
        return Ok(Snapshot::default());
    }
    let head = rev_parse_head(dir);
    let branch = symbolic_branch(dir);
    let upstream = parse_upstream(dir);
    let ahead = match &upstream {
        Some(_) => parse_ahead(dir),
        None => None,
    };
    let staged = count_lines_or_warn(dir, &["diff", "--cached", "--name-only"]);
    let unstaged = count_lines_or_warn(dir, &["diff", "--name-only"]);
    let untracked = count_lines_or_warn(dir, &["ls-files", "--others", "--exclude-standard"]);
    Ok(Snapshot {
        in_repo: true,
        branch,
        head,
        upstream,
        ahead,
        staged,
        unstaged,
        untracked,
    })
}

/// `rev-parse --is-inside-work-tree` exits 0 and prints `true` inside a work
/// tree; any other exit is the expected "not a repository" signal.
fn is_repo(dir: &Path) -> Result<bool, ProbeError> {
    let args = ["rev-parse", "--is-inside-work-tree"];
    let out = run_git(dir, &args)?;
    Ok(out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "true")
}

/// `rev-parse HEAD`: full sha on success, None on failure (no commits yet).
fn rev_parse_head(dir: &Path) -> Option<String> {
    let args = ["rev-parse", "HEAD"];
    let out = run_git_quiet(dir, &args)?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// `symbolic-ref --short -q HEAD`: branch name on success, None for detached.
fn symbolic_branch(dir: &Path) -> Option<String> {
    let args = ["symbolic-ref", "--short", "-q", "HEAD"];
    let out = run_git_quiet(dir, &args)?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// `rev-parse --abbrev-ref --symbolic-full-name @{upstream}`: upstream ref on
/// success, None when no upstream is configured.
fn parse_upstream(dir: &Path) -> Option<String> {
    let args = [
        "rev-parse",
        "--abbrev-ref",
        "--symbolic-full-name",
        "@{upstream}",
    ];
    let out = run_git_quiet(dir, &args)?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// `rev-list --count @{upstream}..HEAD`: ahead count on success.
fn parse_ahead(dir: &Path) -> Option<u64> {
    let args = ["rev-list", "--count", "@{upstream}..HEAD"];
    let out = run_git_quiet(dir, &args)?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<u64>()
        .ok()
}

/// Counts non-empty lines of stdout from a git command; on failure emits the
/// standard probe warning and returns 0 so the rest of the snapshot survives.
fn count_lines_or_warn(dir: &Path, args: &[&str]) -> u64 {
    match count_lines(dir, args) {
        Ok(n) => n,
        Err(e) => {
            eprintln!(
                "herdr-gitui: warning: git probe failed ({}): {}",
                e.cmd, e.err
            );
            0
        }
    }
}

/// Counts non-empty lines of stdout from a git command.
fn count_lines(dir: &Path, args: &[&str]) -> Result<u64, ProbeError> {
    let out = run_git(dir, args)?;
    if !out.status.success() {
        return Err(unexpected(args, &out));
    }
    let count = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count() as u64;
    Ok(count)
}

fn run_git(dir: &Path, args: &[&str]) -> Result<Output, ProbeError> {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| ProbeError::new(command(args), e.to_string()))
}

/// Like `run_git` but returns `Option` — used for probes where failure simply
/// means "not available" rather than a fatal error.
fn run_git_quiet(dir: &Path, args: &[&str]) -> Option<Output> {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()
}

fn unexpected(args: &[&str], out: &Output) -> ProbeError {
    let stderr = String::from_utf8_lossy(&out.stderr);
    ProbeError::new(
        command(args),
        format!("exit {}: {}", out.status, stderr.trim()),
    )
}

fn command(args: &[&str]) -> String {
    format!("git {}", args.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// A canonicalized scratch directory, removed when the test ends.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "herdr-gitui-git-test-{tag}-{}-{nanos}",
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

    /// Runs git in `dir` with an inline identity so no global git config is
    /// required.
    fn git(dir: &Path, args: &[&str]) {
        let mut full = vec!["-c", "user.name=t", "-c", "user.email=t@example.com"];
        full.extend_from_slice(args);
        let out = Command::new("git")
            .args(&full)
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn non_git_dir_is_not_in_repo() {
        let dir = TempDir::new("nonrepo");
        let s = snapshot(dir.path());
        assert!(!s.in_repo);
        assert!(s.branch.is_none());
        assert!(s.head.is_none());
    }

    #[test]
    fn empty_repo_has_no_head() {
        let dir = TempDir::new("empty");
        git(dir.path(), &["init", "-q"]);
        let s = snapshot(dir.path());
        assert!(s.in_repo);
        // No commits yet, so HEAD is None.
        assert!(s.head.is_none());
        // Note: git init sets a default branch name even without commits,
        // so branch may be Some("main") or similar.
    }

    #[test]
    fn committed_repo_has_head_and_branch() {
        let dir = TempDir::new("committed");
        git(dir.path(), &["init", "-q", "-b", "main"]);
        std::fs::write(dir.path().join("a.txt"), "a\n").unwrap();
        git(dir.path(), &["add", "a.txt"]);
        git(dir.path(), &["commit", "-q", "-m", "init"]);
        let s = snapshot(dir.path());
        assert!(s.in_repo);
        assert!(s.head.is_some());
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!(s.staged, 0);
        assert_eq!(s.unstaged, 0);
        assert_eq!(s.untracked, 0);
    }

    #[test]
    fn count_lines_or_warn_returns_zero_on_failure() {
        let dir = TempDir::new("count-warn");
        git(dir.path(), &["init", "-q", "-b", "main"]);
        // A nonsense subcommand will fail; the wrapper should return 0.
        let n = count_lines_or_warn(dir.path(), &["nonsense-subcommand"]);
        assert_eq!(n, 0);
    }

    #[test]
    fn counts_staged_unstaged_untracked() {
        let dir = TempDir::new("counts");
        git(dir.path(), &["init", "-q", "-b", "main"]);
        // Create and commit two tracked files.
        std::fs::write(dir.path().join("a.txt"), "a\n").unwrap();
        std::fs::write(dir.path().join("b.txt"), "b\n").unwrap();
        git(dir.path(), &["add", "a.txt", "b.txt"]);
        git(dir.path(), &["commit", "-q", "-m", "init"]);
        // staged: modify a.txt and add it.
        std::fs::write(dir.path().join("a.txt"), "modified\n").unwrap();
        git(dir.path(), &["add", "a.txt"]);
        // unstaged: modify b.txt but don't add.
        std::fs::write(dir.path().join("b.txt"), "changed\n").unwrap();
        // untracked: new file.
        std::fs::write(dir.path().join("c.txt"), "new\n").unwrap();
        let s = snapshot(dir.path());
        assert!(s.in_repo, "snapshot should detect repo");
        assert_eq!(s.staged, 1, "staged: {:?}", s);
        assert_eq!(s.unstaged, 1, "unstaged: {:?}", s);
        assert_eq!(s.untracked, 1, "untracked: {:?}", s);
    }
}

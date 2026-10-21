//! Integration boundaries used by session workflows.

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::summary_engine::CommitLine;

/// Read-only Git state captured at one point in a session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub(crate) in_repo: bool,
    pub(crate) branch: Option<String>,
    pub(crate) head: Option<String>,
    pub(crate) upstream: Option<String>,
    pub(crate) ahead: Option<u64>,
    pub(crate) staged: u64,
    pub(crate) unstaged: u64,
    pub(crate) untracked: u64,
}

/// Read-only Git operations needed to capture and describe a session.
#[cfg_attr(test, mockall::automock)]
pub(crate) trait Git {
    fn snapshot(&self, dir: &Path) -> Snapshot;
    fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Option<bool>;
    fn log_range(&self, from: &str, to: &str) -> Option<Vec<CommitLine>>;
    fn has_merges_in_range(&self, from: &str, to: &str) -> Option<bool>;
    fn log_head(&self, head: &str, count: usize) -> Option<Vec<CommitLine>>;
}

/// Execution of the chosen Git TUI as a direct child of the pane process.
#[cfg_attr(test, mockall::automock)]
pub(crate) trait TuiProcess {
    fn run(&self, executable: &Path, cwd: &Path) -> Result<Option<i32>, String>;
    fn hold_pane(&self, prompt: &str);
}

/// Herdr callbacks used after the TUI exits.
#[cfg_attr(test, mockall::automock)]
pub(crate) trait Herdr {
    fn require_bin(&self) -> Result<(), String>;
    fn agent_prompt(&self, target_pane: &str, text: &str) -> Result<(), String>;
    fn close_pane(&self, pane_id: &str) -> Result<(), String>;
    fn close_tab(&self, tab_id: &str) -> Result<(), String>;
    fn focus_tab(&self, tab_id: &str) -> Result<String, String>;
}

/// Filesystem operations used for session summaries and state files.
#[cfg_attr(test, mockall::automock)]
pub(crate) trait StateFilesystem {
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    fn write(&self, path: &Path, contents: &[u8]) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn read_dir(&self, path: &Path) -> io::Result<Vec<StateFileEntry>>;
}

/// Filesystem facts needed by best-effort age pruning.
#[derive(Debug)]
pub(crate) struct StateFileEntry {
    pub(crate) path: PathBuf,
    pub(crate) modified: io::Result<SystemTime>,
}

/// Time source used to make pruning and state-file names deterministic in tests.
#[cfg_attr(test, mockall::automock)]
pub(crate) trait Clock {
    fn now(&self) -> SystemTime;
    fn process_id(&self) -> u32;
}

/// Session integrations grouped for workflow orchestration.
pub(crate) struct SessionServices<'a, H, G, P, F, C> {
    pub(crate) herdr: &'a H,
    pub(crate) git: &'a G,
    pub(crate) process: &'a P,
    pub(crate) filesystem: &'a F,
    pub(crate) clock: &'a C,
}

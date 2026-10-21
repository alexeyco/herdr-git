//! Selected Git TUI process integration.

use std::path::Path;
use std::process::{Command, ExitStatus};

use crate::features::session::ports::TuiProcess;

/// Concrete process boundary used by the session workflow.
pub(crate) struct ProcessAdapter;

impl TuiProcess for ProcessAdapter {
    fn run(&self, executable: &Path, cwd: &Path) -> Result<Option<i32>, String> {
        let path = executable.to_string_lossy();
        run_tool(&path, cwd).map(|status| status.code())
    }

    fn hold_pane(&self, prompt: &str) {
        eprintln!("{prompt}");
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
    }
}

/// Validates and runs the selected TUI as a direct child with inherited stdio.
/// The child receives no arguments and discovers the repository from `cwd`.
pub(crate) fn run_tool(path: &str, cwd: &Path) -> Result<ExitStatus, String> {
    Command::new(path)
        .current_dir(cwd)
        .status()
        .map_err(|error| format!("failed to run {path} in {}: {error}", cwd.display()))
}

/// Checks the selected executable before the session prints its startup line.
pub(crate) fn validate_tool(path: &str) -> Result<(), String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("selected Git TUI binary {path} is unavailable: {error}"))?;
    if !metadata.is_file() {
        return Err(format!("selected Git TUI binary {path} is not a file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(format!("selected Git TUI binary {path} is not executable"));
        }
    }
    Ok(())
}

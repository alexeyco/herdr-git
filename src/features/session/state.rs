//! Session-summary file lifecycle and age-based pruning.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::features::session::ports::{Clock, StateFilesystem};

const SESSION_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

pub(crate) fn state_dir() -> PathBuf {
    let configured = std::env::var("HERDR_PLUGIN_STATE_DIR").ok();
    configured
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

pub(crate) fn create_sessions_dir(
    filesystem: &dyn StateFilesystem,
    base: &Path,
) -> Result<PathBuf, String> {
    let sessions = base.join("sessions");
    filesystem
        .create_dir_all(&sessions)
        .map_err(|error| format!("failed to create state dir {}: {error}", sessions.display()))?;
    Ok(sessions)
}

pub(crate) fn prune_old_sessions(
    filesystem: &dyn StateFilesystem,
    dir: &Path,
    now: SystemTime,
) -> Vec<String> {
    let Ok(entries) = filesystem.read_dir(dir) else {
        return Vec::new();
    };
    let mut messages = Vec::new();
    for entry in entries {
        if entry
            .path
            .extension()
            .and_then(|extension| extension.to_str())
            != Some("txt")
        {
            continue;
        }
        let Ok(modified) = entry.modified else {
            continue;
        };
        let Ok(age) = now.duration_since(modified) else {
            continue;
        };
        if age >= SESSION_MAX_AGE
            && let Err(error) = filesystem.remove_file(&entry.path)
        {
            messages.push(format!("cannot remove {}: {error}", entry.path.display()));
        }
    }
    messages
}

pub(crate) fn write_session_file(
    filesystem: &dyn StateFilesystem,
    clock: &dyn Clock,
    sessions_dir: &Path,
    tool_id: &str,
    summary_text: &str,
) -> Result<PathBuf, String> {
    let millis = clock
        .now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| format!("system clock error: {error}"))?;
    let path = sessions_dir.join(format!("{}-{millis}.txt", clock.process_id()));
    let contents = format!("{tool_id} session summary:\n\n{summary_text}");
    filesystem
        .write(&path, contents.as_bytes())
        .map_err(|error| format!("failed to write session file {}: {error}", path.display()))?;
    Ok(path)
}

/// Removes a delivered session summary; missing files count as success.
pub(crate) fn remove_delivered(filesystem: &dyn StateFilesystem, path: &Path) {
    if let Err(error) = filesystem.remove_file(path) {
        if error.kind() == std::io::ErrorKind::NotFound {
            return;
        }
        eprintln!(
            "herdr-gitui: warning: cannot remove {}: {error}",
            path.display()
        );
    }
}

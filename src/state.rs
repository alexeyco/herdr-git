//! Persistent state shared by action and picker entrypoints.
//!
//! The last explicitly selected Git TUI is stored in
//! `$HERDR_PLUGIN_STATE_DIR/last-tool`. Reads are deliberately forgiving so
//! stale or unusable state never prevents the plugin from opening a tool.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// One-shot handoff from a compact picker popup to the action process that
/// opens the configured tool popup.
pub(crate) struct PickerSelectionTicket {
    path: PathBuf,
}

impl PickerSelectionTicket {
    pub(crate) fn create() -> Result<Self, String> {
        let dir = state_dir();
        fs::create_dir_all(&dir).map_err(|error| {
            format!(
                "cannot create picker handoff directory {}: {error}",
                dir.display()
            )
        })?;
        let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = dir.join(format!(
            ".picker-selection-{}-{timestamp}-{sequence}.txt",
            std::process::id()
        ));
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                format!(
                    "cannot create picker handoff file {}: {error}",
                    path.display()
                )
            })?;
        Ok(Self { path })
    }

    pub(crate) fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Waits for `gitui`, `lazygit`, or `cancel` to be published by the picker.
    pub(crate) fn wait(&self) -> Result<Option<&'static str>, String> {
        loop {
            match fs::read_to_string(&self.path) {
                Ok(content) => match content.as_str() {
                    "gitui\n" => return Ok(Some("gitui")),
                    "lazygit\n" => return Ok(Some("lazygit")),
                    "cancel\n" => return Ok(None),
                    _ => std::thread::sleep(Duration::from_millis(20)),
                },
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(error) => {
                    return Err(format!(
                        "cannot read picker handoff file {}: {error}",
                        self.path.display()
                    ));
                }
            }
        }
    }
}

impl Drop for PickerSelectionTicket {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Publishes a confirmed tool id or cancellation for the waiting action.
pub(crate) fn publish_picker_selection(
    path: &std::path::Path,
    selection: Option<&str>,
) -> Result<(), String> {
    let value = match selection {
        Some("gitui") => "gitui\n",
        Some("lazygit") => "lazygit\n",
        Some(tool) => return Err(format!("unknown picker selection '{tool}'")),
        None => "cancel\n",
    };
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_extension(format!("tmp-{}-{sequence}", std::process::id()));
    let result = (|| {
        fs::write(&temporary, value)?;
        fs::rename(&temporary, path)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(format!("cannot publish picker selection: {error}"));
    }
    Ok(())
}

/// Returns the recognized saved preference, or `None` for any unavailable or
/// invalid preference file.
pub(crate) fn read_last_tool() -> Option<&'static str> {
    let content = fs::read_to_string(state_dir().join("last-tool")).ok()?;
    match content.as_str() {
        "gitui\n" => Some("gitui"),
        "lazygit\n" => Some("lazygit"),
        _ => None,
    }
}

/// Saves an explicit tool choice atomically. Failures are best-effort and do
/// not affect the caller's selected tool.
pub(crate) fn save_last_tool(tool_id: &str) {
    if !matches!(tool_id, "gitui" | "lazygit") {
        return;
    }

    if let Err(error) = write_last_tool(tool_id) {
        eprintln!("herdr-gitui: warning: cannot save last tool preference: {error}");
    }
}

/// Attempts to save a preference under a caller-provided state directory.
/// Kept separate so picker-flow tests can exercise a real filesystem failure.
#[cfg(test)]
pub(crate) fn try_save_last_tool_to(dir: &std::path::Path, tool_id: &str) -> std::io::Result<()> {
    if !matches!(tool_id, "gitui" | "lazygit") {
        return Ok(());
    }
    write_last_tool_to(dir, tool_id)
}

fn write_last_tool(tool_id: &str) -> std::io::Result<()> {
    write_last_tool_to(&state_dir(), tool_id)
}

fn write_last_tool_to(dir: &std::path::Path, tool_id: &str) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;

    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = dir.join(format!(
        ".last-tool-{}-{timestamp}-{sequence}.tmp",
        std::process::id()
    ));
    let destination = dir.join("last-tool");

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        writeln!(file, "{tool_id}")?;
        drop(file);
        fs::rename(&temporary, &destination)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn state_dir() -> PathBuf {
    std::env::var_os("HERDR_PLUGIN_STATE_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct StateDir {
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: Option<std::ffi::OsString>,
        path: PathBuf,
    }

    impl StateDir {
        fn new() -> Self {
            let lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
            let previous = std::env::var_os("HERDR_PLUGIN_STATE_DIR");
            let path = std::env::temp_dir().join(format!(
                "herdr-git-state-test-{}-{}",
                std::process::id(),
                TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            unsafe { std::env::set_var("HERDR_PLUGIN_STATE_DIR", &path) };
            Self {
                _lock: lock,
                previous,
                path,
            }
        }
    }

    impl Drop for StateDir {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => unsafe { std::env::set_var("HERDR_PLUGIN_STATE_DIR", value) },
                None => unsafe { std::env::remove_var("HERDR_PLUGIN_STATE_DIR") },
            }
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn saves_bare_tool_id_followed_by_newline() {
        let state = StateDir::new();
        save_last_tool("lazygit");
        assert_eq!(
            fs::read(state.path.join("last-tool")).unwrap(),
            b"lazygit\n"
        );
        assert_eq!(read_last_tool(), Some("lazygit"));
    }

    #[test]
    fn picker_selection_ticket_waits_for_selection_or_cancellation() {
        let _state = StateDir::new();
        let ticket = PickerSelectionTicket::create().unwrap();
        let selection_path = ticket.path().to_owned();
        let writer = std::thread::spawn(move || {
            publish_picker_selection(&selection_path, Some("lazygit")).unwrap();
        });
        assert_eq!(ticket.wait().unwrap(), Some("lazygit"));
        writer.join().unwrap();

        let ticket = PickerSelectionTicket::create().unwrap();
        let selection_path = ticket.path().to_owned();
        let writer = std::thread::spawn(move || {
            publish_picker_selection(&selection_path, None).unwrap();
        });
        assert_eq!(ticket.wait().unwrap(), None);
        writer.join().unwrap();
    }

    #[test]
    fn missing_empty_and_unknown_preferences_are_ignored() {
        let state = StateDir::new();
        assert_eq!(read_last_tool(), None);
        fs::create_dir_all(&state.path).unwrap();
        fs::write(state.path.join("last-tool"), "\n").unwrap();
        assert_eq!(read_last_tool(), None);
        fs::write(state.path.join("last-tool"), "other\n").unwrap();
        assert_eq!(read_last_tool(), None);
    }

    #[test]
    fn unreadable_preference_is_ignored() {
        let state = StateDir::new();
        fs::create_dir_all(state.path.join("last-tool")).unwrap();
        assert_eq!(read_last_tool(), None);
    }

    #[test]
    fn failed_save_does_not_panic_or_replace_selection() {
        let state = StateDir::new();
        fs::create_dir_all(state.path.join("last-tool")).unwrap();
        save_last_tool("gitui");
        assert_eq!(read_last_tool(), None);
    }
}

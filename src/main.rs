//! herdr plugin that opens gitui in a new tab and reports what changed.
//!
//! The binary is dual-mode:
//! - action mode (`HERDR_PLUGIN_ACTION_ID` set) resolves the repository from
//!   the plugin context and opens a `gitui` plugin pane (`plugin pane open
//!   --placement tab`), then exits immediately.
//! - pane mode (`HERDR_PLUGIN_ENTRYPOINT_ID` set) runs inside that pane, execs
//!   gitui directly with the pane tty, reads its exit code, takes read-only
//!   before/after git snapshots, and delivers an English summary of what
//!   changed to the agent pane that requested the session.
//!
//! Plugin docs: https://herdr.dev/docs/plugins/
//! gitui: https://github.com/gitui-org/gitui

mod context;
mod git;
mod gitui;
mod herdr;
mod picker;
mod session;
mod settings;
mod setup;
mod state;
mod summary;

fn main() {
    if let Err(e) = run() {
        eprintln!("herdr-gitui: {e}");
        // Plugin panes auto-close when this process exits, so pane-mode errors
        // must block until the user has read the message.
        if env_set("HERDR_PLUGIN_ENTRYPOINT_ID") {
            session::hold_pane();
        }
        std::process::exit(1);
    }
}

/// Which entry point the process should run.
#[derive(Debug, PartialEq, Eq)]
enum RunMode {
    /// `HERDR_PLUGIN_ENTRYPOINT_ID` is set: running inside a plugin pane.
    Pane(String),
    /// `HERDR_PLUGIN_ACTION_ID` is set: running the action.
    Action,
    /// Neither is set: invoked outside herdr.
    Outside,
}

/// Picks the mode from the entry-point environment. The entrypoint wins when
/// both are set, matching herdr's action-spawns-pane nesting.
fn run_mode() -> RunMode {
    if env_set("HERDR_PLUGIN_ENTRYPOINT_ID") {
        RunMode::Pane(
            std::env::var("HERDR_PLUGIN_ENTRYPOINT_ID")
                .expect("entrypoint is set when pane mode is selected"),
        )
    } else if env_set("HERDR_PLUGIN_ACTION_ID") {
        RunMode::Action
    } else {
        RunMode::Outside
    }
}

fn run() -> Result<(), String> {
    match run_mode() {
        RunMode::Pane(entrypoint) => match entrypoint.as_str() {
            "open" => session::run_pane(),
            "picker" => session::run_picker(),
            "setup" => setup::run(),
            unknown => Err(format!(
                "unknown pane entrypoint '{unknown}'; expected one of: open, picker, setup"
            )),
        },
        RunMode::Action => session::run_action(),
        RunMode::Outside => Err("invoked outside herdr plugin context".to_string()),
    }
}

fn env_set(key: &str) -> bool {
    std::env::var(key)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::Mutex;

    /// Serializes tests that mutate the mode environment.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Sets both mode variables for the duration of a test, restoring the
    /// previous values on drop.
    struct ModeEnv {
        _lock: std::sync::MutexGuard<'static, ()>,
        prev_entry: Option<OsString>,
        prev_action: Option<OsString>,
    }

    impl ModeEnv {
        fn set(entry: Option<&str>, action: Option<&str>) -> Self {
            let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let prev_entry = std::env::var_os("HERDR_PLUGIN_ENTRYPOINT_ID");
            let prev_action = std::env::var_os("HERDR_PLUGIN_ACTION_ID");
            set_var("HERDR_PLUGIN_ENTRYPOINT_ID", entry);
            set_var("HERDR_PLUGIN_ACTION_ID", action);
            Self {
                _lock: lock,
                prev_entry,
                prev_action,
            }
        }
    }

    impl Drop for ModeEnv {
        fn drop(&mut self) {
            match &self.prev_entry {
                Some(v) => unsafe { std::env::set_var("HERDR_PLUGIN_ENTRYPOINT_ID", v) },
                None => unsafe { std::env::remove_var("HERDR_PLUGIN_ENTRYPOINT_ID") },
            }
            match &self.prev_action {
                Some(v) => unsafe { std::env::set_var("HERDR_PLUGIN_ACTION_ID", v) },
                None => unsafe { std::env::remove_var("HERDR_PLUGIN_ACTION_ID") },
            }
        }
    }

    fn set_var(key: &str, value: Option<&str>) {
        match value {
            Some(v) => unsafe { std::env::set_var(key, v) },
            None => unsafe { std::env::remove_var(key) },
        }
    }

    #[test]
    fn entrypoint_only_is_pane() {
        let _env = ModeEnv::set(Some("open"), None);
        assert_eq!(run_mode(), RunMode::Pane("open".to_string()));
    }

    #[test]
    fn action_only_is_action() {
        let _env = ModeEnv::set(None, Some("open"));
        assert_eq!(run_mode(), RunMode::Action);
    }

    #[test]
    fn both_set_prefers_pane() {
        let _env = ModeEnv::set(Some("open"), Some("open"));
        assert_eq!(run_mode(), RunMode::Pane("open".to_string()));
    }

    #[test]
    fn neither_set_is_outside() {
        let _env = ModeEnv::set(None, None);
        assert_eq!(run_mode(), RunMode::Outside);
    }

    #[test]
    fn blank_values_count_as_unset() {
        let _env = ModeEnv::set(Some("   "), Some("\t"));
        assert_eq!(run_mode(), RunMode::Outside);
    }
}

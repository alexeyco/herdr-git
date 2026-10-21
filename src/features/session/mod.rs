//! Pane-mode session workflow.

pub(crate) mod delivery;
pub(crate) mod outcome;
pub(crate) mod ports;
pub(crate) mod process;
pub(crate) mod state;
pub(crate) mod summary;
pub(crate) mod summary_engine;
pub(crate) mod teardown;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use crate::adapters::{
    git::GitAdapter, herdr::HerdrAdapter, process::ProcessAdapter, state::StateAdapter,
};
use crate::features::session::ports::{Herdr as _, TuiProcess as _};

const HOLD_PROMPT: &str = "herdr-gitui: press Enter to close";

/// Holds an error pane open, then restores the caller tab for tiled panes.
pub(crate) fn hold_pane() {
    let herdr = HerdrAdapter;
    let process = ProcessAdapter;
    teardown::hold_and_restore(
        &herdr,
        teardown::is_popup_session(),
        env_non_blank("HERDR_GITUI_CALLER_TAB").as_deref(),
        || process.hold_pane(HOLD_PROMPT),
    );
}

/// Resolves pane-mode inputs and runs one selected Git TUI session.
pub(crate) fn run_pane() -> Result<(), String> {
    let herdr = HerdrAdapter;
    herdr.require_bin()?;
    let dir = match env_non_blank("HERDR_GITUI_DIR") {
        Some(path) => PathBuf::from(path),
        None => std::env::current_dir()
            .map_err(|error| format!("cannot read current directory: {error}"))?,
    };
    let tool_id = env_non_blank("HERDR_GIT_TOOL")
        .ok_or_else(|| "HERDR_GIT_TOOL is not set; reopen this pane from the action".to_string())?;
    let tool_path = env_non_blank("HERDR_GIT_TOOL_PATH").ok_or_else(|| {
        "HERDR_GIT_TOOL_PATH is not set; reopen this pane from the action".to_string()
    })?;
    let git = GitAdapter::new(&dir);
    let process = ProcessAdapter;
    let state = StateAdapter;
    process::run_tool_session(
        &tool_id,
        &tool_path,
        &dir,
        ports::SessionServices {
            herdr: &herdr,
            git: &git,
            process: &process,
            filesystem: &state,
            clock: &state,
        },
    )
}

fn env_non_blank(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

//! TUI process lifecycle and before/after snapshot boundaries.

use crate::adapters::process::validate_tool;
use crate::features::session::{
    delivery,
    ports::{Clock, Git, Herdr, SessionServices, StateFilesystem, TuiProcess},
    state, summary,
};
use std::path::Path;

pub(crate) fn run_tool_session<H, G, P, F, C>(
    tool_id: &str,
    tool_path: &str,
    dir: &Path,
    services: SessionServices<'_, H, G, P, F, C>,
) -> Result<(), String>
where
    H: Herdr,
    G: Git,
    P: TuiProcess,
    F: StateFilesystem,
    C: Clock,
{
    services.herdr.require_bin()?;
    validate_tool(tool_path)?;

    let sessions = state::create_sessions_dir(services.filesystem, &state::state_dir())?;
    for message in state::prune_old_sessions(services.filesystem, &sessions, services.clock.now()) {
        eprintln!("herdr-gitui: warning: {message}");
    }

    let before = services.git.snapshot(dir);
    println!("herdr-gitui: starting {tool_id} in {}", dir.display());
    let code = services.process.run(Path::new(tool_path), dir)?;
    let after = services.git.snapshot(dir);
    let summary_text = summary::generate(services.git, &before, &after);
    let caller_pane = env_non_blank("HERDR_GITUI_CALLER_PANE");
    let caller_agent = env_non_blank("HERDR_GITUI_CALLER_AGENT");

    delivery::finish(
        delivery::FinishContext {
            tool_id,
            code,
            sessions_dir: &sessions,
            summary_text: &summary_text,
            caller_pane: caller_pane.as_deref(),
            caller_agent: caller_agent.as_deref(),
        },
        services.herdr,
        services.process,
        services.filesystem,
        services.clock,
    )
}

fn env_non_blank(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

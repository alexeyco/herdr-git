//! Outcome handling, summary persistence, and delivery to the caller agent.

use std::path::Path;

use crate::features::session::{
    outcome::{self, Outcome},
    ports::{Clock, Herdr, StateFilesystem, TuiProcess},
    state, summary, teardown,
};

const HOLD_PROMPT: &str = "herdr-gitui: press Enter to close";

pub(crate) struct FinishContext<'a> {
    pub(crate) tool_id: &'a str,
    pub(crate) code: Option<i32>,
    pub(crate) sessions_dir: &'a Path,
    pub(crate) summary_text: &'a str,
    pub(crate) caller_pane: Option<&'a str>,
    pub(crate) caller_agent: Option<&'a str>,
}

pub(crate) fn finish<H, P, F, C>(
    context: FinishContext<'_>,
    herdr: &H,
    process: &P,
    filesystem: &F,
    clock: &C,
) -> Result<(), String>
where
    H: Herdr,
    P: TuiProcess,
    F: StateFilesystem,
    C: Clock,
{
    let FinishContext {
        tool_id,
        code,
        sessions_dir,
        summary_text,
        caller_pane,
        caller_agent,
    } = context;
    let outcome = outcome::decide(
        code,
        summary_text,
        caller_pane.is_some() && caller_agent.is_some(),
    );
    match outcome {
        Outcome::Killed => Err("gitui terminated without an exit code (killed?)".to_string()),
        Outcome::Failed(code) => {
            if !summary_text.trim().is_empty() {
                let _ = state::write_session_file(
                    filesystem,
                    clock,
                    sessions_dir,
                    tool_id,
                    summary_text,
                );
            }
            Err(format!("gitui failed with exit code {code}"))
        }
        Outcome::NoChanges => {
            eprintln!("herdr-gitui: no repository changes detected");
            close_pane(herdr);
            Ok(())
        }
        Outcome::NoAgent => {
            let path =
                state::write_session_file(filesystem, clock, sessions_dir, tool_id, summary_text)?;
            eprintln!(
                "herdr-gitui: no agent in focused pane; summary kept at {}",
                path.display()
            );
            hold_pane(herdr, process);
            Ok(())
        }
        Outcome::Deliver => {
            let path =
                state::write_session_file(filesystem, clock, sessions_dir, tool_id, summary_text)?;
            let Some(target) = caller_pane else {
                return Err("internal: deliver outcome without a caller pane".to_string());
            };
            let prompt = summary::prompt(tool_id, summary_text, &path);
            match herdr.agent_prompt(target, &prompt) {
                Ok(()) => {
                    eprintln!("herdr-gitui: sent session summary to agent pane {target}");
                    state::remove_delivered(filesystem, &path);
                    close_pane(herdr);
                    Ok(())
                }
                Err(error) => Err(format!(
                    "failed to send session summary to agent pane {target}: {error}; summary kept at {}",
                    path.display()
                )),
            }
        }
    }
}

fn close_pane<H: Herdr>(herdr: &H) {
    let popup = teardown::is_popup_session();
    if !popup {
        teardown::restore_caller_tab(herdr, env_non_blank("HERDR_GITUI_CALLER_TAB").as_deref());
    }
    teardown::close_pane(
        herdr,
        popup,
        env_non_blank("HERDR_PANE_ID").as_deref(),
        env_non_blank("HERDR_TAB_ID").as_deref(),
    );
}

fn hold_pane<H: Herdr, P: TuiProcess>(herdr: &H, process: &P) {
    teardown::hold_and_restore(
        herdr,
        teardown::is_popup_session(),
        env_non_blank("HERDR_GITUI_CALLER_TAB").as_deref(),
        || process.hold_pane(HOLD_PROMPT),
    );
}

fn env_non_blank(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

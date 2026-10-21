//! Best-effort pane teardown and caller-tab focus policy.

use crate::features::session::ports::Herdr;

pub(crate) fn is_popup_session() -> bool {
    env_non_blank("HERDR_GITUI_PLACEMENT").as_deref() == Some("popup")
        || env_non_blank("HERDR_PLUGIN_ENTRYPOINT_ID").as_deref() == Some("picker")
}

/// Closes a session pane, falling back to its tab. Popup sessions close
/// naturally with the process and must not issue callbacks against the
/// caller's pane context.
pub(crate) fn close_pane<H: Herdr>(herdr: &H, popup: bool, pane: Option<&str>, tab: Option<&str>) {
    if popup {
        return;
    }
    let mut close_tab = true;
    if let Some(pane) = pane {
        match herdr.close_pane(pane) {
            Ok(()) => close_tab = false,
            Err(error) => eprintln!("herdr-gitui: failed to close pane {pane}: {error}"),
        }
    }
    if close_tab
        && let Some(tab) = tab
        && let Err(error) = herdr.close_tab(tab)
    {
        eprintln!("herdr-gitui: failed to close tab {tab}: {error}");
    }
}

/// Reasserts focus on the caller tab after a pane closes or a held pane
/// finishes. Herdr has already received `--focus` for pane-open operations,
/// so callback failures are warnings and never change the workflow result.
pub(crate) fn restore_caller_tab<H: Herdr>(herdr: &H, tab: Option<&str>) {
    let Some(tab) = tab else {
        return;
    };
    if let Err(error) = herdr.focus_tab(tab) {
        eprintln!("herdr-gitui: warning: failed to focus caller tab {tab}: {error}");
    }
}

/// Runs the pane hold action, then restores the caller tab for tiled panes.
pub(crate) fn hold_and_restore<H: Herdr>(
    herdr: &H,
    popup: bool,
    caller_tab: Option<&str>,
    hold: impl FnOnce(),
) {
    hold();
    if !popup {
        restore_caller_tab(herdr, caller_tab);
    }
}

fn env_non_blank(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

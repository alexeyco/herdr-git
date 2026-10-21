//! Selected-tool pane opening and best-effort focus restoration.

use crate::features::action::ports::{Herdr, PaneOpen, SelectedToolPaneRequest};

/// Opens the selected tool and restores tab focus after Herdr's open callback
/// when the reply identifies a tab. Focus restoration is deliberately
/// best-effort: `--focus` already requests focus during the open operation.
pub(crate) fn open_selected_tool<H: Herdr>(
    herdr: &H,
    request: SelectedToolPaneRequest,
) -> Result<(), String> {
    let tool_id = request.tool_id.clone();
    match herdr.open_selected_tool_pane(request)? {
        PaneOpen::Popup => eprintln!("herdr-gitui: opened {} popup", tool_id),
        PaneOpen::Pane { pane_id, tab_id } => match tab_id {
            Some(tab) => {
                eprintln!(
                    "herdr-gitui: opened {} pane {pane_id} in tab {tab}",
                    tool_id
                );
                let _ = herdr.focus_tab(tab);
            }
            None => eprintln!("herdr-gitui: opened {} pane {pane_id}", tool_id),
        },
    }
    Ok(())
}

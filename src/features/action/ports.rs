//! Integration boundaries used by action workflows.

use crate::settings::{Direction, Placement};

/// The result of opening the selected tool's plugin pane.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PaneOpen {
    Pane {
        pane_id: String,
        tab_id: Option<String>,
    },
    Popup,
}

/// Popup dimensions passed to Herdr as configured tokens (for example `80%`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PopupSize {
    pub(crate) width: Option<String>,
    pub(crate) height: Option<String>,
}

/// Inputs needed to open a selected tool in a plugin pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectedToolPaneRequest {
    pub(crate) placement: Placement,
    pub(crate) direction: Direction,
    pub(crate) popup_size: PopupSize,
    pub(crate) workspace_id: Option<String>,
    pub(crate) cwd: String,
    pub(crate) caller_pane: Option<String>,
    pub(crate) caller_agent: Option<String>,
    pub(crate) caller_tab: Option<String>,
    pub(crate) tool_id: String,
    pub(crate) tool_path: String,
}

/// Inputs forwarded when the action opens the tool picker popup.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PickerPopupRequest {
    pub(crate) cwd: String,
    pub(crate) gitui_path: Option<String>,
    pub(crate) lazygit_path: Option<String>,
    pub(crate) last_tool: Option<String>,
    pub(crate) workspace_id: Option<String>,
    pub(crate) caller_pane: Option<String>,
    pub(crate) caller_agent: Option<String>,
    pub(crate) caller_tab: Option<String>,
    pub(crate) selection_file: Option<String>,
}

/// Herdr operations required by action and pane-opening workflows.
#[cfg_attr(test, mockall::automock)]
pub(crate) trait Herdr {
    fn open_setup_popup(&self) -> Result<(), String>;
    fn open_picker_popup(&self, request: PickerPopupRequest) -> Result<(), String>;
    fn open_selected_tool_pane(&self, request: SelectedToolPaneRequest)
    -> Result<PaneOpen, String>;
    fn close_picker_popup(&self) -> Result<(), String>;
    fn focus_tab(&self, tab_id: String) -> Result<String, String>;
}

/// Preference storage used to seed and remember explicit picker choices.
#[cfg_attr(test, mockall::automock)]
pub(crate) trait Preferences {
    fn last_tool(&self) -> Option<String>;
    fn save_last_tool(&self, tool_id: &str);
}

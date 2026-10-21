//! Routes an action to setup guidance, a picker, or a single installed tool.

use crate::adapters::state::PickerSelectionTicket;
use crate::features::action::context::ActionContext;
use crate::features::action::pane;
use crate::features::action::ports::{
    Herdr, PickerPopupRequest, PopupSize, Preferences, SelectedToolPaneRequest,
};
use crate::features::action::tools::{self, Tool};
use crate::settings;

pub(crate) fn run<H: Herdr, P: Preferences>(herdr: &H, preferences: &P) -> Result<(), String> {
    let context = ActionContext::from_environment()?;
    let settings = settings::Settings::load();
    let tools = tools::discover()?;

    route(herdr, preferences, context, &settings, &tools)
}

/// Routes already-resolved inputs so workflow decisions can be tested without
/// changing the process environment, PATH, or config directory.
pub(super) fn route<H: Herdr, P: Preferences>(
    herdr: &H,
    preferences: &P,
    context: ActionContext,
    settings: &settings::Settings,
    tools: &[Tool],
) -> Result<(), String> {
    if tools.is_empty() {
        herdr.open_setup_popup()?;
        eprintln!("herdr-gitui: opened tool setup popup");
        return Ok(());
    }

    if tools.len() > 1 {
        let last_tool = preferences.last_tool();
        let ticket = if settings.placement == settings::Placement::Popup {
            Some(PickerSelectionTicket::create()?)
        } else {
            None
        };
        let selection_file = ticket
            .as_ref()
            .map(|ticket| {
                ticket
                    .path()
                    .to_str()
                    .ok_or_else(|| "picker handoff path is not valid UTF-8".to_string())
            })
            .transpose()?
            .map(str::to_string);
        herdr.open_picker_popup(PickerPopupRequest {
            cwd: context.cwd.clone(),
            gitui_path: tools
                .iter()
                .find(|tool| tool.id == "gitui")
                .map(|tool| tool.path.clone()),
            lazygit_path: tools
                .iter()
                .find(|tool| tool.id == "lazygit")
                .map(|tool| tool.path.clone()),
            last_tool,
            workspace_id: context.workspace_id.clone(),
            caller_pane: context.caller_pane.clone(),
            caller_agent: context.caller_agent.clone(),
            caller_tab: context.caller_tab.clone(),
            selection_file,
        })?;
        eprintln!("herdr-gitui: opened tool picker popup");
        if let Some(ticket) = ticket {
            return complete_popup_picker(herdr, &ticket, tools, &context, settings);
        }
        return Ok(());
    }

    open_selected_tool(herdr, &tools[0], &context, settings)
}

fn complete_popup_picker<H: Herdr>(
    herdr: &H,
    ticket: &PickerSelectionTicket,
    tools: &[Tool],
    context: &ActionContext,
    settings: &settings::Settings,
) -> Result<(), String> {
    complete_popup_selection(herdr, ticket.wait()?, tools, context, settings)
}

pub(super) fn complete_popup_selection<H: Herdr>(
    herdr: &H,
    selection: Option<&str>,
    tools: &[Tool],
    context: &ActionContext,
    settings: &settings::Settings,
) -> Result<(), String> {
    let Some(tool_id) = selection else {
        return Ok(());
    };
    herdr.close_picker_popup()?;
    let tool = tools
        .iter()
        .find(|tool| tool.id == tool_id)
        .ok_or_else(|| format!("picker returned unavailable tool '{tool_id}'"))?;
    open_selected_tool(herdr, tool, context, settings)
}

fn open_selected_tool<H: Herdr>(
    herdr: &H,
    tool: &Tool,
    context: &ActionContext,
    settings: &settings::Settings,
) -> Result<(), String> {
    pane::open_selected_tool(
        herdr,
        SelectedToolPaneRequest {
            placement: settings.placement,
            direction: settings.direction,
            popup_size: PopupSize {
                width: settings.width.clone(),
                height: settings.height.clone(),
            },
            workspace_id: context.workspace_id.clone(),
            cwd: context.cwd.clone(),
            caller_pane: context.caller_pane.clone(),
            caller_agent: context.caller_agent.clone(),
            caller_tab: context.caller_tab.clone(),
            tool_id: tool.id.to_string(),
            tool_path: tool.path.clone(),
        },
    )
}

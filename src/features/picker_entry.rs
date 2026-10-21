//! Picker entrypoint workflow: selection, preference persistence, and handoff.

use std::path::Path;

use crate::adapters::{herdr::HerdrAdapter, state::StateAdapter, terminal::CrosstermTerminal};
use crate::features::{
    action::{
        context::ActionContext,
        pane,
        ports::{Herdr, PopupSize, Preferences, SelectedToolPaneRequest},
        tools::Tool,
    },
    picker,
};
use crate::settings;

pub(crate) fn run() -> Result<(), String> {
    let selection_file = env_non_blank("HERDR_GIT_SELECTION_FILE");
    let herdr = HerdrAdapter;
    let state = StateAdapter;
    let mut terminal = CrosstermTerminal::new();
    let selection = match picker::run(&mut terminal) {
        Ok(selection) => selection,
        Err(error) => {
            if let Some(path) = selection_file.as_deref() {
                crate::adapters::state::publish_picker_selection(Path::new(path), None)?;
            }
            return Err(error);
        }
    };
    save_choice(&state, selection.as_ref());
    let Some(tool) = selection else {
        if let Some(path) = selection_file.as_deref() {
            crate::adapters::state::publish_picker_selection(Path::new(path), None)?;
        }
        return Ok(());
    };

    if let Some(path) = selection_file.as_deref() {
        return crate::adapters::state::publish_picker_selection(Path::new(path), Some(tool.id));
    }

    let cwd = std::env::current_dir()
        .map_err(|error| format!("cannot read current directory: {error}"))?;
    let cwd = cwd
        .to_str()
        .ok_or_else(|| "repository directory is not valid UTF-8".to_string())?;
    let settings = settings::Settings::load();
    open_confirmed_tool(
        &herdr,
        tool,
        ActionContext {
            cwd: cwd.to_string(),
            workspace_id: env_non_blank("HERDR_GITUI_WORKSPACE_ID"),
            caller_pane: env_non_blank("HERDR_GITUI_CALLER_PANE"),
            caller_agent: env_non_blank("HERDR_GITUI_CALLER_AGENT"),
            caller_tab: env_non_blank("HERDR_GITUI_CALLER_TAB"),
        },
        &settings,
    )
}

fn save_choice<P: Preferences>(preferences: &P, selection: Option<&Tool>) {
    if let Some(tool) = selection {
        preferences.save_last_tool(tool.id);
    }
}

fn open_confirmed_tool<H: Herdr>(
    herdr: &H,
    tool: Tool,
    context: ActionContext,
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
            workspace_id: context.workspace_id,
            cwd: context.cwd,
            caller_pane: context.caller_pane,
            caller_agent: context.caller_agent,
            caller_tab: context.caller_tab,
            tool_id: tool.id.to_string(),
            tool_path: tool.path,
        },
    )
}

fn env_non_blank(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use crate::{
        features::action::{
            context::ActionContext,
            ports::{MockHerdr, MockPreferences, PaneOpen, PopupSize, SelectedToolPaneRequest},
            tools::Tool,
        },
        settings::{Direction, Placement, Settings},
    };

    use super::{open_confirmed_tool, save_choice};

    #[test]
    fn confirmed_choice_is_saved_by_its_tool_id() {
        let mut preferences = MockPreferences::new();
        preferences
            .expect_save_last_tool()
            .withf(|tool_id| tool_id == "lazygit")
            .once()
            .return_const(());

        save_choice(
            &preferences,
            Some(&Tool {
                id: "lazygit",
                path: "/canonical/bin/lazygit".to_string(),
            }),
        );
    }

    #[test]
    fn cancelled_choice_does_not_save_a_preference() {
        let mut preferences = MockPreferences::new();
        preferences.expect_save_last_tool().never();

        save_choice(&preferences, None);
    }

    #[test]
    fn confirmed_tool_opens_the_session_pane_with_its_id_and_canonical_path() {
        let mut herdr = MockHerdr::new();
        herdr
            .expect_open_selected_tool_pane()
            .withf(|request| {
                request
                    == &SelectedToolPaneRequest {
                        placement: Placement::Tab,
                        direction: Direction::Right,
                        popup_size: PopupSize::default(),
                        workspace_id: Some("workspace-1".to_string()),
                        cwd: "/repo".to_string(),
                        caller_pane: Some("pane-1".to_string()),
                        caller_agent: Some("codex".to_string()),
                        caller_tab: Some("tab-1".to_string()),
                        tool_id: "lazygit".to_string(),
                        tool_path: "/canonical/bin/lazygit".to_string(),
                    }
            })
            .once()
            .returning(|_| {
                Ok(PaneOpen::Pane {
                    pane_id: "pane-2".to_string(),
                    tab_id: None,
                })
            });
        let tool = Tool {
            id: "lazygit",
            path: "/canonical/bin/lazygit".to_string(),
        };

        assert_eq!(
            open_confirmed_tool(
                &herdr,
                tool,
                ActionContext {
                    cwd: "/repo".to_string(),
                    workspace_id: Some("workspace-1".to_string()),
                    caller_pane: Some("pane-1".to_string()),
                    caller_agent: Some("codex".to_string()),
                    caller_tab: Some("tab-1".to_string()),
                },
                &Settings::default(),
            ),
            Ok(())
        );
    }
}

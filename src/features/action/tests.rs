use super::context::ActionContext;
use super::pane;
use super::ports::{
    MockHerdr, MockPreferences, PaneOpen, PickerPopupRequest, PopupSize, SelectedToolPaneRequest,
};
use super::tools::Tool;
use super::workflow;
use crate::settings::{Direction, Placement, Settings};
use mockall::Sequence;

fn context() -> ActionContext {
    ActionContext {
        cwd: "/repo".to_string(),
        workspace_id: Some("workspace-1".to_string()),
        caller_pane: Some("pane-1".to_string()),
        caller_agent: Some("codex".to_string()),
        caller_tab: Some("tab-1".to_string()),
    }
}

fn tool(id: &'static str, path: &str) -> Tool {
    Tool {
        id,
        path: path.to_string(),
    }
}

#[test]
fn context_uses_environment_fallbacks_only_when_context_ids_are_absent() {
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let previous = [
        (
            "HERDR_PLUGIN_CONTEXT_JSON",
            std::env::var_os("HERDR_PLUGIN_CONTEXT_JSON"),
        ),
        ("HERDR_WORKSPACE_ID", std::env::var_os("HERDR_WORKSPACE_ID")),
        ("HERDR_PANE_ID", std::env::var_os("HERDR_PANE_ID")),
    ];
    // SAFETY: this test serializes its own process-environment mutations.
    unsafe {
        std::env::set_var(
            "HERDR_PLUGIN_CONTEXT_JSON",
            r#"{"workspace_cwd":"/repo","workspace_id":" ","focused_pane_id":null}"#,
        );
        std::env::set_var("HERDR_WORKSPACE_ID", " workspace-env ");
        std::env::set_var("HERDR_PANE_ID", "pane-env");
    }

    let parsed = ActionContext::from_environment().unwrap();

    unsafe {
        for (key, value) in previous {
            if let Some(value) = value {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }
    }
    assert_eq!(parsed.cwd, "/repo");
    assert_eq!(parsed.workspace_id.as_deref(), Some("workspace-env"));
    assert_eq!(parsed.caller_pane.as_deref(), Some("pane-env"));
    assert_eq!(parsed.caller_agent, None);
}

#[test]
fn one_tool_routes_directly_and_ignores_focus_callback_failure() {
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
                    tool_path: "/bin/lazygit".to_string(),
                }
        })
        .times(1)
        .returning(|_| {
            Ok(PaneOpen::Pane {
                pane_id: "new-pane".to_string(),
                tab_id: Some("new-tab".to_string()),
            })
        });
    herdr
        .expect_focus_tab()
        .withf(|tab| tab == "new-tab")
        .times(1)
        .returning(|_| Err("callback unavailable".to_string()));
    let preferences = MockPreferences::new();

    assert_eq!(
        workflow::route(
            &herdr,
            &preferences,
            context(),
            &Settings::default(),
            &[tool("lazygit", "/bin/lazygit")],
        ),
        Ok(())
    );
}

#[test]
fn two_tools_route_to_picker_with_paths_preference_and_context() {
    let mut herdr = MockHerdr::new();
    herdr
        .expect_open_picker_popup()
        .withf(|request| {
            request
                == &PickerPopupRequest {
                    cwd: "/repo".to_string(),
                    gitui_path: Some("/bin/gitui".to_string()),
                    lazygit_path: Some("/bin/lazygit".to_string()),
                    last_tool: Some("gitui".to_string()),
                    workspace_id: Some("workspace-1".to_string()),
                    caller_pane: Some("pane-1".to_string()),
                    caller_agent: Some("codex".to_string()),
                    caller_tab: Some("tab-1".to_string()),
                    selection_file: None,
                }
        })
        .times(1)
        .returning(|_| Ok(()));
    let mut preferences = MockPreferences::new();
    preferences
        .expect_last_tool()
        .times(1)
        .returning(|| Some("gitui".to_string()));

    assert_eq!(
        workflow::route(
            &herdr,
            &preferences,
            context(),
            &Settings::default(),
            &[tool("gitui", "/bin/gitui"), tool("lazygit", "/bin/lazygit")],
        ),
        Ok(())
    );
}

#[test]
fn popup_picker_selection_closes_picker_before_opening_configured_tool_popup() {
    let mut herdr = MockHerdr::new();
    let mut sequence = Sequence::new();
    herdr
        .expect_close_picker_popup()
        .in_sequence(&mut sequence)
        .once()
        .returning(|| Ok(()));
    herdr
        .expect_open_selected_tool_pane()
        .in_sequence(&mut sequence)
        .withf(|request| {
            request
                == &SelectedToolPaneRequest {
                    placement: Placement::Popup,
                    direction: Direction::Right,
                    popup_size: PopupSize {
                        width: Some("48".to_string()),
                        height: Some("12".to_string()),
                    },
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
        .returning(|_| Ok(PaneOpen::Popup));
    let settings = Settings {
        placement: Placement::Popup,
        width: Some("48".to_string()),
        height: Some("12".to_string()),
        ..Settings::default()
    };

    assert_eq!(
        workflow::complete_popup_selection(
            &herdr,
            Some("lazygit"),
            &[
                tool("gitui", "/canonical/bin/gitui"),
                tool("lazygit", "/canonical/bin/lazygit"),
            ],
            &context(),
            &settings,
        ),
        Ok(())
    );
}

#[test]
fn cancelling_popup_picker_does_not_launch_a_tool_or_save_a_preference() {
    let mut herdr = MockHerdr::new();
    herdr.expect_close_picker_popup().never();
    herdr.expect_open_selected_tool_pane().never();

    assert_eq!(
        workflow::complete_popup_selection(
            &herdr,
            None,
            &[
                tool("gitui", "/canonical/bin/gitui"),
                tool("lazygit", "/canonical/bin/lazygit")
            ],
            &context(),
            &Settings {
                placement: Placement::Popup,
                ..Settings::default()
            },
        ),
        Ok(())
    );
}

#[test]
fn no_tools_opens_setup_and_preserves_callback_error() {
    let mut herdr = MockHerdr::new();
    herdr.expect_open_setup_popup().times(1).returning(|| {
        Err("/herdr plugin pane open failed (exit status: 1): unavailable".to_string())
    });
    let preferences = MockPreferences::new();

    assert_eq!(
        workflow::route(&herdr, &preferences, context(), &Settings::default(), &[]),
        Err("/herdr plugin pane open failed (exit status: 1): unavailable".to_string())
    );
}

#[test]
fn pane_reply_popup_and_pane_are_reported_without_changing_result() {
    let mut popup_herdr = MockHerdr::new();
    popup_herdr
        .expect_open_selected_tool_pane()
        .returning(|_| Ok(PaneOpen::Popup));
    assert_eq!(pane::open_selected_tool(&popup_herdr, request()), Ok(()));

    let mut pane_herdr = MockHerdr::new();
    pane_herdr.expect_open_selected_tool_pane().returning(|_| {
        Ok(PaneOpen::Pane {
            pane_id: "pane-2".to_string(),
            tab_id: None,
        })
    });
    assert_eq!(pane::open_selected_tool(&pane_herdr, request()), Ok(()));
}

#[test]
fn pane_open_error_text_is_returned_unchanged() {
    let mut herdr = MockHerdr::new();
    herdr
        .expect_open_selected_tool_pane()
        .returning(|_| Err("herdr plugin pane open failed (exit status: 1): busy".to_string()));

    assert_eq!(
        pane::open_selected_tool(&herdr, request()),
        Err("herdr plugin pane open failed (exit status: 1): busy".to_string())
    );
}

fn request() -> SelectedToolPaneRequest {
    SelectedToolPaneRequest {
        placement: Placement::Popup,
        direction: Direction::Auto,
        popup_size: PopupSize {
            width: Some("80%".to_string()),
            height: Some("24".to_string()),
        },
        workspace_id: Some("workspace-1".to_string()),
        cwd: "/repo".to_string(),
        caller_pane: Some("pane-1".to_string()),
        caller_agent: Some("codex".to_string()),
        caller_tab: Some("tab-1".to_string()),
        tool_id: "gitui".to_string(),
        tool_path: "/bin/gitui".to_string(),
    }
}

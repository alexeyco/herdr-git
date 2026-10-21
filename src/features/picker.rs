//! Terminal boundary used by the Git TUI picker.

use crate::features::action::tools::Tool;

/// Keys understood by the picker independent of a concrete terminal backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickerKey {
    Up,
    Down,
    Enter,
    Cancel,
    Ignore,
}

/// Terminal lifecycle and input/output required by the picker workflow.
#[cfg_attr(test, mockall::automock)]
pub(crate) trait Terminal {
    fn enter(&mut self) -> Result<(), String>;
    fn draw(&mut self, tool_ids: &[String], selected: usize) -> Result<(), String>;
    fn read_key(&mut self) -> Result<PickerKey, String>;
    fn restore(&mut self) -> Result<(), String>;
}

/// Runs the picker against the paths forwarded by the action process.
pub(crate) fn run<T: Terminal>(terminal: &mut T) -> Result<Option<Tool>, String> {
    let tools = available_tools(
        std::env::var("HERDR_GITUI_PATH").ok().as_deref(),
        std::env::var("HERDR_LAZYGIT_PATH").ok().as_deref(),
    )?;
    if tools.is_empty() {
        return Err("no Git TUI paths were provided to the picker".to_string());
    }

    #[cfg(not(unix))]
    {
        let _ = (terminal, tools);
        Err("the Git TUI picker requires a terminal and is only supported on unix".to_string())
    }
    #[cfg(unix)]
    {
        let saved = std::env::var("HERDR_GIT_LAST_TOOL").ok();
        let selected = initial_selection(&tools, saved.as_deref());
        run_with(&tools, selected, terminal)
    }
}

fn available_tools(
    gitui_path: Option<&str>,
    lazygit_path: Option<&str>,
) -> Result<Vec<Tool>, String> {
    let gitui = gitui_path.filter(|path| !path.trim().is_empty());
    let lazygit = lazygit_path.filter(|path| !path.trim().is_empty());
    if gitui.is_none() && lazygit.is_none() {
        return Err("picker received no available Git TUI paths".to_string());
    }

    let mut tools = Vec::with_capacity(2);
    if let Some(path) = gitui {
        tools.push(Tool {
            id: "gitui",
            path: path.to_string(),
        });
    }
    if let Some(path) = lazygit {
        tools.push(Tool {
            id: "lazygit",
            path: path.to_string(),
        });
    }
    Ok(tools)
}

fn initial_selection(tools: &[Tool], saved: Option<&str>) -> usize {
    saved
        .and_then(|id| tools.iter().position(|tool| tool.id == id))
        .or_else(|| tools.iter().position(|tool| tool.id == "gitui"))
        .unwrap_or(0)
}

fn run_with<T: Terminal>(
    tools: &[Tool],
    mut selected: usize,
    terminal: &mut T,
) -> Result<Option<Tool>, String> {
    if let Err(error) = terminal.enter() {
        let _ = terminal.restore();
        return Err(error);
    }

    let ids = tools
        .iter()
        .map(|tool| tool.id.to_string())
        .collect::<Vec<_>>();
    let outcome = loop {
        if let Err(error) = terminal.draw(&ids, selected) {
            break Err(error);
        }
        match terminal.read_key() {
            Ok(PickerKey::Up) => selected = selected.saturating_sub(1),
            Ok(PickerKey::Down) => selected = (selected + 1).min(tools.len() - 1),
            Ok(PickerKey::Enter) => break Ok(Some(selected)),
            Ok(PickerKey::Cancel) => break Ok(None),
            Ok(PickerKey::Ignore) => {}
            Err(error) => break Err(error),
        }
    };
    let restored = terminal.restore();

    match (outcome, restored) {
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Ok(choice), Ok(())) => Ok(choice.map(|index| tools[index].clone())),
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Mutex};

    use super::*;

    fn tools() -> Vec<Tool> {
        available_tools(Some("/bin/gitui"), Some("/bin/lazygit")).unwrap()
    }

    #[test]
    fn available_tools_keep_gitui_then_lazygit_order_and_ignore_blank_paths() {
        let tools = available_tools(Some(" /bin/gitui "), Some(" ")).unwrap();
        assert_eq!(
            tools,
            vec![Tool {
                id: "gitui",
                path: " /bin/gitui ".to_string()
            }]
        );
    }

    #[test]
    fn saved_choice_precedes_gitui_and_missing_choice_falls_back_to_gitui() {
        let tools = tools();
        assert_eq!(initial_selection(&tools, Some("lazygit")), 1);
        assert_eq!(initial_selection(&tools, Some("missing")), 0);
        assert_eq!(initial_selection(&tools[1..], None), 0);
    }

    #[test]
    fn picker_moves_and_returns_the_selected_tool() {
        let keys = Mutex::new(VecDeque::from([PickerKey::Down, PickerKey::Enter]));
        let mut terminal = MockTerminal::new();
        terminal.expect_enter().once().returning(|| Ok(()));
        terminal.expect_draw().times(2).returning(|_, _| Ok(()));
        terminal
            .expect_read_key()
            .times(2)
            .returning(move || Ok(keys.lock().unwrap().pop_front().expect("queued picker key")));
        terminal.expect_restore().once().returning(|| Ok(()));

        assert_eq!(
            run_with(&tools(), 0, &mut terminal).unwrap(),
            Some(Tool {
                id: "lazygit",
                path: "/bin/lazygit".to_string(),
            })
        );
    }

    #[test]
    fn cancel_restores_terminal_and_returns_no_tool() {
        let mut terminal = MockTerminal::new();
        terminal.expect_enter().once().returning(|| Ok(()));
        terminal.expect_draw().once().returning(|_, _| Ok(()));
        terminal
            .expect_read_key()
            .once()
            .returning(|| Ok(PickerKey::Cancel));
        terminal.expect_restore().once().returning(|| Ok(()));

        assert_eq!(run_with(&tools(), 0, &mut terminal).unwrap(), None);
    }

    #[test]
    fn draw_error_restores_terminal_and_preserves_the_error() {
        let mut terminal = MockTerminal::new();
        terminal.expect_enter().once().returning(|| Ok(()));
        terminal
            .expect_draw()
            .once()
            .returning(|_, _| Err("cannot draw Git TUI picker: failure".to_string()));
        terminal.expect_restore().once().returning(|| Ok(()));

        assert_eq!(
            run_with(&tools(), 0, &mut terminal),
            Err("cannot draw Git TUI picker: failure".to_string())
        );
    }

    #[test]
    fn input_error_restores_terminal_and_preserves_the_error() {
        let mut terminal = MockTerminal::new();
        terminal.expect_enter().once().returning(|| Ok(()));
        terminal.expect_draw().once().returning(|_, _| Ok(()));
        terminal
            .expect_read_key()
            .once()
            .returning(|| Err("cannot read Git TUI picker input: failure".to_string()));
        terminal.expect_restore().once().returning(|| Ok(()));

        assert_eq!(
            run_with(&tools(), 0, &mut terminal),
            Err("cannot read Git TUI picker input: failure".to_string())
        );
    }

    #[test]
    fn selection_returns_restore_error_when_cleanup_fails() {
        let mut terminal = MockTerminal::new();
        terminal.expect_enter().once().returning(|| Ok(()));
        terminal.expect_draw().once().returning(|_, _| Ok(()));
        terminal
            .expect_read_key()
            .once()
            .returning(|| Ok(PickerKey::Enter));
        terminal
            .expect_restore()
            .once()
            .returning(|| Err("cannot restore terminal mode: failure".to_string()));

        assert_eq!(
            run_with(&tools(), 0, &mut terminal),
            Err("cannot restore terminal mode: failure".to_string())
        );
    }

    #[test]
    fn original_render_error_takes_precedence_over_restore_error() {
        let mut terminal = MockTerminal::new();
        terminal.expect_enter().once().returning(|| Ok(()));
        terminal
            .expect_draw()
            .once()
            .returning(|_, _| Err("cannot draw Git TUI picker: failure".to_string()));
        terminal
            .expect_restore()
            .once()
            .returning(|| Err("cannot restore terminal mode: failure".to_string()));

        assert_eq!(
            run_with(&tools(), 0, &mut terminal),
            Err("cannot draw Git TUI picker: failure".to_string())
        );
    }

    #[test]
    fn enter_error_attempts_restoration_and_preserves_the_error() {
        let mut terminal = MockTerminal::new();
        terminal
            .expect_enter()
            .once()
            .returning(|| Err("cannot initialize terminal".to_string()));
        terminal.expect_restore().once().returning(|| Ok(()));

        assert_eq!(
            run_with(&tools(), 0, &mut terminal),
            Err("cannot initialize terminal".to_string())
        );
    }
}

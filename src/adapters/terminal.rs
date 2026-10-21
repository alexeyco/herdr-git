//! Crossterm and Ratatui integration used by the Git TUI picker.

use std::io::{self, Stdout};
use std::process::{Command, Stdio};

use ratatui::{
    Frame, Terminal as RatatuiTerminal,
    backend::CrosstermBackend,
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::features::picker::{PickerKey, Terminal};

/// Real terminal backend for the mockable picker port.
pub(crate) struct CrosstermTerminal {
    terminal: Option<RatatuiTerminal<CrosstermBackend<Stdout>>>,
    saved_mode: Option<String>,
}

impl CrosstermTerminal {
    pub(crate) fn new() -> Self {
        Self {
            terminal: None,
            saved_mode: None,
        }
    }
}

impl Terminal for CrosstermTerminal {
    fn enter(&mut self) -> Result<(), String> {
        let saved = stty(&["-g"])?;
        let saved = saved.trim().to_string();
        if saved.is_empty() {
            return Err("stty -g did not report terminal settings".to_string());
        }
        stty(&["raw", "-echo", "min", "0", "time", "1"])?;
        self.saved_mode = Some(saved);

        let terminal = RatatuiTerminal::new(CrosstermBackend::new(io::stdout()))
            .map_err(|error| format!("cannot initialize Git TUI picker terminal: {error}"))?;
        // Keep the terminal as soon as it exists so `run_with` can restore
        // the cursor if hiding it fails partway through initialization.
        self.terminal = Some(terminal);
        let terminal = self
            .terminal
            .as_mut()
            .ok_or_else(|| "cannot initialize Git TUI picker terminal".to_string())?;
        terminal
            .hide_cursor()
            .map_err(|error| format!("cannot hide terminal cursor: {error}"))?;
        Ok(())
    }

    fn draw(&mut self, tool_ids: &[String], selected: usize) -> Result<(), String> {
        let terminal = self
            .terminal
            .as_mut()
            .ok_or_else(|| "Git TUI picker terminal is not initialized".to_string())?;
        terminal
            .draw(|frame| render_picker(frame, tool_ids, selected))
            .map(|_| ())
            .map_err(|error| format!("cannot draw the Git TUI picker: {error}"))
    }

    fn read_key(&mut self) -> Result<PickerKey, String> {
        let event =
            event::read().map_err(|error| format!("cannot read Git TUI picker input: {error}"))?;
        let Event::Key(key) = event else {
            return Ok(PickerKey::Ignore);
        };
        if key.kind != KeyEventKind::Press {
            return Ok(PickerKey::Ignore);
        }
        Ok(match key.code {
            KeyCode::Up | KeyCode::Char('k') => PickerKey::Up,
            KeyCode::Down | KeyCode::Char('j') => PickerKey::Down,
            KeyCode::Enter => PickerKey::Enter,
            KeyCode::Esc | KeyCode::Char('q') => PickerKey::Cancel,
            _ => PickerKey::Ignore,
        })
    }

    fn restore(&mut self) -> Result<(), String> {
        let saved_mode = self.saved_mode.take();
        restore_components(
            saved_mode.is_some(),
            self.terminal.is_some(),
            || match self.terminal.as_mut() {
                Some(terminal) => terminal
                    .show_cursor()
                    .map_err(|error| format!("cannot restore terminal cursor: {error}")),
                None => Ok(()),
            },
            || match saved_mode.as_deref() {
                Some(saved) => stty(&[saved]).map(|_| ()),
                None => Ok(()),
            },
        )
    }
}

fn render_picker(frame: &mut Frame<'_>, tool_ids: &[String], selected: usize) {
    let width = frame.area().width.saturating_sub(2) as usize;
    let mut lines = Vec::with_capacity(tool_ids.len() + 2);
    lines.push(Line::default());
    lines.extend(
        tool_ids
            .iter()
            .enumerate()
            .map(|(index, tool_id)| {
                let marker = if index == selected { "› " } else { "  " };
                let style = if index == selected {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                Line::from(Span::styled(format!("{marker}{tool_id:<width$}"), style))
            })
            .collect::<Vec<_>>(),
    );
    lines.push(Line::default());
    let base = Style::default().fg(Color::Reset).bg(Color::Reset);
    frame.render_widget(Paragraph::new(lines).style(base), frame.area());
}

fn restore_components(
    raw_mode: bool,
    has_terminal: bool,
    show_cursor: impl FnOnce() -> Result<(), String>,
    disable_raw_mode: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let raw_mode_result = if raw_mode { disable_raw_mode() } else { Ok(()) };
    let cursor = if has_terminal { show_cursor() } else { Ok(()) };
    raw_mode_result.and(cursor)
}

fn stty(args: &[&str]) -> Result<String, String> {
    let mut command = Command::new("stty");
    command.args(args);
    match std::fs::File::open("/dev/tty") {
        Ok(tty) => {
            command.stdin(tty);
        }
        Err(_) => {
            command.stdin(Stdio::inherit());
        }
    }
    let output = command
        .output()
        .map_err(|error| format!("failed to run stty {}: {error}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "stty {} failed with {}",
            args.join(" "),
            output.status
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| format!("stty output is not UTF-8: {error}"))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use ratatui::{
        backend::TestBackend,
        style::{Color, Modifier},
    };

    use super::{render_picker, restore_components};

    #[test]
    fn selected_row_is_reversed_and_fills_the_popup_width() {
        let mut terminal = ratatui::Terminal::new(TestBackend::new(24, 4)).unwrap();
        let tools = vec!["gitui".to_string(), "lazygit".to_string()];

        terminal
            .draw(|frame| render_picker(frame, &tools, 1))
            .unwrap();
        let buffer = terminal.backend().buffer();

        for x in 0..24 {
            assert!(
                buffer[(x, 2)]
                    .style()
                    .add_modifier
                    .contains(Modifier::REVERSED)
            );
            assert!(
                !buffer[(x, 1)]
                    .style()
                    .add_modifier
                    .contains(Modifier::REVERSED)
            );
        }
        assert!(buffer[(0, 2)].symbol().starts_with('›'));
        assert_eq!(buffer[(0, 2)].style().fg, Some(Color::Reset));
        assert_eq!(buffer[(0, 2)].style().bg, Some(Color::Reset));
        for x in 0..24 {
            assert_eq!(buffer[(x, 0)].symbol(), " ");
            assert_eq!(buffer[(x, 3)].symbol(), " ");
        }
    }

    #[test]
    fn partial_initialization_restores_raw_mode_without_a_terminal() {
        let raw_mode = true;
        let cursor_restored = Cell::new(false);
        let raw_mode_restored = Cell::new(false);

        let result = restore_components(
            raw_mode,
            false,
            || {
                cursor_restored.set(true);
                Ok(())
            },
            || {
                raw_mode_restored.set(true);
                Ok(())
            },
        );

        assert_eq!(result, Ok(()));
        assert!(!cursor_restored.get());
        assert!(raw_mode_restored.get());
        assert!(raw_mode);
    }

    #[test]
    fn partial_initialization_after_terminal_creation_restores_cursor_and_raw_mode() {
        let raw_mode = true;
        let cursor_restored = Cell::new(false);
        let raw_mode_restored = Cell::new(false);

        let result = restore_components(
            raw_mode,
            true,
            || {
                cursor_restored.set(true);
                Ok(())
            },
            || {
                raw_mode_restored.set(true);
                Ok(())
            },
        );

        assert_eq!(result, Ok(()));
        assert!(cursor_restored.get());
        assert!(raw_mode_restored.get());
        assert!(raw_mode);
    }
}

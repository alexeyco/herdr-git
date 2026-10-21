//! Interactive selector for GitUI and lazygit.

use crate::gitui::Tool;
use std::io::{IsTerminal, Read, Write};

const HIDE_CURSOR: &str = "\x1b[?25l";
const SHOW_CURSOR: &str = "\x1b[?25h";
const RESET: &str = "\x1b[0m";
const POPUP_HEIGHT: usize = 7;
const HELP: &str = "↑/↓ or k/j: select   Enter: open   Esc/q: cancel";

/// Shows the available tools and returns the user's explicit choice.
/// This UI does not persist the choice or open a session pane.
pub(crate) fn run() -> Result<Option<Tool>, String> {
    let tools = available_tools()?;
    if tools.is_empty() {
        return Err("no Git TUI paths were provided to the picker".to_string());
    }

    #[cfg(unix)]
    {
        let saved = std::env::var("HERDR_GIT_LAST_TOOL").ok();
        let selected = saved
            .as_deref()
            .and_then(|id| tools.iter().position(|tool| tool.id == id))
            .or_else(|| tools.iter().position(|tool| tool.id == "gitui"))
            .unwrap_or(0);
        let mut raw = RawMode::enter()?;
        let mut stdin = std::io::stdin().lock();
        let mut stdout = std::io::stdout().lock();
        let outcome = run_with(
            &tools,
            selected,
            &mut stdin,
            &mut stdout,
            std::io::stdin().is_terminal(),
        );
        drop(stdout);
        drop(stdin);
        let restored = raw.restore();
        match (outcome, restored) {
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Ok(choice), Ok(())) => Ok(choice.map(|index| tools[index].clone())),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tools;
        Err("the Git TUI picker requires a terminal and is only supported on unix".to_string())
    }
}

fn available_tools() -> Result<Vec<Tool>, String> {
    let gitui = std::env::var("HERDR_GITUI_PATH")
        .ok()
        .filter(|v| !v.trim().is_empty());
    let lazygit = std::env::var("HERDR_LAZYGIT_PATH")
        .ok()
        .filter(|v| !v.trim().is_empty());
    if gitui.is_none() && lazygit.is_none() {
        return Err("picker received no available Git TUI paths".to_string());
    }
    let mut tools = Vec::with_capacity(2);
    if let Some(path) = gitui {
        tools.push(Tool { id: "gitui", path });
    }
    if let Some(path) = lazygit {
        tools.push(Tool {
            id: "lazygit",
            path,
        });
    }
    Ok(tools)
}

#[cfg(unix)]
fn run_with<R: Read, W: Write>(
    tools: &[Tool],
    mut selected: usize,
    reader: &mut R,
    writer: &mut W,
    read_timeout: bool,
) -> Result<Option<usize>, String> {
    loop {
        write!(writer, "{HIDE_CURSOR}\x1b[H\x1b[J\r\n")
            .map_err(|e| format!("cannot draw the Git TUI picker: {e}"))?;
        for (index, tool) in tools.iter().enumerate() {
            if index == selected {
                write!(writer, "  > \x1b[1m{}{RESET}\r\n", tool.id)
                    .map_err(|e| format!("cannot draw the Git TUI picker: {e}"))?;
            } else {
                write!(writer, "    {}\r\n", tool.id)
                    .map_err(|e| format!("cannot draw the Git TUI picker: {e}"))?;
            }
        }
        write!(writer, "\r\n\x1b[{POPUP_HEIGHT};1H{HELP}")
            .and_then(|()| writer.flush())
            .map_err(|e| format!("cannot draw the Git TUI picker: {e}"))?;

        let mut key = [0u8; 1];
        match reader.read(&mut key) {
            Ok(0) if read_timeout => continue,
            Ok(0) => return Ok(None),
            Ok(_) => match key[0] {
                b'\r' | b'\n' => return Ok(Some(selected)),
                b'q' => return Ok(None),
                b'k' => selected = selected.saturating_sub(1),
                b'j' => selected = (selected + 1).min(tools.len() - 1),
                0x1b => match read_escape(reader)? {
                    Some(true) => selected = selected.saturating_sub(1),
                    Some(false) => selected = (selected + 1).min(tools.len() - 1),
                    None => return Ok(None),
                },
                _ => {}
            },
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(format!("cannot read Git TUI picker input: {error}")),
        }
    }
}

/// Returns Some(true) for up, Some(false) for down, or None for a bare Escape.
#[cfg(unix)]
fn read_escape<R: Read>(reader: &mut R) -> Result<Option<bool>, String> {
    let mut sequence = [0u8; 2];
    let mut read = 0;
    while read < sequence.len() {
        match reader.read(&mut sequence[read..read + 1]) {
            Ok(0) => return Ok(None),
            Ok(count) => read += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(format!("cannot read Git TUI picker input: {error}")),
        }
    }
    Ok(match sequence {
        [b'[', b'A'] | [b'O', b'A'] => Some(true),
        [b'[', b'B'] | [b'O', b'B'] => Some(false),
        _ => Some(true),
    })
}

#[cfg(unix)]
struct RawMode {
    saved: String,
    restored: bool,
}

#[cfg(unix)]
impl RawMode {
    fn enter() -> Result<Self, String> {
        let saved = stty(&["-g"])?;
        let saved = saved.trim().to_string();
        if saved.is_empty() {
            return Err("stty -g did not report terminal settings".to_string());
        }
        stty(&["raw", "-echo", "min", "0", "time", "1"])?;
        let raw = Self {
            saved,
            restored: false,
        };
        let mut cursor = std::io::stdout();
        cursor
            .write_all(HIDE_CURSOR.as_bytes())
            .and_then(|()| cursor.flush())
            .map_err(|e| format!("cannot hide terminal cursor: {e}"))?;
        Ok(raw)
    }

    fn restore(&mut self) -> Result<(), String> {
        if self.restored {
            return Ok(());
        }
        self.restored = true;
        let terminal = stty(&[&self.saved]);
        let mut cursor = std::io::stdout();
        let visible = cursor
            .write_all(SHOW_CURSOR.as_bytes())
            .and_then(|()| cursor.flush())
            .map_err(|e| format!("cannot restore terminal cursor: {e}"));
        terminal.and(visible)
    }
}

#[cfg(unix)]
impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[cfg(unix)]
fn stty(args: &[&str]) -> Result<String, String> {
    use std::process::{Command, Stdio};
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
        .map_err(|e| format!("failed to run stty {}: {e}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "stty {} failed with {}",
            args.join(" "),
            output.status
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("stty output is not UTF-8: {e}"))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn tools() -> Vec<Tool> {
        vec![
            Tool {
                id: "gitui",
                path: "/bin/gitui".to_string(),
            },
            Tool {
                id: "lazygit",
                path: "/bin/lazygit".to_string(),
            },
        ]
    }

    #[test]
    fn saved_choice_is_preselected_when_available() {
        let items = tools();
        let selected = items
            .iter()
            .position(|tool| tool.id == "lazygit")
            .unwrap_or(0);
        let mut input = &b"\n"[..];
        let mut output = Vec::new();
        assert_eq!(
            run_with(&items, selected, &mut input, &mut output, false).unwrap(),
            Some(1)
        );
    }

    #[test]
    fn user_can_move_and_confirm_a_choice() {
        let items = tools();
        let mut input = &b"j\n"[..];
        let mut output = Vec::new();
        assert_eq!(
            run_with(&items, 0, &mut input, &mut output, false).unwrap(),
            Some(1)
        );
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("lazygit"));
        assert!(!output.contains("Select a Git TUI"));
        assert!(output.contains("\x1b[H\x1b[J\r\n"));
        assert!(output.contains("\r\n\x1b[7;1H↑/↓ or k/j: select   Enter: open   Esc/q: cancel"));
    }

    #[test]
    fn user_can_cancel_the_picker() {
        let items = tools();
        let mut input = &b"q"[..];
        let mut output = Vec::new();
        assert_eq!(
            run_with(&items, 0, &mut input, &mut output, false).unwrap(),
            None
        );
    }
}

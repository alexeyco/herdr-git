//! Install guidance shown when neither supported Git TUI is available.

use std::io::{Read, Write};

const GITUI_URL: &str = "https://github.com/gitui-org/gitui";
const LAZYGIT_URL: &str = "https://github.com/jesseduffield/lazygit";

/// Displays links to both supported tools and waits until the popup is closed.
pub(crate) fn run() -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    writeln!(
        stdout,
        "No supported Git TUI was found on PATH.\r\n\r\nInstall GitUI: {GITUI_URL}\r\nInstall lazygit: {LAZYGIT_URL}\r\n\r\nPress Enter to close."
    )
    .and_then(|()| stdout.flush())
    .map_err(|e| format!("cannot display Git TUI setup guidance: {e}"))?;
    drop(stdout);

    let mut input = [0u8; 1];
    std::io::stdin()
        .read(&mut input)
        .map(|_| ())
        .map_err(|e| format!("cannot wait for setup popup close: {e}"))
}

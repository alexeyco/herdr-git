//! Minimal client over the herdr CLI (`HERDR_BIN_PATH`).

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde_json::Value;

use crate::features::action::ports::{
    Herdr as HerdrPort, PaneOpen as ActionPaneOpen, PickerPopupRequest as ActionPickerRequest,
    SelectedToolPaneRequest as ActionSelectedRequest,
};
use crate::features::session::ports::Herdr as SessionHerdrPort;
use crate::settings::{Direction, Placement};

/// Concrete action-facing adapter over this module's Herdr CLI operations.
pub(crate) struct HerdrAdapter;

impl HerdrPort for HerdrAdapter {
    fn open_setup_popup(&self) -> Result<(), String> {
        open_setup_popup()
    }

    fn open_picker_popup(&self, request: ActionPickerRequest) -> Result<(), String> {
        open_picker_popup(PickerPopupRequest {
            cwd: &request.cwd,
            gitui_path: request.gitui_path.as_deref(),
            lazygit_path: request.lazygit_path.as_deref(),
            last_tool: request.last_tool.as_deref().and_then(|tool| match tool {
                "gitui" => Some("gitui"),
                "lazygit" => Some("lazygit"),
                _ => None,
            }),
            workspace_id: request.workspace_id.as_deref(),
            caller_pane: request.caller_pane.as_deref(),
            caller_agent: request.caller_agent.as_deref(),
            caller_tab: request.caller_tab.as_deref(),
            selection_file: request.selection_file.as_deref(),
        })
    }

    fn open_selected_tool_pane(
        &self,
        request: ActionSelectedRequest,
    ) -> Result<ActionPaneOpen, String> {
        let result = selected_tool_pane_open(SelectedToolPaneRequest {
            context: PaneOpenContext {
                placement: request.placement,
                direction: request.direction,
                popup_size: PopupSize {
                    width: request.popup_size.width.as_deref(),
                    height: request.popup_size.height.as_deref(),
                },
                workspace_id: request.workspace_id.as_deref(),
                cwd: &request.cwd,
                caller_pane: request.caller_pane.as_deref(),
                caller_agent: request.caller_agent.as_deref(),
            },
            tool_id: &request.tool_id,
            tool_path: &request.tool_path,
            caller_tab: request.caller_tab.as_deref(),
        })?;
        Ok(match result {
            PaneOpen::Pane { pane_id, tab_id } => ActionPaneOpen::Pane { pane_id, tab_id },
            PaneOpen::Popup => ActionPaneOpen::Popup,
        })
    }

    fn close_picker_popup(&self) -> Result<(), String> {
        popup_close()
    }

    fn focus_tab(&self, tab_id: String) -> Result<String, String> {
        tab_focus(&tab_id)
    }
}

impl SessionHerdrPort for HerdrAdapter {
    fn require_bin(&self) -> Result<(), String> {
        require_bin()
    }

    fn agent_prompt(&self, target_pane: &str, text: &str) -> Result<(), String> {
        agent_prompt(target_pane, text)
    }

    fn close_pane(&self, pane_id: &str) -> Result<(), String> {
        pane_close(pane_id)
    }

    fn close_tab(&self, tab_id: &str) -> Result<(), String> {
        tab_close(tab_id)
    }

    fn focus_tab(&self, tab_id: &str) -> Result<String, String> {
        tab_focus(tab_id)
    }
}

/// Runs the herdr binary with `args`, returning stdout on success.
fn exec(args: &[&str]) -> Result<String, String> {
    exec_with_bin(args, herdr_bin())
}

fn exec_with_bin(args: &[&str], bin: Result<String, String>) -> Result<String, String> {
    let bin = bin?;
    let output = Command::new(&bin)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run {} {}: {e}", bin, args.join(" ")))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "{} {} failed ({}): {}",
            bin,
            args.join(" "),
            output.status,
            snippet(&stderr)
        ))
    }
}

fn herdr_bin() -> Result<String, String> {
    herdr_bin_from(std::env::var("HERDR_BIN_PATH").ok().as_deref())
}

fn herdr_bin_from(value: Option<&str>) -> Result<String, String> {
    match value {
        Some(v) if !v.trim().is_empty() => Ok(v.to_string()),
        _ => Err(
            "herdr did not provide HERDR_BIN_PATH (plugin API v1 contract); cannot call back into herdr"
                .to_string(),
        ),
    }
}

/// Fails early when `HERDR_BIN_PATH` is missing. Pane mode calls this before
/// starting gitui so a bad environment is reported before the TUI opens.
pub(crate) fn require_bin() -> Result<(), String> {
    herdr_bin().map(|_| ())
}

/// Context preserved from action mode when opening a plugin pane.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PaneOpenContext<'a> {
    pub(crate) placement: Placement,
    pub(crate) direction: Direction,
    pub(crate) popup_size: PopupSize<'a>,
    pub(crate) workspace_id: Option<&'a str>,
    pub(crate) cwd: &'a str,
    pub(crate) caller_pane: Option<&'a str>,
    pub(crate) caller_agent: Option<&'a str>,
}

/// Selected-tool data for opening the session pane.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SelectedToolPaneRequest<'a> {
    pub(crate) context: PaneOpenContext<'a>,
    pub(crate) tool_id: &'a str,
    pub(crate) tool_path: &'a str,
    pub(crate) caller_tab: Option<&'a str>,
}

/// Opens the session pane for the tool selected in action mode.
pub(crate) fn selected_tool_pane_open(
    request: SelectedToolPaneRequest<'_>,
) -> Result<PaneOpen, String> {
    let args = selected_tool_pane_args(request);
    open_pane_args(&args, request.context.placement)
}

fn selected_tool_pane_args(request: SelectedToolPaneRequest<'_>) -> Vec<String> {
    let SelectedToolPaneRequest {
        context,
        tool_id,
        tool_path,
        caller_tab,
    } = request;
    let mut args = pane_open_args(
        context.placement,
        context.direction,
        context.popup_size,
        context.workspace_id,
        context.cwd,
        context.caller_pane,
        context.caller_agent,
    );
    let focus = args.pop().expect("pane_open_args always ends with --focus");
    args.extend([
        "--env".to_string(),
        format!("HERDR_GIT_TOOL={tool_id}"),
        "--env".to_string(),
        format!("HERDR_GIT_TOOL_PATH={tool_path}"),
    ]);
    if let Some(workspace) = context.workspace_id {
        args.extend([
            "--env".to_string(),
            format!("HERDR_GITUI_WORKSPACE_ID={workspace}"),
        ]);
    }
    if context.placement != Placement::Popup
        && let Some(tab) = caller_tab
    {
        args.extend(["--env".to_string(), format!("HERDR_GITUI_CALLER_TAB={tab}")]);
    }
    args.push(focus);
    args
}

#[derive(Debug, Clone, Copy)]
struct SelectionPopupRequest<'a> {
    entrypoint: &'a str,
    cwd: Option<&'a str>,
    gitui_path: Option<&'a str>,
    lazygit_path: Option<&'a str>,
    last_tool: Option<&'a str>,
    workspace_id: Option<&'a str>,
    caller_pane: Option<&'a str>,
    caller_agent: Option<&'a str>,
    caller_tab: Option<&'a str>,
    selection_file: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub(crate) struct PickerPopupRequest<'a> {
    pub(crate) cwd: &'a str,
    pub(crate) gitui_path: Option<&'a str>,
    pub(crate) lazygit_path: Option<&'a str>,
    pub(crate) last_tool: Option<&'static str>,
    pub(crate) workspace_id: Option<&'a str>,
    pub(crate) caller_pane: Option<&'a str>,
    pub(crate) caller_agent: Option<&'a str>,
    pub(crate) caller_tab: Option<&'a str>,
    pub(crate) selection_file: Option<&'a str>,
}

/// Opens the setup popup when neither supported TUI is installed.
pub(crate) fn open_setup_popup() -> Result<(), String> {
    open_selection_popup(SelectionPopupRequest {
        entrypoint: "setup",
        cwd: None,
        gitui_path: None,
        lazygit_path: None,
        last_tool: None,
        workspace_id: None,
        caller_pane: None,
        caller_agent: None,
        caller_tab: None,
        selection_file: None,
    })
}

/// Opens the picker popup, forwarding tool paths, saved preference, and the
/// action context needed when the picker requests a session pane.
pub(crate) fn open_picker_popup(request: PickerPopupRequest<'_>) -> Result<(), String> {
    open_selection_popup(SelectionPopupRequest {
        entrypoint: "picker",
        cwd: Some(request.cwd),
        gitui_path: request.gitui_path,
        lazygit_path: request.lazygit_path,
        last_tool: request.last_tool,
        workspace_id: request.workspace_id,
        caller_pane: request.caller_pane,
        caller_agent: request.caller_agent,
        caller_tab: request.caller_tab,
        selection_file: request.selection_file,
    })
}

fn open_selection_popup(request: SelectionPopupRequest<'_>) -> Result<(), String> {
    let args = selection_popup_args(request);
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    exec(&refs).map(|_| ())
}

fn selection_popup_args(request: SelectionPopupRequest<'_>) -> Vec<String> {
    let mut args = vec![
        "plugin",
        "pane",
        "open",
        "--plugin",
        "alexeyco.git",
        "--entrypoint",
        request.entrypoint,
        "--placement",
        "popup",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();
    if request.entrypoint == "picker" {
        args.extend([
            "--width".to_string(),
            "24".to_string(),
            "--height".to_string(),
            "6".to_string(),
        ]);
    }
    if let Some(dir) = request.cwd {
        args.extend(["--cwd".to_string(), dir.to_string()]);
    }
    for (key, value) in [
        ("HERDR_GITUI_PATH", request.gitui_path),
        ("HERDR_LAZYGIT_PATH", request.lazygit_path),
        ("HERDR_GIT_LAST_TOOL", request.last_tool),
        ("HERDR_GITUI_WORKSPACE_ID", request.workspace_id),
        ("HERDR_GITUI_CALLER_PANE", request.caller_pane),
        ("HERDR_GITUI_CALLER_AGENT", request.caller_agent),
        ("HERDR_GITUI_CALLER_TAB", request.caller_tab),
        ("HERDR_GIT_SELECTION_FILE", request.selection_file),
    ] {
        if let Some(value) = value {
            args.extend(["--env".to_string(), format!("{key}={value}")]);
        }
    }
    args.push("--focus".to_string());
    args
}

fn open_pane_args(args: &[String], placement: Placement) -> Result<PaneOpen, String> {
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let out = exec(&refs)?;
    parse_pane_open(&out, placement)
}

/// Queries the plugin config directory with `herdr plugin config-dir alexeyco.git`.
/// The reply is a bare path line (confirmed on herdr 0.9.5), not a JSON
/// envelope, so nothing is parsed beyond trimming stdout: a single non-blank
/// line is the directory. A blank reply, a multi-line reply, or a failed
/// exec (including the `HERDR_BIN_PATH` guard rejecting it) is an `Err` the
/// settings layer maps to a "config directory unavailable" warning plus
/// defaults — never an action failure.
pub(crate) fn plugin_config_dir() -> Result<String, String> {
    let out = exec(&["plugin", "config-dir", "alexeyco.git"])?;
    parse_config_dir(&out)
}

/// Parses the bare-path `plugin config-dir` reply: trimmed stdout must be
/// exactly one non-blank line, which is the directory.
fn parse_config_dir(out: &str) -> Result<String, String> {
    let mut lines = out
        .trim()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let dir = lines
        .next()
        .ok_or_else(|| "herdr plugin config-dir returned a blank reply".to_string())?;
    if lines.next().is_some() {
        let raw = snippet(out);
        return Err(format!(
            "herdr plugin config-dir returned more than one line: {raw}"
        ));
    }
    Ok(dir.to_string())
}

/// The resolved popup size settings: each value is the configured token
/// (`80` cells or `80%`) passed verbatim into its own flag, present only
/// when the corresponding settings key is valid. Ignored for every
/// non-popup placement, like `direction` for non-split.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PopupSize<'a> {
    /// The `--width` value for a popup open.
    pub(crate) width: Option<&'a str>,
    /// The `--height` value for a popup open.
    pub(crate) height: Option<&'a str>,
}

/// The successful reply of a `herdr plugin pane open`, distinguished by
/// placement: a popup is a singleton session resource rather than a Herdr
/// pane, so its reply carries neither a pane id nor a tab id.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PaneOpen {
    /// A Herdr pane: `result.plugin_pane.pane.pane_id` plus optional tab id.
    Pane {
        pane_id: String,
        tab_id: Option<String>,
    },
    /// A popup open confirmed by the daemon's id-less `{"type":"ok"}` reply.
    Popup,
}

/// The `Placement` argv value, mapped exhaustively so a new variant fails
/// to compile here rather than dropping the flag.
fn placement_value(placement: Placement) -> &'static str {
    match placement {
        Placement::Tab => "tab",
        Placement::Split => "split",
        Placement::Overlay => "overlay",
        Placement::Zoomed => "zoomed",
        Placement::Popup => "popup",
    }
}

/// `herdr plugin pane open` argv in a fixed order: `plugin pane open`,
/// `--plugin alexeyco.git`, `--entrypoint open`, the config-driven
/// `--placement <value>`, `--width <v>`/`--height <v>` immediately after the
/// placement value only for `popup` with the corresponding valid size
/// setting (each pair independently, values verbatim), `--direction
/// right|down` immediately after the placement position only when placement
/// is `split` and the direction is `Right`/`Down` (`Auto` is the settings
/// meaning "do not pass the flag"), `--target-pane <caller-pane>` after the
/// direction position for `split`/`zoomed` with a resolved caller pane, the
/// conditional `--workspace` (only for `tab` — the daemon rejects it on
/// split/zoomed/overlay/popup), `--cwd`, the `HERDR_GITUI_DIR` export, the
/// popup-only `HERDR_GITUI_PLACEMENT=popup` export immediately after it, the
/// optional caller `--env` pairs, and the unconditional trailing `--focus`.
///
/// `split`/`zoomed` without a resolved caller pane degrade to the `tab`
/// shape with a stderr warning (see the function body); `overlay` and
/// `popup` carry neither targeting flag — the daemon anchors them to the
/// active pane itself.
pub(crate) fn pane_open_args(
    placement: Placement,
    direction: Direction,
    popup_size: PopupSize<'_>,
    workspace_id: Option<&str>,
    cwd: &str,
    caller_pane: Option<&str>,
    caller_agent: Option<&str>,
) -> Vec<String> {
    // Live probe against herdr 0.9.5: split/zoomed require `--target-pane`,
    // and the daemon rejects the open with `invalid_params` without one. A
    // raw daemon failure would be unmappable by the user to their own config
    // choice, so degrade to `tab` — which still opens the pane — and name the
    // degraded knob on stderr, consistent with the never-fatal settings
    // philosophy.
    let placement = match (placement, caller_pane) {
        (p @ (Placement::Split | Placement::Zoomed), None) => {
            eprintln!(
                "herdr-gitui: warning: placement {} needs a resolved caller pane to target; \
                 falling back to placement tab",
                placement_value(p)
            );
            Placement::Tab
        }
        (p, _) => p,
    };
    let placement_value = placement_value(placement);
    let mut args = vec![
        "plugin".to_string(),
        "pane".to_string(),
        "open".to_string(),
        "--plugin".to_string(),
        "alexeyco.git".to_string(),
        "--entrypoint".to_string(),
        "open".to_string(),
        // The manifest stays `placement = "tab"`: that value is declarative
        // documentation, while this config-driven `--placement` is what the
        // action actually passes at open time, so the two can drift.
        "--placement".to_string(),
        placement_value.to_string(),
    ];
    // herdr 0.9.5: popup size flags sit immediately after the placement
    // value, each pair independently, with the configured token passed
    // verbatim (`80` cells or `80%`); omitted or invalid size settings emit
    // no flag and the daemon renders its default half-size popup. Values
    // below the popup minimum are clamped by herdr. Non-popup placements
    // never carry the flags.
    if placement == Placement::Popup {
        if let Some(width) = popup_size.width {
            args.push("--width".to_string());
            args.push(width.to_string());
        }
        if let Some(height) = popup_size.height {
            args.push("--height".to_string());
            args.push(height.to_string());
        }
    }
    // herdr 0.9.5 documents `--direction right|down` for `plugin pane open`
    // but does not guarantee acceptance for non-split placements, so a
    // configured direction is ignored (no flag, no warning) unless placement
    // is `split`. `Auto` lets herdr choose: no flag at all.
    if placement == Placement::Split {
        let direction_value = match direction {
            Direction::Right => Some("right"),
            Direction::Down => Some("down"),
            Direction::Auto => None,
        };
        if let Some(dir) = direction_value {
            args.push("--direction".to_string());
            args.push(dir.to_string());
        }
    }
    // split/zoomed anchor on the caller pane; the fallback above guarantees a
    // resolved caller pane whenever the effective placement is still
    // split/zoomed. tab/overlay never carry the flag.
    if matches!(placement, Placement::Split | Placement::Zoomed)
        && let Some(pane) = caller_pane
    {
        args.push("--target-pane".to_string());
        args.push(pane.to_string());
    }
    // Probes 1 and 3: the daemon rejects `--workspace` on split/zoomed
    // together with `--target-pane` ("split and zoomed plugin panes target an
    // existing pane; use target_pane_id") and on overlay or popup outright
    // ("overlay and popup plugin panes target the active pane"). Those
    // placements are pane-anchored, so the pair is emitted only on the tab
    // shape — which also puts `--workspace` back on the fallback-to-tab argv.
    if placement == Placement::Tab
        && let Some(ws) = workspace_id
    {
        args.push("--workspace".to_string());
        args.push(ws.to_string());
    }
    args.push("--cwd".to_string());
    args.push(cwd.to_string());
    // Runtime commands default to the plugin directory as cwd; export the
    // resolved directory so pane mode does not have to trust its own cwd.
    args.push("--env".to_string());
    args.push(format!("HERDR_GITUI_DIR={cwd}"));
    // Popup-only marker, exported immediately after the DIR pair so every
    // non-popup argv stays byte-identical: pane mode gates its teardown on
    // it (the popup process carries the caller pane's plugin context, so a
    // close-the-own-pane call would close the user's agent pane).
    if placement == Placement::Popup {
        args.push("--env".to_string());
        args.push("HERDR_GITUI_PLACEMENT=popup".to_string());
    }
    if let Some(pane) = caller_pane {
        args.push("--env".to_string());
        args.push(format!("HERDR_GITUI_CALLER_PANE={pane}"));
    }
    if let Some(agent) = caller_agent {
        args.push("--env".to_string());
        args.push(format!("HERDR_GITUI_CALLER_AGENT={agent}"));
    }
    args.push("--focus".to_string());
    args
}

/// Parses the `plugin pane open` reply. Non-popup: `result.plugin_pane.pane.
/// pane_id` is required, `...tab_id` is optional (overlay panes may carry no
/// tab). Popup (live-verified on herdr 0.9.5): the daemon answers
/// `{"id":"cli:plugin","result":{"type":"ok"}}` with no pane id at all — a
/// popup is a singleton session resource, not a Herdr pane — so that ok
/// reply is the success shape and is exempt from the pane-id requirement;
/// anything else (a non-`ok` result such as `ui_busy` while another modal is
/// active, or invalid JSON) falls through to the unchanged pane-id error
/// path.
fn parse_pane_open(json: &str, placement: Placement) -> Result<PaneOpen, String> {
    let raw = snippet(json);
    let v: Value = serde_json::from_str(json)
        .map_err(|e| format!("invalid plugin pane open JSON: {e}; raw: {raw}"))?;
    if placement == Placement::Popup
        && v.pointer("/result/type").and_then(Value::as_str) == Some("ok")
    {
        return Ok(PaneOpen::Popup);
    }
    let pane_id = v
        .pointer("/result/plugin_pane/pane/pane_id")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            format!("missing result.plugin_pane.pane.pane_id in plugin pane open JSON; raw: {raw}")
        })?;
    let tab_id = v
        .pointer("/result/plugin_pane/pane/tab_id")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string);
    Ok(PaneOpen::Pane {
        pane_id: pane_id.to_string(),
        tab_id,
    })
}

/// Closes a plugin pane by id.
pub(crate) fn pane_close(pane_id: &str) -> Result<(), String> {
    exec(&["plugin", "pane", "close", pane_id]).map(|_| ())
}

/// Closes a tab by id.
pub(crate) fn tab_close(tab_id: &str) -> Result<(), String> {
    exec(&["tab", "close", tab_id]).map(|_| ())
}

/// `herdr tab focus` argv.
pub(crate) fn tab_focus_args(tab_id: &str) -> Vec<String> {
    vec!["tab".to_string(), "focus".to_string(), tab_id.to_string()]
}

/// Focuses a tab by id. Used as a best-effort re-assert after opening the
/// gitui pane; herdr already applies `--focus` at open.
pub(crate) fn tab_focus(tab_id: &str) -> Result<String, String> {
    tab_focus_with_bin(tab_id, herdr_bin())
}

/// Closes the active picker popup through Herdr's socket API. There is no CLI
/// wrapper for `popup.close`; the action process owns this call so closing the
/// popup cannot terminate the process that opens the selected TUI.
pub(crate) fn popup_close() -> Result<(), String> {
    let path = std::env::var("HERDR_SOCKET_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            "herdr did not provide HERDR_SOCKET_PATH; cannot close picker popup".to_string()
        })?;
    popup_close_at(Path::new(&path))
}

#[cfg(unix)]
fn popup_close_at(path: &Path) -> Result<(), String> {
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(path)
        .map_err(|error| format!("cannot connect to herdr socket {}: {error}", path.display()))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("cannot set herdr socket timeout: {error}"))?;
    popup_close_exchange(&mut stream)
}

fn popup_close_exchange(stream: &mut (impl Read + Write)) -> Result<(), String> {
    let request = serde_json::json!({
        "id": "herdr-git-popup-close",
        "method": "popup.close",
        "params": {},
    });
    serde_json::to_writer(&mut *stream, &request)
        .map_err(|error| format!("cannot encode popup.close request: {error}"))?;
    stream
        .write_all(b"\n")
        .map_err(|error| format!("cannot send popup.close request: {error}"))?;

    let mut response = String::new();
    BufReader::new(&mut *stream)
        .read_line(&mut response)
        .map_err(|error| format!("cannot read popup.close response: {error}"))?;
    let response: Value = serde_json::from_str(&response)
        .map_err(|error| format!("invalid popup.close response: {error}"))?;
    if response["error"]["code"] == "popup_not_open" {
        return Ok(());
    }
    if let Some(error) = response.get("error") {
        return Err(format!("herdr popup.close failed: {error}"));
    }
    if response["id"] != "herdr-git-popup-close" || response.get("result").is_none() {
        return Err(format!("invalid popup.close response: {response}"));
    }
    Ok(())
}

#[cfg(not(unix))]
fn popup_close_at(_path: &Path) -> Result<(), String> {
    Err("popup.close requires a Unix socket".to_string())
}

fn tab_focus_with_bin(tab_id: &str, bin: Result<String, String>) -> Result<String, String> {
    let args = tab_focus_args(tab_id);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    exec_with_bin(&refs, bin)
}

/// Submits `text` to the agent hosted in `target_pane`.
pub(crate) fn agent_prompt(target_pane: &str, text: &str) -> Result<(), String> {
    exec(&["agent", "prompt", target_pane, text]).map(|_| ())
}

/// Trims and truncates a snippet for error messages.
fn snippet(s: &str) -> &str {
    let s = s.trim();
    match s.char_indices().nth(200) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const PANE_OPEN: &str = concat!(
        r#"{"ok":true,"result":{"plugin_pane":{"plugin_id":"alexeyco.git","entrypoint":"open","#,
        r#""pane":{"pane_id":"pane-1","tab_id":"tab-1","workspace_id":"ws-1","cwd":"/repo"}}}}"#
    );
    /// The popup reply verified live on herdr 0.9.5: no pane id, no tab id.
    const POPUP_OK: &str = r#"{"id":"cli:plugin","result":{"type":"ok"}}"#;

    #[test]
    fn selected_session_pane_args_forward_caller_tab() {
        let args = selected_tool_pane_args(SelectedToolPaneRequest {
            context: PaneOpenContext {
                placement: Placement::Tab,
                direction: Direction::Right,
                popup_size: PopupSize::default(),
                workspace_id: None,
                cwd: "/repo",
                caller_pane: None,
                caller_agent: None,
            },
            tool_id: "lazygit",
            tool_path: "/canonical/bin/lazygit",
            caller_tab: Some("tab-1"),
        });
        let got = args.iter().map(String::as_str).collect::<Vec<_>>();

        assert_eq!(
            got,
            [
                "plugin",
                "pane",
                "open",
                "--plugin",
                "alexeyco.git",
                "--entrypoint",
                "open",
                "--placement",
                "tab",
                "--cwd",
                "/repo",
                "--env",
                "HERDR_GITUI_DIR=/repo",
                "--env",
                "HERDR_GIT_TOOL=lazygit",
                "--env",
                "HERDR_GIT_TOOL_PATH=/canonical/bin/lazygit",
                "--env",
                "HERDR_GITUI_CALLER_TAB=tab-1",
                "--focus",
            ]
        );
    }

    #[test]
    fn selected_session_pane_args_are_exact_with_minimal_context() {
        let args = selected_tool_pane_args(SelectedToolPaneRequest {
            context: PaneOpenContext {
                placement: Placement::Tab,
                direction: Direction::Right,
                popup_size: PopupSize::default(),
                workspace_id: None,
                cwd: "/repo",
                caller_pane: None,
                caller_agent: None,
            },
            tool_id: "gitui",
            tool_path: "/canonical/bin/gitui",
            caller_tab: None,
        });
        let got = args.iter().map(String::as_str).collect::<Vec<_>>();

        assert_eq!(
            got,
            [
                "plugin",
                "pane",
                "open",
                "--plugin",
                "alexeyco.git",
                "--entrypoint",
                "open",
                "--placement",
                "tab",
                "--cwd",
                "/repo",
                "--env",
                "HERDR_GITUI_DIR=/repo",
                "--env",
                "HERDR_GIT_TOOL=gitui",
                "--env",
                "HERDR_GIT_TOOL_PATH=/canonical/bin/gitui",
                "--focus",
            ]
        );
    }

    struct MockSocket {
        request: Vec<u8>,
        response: Cursor<Vec<u8>>,
    }

    impl Read for MockSocket {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            self.response.read(buffer)
        }
    }

    impl Write for MockSocket {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.request.extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn popup_close_sends_socket_request_and_accepts_already_closed_popup() {
        for response in [
            r#"{"id":"herdr-git-popup-close","result":{"type":"ok"}}"#,
            r#"{"id":"herdr-git-popup-close","error":{"code":"popup_not_open","message":"no popup"}}"#,
        ] {
            let mut socket = MockSocket {
                request: Vec::new(),
                response: Cursor::new(format!("{response}\n").into_bytes()),
            };
            popup_close_exchange(&mut socket).unwrap();
            let request: Value = serde_json::from_slice(&socket.request).unwrap();
            assert_eq!(request["method"], "popup.close");
            assert_eq!(request["params"], serde_json::json!({}));
        }
    }

    #[test]
    fn parses_pane_open_success() {
        let reply = parse_pane_open(PANE_OPEN, Placement::Tab).unwrap();
        assert_eq!(
            reply,
            PaneOpen::Pane {
                pane_id: "pane-1".to_string(),
                tab_id: Some("tab-1".to_string())
            }
        );
    }

    #[test]
    fn pane_open_missing_pane_id_is_an_error() {
        let json = r#"{"result":{"plugin_pane":{"pane":{"tab_id":"tab-1"}}}}"#;
        let err = parse_pane_open(json, Placement::Tab).unwrap_err();
        assert!(err.contains("result.plugin_pane.pane.pane_id"), "{err}");
    }

    #[test]
    fn pane_open_without_tab_id_is_ok() {
        let json = r#"{"result":{"plugin_pane":{"pane":{"pane_id":"pane-1"}}}}"#;
        assert_eq!(
            parse_pane_open(json, Placement::Tab).unwrap(),
            PaneOpen::Pane {
                pane_id: "pane-1".to_string(),
                tab_id: None
            }
        );
    }

    #[test]
    fn pane_open_invalid_json_reports_raw() {
        let err = parse_pane_open("{not json", Placement::Tab).unwrap_err();
        assert!(err.contains("invalid plugin pane open JSON"), "{err}");
        assert!(err.contains("{not json"), "{err}");
    }

    #[test]
    fn popup_ok_reply_without_pane_id_is_success() {
        // "Popup opens without pane id reply": the id-less ok envelope is a
        // popup success, not the missing-pane-id error.
        assert_eq!(
            parse_pane_open(POPUP_OK, Placement::Popup).unwrap(),
            PaneOpen::Popup
        );
    }

    #[test]
    fn popup_non_ok_reply_fails_through_unchanged_error_path() {
        // A `ui_busy`-style non-ok result still raises the missing-pane-id
        // error with the raw snippet.
        let json = r#"{"id":"cli:plugin","result":{"type":"error","code":"ui_busy"}}"#;
        let err = parse_pane_open(json, Placement::Popup).unwrap_err();
        assert!(err.contains("result.plugin_pane.pane.pane_id"), "{err}");
        assert!(err.contains("ui_busy"), "{err}");
    }

    #[test]
    fn popup_invalid_json_reports_raw() {
        let err = parse_pane_open("{not json", Placement::Popup).unwrap_err();
        assert!(err.contains("invalid plugin pane open JSON"), "{err}");
    }

    #[test]
    fn non_popup_reply_without_pane_id_still_fails() {
        // The exemption is popup-only: the same ok envelope fails for tab.
        let err = parse_pane_open(POPUP_OK, Placement::Tab).unwrap_err();
        assert!(err.contains("result.plugin_pane.pane.pane_id"), "{err}");
    }

    #[test]
    fn parses_config_dir_bare_path() {
        assert_eq!(
            parse_config_dir("/home/user/.config/herdr/plugins/config/gitui\n").unwrap(),
            "/home/user/.config/herdr/plugins/config/gitui"
        );
    }

    #[test]
    fn config_dir_blank_reply_is_an_error() {
        let err = parse_config_dir("  \n\n").unwrap_err();
        assert!(err.contains("blank reply"), "{err}");
    }

    #[test]
    fn config_dir_multiline_reply_is_an_error() {
        let err = parse_config_dir("/one\ntwo\n").unwrap_err();
        assert!(err.contains("more than one line"), "{err}");
    }

    #[test]
    fn pane_open_args_full_flag_order() {
        let args = pane_open_args(
            Placement::Tab,
            Direction::Auto,
            PopupSize::default(),
            Some("ws-1"),
            "/repo",
            Some("pane-9"),
            Some("claude"),
        );
        let got: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            [
                "plugin",
                "pane",
                "open",
                "--plugin",
                "alexeyco.git",
                "--entrypoint",
                "open",
                "--placement",
                "tab",
                "--workspace",
                "ws-1",
                "--cwd",
                "/repo",
                "--env",
                "HERDR_GITUI_DIR=/repo",
                "--env",
                "HERDR_GITUI_CALLER_PANE=pane-9",
                "--env",
                "HERDR_GITUI_CALLER_AGENT=claude",
                "--focus",
            ]
        );
    }

    #[test]
    fn pane_open_args_omit_workspace_and_callers() {
        let args = pane_open_args(
            Placement::Tab,
            Direction::Auto,
            PopupSize::default(),
            None,
            "/repo",
            None,
            None,
        );
        let got: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            [
                "plugin",
                "pane",
                "open",
                "--plugin",
                "alexeyco.git",
                "--entrypoint",
                "open",
                "--placement",
                "tab",
                "--cwd",
                "/repo",
                "--env",
                "HERDR_GITUI_DIR=/repo",
                "--focus",
            ]
        );
    }

    #[test]
    fn pane_open_args_include_only_the_present_env() {
        let args = pane_open_args(
            Placement::Tab,
            Direction::Auto,
            PopupSize::default(),
            None,
            "/repo",
            Some("pane-9"),
            None,
        );
        let got: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            [
                "plugin",
                "pane",
                "open",
                "--plugin",
                "alexeyco.git",
                "--entrypoint",
                "open",
                "--placement",
                "tab",
                "--cwd",
                "/repo",
                "--env",
                "HERDR_GITUI_DIR=/repo",
                "--env",
                "HERDR_GITUI_CALLER_PANE=pane-9",
                "--focus",
            ]
        );
    }

    #[test]
    fn pane_open_args_split_with_down() {
        // Split is pane-anchored: `--target-pane` follows the direction pair
        // and `--workspace` is dropped even with a resolved workspace id.
        let args = pane_open_args(
            Placement::Split,
            Direction::Down,
            PopupSize::default(),
            Some("ws-1"),
            "/repo",
            Some("pane-9"),
            None,
        );
        let got: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            [
                "plugin",
                "pane",
                "open",
                "--plugin",
                "alexeyco.git",
                "--entrypoint",
                "open",
                "--placement",
                "split",
                "--direction",
                "down",
                "--target-pane",
                "pane-9",
                "--cwd",
                "/repo",
                "--env",
                "HERDR_GITUI_DIR=/repo",
                "--env",
                "HERDR_GITUI_CALLER_PANE=pane-9",
                "--focus",
            ]
        );
        assert!(!got.contains(&"--workspace"), "{got:?}");
    }

    #[test]
    fn pane_open_args_split_with_effective_default_right() {
        // Settings layer maps an omitted `direction` key to the default
        // `right`, so the argv carries `--direction right`.
        let args = pane_open_args(
            Placement::Split,
            Direction::Right,
            PopupSize::default(),
            Some("ws-1"),
            "/repo",
            Some("pane-9"),
            None,
        );
        let got: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            [
                "plugin",
                "pane",
                "open",
                "--plugin",
                "alexeyco.git",
                "--entrypoint",
                "open",
                "--placement",
                "split",
                "--direction",
                "right",
                "--target-pane",
                "pane-9",
                "--cwd",
                "/repo",
                "--env",
                "HERDR_GITUI_DIR=/repo",
                "--env",
                "HERDR_GITUI_CALLER_PANE=pane-9",
                "--focus",
            ]
        );
        assert!(!got.contains(&"--workspace"), "{got:?}");
    }

    /// `auto` is `Direction::Auto` at this layer: herdr chooses and no
    /// `--direction` flag is passed at all, per the spec wording "the
    /// configured direction is not `auto`".
    #[test]
    fn pane_open_args_split_with_auto_direction_omits_flag() {
        let args = pane_open_args(
            Placement::Split,
            Direction::Auto,
            PopupSize::default(),
            Some("ws-1"),
            "/repo",
            Some("pane-9"),
            None,
        );
        let got: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            [
                "plugin",
                "pane",
                "open",
                "--plugin",
                "alexeyco.git",
                "--entrypoint",
                "open",
                "--placement",
                "split",
                "--target-pane",
                "pane-9",
                "--cwd",
                "/repo",
                "--env",
                "HERDR_GITUI_DIR=/repo",
                "--env",
                "HERDR_GITUI_CALLER_PANE=pane-9",
                "--focus",
            ]
        );
        assert!(!got.contains(&"--direction"), "{got:?}");
        assert!(!got.contains(&"--workspace"), "{got:?}");
    }

    #[test]
    fn pane_open_args_zoomed_targets_caller_pane() {
        // "Zoomed targets the caller pane": `--target-pane` sits directly
        // after the placement value, with no direction and no workspace.
        let args = pane_open_args(
            Placement::Zoomed,
            Direction::Auto,
            PopupSize::default(),
            Some("ws-1"),
            "/repo",
            Some("pane-9"),
            Some("claude"),
        );
        let got: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            [
                "plugin",
                "pane",
                "open",
                "--plugin",
                "alexeyco.git",
                "--entrypoint",
                "open",
                "--placement",
                "zoomed",
                "--target-pane",
                "pane-9",
                "--cwd",
                "/repo",
                "--env",
                "HERDR_GITUI_DIR=/repo",
                "--env",
                "HERDR_GITUI_CALLER_PANE=pane-9",
                "--env",
                "HERDR_GITUI_CALLER_AGENT=claude",
                "--focus",
            ]
        );
        assert!(!got.contains(&"--direction"), "{got:?}");
        assert!(!got.contains(&"--workspace"), "{got:?}");
    }

    #[test]
    fn pane_open_args_overlay_omits_workspace_and_target() {
        // "Overlay omits workspace targeting": the daemon anchors overlay to
        // the active pane itself — no `--workspace`, no `--target-pane`.
        let args = pane_open_args(
            Placement::Overlay,
            Direction::Auto,
            PopupSize::default(),
            Some("ws-1"),
            "/repo",
            Some("pane-9"),
            None,
        );
        let got: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            [
                "plugin",
                "pane",
                "open",
                "--plugin",
                "alexeyco.git",
                "--entrypoint",
                "open",
                "--placement",
                "overlay",
                "--cwd",
                "/repo",
                "--env",
                "HERDR_GITUI_DIR=/repo",
                "--env",
                "HERDR_GITUI_CALLER_PANE=pane-9",
                "--focus",
            ]
        );
        assert!(!got.contains(&"--workspace"), "{got:?}");
        assert!(!got.contains(&"--target-pane"), "{got:?}");
    }

    #[test]
    fn pane_open_args_popup_with_width_and_height() {
        // "Popup passes size flags": exact argv — size pairs directly after
        // the placement value, values verbatim, popup-only env export right
        // after the DIR pair, and none of the targeting flags despite
        // workspace ws-1, caller pane pane-9, and a direction all resolving.
        let args = pane_open_args(
            Placement::Popup,
            Direction::Right,
            PopupSize {
                width: Some("80"),
                height: Some("80%"),
            },
            Some("ws-1"),
            "/repo",
            Some("pane-9"),
            None,
        );
        let got: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            [
                "plugin",
                "pane",
                "open",
                "--plugin",
                "alexeyco.git",
                "--entrypoint",
                "open",
                "--placement",
                "popup",
                "--width",
                "80",
                "--height",
                "80%",
                "--cwd",
                "/repo",
                "--env",
                "HERDR_GITUI_DIR=/repo",
                "--env",
                "HERDR_GITUI_PLACEMENT=popup",
                "--env",
                "HERDR_GITUI_CALLER_PANE=pane-9",
                "--focus",
            ]
        );
        assert!(!got.contains(&"--workspace"), "{got:?}");
        assert!(!got.contains(&"--target-pane"), "{got:?}");
        assert!(!got.contains(&"--direction"), "{got:?}");
    }

    #[test]
    fn pane_open_args_popup_width_only_omits_height() {
        // Each size pair is emitted independently: a width-only config
        // carries only --width, and no --height token appears.
        let args = pane_open_args(
            Placement::Popup,
            Direction::Auto,
            PopupSize {
                width: Some("80"),
                height: None,
            },
            None,
            "/repo",
            Some("pane-9"),
            Some("claude"),
        );
        let got: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            [
                "plugin",
                "pane",
                "open",
                "--plugin",
                "alexeyco.git",
                "--entrypoint",
                "open",
                "--placement",
                "popup",
                "--width",
                "80",
                "--cwd",
                "/repo",
                "--env",
                "HERDR_GITUI_DIR=/repo",
                "--env",
                "HERDR_GITUI_PLACEMENT=popup",
                "--env",
                "HERDR_GITUI_CALLER_PANE=pane-9",
                "--env",
                "HERDR_GITUI_CALLER_AGENT=claude",
                "--focus",
            ]
        );
        assert!(!got.contains(&"--height"), "{got:?}");
    }

    #[test]
    fn pane_open_args_popup_without_size_keys_emits_no_size_flags() {
        // Omitted/invalid size settings emit no flag; herdr renders its
        // default half-size popup.
        let args = pane_open_args(
            Placement::Popup,
            Direction::Auto,
            PopupSize::default(),
            Some("ws-1"),
            "/repo",
            None,
            None,
        );
        let got: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(&got[7..9], ["--placement", "popup"], "{got:?}");
        assert_eq!(got[9], "--cwd", "no size flags before --cwd: {got:?}");
        assert!(!got.contains(&"--width"), "{got:?}");
        assert!(!got.contains(&"--height"), "{got:?}");
        assert!(got.contains(&"HERDR_GITUI_PLACEMENT=popup"), "{got:?}");
    }

    /// "Size keys ignored for non-popup placements": resolved size values
    /// never reach a non-popup argv, which stays byte-identical to the same
    /// call without sizes.
    #[test]
    fn pane_open_args_size_values_ignored_for_non_popup_placements() {
        let sizes = PopupSize {
            width: Some("80"),
            height: Some("80%"),
        };
        for (placement, caller_pane) in [
            (Placement::Tab, Some("pane-9")),
            (Placement::Split, Some("pane-9")),
            (Placement::Overlay, Some("pane-9")),
            (Placement::Zoomed, Some("pane-9")),
            (Placement::Split, None),
        ] {
            let with_sizes = pane_open_args(
                placement,
                Direction::Auto,
                sizes,
                Some("ws-1"),
                "/repo",
                caller_pane,
                None,
            );
            let without = pane_open_args(
                placement,
                Direction::Auto,
                PopupSize::default(),
                Some("ws-1"),
                "/repo",
                caller_pane,
                None,
            );
            assert_eq!(with_sizes, without, "{placement:?} argv must ignore sizes");
        }
    }

    #[test]
    fn pane_open_args_split_without_caller_pane_falls_back_to_tab() {
        // "Unresolvable caller pane falls back to tab with warning": the argv
        // is the exact tab shape — no direction, no target — and the
        // workspace pair comes back on the tab shape.
        let args = pane_open_args(
            Placement::Split,
            Direction::Down,
            PopupSize::default(),
            Some("ws-1"),
            "/repo",
            None,
            Some("claude"),
        );
        let got: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            [
                "plugin",
                "pane",
                "open",
                "--plugin",
                "alexeyco.git",
                "--entrypoint",
                "open",
                "--placement",
                "tab",
                "--workspace",
                "ws-1",
                "--cwd",
                "/repo",
                "--env",
                "HERDR_GITUI_DIR=/repo",
                "--env",
                "HERDR_GITUI_CALLER_AGENT=claude",
                "--focus",
            ]
        );
        assert!(!got.contains(&"--direction"), "{got:?}");
        assert!(!got.contains(&"--target-pane"), "{got:?}");
    }

    #[test]
    fn pane_open_args_zoomed_without_caller_pane_falls_back_to_tab() {
        let args = pane_open_args(
            Placement::Zoomed,
            Direction::Auto,
            PopupSize::default(),
            None,
            "/repo",
            None,
            None,
        );
        let got: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            [
                "plugin",
                "pane",
                "open",
                "--plugin",
                "alexeyco.git",
                "--entrypoint",
                "open",
                "--placement",
                "tab",
                "--cwd",
                "/repo",
                "--env",
                "HERDR_GITUI_DIR=/repo",
                "--focus",
            ]
        );
        assert!(!got.contains(&"--target-pane"), "{got:?}");
    }

    /// Direction is ignored (no warning, no flag) for non-split placements,
    /// and each placement carries exactly its allowed targeting flags.
    #[test]
    fn pane_open_args_direction_ignored_for_non_split_placements() {
        for (placement, placement_value) in [
            (Placement::Zoomed, "zoomed"),
            (Placement::Overlay, "overlay"),
            (Placement::Tab, "tab"),
            (Placement::Popup, "popup"),
        ] {
            let args = pane_open_args(
                placement,
                Direction::Down,
                PopupSize::default(),
                Some("ws-1"),
                "/repo",
                Some("pane-9"),
                None,
            );
            let got: Vec<&str> = args.iter().map(String::as_str).collect();
            assert_eq!(
                &got[7..9],
                ["--placement", placement_value],
                "configured direction must be ignored for {placement_value}: {got:?}"
            );
            assert!(!got.contains(&"--direction"), "{got:?}");
            let has_target = got.contains(&"--target-pane");
            assert_eq!(
                has_target,
                placement_value == "zoomed",
                "only zoomed targets the caller pane: {got:?}"
            );
            let has_workspace = got.contains(&"--workspace");
            assert_eq!(
                has_workspace,
                placement_value == "tab",
                "only tab keeps workspace targeting: {got:?}"
            );
        }
    }

    #[test]
    fn tab_focus_args_has_command_and_id() {
        let args = tab_focus_args("tab-1");
        let got: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(got, ["tab", "focus", "tab-1"]);
    }

    #[test]
    fn tab_focus_uses_the_shared_bin_path_guard() {
        let expected = "herdr did not provide HERDR_BIN_PATH (plugin API v1 contract); cannot call back into herdr";
        for value in [None, Some("   ")] {
            let err = tab_focus_with_bin("tab-1", herdr_bin_from(value)).unwrap_err();
            assert_eq!(err, expected);
        }
    }

    #[test]
    fn setup_popup_uses_its_dedicated_entrypoint() {
        let args = selection_popup_args(SelectionPopupRequest {
            entrypoint: "setup",
            cwd: None,
            gitui_path: None,
            lazygit_path: None,
            last_tool: None,
            workspace_id: None,
            caller_pane: None,
            caller_agent: None,
            caller_tab: None,
            selection_file: None,
        });
        let got = args.iter().map(String::as_str).collect::<Vec<_>>();
        assert_eq!(
            got,
            [
                "plugin",
                "pane",
                "open",
                "--plugin",
                "alexeyco.git",
                "--entrypoint",
                "setup",
                "--placement",
                "popup",
                "--focus",
            ]
        );
    }

    #[test]
    fn picker_popup_passes_available_tools_preference_and_action_context() {
        let args = selection_popup_args(SelectionPopupRequest {
            entrypoint: "picker",
            cwd: Some("/repo"),
            gitui_path: Some("/bin/gitui"),
            lazygit_path: Some("/bin/lazygit"),
            last_tool: Some("lazygit"),
            workspace_id: Some("ws-1"),
            caller_pane: Some("pane-1"),
            caller_agent: Some("agent-1"),
            caller_tab: Some("tab-1"),
            selection_file: Some("/tmp/picker-ticket"),
        });
        let got = args.iter().map(String::as_str).collect::<Vec<_>>();
        assert_eq!(
            &got[..15],
            [
                "plugin",
                "pane",
                "open",
                "--plugin",
                "alexeyco.git",
                "--entrypoint",
                "picker",
                "--placement",
                "popup",
                "--width",
                "24",
                "--height",
                "6",
                "--cwd",
                "/repo",
            ]
        );
        assert!(
            !got.iter()
                .any(|arg| matches!(*arg, "--workspace" | "--target-pane" | "--direction")),
            "selection popups must not request pane targeting: {got:?}"
        );
        for value in [
            "HERDR_GITUI_PATH=/bin/gitui",
            "HERDR_LAZYGIT_PATH=/bin/lazygit",
            "HERDR_GIT_LAST_TOOL=lazygit",
            "HERDR_GITUI_WORKSPACE_ID=ws-1",
            "HERDR_GITUI_CALLER_PANE=pane-1",
            "HERDR_GITUI_CALLER_AGENT=agent-1",
            "HERDR_GITUI_CALLER_TAB=tab-1",
            "HERDR_GIT_SELECTION_FILE=/tmp/picker-ticket",
        ] {
            assert!(
                got.windows(2).any(|pair| pair == ["--env", value]),
                "{got:?}"
            );
        }
        assert_eq!(got.last(), Some(&"--focus"));
    }

    #[test]
    fn snippet_truncates_long_output() {
        let long = "x".repeat(500);
        assert_eq!(snippet(&long).len(), 200);
    }
}

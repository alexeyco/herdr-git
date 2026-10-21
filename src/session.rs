//! Session orchestration.
//!
//! The binary is dual-mode. Action mode resolves the repository from the plugin
//! context and opens the `gitui` plugin pane, then exits. Pane mode runs
//! inside that pane: it takes a read-only git snapshot, spawns gitui as a
//! direct child on the pane tty, reads its exit code, takes an after snapshot,
//! builds a summary, and delivers it to the agent pane that requested the
//! session.
//!
//! Pane mode keeps summaries under `HERDR_PLUGIN_STATE_DIR/sessions/` as
//! `<pid>-<unix millis>.txt`. The pid keeps two sessions started in the same
//! millisecond from clobbering each other, and kept summaries older than
//! `SESSION_MAX_AGE` are pruned best-effort at pane start.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::context;
use crate::git;
use crate::gitui::{self, Tool};
use crate::herdr;
use crate::settings;
use crate::summary::{self, CommitLine, GitQueries};

/// Printed before blocking on stdin so an auto-closing plugin pane stays open
/// on error and no-agent paths.
const HOLD_PROMPT: &str = "herdr-gitui: press Enter to close";
/// Kept session summaries older than this are pruned at pane start.
const SESSION_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Cap on the delivered agent prompt, well under ARG_MAX (1 MiB macOS / 2 MiB
/// Linux): the summary travels as a single argv element.
const MAX_PROMPT_BYTES: usize = 65536;
/// What to do with the session result.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Summary exists and an agent target is available.
    Deliver,
    /// gitui quit cleanly with nothing to report.
    NoChanges,
    /// Summary exists, but no agent to send it to.
    NoAgent,
    /// gitui exited with an error; the summary (if any) is kept.
    Failed(i32),
    /// gitui was killed (no exit code).
    Killed,
}

/// Decides the outcome from gitui's exit code, summary content, and agent
/// availability.
pub(crate) fn decide(code: Option<i32>, summary_text: &str, has_agent: bool) -> Outcome {
    let Some(rc) = code else {
        return Outcome::Killed;
    };
    if rc != 0 {
        return Outcome::Failed(rc);
    }
    if summary_text.trim().is_empty() {
        return Outcome::NoChanges;
    }
    if has_agent {
        Outcome::Deliver
    } else {
        Outcome::NoAgent
    }
}

/// Prompt text sent to the agent.
pub(crate) fn prompt_text(tool_id: &str, summary_text: &str) -> String {
    format!("{tool_id} session summary:\n\n{}", summary_text.trim())
}

/// Caps the delivered prompt at `max_bytes`, cutting on a UTF-8 boundary and
/// appending a note pointing at the kept summary file when truncation happens.
/// Returns the text unchanged when it already fits.
fn cap_prompt(text: &str, max_bytes: usize, kept_path: &Path) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut cut = max_bytes;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let mut capped = text[..cut].to_string();
    capped.push_str(&format!(
        "\n\n(output truncated; full summary kept at {})",
        kept_path.display()
    ));
    capped
}

/// Action mode: resolve available tools and open the appropriate popup or session pane.
pub(crate) fn run_action() -> Result<(), String> {
    let json = std::env::var("HERDR_PLUGIN_CONTEXT_JSON")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| {
            "HERDR_PLUGIN_CONTEXT_JSON is not set; invoke this action from herdr".to_string()
        })?;
    let ctx = context::parse(&json)?;
    let settings = settings::Settings::load();

    let tools = gitui::find_tools()?;
    if tools.is_empty() {
        herdr::open_setup_popup()?;
        eprintln!("herdr-gitui: opened tool setup popup");
        return Ok(());
    }

    let workspace_id = ctx
        .workspace_id
        .or_else(|| env_non_blank("HERDR_WORKSPACE_ID"));
    let focused_pane_id = ctx
        .focused_pane_id
        .or_else(|| env_non_blank("HERDR_PANE_ID"));
    let focused_pane_agent = ctx.focused_pane_agent;
    let caller_tab = ctx.tab_id;

    if tools.len() > 1 {
        let last_tool = crate::state::read_last_tool();
        let ticket = if settings.placement == settings::Placement::Popup {
            Some(crate::state::PickerSelectionTicket::create()?)
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
            .transpose()?;
        herdr::open_picker_popup(herdr::PickerPopupRequest {
            cwd: &ctx.dir,
            gitui_path: tools
                .iter()
                .find(|tool| tool.id == "gitui")
                .map(|tool| tool.path.as_str()),
            lazygit_path: tools
                .iter()
                .find(|tool| tool.id == "lazygit")
                .map(|tool| tool.path.as_str()),
            last_tool,
            workspace_id: workspace_id.as_deref(),
            caller_pane: focused_pane_id.as_deref(),
            caller_agent: focused_pane_agent.as_deref(),
            caller_tab: caller_tab.as_deref(),
            selection_file,
        })?;
        eprintln!("herdr-gitui: opened tool picker popup");
        if let Some(ticket) = ticket.as_ref() {
            return complete_popup_picker(ticket, &tools, herdr::popup_close, |tool| {
                open_selected_tool(
                    tool,
                    &ctx.dir,
                    workspace_id.as_deref(),
                    focused_pane_id.as_deref(),
                    focused_pane_agent.as_deref(),
                    caller_tab.as_deref(),
                    &settings,
                )
            });
        }
        return Ok(());
    }

    open_selected_tool(
        &tools[0],
        &ctx.dir,
        workspace_id.as_deref(),
        focused_pane_id.as_deref(),
        focused_pane_agent.as_deref(),
        caller_tab.as_deref(),
        &settings,
    )
}

fn complete_popup_picker(
    ticket: &crate::state::PickerSelectionTicket,
    tools: &[Tool],
    close_popup: impl FnOnce() -> Result<(), String>,
    open_tool: impl FnOnce(&Tool) -> Result<(), String>,
) -> Result<(), String> {
    let Some(tool_id) = ticket.wait()? else {
        return Ok(());
    };
    close_popup()?;
    let tool = tools
        .iter()
        .find(|tool| tool.id == tool_id)
        .ok_or_else(|| format!("picker returned unavailable tool '{tool_id}'"))?;
    open_tool(tool)
}

/// Consumes the picker result and either runs it in this popup or opens a
/// separate configured session pane.
pub(crate) fn run_picker() -> Result<(), String> {
    let selection_file = env_non_blank("HERDR_GIT_SELECTION_FILE");
    run_picker_with(
        || {
            let selection = match crate::picker::run() {
                Ok(selection) => selection,
                Err(error) => {
                    if let Some(path) = selection_file.as_deref() {
                        crate::state::publish_picker_selection(Path::new(path), None)?;
                    }
                    return Err(error);
                }
            };
            if selection.is_none()
                && let Some(path) = selection_file.as_deref()
            {
                crate::state::publish_picker_selection(Path::new(path), None)?;
            }
            Ok(selection)
        },
        crate::state::save_last_tool,
        |tool| {
            if let Some(path) = selection_file.as_deref() {
                return crate::state::publish_picker_selection(Path::new(path), Some(tool.id));
            }
            let dir = std::env::current_dir()
                .map_err(|e| format!("cannot read current directory: {e}"))?;
            let dir = dir
                .to_str()
                .ok_or_else(|| "repository directory is not valid UTF-8".to_string())?;
            let workspace_id = env_non_blank("HERDR_GITUI_WORKSPACE_ID");
            let caller_pane = env_non_blank("HERDR_GITUI_CALLER_PANE");
            let caller_agent = env_non_blank("HERDR_GITUI_CALLER_AGENT");
            let caller_tab = env_non_blank("HERDR_GITUI_CALLER_TAB");
            open_selected_tool(
                tool,
                dir,
                workspace_id.as_deref(),
                caller_pane.as_deref(),
                caller_agent.as_deref(),
                caller_tab.as_deref(),
                &settings::Settings::load(),
            )
        },
    )
}

fn run_picker_with(
    picker: impl FnOnce() -> Result<Option<Tool>, String>,
    save: impl FnOnce(&str),
    open: impl FnOnce(&Tool) -> Result<(), String>,
) -> Result<(), String> {
    let Some(tool) = picker()? else {
        return Ok(());
    };
    save(tool.id);
    open(&tool)
}

fn open_selected_tool(
    tool: &Tool,
    dir: &str,
    workspace_id: Option<&str>,
    caller_pane: Option<&str>,
    caller_agent: Option<&str>,
    caller_tab: Option<&str>,
    settings: &settings::Settings,
) -> Result<(), String> {
    open_selected_tool_with(
        selected_tool_pane_request(
            tool,
            dir,
            workspace_id,
            caller_pane,
            caller_agent,
            caller_tab,
            settings,
        ),
        herdr::selected_tool_pane_open,
    )
}

fn selected_tool_pane_request<'a>(
    tool: &'a Tool,
    dir: &'a str,
    workspace_id: Option<&'a str>,
    caller_pane: Option<&'a str>,
    caller_agent: Option<&'a str>,
    caller_tab: Option<&'a str>,
    settings: &'a settings::Settings,
) -> herdr::SelectedToolPaneRequest<'a> {
    let popup_size = herdr::PopupSize {
        width: settings.width.as_deref(),
        height: settings.height.as_deref(),
    };
    herdr::SelectedToolPaneRequest {
        context: herdr::PaneOpenContext {
            placement: settings.placement,
            direction: settings.direction,
            popup_size,
            workspace_id,
            cwd: dir,
            caller_pane,
            caller_agent,
        },
        tool_id: tool.id,
        tool_path: &tool.path,
        caller_tab,
    }
}

fn open_selected_tool_with(
    request: herdr::SelectedToolPaneRequest<'_>,
    open: impl FnOnce(herdr::SelectedToolPaneRequest<'_>) -> Result<herdr::PaneOpen, String>,
) -> Result<(), String> {
    let tool_id = request.tool_id;
    let reply = open(request)?;
    match reply {
        // A popup reply carries no pane id and no tab id: report the popup
        // without identifiers and skip the best-effort `tab focus` re-assert
        // — there is no tab to focus (`--focus` itself stays in the argv).
        herdr::PaneOpen::Popup => eprintln!("herdr-gitui: opened {} popup", tool_id),
        herdr::PaneOpen::Pane { pane_id, tab_id } => match &tab_id {
            Some(tab) => {
                eprintln!(
                    "herdr-gitui: opened {} pane {pane_id} in tab {tab}",
                    tool_id
                );
                // Defensive: re-assert focus best-effort against a
                // client-side rendering race. Errors are deliberately
                // ignored; herdr already applies `--focus` at open.
                let _ = herdr::tab_focus(tab);
            }
            None => eprintln!("herdr-gitui: opened {} pane {pane_id}", tool_id),
        },
    }
    Ok(())
}

/// Pane mode: take snapshots, run gitui, build summary, deliver.
pub(crate) fn run_pane() -> Result<(), String> {
    herdr::require_bin()?;
    // Runtime commands may default to the plugin directory as cwd; prefer the
    // directory the action resolved and exported, falling back to the process
    // cwd when the variable is absent.
    let dir = match env_non_blank("HERDR_GITUI_DIR") {
        Some(d) => PathBuf::from(d),
        None => {
            std::env::current_dir().map_err(|e| format!("cannot read current directory: {e}"))?
        }
    };

    // Read the selected tool passed by action mode; do not rediscover or
    // choose a tool in pane mode.
    let tool_id = env_non_blank("HERDR_GIT_TOOL")
        .ok_or_else(|| "HERDR_GIT_TOOL is not set; reopen this pane from the action".to_string())?;
    let tool_path = env_non_blank("HERDR_GIT_TOOL_PATH").ok_or_else(|| {
        "HERDR_GIT_TOOL_PATH is not set; reopen this pane from the action".to_string()
    })?;

    run_tool_session(&tool_id, &tool_path, dir)
}

fn run_tool_session(tool_id: &str, tool_path: &str, dir: PathBuf) -> Result<(), String> {
    herdr::require_bin()?;

    // Validate the selected executable before any banner or snapshot.
    validate_tool_binary(tool_path)?;

    // Create session state after validating the binary, so a stale handoff
    // fails before any user-visible output or snapshot.
    let sessions = create_sessions_dir(&state_dir())?;
    prune_old_sessions(&sessions, SystemTime::now());

    // Before snapshot.
    let before = git::snapshot(&dir);

    println!("herdr-gitui: starting {tool_id} in {}", dir.display());
    // Both tools discover the repository from cwd; pass no positional path
    // or directory flag and inherit the pane's stdio.
    let status = Command::new(tool_path)
        .current_dir(&dir)
        .status()
        .map_err(|e| format!("failed to run {tool_path} in {}: {e}", dir.display()))?;

    // After snapshot.
    let after = git::snapshot(&dir);

    // Build summary.
    let queries = RealGitQueries { dir: &dir };
    let summary_text = summary::summary(&before, &after, &queries);

    let caller_pane = env_non_blank("HERDR_GITUI_CALLER_PANE");
    let caller_agent = env_non_blank("HERDR_GITUI_CALLER_AGENT");

    finish(
        tool_id,
        status.code(),
        &sessions,
        &summary_text,
        caller_pane.as_deref(),
        caller_agent.as_deref(),
    )
}

fn validate_tool_binary(path: &str) -> Result<(), String> {
    let path_obj = Path::new(path);
    let metadata = std::fs::metadata(path_obj)
        .map_err(|e| format!("selected Git TUI binary {path} is unavailable: {e}"))?;
    if !metadata.is_file() {
        return Err(format!("selected Git TUI binary {path} is not a file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(format!("selected Git TUI binary {path} is not executable"));
        }
    }
    Ok(())
}

/// Real git queries implementation that calls git in the session directory.
struct RealGitQueries<'a> {
    dir: &'a Path,
}

impl<'a> GitQueries for RealGitQueries<'a> {
    fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Option<bool> {
        let args = ["merge-base", "--is-ancestor", ancestor, descendant];
        let out = run_git_quiet(self.dir, &args)?;
        let code = out.status.code();
        match code {
            Some(0) => Some(true),
            Some(1) => Some(false),
            _ => {
                // Exit 1 means "not an ancestor"; any other non-zero exit
                // (e.g. 128 for unknown revision) is a probe failure.
                let stderr = String::from_utf8_lossy(&out.stderr);
                eprintln!(
                    "herdr-gitui: warning: git probe failed (git {}): exit {}",
                    args.join(" "),
                    code.map(|c| c.to_string())
                        .unwrap_or_else(|| "signal".to_string())
                );
                if !stderr.trim().is_empty() {
                    eprintln!("herdr-gitui: warning: {}", stderr.trim());
                }
                None
            }
        }
    }

    fn log_range(&self, from: &str, to: &str) -> Option<Vec<CommitLine>> {
        let args = [
            "log",
            "--no-merges",
            "--reverse",
            "--format=%h%x09%s",
            &format!("{from}..{to}"),
        ];
        let out = run_git_quiet(self.dir, &args)?;
        if !out.status.success() {
            return None;
        }
        Some(parse_commit_lines(&out.stdout))
    }

    fn has_merges_in_range(&self, from: &str, to: &str) -> Option<bool> {
        let args = ["rev-list", "--merges", &format!("{from}..{to}")];
        let out = run_git_quiet(self.dir, &args)?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        Some(!text.trim().is_empty())
    }

    fn log_head(&self, head: &str, n: usize) -> Option<Vec<CommitLine>> {
        let args = [
            "log",
            "--no-merges",
            "--format=%h%x09%s",
            "-n",
            &n.to_string(),
            head,
        ];
        let out = run_git_quiet(self.dir, &args)?;
        if !out.status.success() {
            return None;
        }
        Some(parse_commit_lines(&out.stdout))
    }
}

/// Parses git log output into commit lines. Each line is `<hash>\t<subject>`.
fn parse_commit_lines(data: &[u8]) -> Vec<CommitLine> {
    let text = String::from_utf8_lossy(data);
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let mut parts = line.splitn(2, '\t');
            let hash = parts.next()?.trim().to_string();
            let subject = parts.next().unwrap_or("").to_string();
            if hash.is_empty() {
                None
            } else {
                Some(CommitLine { hash, subject })
            }
        })
        .collect()
}

fn run_git_quiet(dir: &Path, args: &[&str]) -> Option<Output> {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()
}

/// Maps gitui's exit code to the delivery decision. Quiet paths close the
/// pane; error and no-agent paths keep the summary file and block on stdin so
/// the auto-closing pane stays readable.
fn finish(
    tool_id: &str,
    code: Option<i32>,
    sessions_dir: &Path,
    summary_text: &str,
    caller_pane: Option<&str>,
    caller_agent: Option<&str>,
) -> Result<(), String> {
    let has_agent = caller_pane.is_some() && caller_agent.is_some();
    let outcome = decide(code, summary_text, has_agent);

    match outcome {
        Outcome::Killed => {
            // If a summary was already written (shouldn't be at this point),
            // it's kept. No file is written here since we haven't gotten to
            // the write step.
            Err("gitui terminated without an exit code (killed?)".to_string())
        }
        Outcome::Failed(rc) => {
            // Always hold: if a summary exists, write it first so it is kept.
            // Then return the error so main prints it and holds the pane —
            // the pane must never flash-close before the user reads gitui's
            // own error output and the plugin message.
            if !summary_text.trim().is_empty() {
                let _ = write_session_file(sessions_dir, tool_id, summary_text);
            }
            Err(format!("gitui failed with exit code {rc}"))
        }
        Outcome::NoChanges => {
            eprintln!("herdr-gitui: no repository changes detected");
            close_pane();
            Ok(())
        }
        Outcome::NoAgent => {
            let path = write_session_file(sessions_dir, tool_id, summary_text)?;
            eprintln!(
                "herdr-gitui: no agent in focused pane; summary kept at {}",
                path.display()
            );
            hold_pane();
            Ok(())
        }
        Outcome::Deliver => {
            let path = write_session_file(sessions_dir, tool_id, summary_text)?;
            // `decide` returns Deliver only when both caller pane and agent are
            // present, so the pane is always Some here; avoid expect() anyway.
            let Some(target) = caller_pane else {
                return Err("internal: deliver outcome without a caller pane".to_string());
            };
            let prompt = cap_prompt(&prompt_text(tool_id, summary_text), MAX_PROMPT_BYTES, &path);
            match herdr::agent_prompt(target, &prompt) {
                Ok(()) => {
                    eprintln!("herdr-gitui: sent session summary to agent pane {target}");
                    remove(&path);
                    close_pane();
                    Ok(())
                }
                Err(e) => Err(format!(
                    "failed to send session summary to agent pane {target}: {e}; summary kept at {}",
                    path.display()
                )),
            }
        }
    }
}

/// Writes the summary to a session file, returning its path.
fn write_session_file(
    sessions_dir: &Path,
    tool_id: &str,
    summary_text: &str,
) -> Result<PathBuf, String> {
    let path = sessions_dir.join(state_file_name(std::process::id(), unix_millis()?));
    let contents = format!("{} session summary:\n\n{}", tool_id, summary_text);
    std::fs::write(&path, contents)
        .map_err(|e| format!("failed to write session file {}: {e}", path.display()))?;
    Ok(path)
}

/// Best-effort pane teardown: close the pane by id, falling back to closing
/// its tab. Failures are logged, never propagated. Popup sessions are the
/// one gated case: the popup process carries the plugin context of the
/// underlying tiled **caller pane**, so any close-the-own-pane call would
/// close the user's agent pane; the popup closes naturally when this process
/// exits (herdr-side behavior), so the teardown makes zero herdr calls.
fn close_pane() {
    let popup = is_popup_session();
    if !popup {
        restore_caller_tab();
    }
    let messages = close_pane_with(
        popup,
        env_non_blank("HERDR_PANE_ID").as_deref(),
        env_non_blank("HERDR_TAB_ID").as_deref(),
        herdr::pane_close,
        herdr::tab_close,
    );
    for message in messages {
        eprintln!("herdr-gitui: {message}");
    }
}

fn close_pane_with(
    popup: bool,
    pane: Option<&str>,
    tab: Option<&str>,
    mut close_pane: impl FnMut(&str) -> Result<(), String>,
    mut close_tab: impl FnMut(&str) -> Result<(), String>,
) -> Vec<String> {
    if popup {
        return Vec::new();
    }
    let mut messages = Vec::new();
    if let Some(pane) = pane {
        match close_pane(pane) {
            Ok(()) => return messages,
            Err(e) => messages.push(format!("failed to close pane {pane}: {e}")),
        }
    }
    if let Some(tab) = tab
        && let Err(e) = close_tab(tab)
    {
        messages.push(format!("failed to close tab {tab}: {e}"));
    }
    messages
}

/// Blocks the pane process on stdin so an auto-closing plugin pane stays
/// readable on error and no-agent paths. A closed stdin (EOF) returns at once,
/// so the hold never hangs a non-interactive run.
pub(crate) fn hold_pane() {
    eprintln!("{HOLD_PROMPT}");
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    if !is_popup_session() {
        restore_caller_tab();
    }
}

fn is_popup_session() -> bool {
    env_non_blank("HERDR_GITUI_PLACEMENT").as_deref() == Some("popup")
        || env_non_blank("HERDR_PLUGIN_ENTRYPOINT_ID").as_deref() == Some("picker")
}

fn restore_caller_tab() {
    let Some(tab) = env_non_blank("HERDR_GITUI_CALLER_TAB") else {
        return;
    };
    if let Err(error) = herdr::tab_focus(&tab) {
        eprintln!("herdr-gitui: warning: failed to focus caller tab {tab}: {error}");
    }
}

/// State filename: `<pid>-<unix millis>.txt`. The pid keeps two sessions
/// started in the same millisecond from clobbering each other.
fn state_file_name(pid: u32, millis: u128) -> String {
    format!("{pid}-{millis}.txt")
}

/// Removes `*.txt` files directly under `dir` whose mtime is older than
/// `SESSION_MAX_AGE`. Best-effort: a missing dir is ignored, removal failures
/// are logged, and non-`.txt` entries are never touched.
fn prune_old_sessions(dir: &Path, now: SystemTime) {
    for message in prune_old_sessions_with(
        dir,
        now,
        |entry| entry.metadata().and_then(|m| m.modified()),
        |path| std::fs::remove_file(path),
    ) {
        eprintln!("herdr-gitui: warning: {message}");
    }
}

fn prune_old_sessions_with(
    dir: &Path,
    now: SystemTime,
    mut modified: impl FnMut(&std::fs::DirEntry) -> std::io::Result<SystemTime>,
    mut remove_file: impl FnMut(&Path) -> std::io::Result<()>,
) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut messages = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let Ok(modified) = modified(&entry) else {
            continue;
        };
        let Ok(age) = now.duration_since(modified) else {
            continue;
        };
        if age < SESSION_MAX_AGE {
            continue;
        }
        if let Err(e) = remove_file(&path) {
            messages.push(format!("cannot remove {}: {e}", path.display()));
        }
    }
    messages
}

fn state_dir() -> PathBuf {
    state_dir_from(
        std::env::var("HERDR_PLUGIN_STATE_DIR").ok().as_deref(),
        std::env::temp_dir(),
    )
}

fn state_dir_from(value: Option<&str>, temp_dir: PathBuf) -> PathBuf {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or(temp_dir)
}

fn create_sessions_dir(base: &Path) -> Result<PathBuf, String> {
    let sessions = base.join("sessions");
    std::fs::create_dir_all(&sessions)
        .map_err(|e| format!("failed to create state dir {}: {e}", sessions.display()))?;
    Ok(sessions)
}

/// Environment variable trimmed to a non-empty value, or `None`. Shared with
/// the settings layer, which reads `HERDR_PLUGIN_CONFIG_DIR` through it.
pub(crate) fn env_non_blank(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn unix_millis() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .map_err(|e| format!("system clock error: {e}"))
}

/// Removes a session file; already gone counts as success, any other failure
/// is logged, never propagated.
fn remove(path: &Path) {
    if let Err(e) = std::fs::remove_file(path) {
        if e.kind() == std::io::ErrorKind::NotFound {
            return;
        }
        eprintln!(
            "herdr-gitui: warning: cannot remove {}: {e}",
            path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A scratch directory removed when the test ends.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "herdr-gitui-session-test-{tag}-{}-{nanos}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn set_mtime(path: &Path, t: SystemTime) {
        let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(t))
            .unwrap();
    }

    #[test]
    fn prompt_text_has_header_and_trimmed_body() {
        assert_eq!(
            prompt_text("gitui", "  - created commit abc: fix\n"),
            "gitui session summary:\n\n- created commit abc: fix"
        );
    }

    #[test]
    fn picker_confirmation_hands_selected_tool_to_session_pane_opener() {
        let selected = Tool {
            id: "lazygit",
            path: "/canonical/lazygit".to_string(),
        };
        let mut saved = None;
        let mut opened = None;
        run_picker_with(
            || Ok(Some(selected.clone())),
            |tool_id| saved = Some(tool_id.to_string()),
            |tool| {
                opened = Some((tool.id, tool.path.clone()));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(saved.as_deref(), Some("lazygit"));
        assert_eq!(opened, Some(("lazygit", "/canonical/lazygit".to_string())));
    }

    #[test]
    fn picker_forwards_caller_tab_to_selected_session_pane_request() {
        let selected = Tool {
            id: "gitui",
            path: "/canonical/gitui".to_string(),
        };
        let caller_tab = RefCell::new(None);
        run_picker_with(
            || Ok(Some(selected.clone())),
            |_| {},
            |tool| {
                open_selected_tool_with(
                    selected_tool_pane_request(
                        tool,
                        "/repo",
                        None,
                        None,
                        None,
                        Some("T"),
                        &settings::Settings::default(),
                    ),
                    |selected| {
                        *caller_tab.borrow_mut() = selected.caller_tab.map(str::to_string);
                        Ok(herdr::PaneOpen::Pane {
                            pane_id: "pane".to_string(),
                            tab_id: Some("T".to_string()),
                        })
                    },
                )
            },
        )
        .unwrap();
        assert_eq!(caller_tab.borrow().as_deref(), Some("T"));
    }

    #[test]
    fn popup_picker_closes_before_opening_confirmed_tool() {
        let ticket = crate::state::PickerSelectionTicket::create().unwrap();
        let path = ticket.path().to_owned();
        let publisher = std::thread::spawn(move || {
            crate::state::publish_picker_selection(&path, Some("lazygit")).unwrap();
        });
        let tools = [
            Tool {
                id: "gitui",
                path: "/bin/gitui".to_string(),
            },
            Tool {
                id: "lazygit",
                path: "/bin/lazygit".to_string(),
            },
        ];
        let calls = RefCell::new(Vec::new());
        let request = RefCell::new(None);
        complete_popup_picker(
            &ticket,
            &tools,
            || {
                calls.borrow_mut().push("close".to_string());
                Ok(())
            },
            |tool| {
                calls.borrow_mut().push(format!("open:{}", tool.id));
                open_selected_tool_with(
                    selected_tool_pane_request(
                        tool,
                        "/repo",
                        None,
                        None,
                        None,
                        None,
                        &settings::Settings {
                            placement: settings::Placement::Popup,
                            width: Some("80".to_string()),
                            height: Some("24".to_string()),
                            ..settings::Settings::default()
                        },
                    ),
                    |selected| {
                        *request.borrow_mut() = Some((
                            selected.tool_id.to_string(),
                            selected.tool_path.to_string(),
                            selected.context.placement,
                            selected.context.popup_size.width.map(str::to_string),
                            selected.context.popup_size.height.map(str::to_string),
                        ));
                        Ok(herdr::PaneOpen::Popup)
                    },
                )
            },
        )
        .unwrap();
        publisher.join().unwrap();
        assert_eq!(*calls.borrow(), ["close", "open:lazygit"]);
        assert_eq!(
            *request.borrow(),
            Some((
                "lazygit".to_string(),
                "/bin/lazygit".to_string(),
                settings::Placement::Popup,
                Some("80".to_string()),
                Some("24".to_string())
            ))
        );
    }

    #[test]
    fn canceled_popup_picker_does_not_close_or_open_a_tool_popup() {
        let ticket = crate::state::PickerSelectionTicket::create().unwrap();
        let path = ticket.path().to_owned();
        let publisher = std::thread::spawn(move || {
            crate::state::publish_picker_selection(&path, None).unwrap();
        });
        let tools = [Tool {
            id: "gitui",
            path: "/bin/gitui".to_string(),
        }];
        complete_popup_picker(
            &ticket,
            &tools,
            || panic!("cancel must not close popup"),
            |_| panic!("cancel must not open tool popup"),
        )
        .unwrap();
        publisher.join().unwrap();
    }

    #[test]
    fn picker_cancellation_does_not_save_or_open_a_tool() {
        let mut saved = false;
        let mut opened = false;
        run_picker_with(
            || Ok(None),
            |_| saved = true,
            |_| {
                opened = true;
                Ok(())
            },
        )
        .unwrap();
        assert!(!saved);
        assert!(!opened);
    }

    #[test]
    fn picker_still_opens_selected_tool_when_preference_save_fails() {
        let dir = TempDir::new("picker-save-failure");
        let blocker = dir.path().join("not-a-directory");
        std::fs::write(&blocker, "block").unwrap();
        let selected = Tool {
            id: "gitui",
            path: "/canonical/gitui".to_string(),
        };
        let mut opened = false;
        run_picker_with(
            || Ok(Some(selected)),
            |tool_id| {
                let result = crate::state::try_save_last_tool_to(&blocker, tool_id);
                assert!(result.is_err());
            },
            |tool| {
                opened = tool.id == "gitui";
                Ok(())
            },
        )
        .unwrap();
        assert!(opened);
    }

    #[test]
    fn prompt_text_identifies_lazygit() {
        assert_eq!(
            prompt_text("lazygit", "- changed\n"),
            "lazygit session summary:\n\n- changed"
        );
    }

    #[test]
    fn session_files_identify_selected_tool_and_preserve_summary() {
        for (tool_id, expected_header) in [
            ("gitui", "gitui session summary:\n\n"),
            ("lazygit", "lazygit session summary:\n\n"),
        ] {
            let dir = TempDir::new(tool_id);
            let path = write_session_file(dir.path(), tool_id, "- changed\n").unwrap();
            let contents = std::fs::read_to_string(path).unwrap();
            assert_eq!(contents, format!("{expected_header}- changed\n"));
        }
    }

    #[test]
    fn decide_killed_when_no_exit_code() {
        assert_eq!(decide(None, "some summary", true), Outcome::Killed);
        assert_eq!(decide(None, "", false), Outcome::Killed);
    }

    #[test]
    fn decide_nonzero_is_failure() {
        assert_eq!(decide(Some(1), "summary", true), Outcome::Failed(1));
        assert_eq!(decide(Some(2), "", false), Outcome::Failed(2));
    }

    #[test]
    fn decide_rc0_empty_summary_is_no_changes() {
        assert_eq!(decide(Some(0), "", true), Outcome::NoChanges);
        assert_eq!(decide(Some(0), "   \n\t", false), Outcome::NoChanges);
    }

    #[test]
    fn decide_rc0_summary_with_agent_delivers() {
        assert_eq!(decide(Some(0), "- changed", true), Outcome::Deliver);
    }

    #[test]
    fn decide_rc0_summary_without_agent_keeps() {
        assert_eq!(decide(Some(0), "- changed", false), Outcome::NoAgent);
    }

    #[test]
    fn state_file_name_has_pid_and_txt_extension() {
        let name = state_file_name(4242, 1_700_000_000_000);
        assert_eq!(name, "4242-1700000000000.txt");
        assert!(name.starts_with("4242-"), "{name}");
        assert!(name.ends_with(".txt"), "{name}");
    }

    #[test]
    fn cap_prompt_short_text_is_unchanged() {
        let path = Path::new("/state/sessions/1.txt");
        assert_eq!(cap_prompt("short", MAX_PROMPT_BYTES, path), "short");
    }

    #[test]
    fn cap_prompt_truncates_long_ascii_and_notes_path() {
        let path = Path::new("/state/sessions/1.txt");
        let text = "a".repeat(MAX_PROMPT_BYTES + 100);
        let capped = cap_prompt(&text, MAX_PROMPT_BYTES, path);
        assert!(capped.starts_with(&"a".repeat(MAX_PROMPT_BYTES)));
        assert!(
            capped.contains("(output truncated; full summary kept at /state/sessions/1.txt)"),
            "{capped}"
        );
        assert!(capped.len() < text.len());
    }

    #[test]
    fn cap_prompt_does_not_split_multibyte_char() {
        let path = Path::new("/state/sessions/1.txt");
        let text = "é".repeat(100);
        let capped = cap_prompt(&text, 3, path);
        assert!(capped.starts_with('é'), "{capped}");
        assert!(capped.contains("(output truncated"), "{capped}");
    }

    #[test]
    fn prune_removes_only_old_txt_files() {
        let dir = TempDir::new("prune");
        let old = dir.path().join("old.txt");
        let fresh = dir.path().join("fresh.txt");
        let keep = dir.path().join("keep.out");
        std::fs::write(&old, "x").unwrap();
        std::fs::write(&fresh, "x").unwrap();
        std::fs::write(&keep, "x").unwrap();
        let now = SystemTime::now();
        set_mtime(&old, now - Duration::from_secs(8 * 24 * 60 * 60));
        set_mtime(&fresh, now);

        prune_old_sessions(dir.path(), now);

        assert!(!old.exists(), "old .txt should be pruned");
        assert!(fresh.exists(), "fresh .txt should be kept");
        assert!(keep.exists(), "non-.txt entry should be kept");
    }

    #[test]
    fn failed_outcome_keeps_nonempty_summary_file() {
        let dir = TempDir::new("failed-keeps");
        let result = finish("gitui", Some(7), dir.path(), "changed", None, None);
        assert_eq!(result.unwrap_err(), "gitui failed with exit code 7");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn killed_outcome_writes_no_session_file() {
        let dir = TempDir::new("killed-no-file");
        let result = finish("gitui", None, dir.path(), "changed", None, None);
        assert_eq!(
            result.unwrap_err(),
            "gitui terminated without an exit code (killed?)"
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn failed_blank_summary_writes_no_session_file() {
        let dir = TempDir::new("failed-blank");
        let result = finish("gitui", Some(2), dir.path(), " \n\t", None, None);
        assert_eq!(result.unwrap_err(), "gitui failed with exit code 2");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn failed_and_no_agent_write_failures_follow_their_contract() {
        let dir = TempDir::new("write-fails");
        let missing = dir.path().join("missing");
        let failed = finish("gitui", Some(1), &missing, "changed", None, None);
        assert_eq!(failed.unwrap_err(), "gitui failed with exit code 1");
        let no_agent = finish("gitui", Some(0), &missing, "changed", None, None);
        assert!(
            no_agent
                .unwrap_err()
                .starts_with("failed to write session file ")
        );
    }

    #[test]
    fn pane_close_failure_falls_back_to_tab_close() {
        let calls = std::cell::RefCell::new(Vec::new());
        let messages = close_pane_with(
            false,
            Some("pane-1"),
            Some("tab-1"),
            |id| {
                calls.borrow_mut().push(format!("pane {id}"));
                Err("pane unavailable".into())
            },
            |id| {
                calls.borrow_mut().push(format!("tab {id}"));
                Ok(())
            },
        );
        assert_eq!(*calls.borrow(), ["pane pane-1", "tab tab-1"]);
        assert_eq!(messages, ["failed to close pane pane-1: pane unavailable"]);
    }

    #[test]
    fn teardown_with_no_ids_makes_no_calls() {
        let called = std::cell::Cell::new(false);
        let messages = close_pane_with(
            false,
            None,
            None,
            |_| {
                called.set(true);
                Ok(())
            },
            |_| {
                called.set(true);
                Ok(())
            },
        );
        assert!(!called.get());
        assert!(messages.is_empty());
    }

    #[test]
    fn popup_teardown_makes_no_close_calls() {
        let called = std::cell::Cell::new(false);
        let messages = close_pane_with(
            true,
            Some("caller"),
            Some("tab"),
            |_| {
                called.set(true);
                Ok(())
            },
            |_| {
                called.set(true);
                Ok(())
            },
        );
        assert!(!called.get());
        assert!(messages.is_empty());
    }

    #[test]
    fn state_dir_uses_nonblank_override_or_temp_fallback() {
        let temp = PathBuf::from("/tmp/herdr-test");
        assert_eq!(state_dir_from(Some("  "), temp.clone()), temp);
        assert_eq!(state_dir_from(None, temp.clone()), temp);
        assert_eq!(
            state_dir_from(Some(" /var/state "), temp),
            PathBuf::from("/var/state")
        );
    }

    #[test]
    fn unusable_state_dir_fails_when_sessions_directory_cannot_be_created() {
        let dir = TempDir::new("unusable-state");
        let state_path_is_file = dir.path().join("state");
        std::fs::write(&state_path_is_file, "file").unwrap();
        let error = create_sessions_dir(&state_path_is_file).unwrap_err();
        assert!(error.starts_with("failed to create state dir "));
        assert!(error.contains("state/sessions"));
    }

    #[test]
    fn pruning_uses_session_file_metadata_without_reading_contents() {
        let dir = TempDir::new("prune-unreadable-content");
        let summary = dir.path().join("kept.txt");
        std::fs::write(&summary, "private summary").unwrap();
        let now = SystemTime::now();
        set_mtime(&summary, now);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&summary, std::fs::Permissions::from_mode(0o0)).unwrap();
        }
        let messages = prune_old_sessions_with(
            dir.path(),
            now,
            |entry| entry.metadata()?.modified(),
            |path| std::fs::remove_file(path),
        );
        assert!(messages.is_empty());
        assert!(summary.exists());
    }

    #[test]
    fn prune_removes_file_at_exact_seven_day_boundary() {
        let dir = TempDir::new("prune-boundary");
        let boundary = dir.path().join("boundary.txt");
        std::fs::write(&boundary, "x").unwrap();
        let now = SystemTime::now();
        set_mtime(&boundary, now - SESSION_MAX_AGE);
        prune_old_sessions(dir.path(), now);
        assert!(!boundary.exists());
    }

    #[test]
    fn prune_skips_metadata_errors_and_future_mtimes() {
        let dir = TempDir::new("prune-skip");
        let unreadable = dir.path().join("metadata-error.txt");
        let mtime_error = dir.path().join("mtime-error.txt");
        let future = dir.path().join("future.txt");
        std::fs::write(&unreadable, "x").unwrap();
        std::fs::write(&mtime_error, "x").unwrap();
        std::fs::write(&future, "x").unwrap();
        let now = SystemTime::now();
        set_mtime(&future, now + Duration::from_secs(60));
        let messages = prune_old_sessions_with(
            dir.path(),
            now,
            |entry| {
                if entry.file_name() == "metadata-error.txt" {
                    Err(std::io::Error::other("metadata unavailable"))
                } else if entry.file_name() == "mtime-error.txt" {
                    let _metadata = entry.metadata()?;
                    Err(std::io::Error::other("mtime unavailable"))
                } else {
                    entry.metadata()?.modified()
                }
            },
            |path| std::fs::remove_file(path),
        );
        assert!(messages.is_empty());
        assert!(unreadable.exists());
        assert!(mtime_error.exists());
        assert!(future.exists());
    }

    #[test]
    fn prune_logs_vanished_file_and_continues_after_removal_failure() {
        let dir = TempDir::new("prune-remove-errors");
        let vanished = dir.path().join("vanished.txt");
        let removable = dir.path().join("removable.txt");
        std::fs::write(&vanished, "x").unwrap();
        std::fs::write(&removable, "x").unwrap();
        let now = SystemTime::now();
        for path in [&vanished, &removable] {
            set_mtime(path, now - SESSION_MAX_AGE - Duration::from_secs(1));
        }
        let messages = prune_old_sessions_with(
            dir.path(),
            now,
            |entry| entry.metadata()?.modified(),
            |path| {
                if path == vanished {
                    Err(std::io::Error::from(std::io::ErrorKind::NotFound))
                } else {
                    std::fs::remove_file(path)
                }
            },
        );
        assert_eq!(messages.len(), 1);
        assert!(messages[0].starts_with("cannot remove "));
        assert!(messages[0].contains("vanished.txt"));
        assert!(!removable.exists(), "a later removal must still run");
    }

    #[test]
    fn prune_missing_dir_is_noop() {
        let dir = TempDir::new("prune-missing");
        let missing = dir.path().join("does-not-exist");
        prune_old_sessions(&missing, SystemTime::now());
    }

    #[test]
    fn parse_commit_lines_handles_tab_separated() {
        let data = b"abc1234\tfix: handle edge case\ndef5678\tfeat: add thing\n";
        let lines = parse_commit_lines(data);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].hash, "abc1234");
        assert_eq!(lines[0].subject, "fix: handle edge case");
        assert_eq!(lines[1].hash, "def5678");
    }

    #[test]
    fn parse_commit_lines_skips_empty_lines() {
        let data = b"abc1234\tfix\n\n\ndef5678\tfeat\n";
        let lines = parse_commit_lines(data);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn parse_commit_lines_empty_input() {
        let lines = parse_commit_lines(b"");
        assert!(lines.is_empty());
    }
}

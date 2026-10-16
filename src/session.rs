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
use crate::gitui;
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
/// Prompt header delivered to the agent.
const PROMPT_HEADER: &str = "gitui session summary:\n\n";

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
pub(crate) fn prompt_text(summary_text: &str) -> String {
    format!("{PROMPT_HEADER}{}", summary_text.trim())
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

/// Action mode: resolve the repository and open the gitui pane, then exit.
pub(crate) fn run_action() -> Result<(), String> {
    let json = std::env::var("HERDR_PLUGIN_CONTEXT_JSON")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| {
            "HERDR_PLUGIN_CONTEXT_JSON is not set; invoke this action from herdr".to_string()
        })?;
    let ctx = context::parse(&json)?;

    // Read the pane layout settings exactly once per action launch, before
    // the pane open. `Settings::load` never fails: every config error is
    // already degraded to defaults plus stderr warnings inside it, so no
    // config problem can abort the action here. Pane mode reads no config.
    let settings = settings::Settings::load();

    let workspace_id = ctx
        .workspace_id
        .or_else(|| env_non_blank("HERDR_WORKSPACE_ID"));
    let focused_pane_id = ctx
        .focused_pane_id
        .or_else(|| env_non_blank("HERDR_PANE_ID"));
    let focused_pane_agent = ctx.focused_pane_agent;

    let popup_size = herdr::PopupSize {
        width: settings.width.as_deref(),
        height: settings.height.as_deref(),
    };
    let reply = herdr::plugin_pane_open(
        settings.placement,
        settings.direction,
        popup_size,
        workspace_id.as_deref(),
        &ctx.dir,
        focused_pane_id.as_deref(),
        focused_pane_agent.as_deref(),
    )?;
    match reply {
        // A popup reply carries no pane id and no tab id: report the popup
        // without identifiers and skip the best-effort `tab focus` re-assert
        // — there is no tab to focus (`--focus` itself stays in the argv).
        herdr::PaneOpen::Popup => eprintln!("herdr-gitui: opened gitui popup"),
        herdr::PaneOpen::Pane { pane_id, tab_id } => match &tab_id {
            Some(tab) => {
                eprintln!("herdr-gitui: opened gitui pane {pane_id} in tab {tab}");
                // Defensive: re-assert focus best-effort against a
                // client-side rendering race. Errors are deliberately
                // ignored; herdr already applies `--focus` at open.
                let _ = herdr::tab_focus(tab);
            }
            None => eprintln!("herdr-gitui: opened gitui pane {pane_id}"),
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

    let sessions = state_dir().join("sessions");
    std::fs::create_dir_all(&sessions)
        .map_err(|e| format!("failed to create state dir {}: {e}", sessions.display()))?;
    prune_old_sessions(&sessions, SystemTime::now());

    // Resolve gitui before anything else so a missing binary fails before
    // any user-visible output.
    let gitui_bin = gitui::find_gitui()?;

    // Before snapshot.
    let before = git::snapshot(&dir);

    println!("herdr-gitui: starting gitui in {}", dir.display());
    // gitui takes no positional path argument (gitui 0.28 rejects one with
    // "error: unexpected argument") and its -d/--directory flag is the git dir
    // (GIT_DIR), not the repo root. Discover the repo from cwd instead.
    let status = Command::new(&gitui_bin)
        .current_dir(&dir)
        .status()
        .map_err(|e| format!("failed to run {gitui_bin} in {}: {e}", dir.display()))?;

    // After snapshot.
    let after = git::snapshot(&dir);

    // Build summary.
    let queries = RealGitQueries { dir: &dir };
    let summary_text = summary::summary(&before, &after, &queries);

    let caller_pane = env_non_blank("HERDR_GITUI_CALLER_PANE");
    let caller_agent = env_non_blank("HERDR_GITUI_CALLER_AGENT");

    finish(
        status.code(),
        &sessions,
        &summary_text,
        caller_pane.as_deref(),
        caller_agent.as_deref(),
    )
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
                let _ = write_session_file(sessions_dir, summary_text);
            }
            Err(format!("gitui failed with exit code {rc}"))
        }
        Outcome::NoChanges => {
            eprintln!("herdr-gitui: no repository changes detected");
            close_pane();
            Ok(())
        }
        Outcome::NoAgent => {
            let path = write_session_file(sessions_dir, summary_text)?;
            eprintln!(
                "herdr-gitui: no agent in focused pane; summary kept at {}",
                path.display()
            );
            hold_pane();
            Ok(())
        }
        Outcome::Deliver => {
            let path = write_session_file(sessions_dir, summary_text)?;
            // `decide` returns Deliver only when both caller pane and agent are
            // present, so the pane is always Some here; avoid expect() anyway.
            let Some(target) = caller_pane else {
                return Err("internal: deliver outcome without a caller pane".to_string());
            };
            let prompt = cap_prompt(&prompt_text(summary_text), MAX_PROMPT_BYTES, &path);
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
fn write_session_file(sessions_dir: &Path, summary_text: &str) -> Result<PathBuf, String> {
    let path = sessions_dir.join(state_file_name(std::process::id(), unix_millis()?));
    std::fs::write(&path, summary_text)
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
    if env_non_blank("HERDR_GITUI_PLACEMENT").as_deref() == Some("popup") {
        return;
    }
    if let Some(pane) = env_non_blank("HERDR_PANE_ID") {
        match herdr::pane_close(&pane) {
            Ok(()) => return,
            Err(e) => eprintln!("herdr-gitui: failed to close pane {pane}: {e}"),
        }
    }
    if let Some(tab) = env_non_blank("HERDR_TAB_ID")
        && let Err(e) = herdr::tab_close(&tab)
    {
        eprintln!("herdr-gitui: failed to close tab {tab}: {e}");
    }
}

/// Blocks the pane process on stdin so an auto-closing plugin pane stays
/// readable on error and no-agent paths. A closed stdin (EOF) returns at once,
/// so the hold never hangs a non-interactive run.
pub(crate) fn hold_pane() {
    eprintln!("{HOLD_PROMPT}");
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
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
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        let Ok(age) = now.duration_since(modified) else {
            continue;
        };
        if age < SESSION_MAX_AGE {
            continue;
        }
        if let Err(e) = std::fs::remove_file(&path) {
            eprintln!(
                "herdr-gitui: warning: cannot remove {}: {e}",
                path.display()
            );
        }
    }
}

fn state_dir() -> PathBuf {
    env_non_blank("HERDR_PLUGIN_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
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
            prompt_text("  - created commit abc: fix\n"),
            "gitui session summary:\n\n- created commit abc: fix"
        );
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

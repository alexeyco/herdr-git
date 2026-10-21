//! End-to-end tests for the dual-mode plugin binary.
//!
//! Each test spawns the real binary (`CARGO_BIN_EXE_herdr-gitui`) with a
//! hermetic environment, a stub `herdr` callback, and fake Git TUI binaries.
//! The stubs append their argv to `$STUB_LOG`; assertions grep that log.
//! Everything runs in a unique temp directory and needs no network or global
//! git config.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

/// Stub `herdr`: logs every call, answers `plugin pane open` with a canned JSON
/// reply (the popup-shaped `{"id":"cli:plugin","result":{"type":"ok"}}` with no
/// pane id when `$STUB_PANE_OPEN` is set, else a pane id and tab id), and
/// answers `plugin config-dir` with the bare path in
/// `$STUB_CONFIG_DIR` (a blank reply when that variable is unset; a non-zero
/// exit when `$STUB_CONFIG_DIR_FAIL` is set). Exits 0 otherwise.
const HERDR_STUB: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "$STUB_LOG"
if [ "$1" = "plugin" ] && [ "$2" = "pane" ] && [ "$3" = "open" ]; then
  if [ -n "$STUB_PANE_OPEN" ]; then
    printf '%s\n' "$STUB_PANE_OPEN"
  else
    printf '%s\n' '{"result":{"plugin_pane":{"pane":{"pane_id":"pane-stub","tab_id":"tab-stub"}}}}'
  fi
elif [ "$1" = "plugin" ] && [ "$2" = "config-dir" ]; then
  if [ -n "$STUB_CONFIG_DIR_FAIL" ]; then
    printf 'config store unavailable\n' >&2
    exit 1
  fi
  if [ -n "$STUB_CONFIG_DIR" ]; then
    printf '%s\n' "$STUB_CONFIG_DIR"
  fi
elif [ "$1" = "tab" ] && [ "$2" = "focus" ] && [ -n "$STUB_FOCUS_FAIL" ]; then
  printf 'focus unavailable\n' >&2
  exit 1
fi
exit 0
"#;

/// Stub `gitui`: logs `GITUI <argv>`, runs `$GITUI_SCRIPT` if set (a shell
/// script that mutates the test repo between snapshots), then exits
/// `$GITUI_RC` (default 0).
const GITUI_STUB: &str = r#"#!/bin/sh
printf 'GITUI %s\n' "$*" >> "$STUB_LOG"
if [ -n "$GITUI_STDOUT" ]; then printf '%s\n' "$GITUI_STDOUT"; fi
if [ -n "$GITUI_STDERR" ]; then printf '%s\n' "$GITUI_STDERR" >&2; fi
printf 'PWD %s\n' "$PWD" >> "$STUB_LOG"
if [ -n "$GITUI_SCRIPT" ]; then
  sh "$GITUI_SCRIPT"
fi
exit "${GITUI_RC:-0}"
"#;

/// A unique scratch directory removed (best-effort) when the test ends.
struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("herdr-gitui-e2e-{tag}-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    /// Writes the `herdr`, `gitui`, and `lazygit` stubs and returns `(herdr, gitui,
    /// stub_log)` paths.
    fn stubs(&self) -> (PathBuf, PathBuf, PathBuf) {
        let herdr = self.dir.join("herdr");
        let gitui = self.dir.join("gitui");
        let lazygit_dir = self.subdir("lazygit-bin");
        let lazygit = lazygit_dir.join("lazygit");
        write_executable(&herdr, HERDR_STUB);
        write_executable(&gitui, GITUI_STUB);
        write_executable(&lazygit, GITUI_STUB);
        (herdr, gitui, self.dir.join("stub.log"))
    }

    /// Creates a subdirectory and returns its path.
    fn subdir(&self, name: &str) -> PathBuf {
        let p = self.dir.join(name);
        fs::create_dir_all(&p).unwrap();
        p
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// Spawns the plugin with empty stdin so held paths return at EOF.
fn run_plugin(envs: &[(&str, &str)]) -> Output {
    run_plugin_with_stdin(envs, &[])
}

/// Runs the plugin with caller-provided stdin bytes, then waits for it to exit.
fn run_plugin_with_stdin(envs: &[(&str, &str)], input: &[u8]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_herdr-gitui"));
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear();
    cmd.env("PATH", std::env::var("PATH").unwrap_or_default());
    cmd.env("HOME", std::env::temp_dir());
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null");
    cmd.env("GIT_CONFIG_SYSTEM", "/dev/null");
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let mut child = cmd.spawn().unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

fn run_plugin_in(dir: &Path, envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_herdr-gitui"));
    cmd.current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear();
    cmd.env("PATH", std::env::var("PATH").unwrap_or_default());
    cmd.env("HOME", std::env::temp_dir());
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null");
    cmd.env("GIT_CONFIG_SYSTEM", "/dev/null");
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().unwrap()
}

/// Runs git in `dir` with global config disabled and an inline identity where
/// needed. Panics with git's stderr on failure.
fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn commit(dir: &Path, message: &str) {
    git(
        dir,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-q",
            "-m",
            message,
        ],
    );
}

/// Initializes a repository with a commit and a clean tree.
fn clean_repo(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    fs::write(dir.join("a.txt"), "a\n").unwrap();
    git(dir, &["add", "a.txt"]);
    commit(dir, "base");
}

fn read_log(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// `*.txt` files directly under `dir`.
fn txt_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("txt"))
        .collect()
}

/// The `GITUI ...` log line, if any.
fn gitui_line(log: &str) -> &str {
    log.lines().find(|l| l.starts_with("GITUI ")).unwrap_or("")
}

/// Pane-mode env pairs shared by the happy-path tests: entrypoint, repo, state,
/// pane/tab ids, both stubs and the stub log.
/// The `combined_path` must include both the stub directory (for gitui) and
/// the system PATH (for git commands inside scripts and probes).
fn pane_env<'a>(
    repo: &'a Path,
    state: &'a Path,
    herdr: &'a Path,
    tool_path: &'a Path,
    log: &'a Path,
    combined_path: &'a str,
) -> Vec<(&'a str, &'a str)> {
    vec![
        ("HERDR_PLUGIN_ENTRYPOINT_ID", "open"),
        ("HERDR_GITUI_DIR", repo.to_str().unwrap()),
        ("HERDR_PLUGIN_STATE_DIR", state.to_str().unwrap()),
        ("HERDR_GIT_TOOL", "gitui"),
        ("HERDR_GIT_TOOL_PATH", tool_path.to_str().unwrap()),
        ("HERDR_PANE_ID", "pane-7"),
        ("HERDR_TAB_ID", "tab-7"),
        ("HERDR_BIN_PATH", herdr.to_str().unwrap()),
        ("PATH", combined_path),
        ("STUB_LOG", log.to_str().unwrap()),
    ]
}

/// Builds a PATH that includes the stub directory and the system PATH.
fn combined_path(stub_dir: &Path) -> String {
    let system_path = std::env::var("PATH").unwrap_or_default();
    format!("{}:{system_path}", stub_dir.display())
}

/// The `HERDR_PLUGIN_CONTEXT_JSON` value used by the action-mode tests.
fn action_ctx(repo: &Path) -> String {
    format!(
        r#"{{"workspace_id":"ws-1","tab_id":"tab-1","focused_pane_id":"p1","focused_pane_agent":"claude","worktree":{{"checkout_path":"{}"}}}}"#,
        repo.display()
    )
}

/// Action-mode env pairs: action id, context, the herdr stub, its log, and
/// the settings fixture dir. `run_plugin` runs `.env_clear()`, so tests must
/// pass `HERDR_PLUGIN_CONFIG_DIR` explicitly (`Some(dir)`); `None` leaves it
/// unset to exercise the `plugin config-dir` callback fallback.
fn action_env<'a>(
    ctx: &'a str,
    herdr: &'a Path,
    log: &'a Path,
    config_dir: Option<&'a Path>,
) -> Vec<(&'a str, &'a str)> {
    let mut env = vec![
        ("HERDR_PLUGIN_ACTION_ID", "open"),
        ("HERDR_PLUGIN_CONTEXT_JSON", ctx),
        ("HERDR_BIN_PATH", herdr.to_str().unwrap()),
        ("STUB_LOG", log.to_str().unwrap()),
    ];
    if let Some(dir) = config_dir {
        env.push(("HERDR_PLUGIN_CONFIG_DIR", dir.to_str().unwrap()));
    }
    // Existing action fixtures model only gitui, independent of host tools.
    env.push(("PATH", herdr.parent().unwrap().to_str().unwrap()));
    env
}

/// A settings fixture dir; `content` is written to its `config.toml` when set
/// (a `None` dir stays empty: missing file, silent defaults).
fn config_fixture(fx: &Fixture, name: &str, content: Option<&str>) -> PathBuf {
    let dir = fx.subdir(name);
    if let Some(content) = content {
        fs::write(dir.join("config.toml"), content).unwrap();
    }
    dir
}

/// All logged `plugin pane open` argv lines, in launch order.
fn open_lines(log: &str) -> Vec<String> {
    log.lines()
        .filter(|l| l.starts_with("plugin pane open"))
        .map(str::to_string)
        .collect()
}

/// The single `plugin pane open` line a one-launch test expects.
fn sole_open_line(log: &str) -> String {
    let lines = open_lines(log);
    assert_eq!(lines.len(), 1, "expected one pane-open line in:\n{log}");
    lines.into_iter().next().unwrap()
}

/// Asserts the logged argv uses the default layout: `--placement tab` with
/// its `--workspace` targeting pair, no `--direction` flag, and the
/// unconditional trailing `--focus`. All action fixtures resolve workspace
/// `ws-1`, so the pair's presence is meaningful.
fn assert_default_layout_argv(open: &str) {
    assert!(open.contains("--placement tab --workspace ws-1"), "{open}");
    assert!(!open.contains("--direction"), "{open}");
    assert!(open.ends_with("--focus"), "{open}");
}

#[test]
fn manifest_declares_qualified_action_without_legacy_plugin_id() {
    let manifest: toml::Value = toml::from_str(include_str!("../herdr-plugin.toml")).unwrap();
    let plugin_id = manifest["id"].as_str().unwrap();
    let action_id = manifest["actions"][0]["id"].as_str().unwrap();
    let pane_id = manifest["panes"][0]["id"].as_str().unwrap();

    assert_eq!(plugin_id, "alexeyco.git");
    assert_eq!(action_id, "open");
    assert_eq!(pane_id, "open");
    assert_eq!(format!("{plugin_id}.{action_id}"), "alexeyco.git.open");
    assert_ne!(format!("{plugin_id}.{action_id}"), "alexeyco.gitui.open");
    assert_ne!(format!("{plugin_id}.{action_id}"), "gitui.open");
}

#[test]
fn manifest_declares_all_pane_entrypoints_and_placements() {
    let manifest: toml::Value = toml::from_str(include_str!("../herdr-plugin.toml")).unwrap();
    let panes = manifest["panes"].as_array().unwrap();
    let actual = panes
        .iter()
        .map(|pane| {
            (
                pane["id"].as_str().unwrap(),
                pane["placement"].as_str().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        [("open", "tab"), ("picker", "popup"), ("setup", "popup")]
    );
    let picker = panes
        .iter()
        .find(|pane| pane["id"].as_str() == Some("picker"))
        .unwrap();
    assert_eq!(picker["width"].as_integer(), Some(24));
    assert_eq!(picker["height"].as_integer(), Some(6));
}

#[test]
fn pane_entrypoint_dispatches_open_picker_setup_and_rejects_unknown() {
    let open = run_plugin(&[("HERDR_PLUGIN_ENTRYPOINT_ID", "open")]);
    assert_eq!(open.status.code(), Some(1));
    assert!(
        stderr(&open).contains("HERDR_BIN_PATH"),
        "{}",
        stderr(&open)
    );

    let picker = run_plugin(&[
        ("HERDR_PLUGIN_ENTRYPOINT_ID", "picker"),
        ("HERDR_GITUI_PATH", "/canonical/gitui"),
    ]);
    assert_eq!(picker.status.code(), Some(1));
    assert!(
        stderr(&picker).contains("stty -g failed"),
        "{}",
        stderr(&picker)
    );

    let setup = run_plugin(&[("HERDR_PLUGIN_ENTRYPOINT_ID", "setup")]);
    assert_eq!(setup.status.code(), Some(0), "{}", stderr(&setup));
    assert!(stdout(&setup).contains("lazygit"), "{}", stdout(&setup));

    let unknown = run_plugin(&[("HERDR_PLUGIN_ENTRYPOINT_ID", "other")]);
    assert_eq!(unknown.status.code(), Some(1));
    assert!(
        stderr(&unknown)
            .contains("unknown pane entrypoint 'other'; expected one of: open, picker, setup")
    );
}

/// One action launch with a settings dir whose `config.toml` contains
/// `content`; returns the plugin output and the stub log.
fn launch_with_config(fx: &Fixture, content: &str) -> (Output, String) {
    let repo = fx.subdir("repo");
    let cfg = config_fixture(fx, "config", Some(content));
    let (herdr, _gitui, log) = fx.stubs();
    let ctx = action_ctx(&repo);
    let env = action_env(&ctx, &herdr, &log, Some(&cfg));
    let out = run_plugin(&env);
    (out, read_log(&log))
}

#[test]
fn pane_happy_path_commit_delivers_summary() {
    let fx = Fixture::new("happy-commit");
    let repo = fx.subdir("repo");
    clean_repo(&repo);
    let (herdr, gitui, log) = fx.stubs();
    let state = fx.subdir("state");

    // Script that commits a file between snapshots.
    let script = fx.dir.join("commit_script.sh");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\ncd '{}'\necho 'hello' > new.txt\ngit -c user.name=t -c user.email=t@example.com \
             --no-pager add new.txt\ngit -c user.name=t -c user.email=t@example.com \
             --no-pager commit -q -m 'add new file'\n",
            repo.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let path = combined_path(gitui.parent().unwrap());
    let tool_path = gitui.canonicalize().unwrap();
    let mut env = pane_env(&repo, &state, &herdr, &tool_path, &log, &path);
    env.extend([
        ("HERDR_GITUI_CALLER_PANE", "caller-1"),
        ("HERDR_GITUI_CALLER_AGENT", "claude"),
        ("HERDR_GITUI_CALLER_TAB", "caller-tab"),
        ("GITUI_SCRIPT", script.to_str().unwrap()),
    ]);
    let out = run_plugin(&env);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("herdr-gitui: starting gitui in"),
        "stdout: {}",
        stdout(&out)
    );
    let logged = read_log(&log);
    assert!(
        logged.contains("agent prompt caller-1 gitui session summary:"),
        "{logged}"
    );
    assert!(
        logged.contains("created commit"),
        "expected 'created commit' in {logged}"
    );
    assert!(logged.contains("plugin pane close pane-7"), "{logged}");
    let focus = logged.find("tab focus caller-tab").unwrap();
    let close = logged.find("plugin pane close pane-7").unwrap();
    assert!(
        focus < close,
        "caller tab must be focused before pane close: {logged}"
    );
    // gitui is invoked with bare argv (no positional path); the stub logs
    // "GITUI " followed by its (empty) argument list — no repo dir argument.
    let line = gitui_line(&logged);
    assert!(
        line == "GITUI ",
        "expected empty argument list with no positional path or -d, got: {logged}"
    );
    assert!(
        logged
            .lines()
            .any(|line| line == format!("PWD {}", repo.canonicalize().unwrap().display()))
    );
    assert!(txt_files(&state.join("sessions")).is_empty());
}

#[test]
fn pane_lazygit_happy_path_runs_without_path_argument_and_delivers_lazygit_summary() {
    let fx = Fixture::new("lazygit-happy-path");
    let repo = fx.subdir("repo");
    clean_repo(&repo);
    let (herdr, _gitui, log) = fx.stubs();
    let lazygit = fx.dir.join("lazygit-bin/lazygit");
    write_executable(
        &lazygit,
        "#!/bin/sh\nprintf 'LAZYGIT %s\\n' \"$*\" >> \"$STUB_LOG\"\nprintf 'PWD %s\\n' \"$PWD\" >> \"$STUB_LOG\"\nif [ -n \"$GITUI_SCRIPT\" ]; then sh \"$GITUI_SCRIPT\"; fi\nexit \"${GITUI_RC:-0}\"\n",
    );
    let state = fx.subdir("state");

    let script = fx.dir.join("lazygit_script.sh");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\ncd '{}'\necho 'hello' > new.txt\ngit -c user.name=t -c user.email=t@example.com --no-pager add new.txt\n",
            repo.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let path = combined_path(lazygit.parent().unwrap());
    let tool_path = lazygit.canonicalize().unwrap();
    let mut env = pane_env(&repo, &state, &herdr, &tool_path, &log, &path);
    env.extend([
        ("HERDR_GIT_TOOL", "lazygit"),
        ("HERDR_GITUI_CALLER_PANE", "caller-1"),
        ("HERDR_GITUI_CALLER_AGENT", "claude"),
        ("GITUI_SCRIPT", script.to_str().unwrap()),
    ]);
    let out = run_plugin(&env);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("herdr-gitui: starting lazygit in"),
        "stdout: {}",
        stdout(&out)
    );
    let logged = read_log(&log);
    assert!(
        logged.contains("agent prompt caller-1 lazygit session summary:"),
        "{logged}"
    );
    assert!(logged.contains("staged 1 more file(s)"), "{logged}");
    assert_eq!(
        logged.lines().find(|line| line.starts_with("LAZYGIT ")),
        Some("LAZYGIT "),
        "lazygit must receive no positional path or other arguments: {logged}"
    );
    assert!(
        logged
            .lines()
            .any(|line| line == format!("PWD {}", repo.canonicalize().unwrap().display())),
        "lazygit must run from the repository directory: {logged}"
    );
    assert!(logged.contains("plugin pane close pane-7"), "{logged}");
}

#[test]
fn pane_child_inherits_stdout_and_stderr() {
    let fx = Fixture::new("stdio");
    let repo = fx.subdir("repo");
    clean_repo(&repo);
    let (herdr, gitui, log) = fx.stubs();
    let state = fx.subdir("state");
    let path = combined_path(gitui.parent().unwrap());
    let tool_path = gitui.canonicalize().unwrap();
    let mut env = pane_env(&repo, &state, &herdr, &tool_path, &log, &path);
    env.extend([
        ("GITUI_STDOUT", "child-stdout"),
        ("GITUI_STDERR", "child-stderr"),
    ]);

    let out = run_plugin(&env);

    assert!(
        stdout(&out).contains("child-stdout"),
        "stdout: {}",
        stdout(&out)
    );
    assert!(
        stderr(&out).contains("child-stderr"),
        "stderr: {}",
        stderr(&out)
    );
}

#[test]
fn pane_spawn_failure_reports_selected_path_and_directory() {
    let fx = Fixture::new("spawn-failure");
    let repo = fx.subdir("repo");
    clean_repo(&repo);
    let (herdr, _gitui, log) = fx.stubs();
    let state = fx.subdir("state");
    let unspawnable = fx.dir.join("unspawnable");
    fs::write(&unspawnable, "#!/missing-interpreter\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&unspawnable, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = combined_path(herdr.parent().unwrap());
    let mut env = pane_env(&repo, &state, &herdr, &unspawnable, &log, &path);
    env.push(("HERDR_GIT_TOOL", "lazygit"));

    let out = run_plugin(&env);

    let err = stderr(&out);
    assert!(err.contains("failed to run"), "stderr: {err}");
    assert!(err.contains(unspawnable.to_str().unwrap()), "stderr: {err}");
    assert!(err.contains(repo.to_str().unwrap()), "stderr: {err}");
}

#[test]
fn pane_rejects_selected_binary_that_disappeared_before_launch() {
    let fx = Fixture::new("disappeared-tool");
    let repo = fx.subdir("repo");
    clean_repo(&repo);
    let (herdr, gitui, log) = fx.stubs();
    let state = fx.subdir("state");
    let path = gitui.canonicalize().unwrap();
    fs::remove_file(&gitui).unwrap();
    let path_env = combined_path(herdr.parent().unwrap());
    let env = pane_env(&repo, &state, &herdr, &path, &log, &path_env);

    let out = run_plugin(&env);

    let err = stderr(&out);
    assert!(err.contains("selected Git TUI binary"), "stderr: {err}");
    assert!(err.contains(path.to_str().unwrap()), "stderr: {err}");
    assert!(
        !stdout(&out).contains("starting gitui"),
        "stdout: {}",
        stdout(&out)
    );
    assert!(
        !read_log(&log).contains("gitui status"),
        "{}",
        read_log(&log)
    );
    assert!(!state.join("sessions").exists());
}

#[test]
fn pane_snapshots_do_not_mutate_index_or_working_tree() {
    let fx = Fixture::new("snapshot-read-only");
    let repo = fx.subdir("repo");
    clean_repo(&repo);
    let before_index = fs::read(repo.join(".git/index")).unwrap();
    let (herdr, gitui, log) = fx.stubs();
    let state = fx.subdir("state");
    let path = combined_path(gitui.parent().unwrap());
    let tool_path = gitui.canonicalize().unwrap();
    let out = run_plugin(&pane_env(&repo, &state, &herdr, &tool_path, &log, &path));

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), before_index);
    assert!(!repo.join("new.txt").exists());
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .unwrap();
    assert!(status.status.success());
    assert!(status.stdout.is_empty());
}

#[test]
fn blank_gitui_dir_falls_back_to_process_cwd() {
    for (label, value) in [
        ("unset", None),
        ("empty", Some("")),
        ("whitespace", Some(" \t ")),
    ] {
        let fx = Fixture::new(&format!("cwd-fallback-{label}"));
        let repo = fx.subdir("repo");
        clean_repo(&repo);
        let (herdr, gitui, log) = fx.stubs();
        let state = fx.subdir("state");
        let path = combined_path(gitui.parent().unwrap());
        let tool_path = gitui.canonicalize().unwrap();
        let mut env = pane_env(&repo, &state, &herdr, &tool_path, &log, &path);
        env.retain(|(key, _)| *key != "HERDR_GITUI_DIR");
        if let Some(value) = value {
            env.push(("HERDR_GITUI_DIR", value));
        }
        let out = run_plugin_in(&fx.dir, &env);

        assert_eq!(out.status.code(), Some(0), "{label}: {}", stderr(&out));
        let cwd = fx.dir.canonicalize().unwrap();
        assert!(
            read_log(&log)
                .lines()
                .any(|line| line == format!("PWD {}", cwd.display())),
            "{label}: {}",
            read_log(&log)
        );
        assert!(
            stdout(&out).contains(&format!("starting gitui in {}", cwd.display())),
            "{label}: {}",
            stdout(&out)
        );
    }
}

#[test]
fn pane_no_changes_closes_pane() {
    let fx = Fixture::new("no-changes");
    let repo = fx.subdir("repo");
    clean_repo(&repo);
    let (herdr, gitui, log) = fx.stubs();
    let state = fx.subdir("state");

    let path = combined_path(gitui.parent().unwrap());
    let tool_path = gitui.canonicalize().unwrap();
    let mut env = pane_env(&repo, &state, &herdr, &tool_path, &log, &path);
    env.extend([
        ("HERDR_GITUI_CALLER_PANE", "caller-1"),
        ("HERDR_GITUI_CALLER_AGENT", "claude"),
        ("HERDR_GITUI_CALLER_TAB", "caller-tab"),
    ]);
    let out = run_plugin(&env);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let logged = read_log(&log);
    assert!(!logged.contains("agent prompt"), "{logged}");
    assert!(logged.contains("plugin pane close pane-7"), "{logged}");
    let focus = logged.find("tab focus caller-tab").unwrap();
    let close = logged.find("plugin pane close pane-7").unwrap();
    assert!(
        focus < close,
        "caller tab must be focused before pane close: {logged}"
    );
    assert!(txt_files(&state.join("sessions")).is_empty());
    assert!(
        stderr(&out).contains("herdr-gitui: no repository changes detected"),
        "stderr: {}",
        stderr(&out)
    );
}

#[test]
fn pane_nonzero_rc_holds_and_errors() {
    let fx = Fixture::new("rc-fail");
    let repo = fx.subdir("repo");
    clean_repo(&repo);
    let (herdr, gitui, log) = fx.stubs();
    let state = fx.subdir("state");

    let path = combined_path(gitui.parent().unwrap());
    let tool_path = gitui.canonicalize().unwrap();
    let mut env = pane_env(&repo, &state, &herdr, &tool_path, &log, &path);
    env.extend([
        ("HERDR_GITUI_CALLER_PANE", "caller-1"),
        ("HERDR_GITUI_CALLER_AGENT", "claude"),
        ("GITUI_RC", "1"),
    ]);
    let out = run_plugin(&env);

    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("gitui failed with exit code 1"),
        "stderr: {err}"
    );
    // rc != 0 always holds the pane so the user can read the error.
    assert!(err.contains("press Enter to close"), "stderr: {err}");
    let logged = read_log(&log);
    assert!(!logged.contains("agent prompt"), "{logged}");
}

#[test]
fn pane_no_agent_keeps_summary_and_holds() {
    let fx = Fixture::new("no-agent");
    let repo = fx.subdir("repo");
    clean_repo(&repo);
    let (herdr, gitui, log) = fx.stubs();
    let state = fx.subdir("state");

    // Script that commits a file between snapshots.
    let script = fx.dir.join("commit_script2.sh");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\ncd '{}'\necho 'hello' > new.txt\ngit -c user.name=t -c user.email=t@example.com \
             --no-pager add new.txt\ngit -c user.name=t -c user.email=t@example.com \
             --no-pager commit -q -m 'add new file'\n",
            repo.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let path = combined_path(gitui.parent().unwrap());
    let tool_path = gitui.canonicalize().unwrap();
    let mut env = pane_env(&repo, &state, &herdr, &tool_path, &log, &path);
    env.push(("GITUI_SCRIPT", script.to_str().unwrap()));
    env.push(("HERDR_GITUI_CALLER_TAB", "caller-tab"));
    // No caller context; a generic agent environment variable must not supply it.
    env.push(("HERDR_AGENT_ID", "claude"));
    let out = run_plugin_with_stdin(&env, b"\n");

    // Enter releases the hold, then the caller tab is restored before exit.
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("no agent in focused pane; summary kept at"),
        "stderr: {}",
        stderr(&out)
    );
    let logged = read_log(&log);
    assert!(!logged.contains("agent prompt"), "{logged}");
    assert_eq!(txt_files(&state.join("sessions")).len(), 1);
    assert!(logged.contains("tab focus caller-tab"), "{logged}");
}

#[test]
fn action_opens_pane_with_env_and_focus() {
    let fx = Fixture::new("action");
    let repo = fx.subdir("repo");
    let cfg = config_fixture(&fx, "config", None);
    let (herdr, _gitui, log) = fx.stubs();
    let ctx = action_ctx(&repo);

    let out = run_plugin(&action_env(&ctx, &herdr, &log, Some(&cfg)));

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("opened gitui pane pane-stub in tab tab-stub"),
        "stderr: {}",
        stderr(&out)
    );
    assert!(
        !stderr(&out).contains("herdr-gitui: warning:"),
        "stderr: {}",
        stderr(&out)
    );
    let logged = read_log(&log);
    let open = sole_open_line(&logged);
    assert!(open.contains("--workspace ws-1"), "{open}");
    assert!(
        open.contains(&format!("--cwd {}", repo.display())),
        "{open}"
    );
    assert!(
        open.contains(&format!("--env HERDR_GITUI_DIR={}", repo.display())),
        "{open}"
    );
    assert!(open.contains("--env HERDR_GITUI_CALLER_PANE=p1"), "{open}");
    assert!(
        open.contains("--env HERDR_GITUI_CALLER_TAB=tab-1"),
        "{open}"
    );
    assert!(
        open.contains("--env HERDR_GITUI_CALLER_AGENT=claude"),
        "{open}"
    );
    // Empty settings dir: missing config.toml is silent defaults, so the
    // argv keeps the default `--placement tab` with no `--direction`.
    assert_default_layout_argv(&open);
    assert!(open.ends_with("--focus"), "{open}");
    assert!(logged.contains("tab focus tab-stub"), "{logged}");
}

#[test]
fn action_reports_tab_and_ignores_focus_failure() {
    let fx = Fixture::new("focus-failure");
    let repo = fx.subdir("repo");
    let cfg = config_fixture(&fx, "config", None);
    let (herdr, _gitui, log) = fx.stubs();
    let ctx = action_ctx(&repo);
    let mut env = action_env(&ctx, &herdr, &log, Some(&cfg));
    env.push(("STUB_FOCUS_FAIL", "1"));
    let out = run_plugin(&env);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("opened gitui pane pane-stub in tab tab-stub"));
    let logged = read_log(&log);
    assert!(logged.contains("tab focus tab-stub"), "{logged}");
}

#[test]
fn action_without_tab_reports_pane_and_skips_focus() {
    let fx = Fixture::new("no-tab");
    let repo = fx.subdir("repo");
    let cfg = config_fixture(&fx, "config", None);
    let (herdr, _gitui, log) = fx.stubs();
    let ctx = action_ctx(&repo);
    let mut env = action_env(&ctx, &herdr, &log, Some(&cfg));
    env.push((
        "STUB_PANE_OPEN",
        r#"{"result":{"plugin_pane":{"pane":{"pane_id":"pane-only"}}}}"#,
    ));
    let out = run_plugin(&env);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("opened gitui pane pane-only"),
        "{}",
        stderr(&out)
    );
    assert!(!stderr(&out).contains("in tab"), "{}", stderr(&out));
    assert!(!read_log(&log).contains("tab focus"), "{}", read_log(&log));
}

#[test]
fn action_context_missing_or_blank_fails_without_herdr_callback() {
    let fx = Fixture::new("context-required");
    let (herdr, _gitui, log) = fx.stubs();
    for context in [None, Some("   ")] {
        let mut env = vec![
            ("HERDR_PLUGIN_ACTION_ID", "open"),
            ("HERDR_BIN_PATH", herdr.to_str().unwrap()),
            ("STUB_LOG", log.to_str().unwrap()),
            ("PATH", herdr.parent().unwrap().to_str().unwrap()),
        ];
        if let Some(value) = context {
            env.push(("HERDR_PLUGIN_CONTEXT_JSON", value));
        }
        let out = run_plugin(&env);
        assert_eq!(out.status.code(), Some(1));
        assert!(stderr(&out).contains("HERDR_PLUGIN_CONTEXT_JSON is not set"));
    }
    assert!(read_log(&log).is_empty(), "{}", read_log(&log));
}

#[test]
fn old_user_setting_is_untouched_and_is_not_a_manifest_alias() {
    let fx = Fixture::new("old-setting-user-owned");
    let repo = fx.subdir("repo");
    let cfg = config_fixture(&fx, "config", None);
    let old_settings = fx.dir.join("herdr-settings.toml");
    let contents = "command = \"alexeyco.gitui.open\"\n";
    fs::write(&old_settings, contents).unwrap();
    let (herdr, _gitui, log) = fx.stubs();
    let ctx = action_ctx(&repo);
    let out = run_plugin(&action_env(&ctx, &herdr, &log, Some(&cfg)));
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(fs::read_to_string(old_settings).unwrap(), contents);
    assert_eq!(
        format!("{}.{}", "alexeyco.git", "open"),
        "alexeyco.git.open"
    );
    assert_ne!(
        format!("{}.{}", "alexeyco.git", "open"),
        "alexeyco.gitui.open"
    );
    assert_ne!(format!("{}.{}", "alexeyco.git", "open"), "gitui.open");
}

#[test]
fn action_split_down_passes_placement_and_direction() {
    let fx = Fixture::new("split-down");
    let (out, logged) = launch_with_config(&fx, "placement = \"split\"\ndirection = \"down\"\n");

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        !stderr(&out).contains("herdr-gitui: warning:"),
        "stderr: {}",
        stderr(&out)
    );
    let open = sole_open_line(&logged);
    // "Split placement with explicit direction": the direction pair sits
    // immediately after the placement value, `--target-pane` follows it, and
    // the pane-anchored open carries no `--workspace` despite ws-1 resolving.
    assert!(
        open.contains("--placement split --direction down --target-pane p1 --cwd"),
        "{open}"
    );
    assert_eq!(open.matches("--placement").count(), 1, "{open}");
    assert_eq!(open.matches("--direction").count(), 1, "{open}");
    assert!(!open.contains("--workspace"), "{open}");
    assert!(open.ends_with("--focus"), "{open}");
    // Focus is not configurable: the tab re-assert still follows a split open.
    assert!(logged.contains("tab focus tab-stub"), "{logged}");
}

#[test]
fn action_split_auto_omits_direction_flag() {
    let fx = Fixture::new("split-auto");
    let (out, logged) = launch_with_config(&fx, "placement = \"split\"\ndirection = \"auto\"\n");

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let open = sole_open_line(&logged);
    // "Split placement with auto direction": placement applies, the flag is
    // absent entirely, and `--target-pane` sits directly after the placement
    // value with no `--workspace`.
    assert!(
        open.contains("--placement split --target-pane p1 --cwd"),
        "{open}"
    );
    assert!(!open.contains("--direction"), "{open}");
    assert!(!open.contains("--workspace"), "{open}");
    assert!(open.ends_with("--focus"), "{open}");
}

#[test]
fn action_split_omitted_direction_defaults_to_right() {
    let fx = Fixture::new("split-dir-omitted");
    let (out, logged) = launch_with_config(&fx, "placement = \"split\"\n");

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let open = sole_open_line(&logged);
    // A missing `direction` key keeps its documented default `right`, so the
    // argv carries the flag even though the config never mentions it, and the
    // split open is pane-anchored without `--workspace`.
    assert!(
        open.contains("--placement split --direction right --target-pane p1 --cwd"),
        "{open}"
    );
    assert!(!open.contains("--workspace"), "{open}");
    assert!(open.ends_with("--focus"), "{open}");
}

#[test]
fn action_split_targets_caller_pane_and_keeps_env_tail() {
    // "Split targets the caller pane": `--target-pane <caller-pane>` sits
    // immediately after the direction position and the resolved workspace id
    // is not passed; the --cwd/--env/--focus tail is byte-for-byte the tab
    // tail.
    let fx = Fixture::new("split-target");
    let (out, logged) = launch_with_config(&fx, "placement = \"split\"\ndirection = \"down\"\n");
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        !stderr(&out).contains("herdr-gitui: warning"),
        "stderr: {}",
        stderr(&out)
    );
    let open = sole_open_line(&logged);
    assert!(
        open.contains("--placement split --direction down --target-pane p1 --cwd "),
        "{open}"
    );
    assert!(!open.contains("--workspace"), "{open}");
    assert!(
        open.contains("--env HERDR_GITUI_CALLER_PANE=p1")
            && open.contains("--env HERDR_GITUI_CALLER_AGENT=claude")
            && open.contains("--env HERDR_GIT_TOOL=gitui")
            && open.ends_with(
                "--env HERDR_GITUI_WORKSPACE_ID=ws-1 --env HERDR_GITUI_CALLER_TAB=tab-1 --focus"
            ),
        "{open}"
    );
}

#[test]
fn action_zoomed_targets_caller_pane_without_workspace() {
    // "Zoomed targets the caller pane": `--target-pane` directly after the
    // placement value — no direction, no workspace.
    let fx = Fixture::new("zoomed-target");
    let (out, logged) = launch_with_config(&fx, "placement = \"zoomed\"\n");
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        !stderr(&out).contains("herdr-gitui: warning"),
        "stderr: {}",
        stderr(&out)
    );
    let open = sole_open_line(&logged);
    assert!(
        open.contains("--placement zoomed --target-pane p1 --cwd "),
        "{open}"
    );
    assert!(!open.contains("--direction"), "{open}");
    assert!(!open.contains("--workspace"), "{open}");
    assert!(open.ends_with("--focus"), "{open}");
}

#[test]
fn action_overlay_omits_workspace_and_target() {
    // "Overlay omits workspace targeting": the daemon anchors the overlay to
    // the active pane itself; the plugin passes neither targeting flag even
    // though both workspace `ws-1` and caller pane `p1` resolve.
    let fx = Fixture::new("overlay-no-target");
    let (out, logged) = launch_with_config(&fx, "placement = \"overlay\"\n");
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        !stderr(&out).contains("herdr-gitui: warning"),
        "stderr: {}",
        stderr(&out)
    );
    let open = sole_open_line(&logged);
    assert!(open.contains("--placement overlay --cwd "), "{open}");
    assert!(!open.contains("--workspace"), "{open}");
    assert!(!open.contains("--target-pane"), "{open}");
    assert!(open.ends_with("--focus"), "{open}");
}

#[test]
fn action_split_without_caller_pane_falls_back_to_tab() {
    // "Unresolvable caller pane falls back to tab with warning": the context
    // resolves workspace ws-1 but no focused pane and no HERDR_PANE_ID is in
    // the environment, so the split request degrades to the exact tab argv —
    // with --workspace back — plus a stderr warning, and the action succeeds.
    let fx = Fixture::new("split-no-caller");
    let repo = fx.subdir("repo");
    let cfg = config_fixture(
        &fx,
        "config",
        Some("placement = \"split\"\ndirection = \"down\"\n"),
    );
    let (herdr, _gitui, log) = fx.stubs();
    let ctx = format!(
        r#"{{"workspace_id":"ws-1","focused_pane_agent":"claude","worktree":{{"checkout_path":"{}"}}}}"#,
        repo.display()
    );
    let out = run_plugin(&action_env(&ctx, &herdr, &log, Some(&cfg)));
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("herdr-gitui: warning:") && err.contains("falling back to placement tab"),
        "stderr: {err}"
    );
    let open = sole_open_line(&read_log(&log));
    assert!(
        open.contains("--placement tab --workspace ws-1 --cwd "),
        "{open}"
    );
    assert!(!open.contains("--direction"), "{open}");
    assert!(!open.contains("--target-pane"), "{open}");
    assert!(open.ends_with("--focus"), "{open}");
}

#[test]
fn action_direction_ignored_for_non_split_placements() {
    let fx = Fixture::new("nonsplit-direction");
    let repo = fx.subdir("repo");
    let cfg = config_fixture(
        &fx,
        "config",
        Some("placement = \"overlay\"\ndirection = \"down\"\n"),
    );
    let (herdr, _gitui, log) = fx.stubs();
    let ctx = action_ctx(&repo);
    let env = action_env(&ctx, &herdr, &log, Some(&cfg));

    let out = run_plugin(&env);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    // "silently ignored (no warning)" (design.md): the ignored direction must
    // not add any settings warning to stderr.
    assert!(
        !stderr(&out).contains("herdr-gitui: warning:"),
        "stderr: {}",
        stderr(&out)
    );
    fs::write(
        cfg.join("config.toml"),
        "placement = \"zoomed\"\ndirection = \"down\"\n",
    )
    .unwrap();
    let out = run_plugin(&env);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        !stderr(&out).contains("herdr-gitui: warning:"),
        "stderr: {}",
        stderr(&out)
    );

    // "Direction ignored for non-split": overlay and zoomed both keep the
    // placement and drop --direction; every launch ends with --focus. Both
    // are pane-anchored (no --workspace despite ws-1 resolving) and only
    // zoomed carries --target-pane.
    let lines = open_lines(&read_log(&log));
    assert_eq!(lines.len(), 2, "{lines:#?}");
    for (line, placement) in [(&lines[0], "overlay"), (&lines[1], "zoomed")] {
        assert!(line.contains(&format!("--placement {placement}")), "{line}");
        assert!(!line.contains("--direction"), "{line}");
        assert!(!line.contains("--workspace"), "{line}");
        assert_eq!(
            line.contains("--target-pane p1"),
            placement == "zoomed",
            "{line}"
        );
        assert!(line.ends_with("--focus"), "{line}");
    }
}

#[test]
fn action_late_edit_applies_next_launch() {
    let fx = Fixture::new("late-edit");
    let repo = fx.subdir("repo");
    let cfg = config_fixture(&fx, "config", Some("placement = \"split\"\n"));
    let (herdr, _gitui, log) = fx.stubs();
    let ctx = action_ctx(&repo);
    let env = action_env(&ctx, &herdr, &log, Some(&cfg));

    // "Late edit does not affect the running session": settings load once per
    // launch, so rewriting config.toml between launches changes only the
    // second launch's argv.
    let out = run_plugin(&env);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    fs::write(cfg.join("config.toml"), "placement = \"overlay\"\n").unwrap();
    let out = run_plugin(&env);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));

    let lines = open_lines(&read_log(&log));
    assert_eq!(lines.len(), 2, "{lines:#?}");
    assert!(
        lines[0].contains("--placement split --direction right --target-pane p1"),
        "{}",
        lines[0]
    );
    assert!(!lines[0].contains("--workspace"), "{}", lines[0]);
    assert!(lines[1].contains("--placement overlay"), "{}", lines[1]);
    assert!(!lines[1].contains("--direction"), "{}", lines[1]);
    assert!(!lines[1].contains("--workspace"), "{}", lines[1]);
    assert!(!lines[1].contains("--target-pane"), "{}", lines[1]);
}

#[test]
fn action_corrupt_toml_warns_and_opens_with_defaults() {
    let fx = Fixture::new("corrupt-toml");
    let (out, logged) = launch_with_config(&fx, "placement = ");

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("herdr-gitui: warning:") && err.contains("invalid config.toml"),
        "stderr: {err}"
    );
    // "Corrupt TOML": all defaults, so --placement tab and no --direction.
    assert_default_layout_argv(&sole_open_line(&logged));
}

#[test]
fn action_unknown_key_warns_and_known_keys_apply() {
    let fx = Fixture::new("unknown-key");
    let (out, logged) = launch_with_config(&fx, "focus = false\nplacement = \"split\"\n");

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("herdr-gitui: warning:") && err.contains("unknown key"),
        "stderr: {err}"
    );
    assert!(err.contains("focus"), "stderr: {err}");
    // The known key keeps its effect; the omitted direction defaults right;
    // the split open targets the caller pane and omits --workspace.
    let open = sole_open_line(&logged);
    assert!(
        open.contains("--placement split --direction right --target-pane p1 --cwd"),
        "{open}"
    );
    assert!(!open.contains("--workspace"), "{open}");
    assert!(open.ends_with("--focus"), "{open}");
}

#[test]
fn action_invalid_placement_falls_back_per_key() {
    let fx = Fixture::new("invalid-placement");
    let (out, logged) = launch_with_config(&fx, "placement = \"zigzag\"\ndirection = \"down\"\n");

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("herdr-gitui: warning:")
            && err.contains("invalid placement")
            && err.contains("zigzag"),
        "stderr: {err}"
    );
    // "Invalid value falls back per key": default placement tab, and the
    // still-valid direction has no effect for a non-split placement, so the
    // argv carries no --direction.
    assert_default_layout_argv(&sole_open_line(&logged));
}

#[test]
fn action_config_dir_query_failure_warns_and_uses_defaults() {
    let fx = Fixture::new("query-fail");
    let repo = fx.subdir("repo");
    let (herdr, _gitui, log) = fx.stubs();
    let ctx = action_ctx(&repo);

    // HERDR_BIN_PATH present but the query exits non-zero.
    let mut env = action_env(&ctx, &herdr, &log, None);
    env.push(("STUB_CONFIG_DIR_FAIL", "1"));
    let out = run_plugin(&env);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("herdr-gitui: warning:") && err.contains("config directory unavailable"),
        "stderr: {err}"
    );
    let logged = read_log(&log);
    assert!(
        logged.contains("plugin config-dir alexeyco.git"),
        "{logged}"
    );
    assert_default_layout_argv(&sole_open_line(&logged));

    // Same degradation on a blank (zero-exit, empty-stdout) reply; a second
    // log keeps the one-line assertions above meaningful.
    let log2 = fx.dir.join("stub2.log");
    let env = action_env(&ctx, &herdr, &log2, None);
    let out = run_plugin(&env);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("herdr-gitui: warning:") && err.contains("config directory unavailable"),
        "stderr: {err}"
    );
    let logged = read_log(&log2);
    assert!(
        logged.contains("plugin config-dir alexeyco.git"),
        "{logged}"
    );
    assert_default_layout_argv(&sole_open_line(&logged));
}

#[test]
fn action_without_bin_path_degrades_without_callback() {
    let fx = Fixture::new("no-bin-path");
    let repo = fx.subdir("repo");
    let (_herdr, _gitui, log) = fx.stubs();
    let ctx = action_ctx(&repo);

    // "Config-dir query passes through the same guard": with no
    // HERDR_PLUGIN_CONFIG_DIR and no usable HERDR_BIN_PATH the settings layer
    // must not attempt any callback; it warns and the action continues to the
    // unchanged pane-open contract error.
    let env = vec![
        ("HERDR_PLUGIN_ACTION_ID", "open"),
        ("HERDR_PLUGIN_CONTEXT_JSON", ctx.as_str()),
        ("STUB_LOG", log.to_str().unwrap()),
        ("PATH", fx.dir.to_str().unwrap()),
    ];
    let out = run_plugin(&env);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("herdr-gitui: warning:") && err.contains("config directory unavailable"),
        "stderr: {err}"
    );
    assert!(
        err.contains("herdr did not provide HERDR_BIN_PATH (plugin API v1 contract); cannot call back into herdr"),
        "stderr: {err}"
    );
    assert!(
        !read_log(&log).contains("plugin config-dir"),
        "stub was invoked"
    );

    // A whitespace-only HERDR_BIN_PATH is rejected identically.
    let log2 = fx.dir.join("stub2.log");
    let mut env = vec![
        ("HERDR_PLUGIN_ACTION_ID", "open"),
        ("HERDR_PLUGIN_CONTEXT_JSON", ctx.as_str()),
        ("STUB_LOG", log2.to_str().unwrap()),
        ("PATH", fx.dir.to_str().unwrap()),
    ];
    env.push(("HERDR_BIN_PATH", "   "));
    let out = run_plugin(&env);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("herdr-gitui: warning:") && err.contains("config directory unavailable"),
        "stderr: {err}"
    );
    assert!(
        err.contains("herdr did not provide HERDR_BIN_PATH (plugin API v1 contract); cannot call back into herdr"),
        "stderr: {err}"
    );
    assert!(
        !read_log(&log2).contains("plugin config-dir"),
        "stub was invoked"
    );
}

#[test]
fn action_without_tools_requests_setup_popup() {
    let fx = Fixture::new("setup-request");
    let repo = fx.subdir("repo");
    let (herdr, _gitui, log) = fx.stubs();
    let ctx = action_ctx(&repo);
    let mut env = action_env(&ctx, &herdr, &log, None);
    let empty_path = fx.subdir("empty-path");
    env.push(("PATH", empty_path.to_str().unwrap()));
    let out = run_plugin(&env);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("opened tool setup popup"),
        "{}",
        stderr(&out)
    );
    let open = sole_open_line(&read_log(&log));
    assert!(
        open.contains("--plugin alexeyco.git --entrypoint setup --placement popup"),
        "{open}"
    );
    assert!(!open.contains("--entrypoint open"), "{open}");
}

#[test]
fn action_with_lazygit_only_hands_off_selected_binary_and_context() {
    let fx = Fixture::new("lazygit-only");
    let repo = fx.subdir("repo");
    let cfg = config_fixture(&fx, "config", None);
    let (herdr, _gitui, log) = fx.stubs();
    let ctx = action_ctx(&repo);
    let mut env = action_env(&ctx, &herdr, &log, Some(&cfg));
    let state = fx.subdir("state");
    fs::write(state.join("last-tool"), "gitui\n").unwrap();
    env.push(("HERDR_PLUGIN_STATE_DIR", state.to_str().unwrap()));
    let lazygit_only = fx.subdir("lazygit-only-path");
    let lazygit = fx.dir.join("lazygit-bin/lazygit");
    fs::copy(&lazygit, lazygit_only.join("lazygit")).unwrap();
    env.push(("PATH", lazygit_only.to_str().unwrap()));
    let out = run_plugin(&env);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let open = sole_open_line(&read_log(&log));
    assert!(open.contains("--entrypoint open"), "{open}");
    assert!(
        open.contains(&format!(
            "--env HERDR_GIT_TOOL=lazygit --env HERDR_GIT_TOOL_PATH={}",
            lazygit_only
                .join("lazygit")
                .canonicalize()
                .unwrap()
                .display()
        )),
        "{open}"
    );
    assert!(
        open.contains(&format!("--cwd {}", repo.display())),
        "{open}"
    );
    assert!(open.contains("--env HERDR_GITUI_CALLER_PANE=p1"), "{open}");
    assert!(
        open.contains("--env HERDR_GITUI_CALLER_AGENT=claude"),
        "{open}"
    );
    assert_eq!(
        fs::read_to_string(state.join("last-tool")).unwrap(),
        "gitui\n"
    );
}

#[test]
fn action_with_both_tools_requests_picker_every_time_with_context_and_saved_choice() {
    let fx = Fixture::new("picker-request");
    let repo = fx.subdir("repo");
    let cfg = config_fixture(&fx, "config", None);
    let (herdr, _gitui, log) = fx.stubs();
    let ctx = action_ctx(&repo);
    let mut env = action_env(&ctx, &herdr, &log, Some(&cfg));
    let state = fx.subdir("state");
    fs::write(state.join("last-tool"), "lazygit\n").unwrap();
    env.push(("HERDR_PLUGIN_STATE_DIR", state.to_str().unwrap()));
    let both_path = fx.subdir("both-tools");
    fs::copy(fx.dir.join("gitui"), both_path.join("gitui")).unwrap();
    fs::copy(
        fx.dir.join("lazygit-bin/lazygit"),
        both_path.join("lazygit"),
    )
    .unwrap();
    env.push(("PATH", both_path.to_str().unwrap()));

    for _ in 0..2 {
        let out = run_plugin(&env);
        assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    }
    let opens = open_lines(&read_log(&log));
    assert_eq!(opens.len(), 2, "{opens:#?}");
    let gitui = both_path.join("gitui").canonicalize().unwrap();
    let lazygit = both_path.join("lazygit").canonicalize().unwrap();
    for open in opens {
        assert!(
            open.contains("--plugin alexeyco.git --entrypoint picker --placement popup"),
            "{open}"
        );
        assert!(open.contains("--width 24 --height 6"), "{open}");
        assert!(
            open.contains(&format!("--cwd {}", repo.display())),
            "{open}"
        );
        assert!(
            open.contains(&format!("HERDR_GITUI_PATH={}", gitui.display())),
            "{open}"
        );
        assert!(
            open.contains(&format!("HERDR_LAZYGIT_PATH={}", lazygit.display())),
            "{open}"
        );
        assert!(open.contains("HERDR_GIT_LAST_TOOL=lazygit"), "{open}");
        assert!(open.contains("HERDR_GITUI_WORKSPACE_ID=ws-1"), "{open}");
        assert!(open.contains("HERDR_GITUI_CALLER_PANE=p1"), "{open}");
        assert!(open.contains("HERDR_GITUI_CALLER_AGENT=claude"), "{open}");
        assert!(open.contains("HERDR_GITUI_CALLER_TAB=tab-1"), "{open}");
    }
}

#[test]
fn action_config_dir_env_preferred_over_callback() {
    let fx = Fixture::new("cfg-env-preferred");
    let repo = fx.subdir("repo");
    let cfg = config_fixture(&fx, "config", Some("placement = \"split\"\n"));
    // A decoy dir the callback would report: reading it would change argv.
    let decoy = config_fixture(&fx, "decoy", Some("placement = \"zoomed\"\n"));
    let (herdr, _gitui, log) = fx.stubs();
    let ctx = action_ctx(&repo);

    let mut env = action_env(&ctx, &herdr, &log, Some(&cfg));
    env.push(("STUB_CONFIG_DIR", decoy.to_str().unwrap()));
    let out = run_plugin(&env);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        !stderr(&out).contains("herdr-gitui: warning:"),
        "stderr: {}",
        stderr(&out)
    );
    let logged = read_log(&log);
    // "Environment variable preferred": no callback at all, and the env dir's
    // values (split, not the decoy's zoomed) are applied.
    assert!(!logged.contains("plugin config-dir"), "{logged}");
    let open = sole_open_line(&logged);
    assert!(
        open.contains("--placement split --direction right --target-pane p1 --cwd"),
        "{open}"
    );
    assert!(!open.contains("--workspace"), "{open}");
    assert!(open.ends_with("--focus"), "{open}");
}

#[test]
fn action_config_dir_cli_fallback_when_env_absent() {
    let fx = Fixture::new("cli-fallback");
    let repo = fx.subdir("repo");
    let reported = config_fixture(&fx, "reported", Some("placement = \"zoomed\"\n"));
    let (herdr, _gitui, log) = fx.stubs();
    let ctx = action_ctx(&repo);

    // "CLI fallback when env absent": unset HERDR_PLUGIN_CONFIG_DIR (the
    // harness env_clear()s the parent env) makes the query, and the reported
    // directory is the one config.toml is read from.
    let mut env = action_env(&ctx, &herdr, &log, None);
    env.push(("STUB_CONFIG_DIR", reported.to_str().unwrap()));
    let out = run_plugin(&env);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        !stderr(&out).contains("herdr-gitui: warning:"),
        "stderr: {}",
        stderr(&out)
    );
    let logged = read_log(&log);
    assert!(
        logged.contains("plugin config-dir alexeyco.git"),
        "{logged}"
    );
    let open = sole_open_line(&logged);
    // Zoomed from the reported dir: anchored on the caller pane, workspace
    // targeting omitted despite ws-1 resolving.
    assert!(
        open.contains("--placement zoomed --target-pane p1"),
        "{open}"
    );
    assert!(!open.contains("--direction"), "{open}");
    assert!(!open.contains("--workspace"), "{open}");
    assert!(open.ends_with("--focus"), "{open}");

    // A blank env value falls back to the same callback path.
    let log2 = fx.dir.join("stub2.log");
    let mut env = action_env(&ctx, &herdr, &log2, None);
    env.push(("HERDR_PLUGIN_CONFIG_DIR", "   "));
    env.push(("STUB_CONFIG_DIR", reported.to_str().unwrap()));
    let out = run_plugin(&env);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let logged = read_log(&log2);
    assert!(
        logged.contains("plugin config-dir alexeyco.git"),
        "{logged}"
    );
    assert!(
        sole_open_line(&logged).contains("--placement zoomed"),
        "{logged}"
    );
}

/// The popup pane-open reply live-verified on herdr 0.9.5: `ok` with no
/// pane id and no tab id (a popup is a singleton session resource, not a
/// Herdr pane).
const POPUP_OK_REPLY: &str = r#"{"id":"cli:plugin","result":{"type":"ok"}}"#;

/// One action launch with popup settings and the id-less popup reply from
/// the stub; returns the plugin output, the stub log, and the repo dir (the
/// argv assertions quote its path).
fn launch_popup_with_config(fx: &Fixture, tag: &str, content: &str) -> (Output, String, PathBuf) {
    let repo = fx.subdir(&format!("repo-{tag}"));
    let cfg = config_fixture(fx, &format!("config-{tag}"), Some(content));
    let (herdr, _gitui, log) = fx.stubs();
    let ctx = action_ctx(&repo);
    let mut env = action_env(&ctx, &herdr, &log, Some(&cfg));
    env.push(("STUB_PANE_OPEN", POPUP_OK_REPLY));
    let out = run_plugin(&env);
    (out, read_log(&log), repo)
}

#[test]
fn action_popup_passes_size_flags_and_placement_env() {
    // "Popup passes size flags": `--width 80 --height 80%` sit immediately
    // after `--placement popup`, the popup-only env export follows the DIR
    // pair, and none of the targeting flags appear despite `ws-1`/`p1`
    // resolving and a default direction of right.
    let fx = Fixture::new("popup-size");
    let (out, logged, repo) = launch_popup_with_config(
        &fx,
        "size",
        "placement = \"popup\"\nwidth = 80\nheight = \"80%\"\n",
    );
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        !stderr(&out).contains("herdr-gitui: warning:"),
        "stderr: {}",
        stderr(&out)
    );
    let open = sole_open_line(&logged);
    assert!(
        open.contains(&format!(
            "--placement popup --width 80 --height 80% --cwd {} \
             --env HERDR_GITUI_DIR={} --env HERDR_GITUI_PLACEMENT=popup \
             --env HERDR_GITUI_CALLER_PANE=p1",
            repo.display(),
            repo.display()
        )),
        "{open}"
    );
    assert!(!open.contains("--workspace"), "{open}");
    assert!(!open.contains("--target-pane"), "{open}");
    assert!(!open.contains("--direction"), "{open}");
    assert!(
        open.contains("--env HERDR_GITUI_CALLER_AGENT=claude"),
        "{open}"
    );
    assert!(open.contains("--env HERDR_GIT_TOOL=gitui"), "{open}");
    assert!(open.contains("--env HERDR_GIT_TOOL_PATH="), "{open}");
    assert!(
        open.contains("--env HERDR_GITUI_WORKSPACE_ID=ws-1"),
        "{open}"
    );
    assert!(open.ends_with("--focus"), "{open}");
}

#[test]
fn action_popup_width_only_emits_no_height_flag() {
    // Each size pair is emitted independently: an omitted `height` (invalid
    // values are the settings layer's case) adds no `--height` token.
    let fx = Fixture::new("popup-width-only");
    let (out, logged, _repo) =
        launch_popup_with_config(&fx, "width", "placement = \"popup\"\nwidth = 80\n");
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let open = sole_open_line(&logged);
    assert!(
        open.contains("--placement popup --width 80 --cwd "),
        "{open}"
    );
    assert!(!open.contains("--height"), "{open}");
    assert!(open.contains("--env HERDR_GITUI_PLACEMENT=popup"), "{open}");
    assert!(open.ends_with("--focus"), "{open}");
}

#[test]
fn action_popup_without_size_keys_and_opens_without_pane_id_reply() {
    // No size keys: no size tokens at all (herdr's default half-size popup)
    // and no warning. The id-less `ok` reply is success: the popup notice
    // appears, exit is 0, and no `tab focus` call is logged.
    let fx = Fixture::new("popup-no-size");
    let (out, logged, _repo) = launch_popup_with_config(&fx, "nosize", "placement = \"popup\"\n");
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("herdr-gitui: opened gitui popup"),
        "stderr: {err}"
    );
    assert!(!err.contains("pane-stub"), "stderr: {err}");
    assert!(!err.contains("herdr-gitui: warning:"), "stderr: {err}");
    let open = sole_open_line(&logged);
    assert!(open.contains("--placement popup --cwd "), "{open}");
    assert!(!open.contains("--width"), "{open}");
    assert!(!open.contains("--height"), "{open}");
    assert!(open.contains("--env HERDR_GITUI_PLACEMENT=popup"), "{open}");
    assert!(open.ends_with("--focus"), "{open}");
    assert!(!logged.contains("tab focus"), "{logged}");
}

#[test]
fn action_size_keys_ignored_for_tab_placement() {
    // "Size keys ignored for non-popup placements": `width`/`height` with
    // `placement = "tab"` warn nothing and add no flags — the default argv
    // is byte-identical and no placement env export appears.
    let fx = Fixture::new("tab-sizes");
    let (out, logged) =
        launch_with_config(&fx, "placement = \"tab\"\nwidth = 80\nheight = \"80%\"\n");
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        !stderr(&out).contains("herdr-gitui: warning:"),
        "stderr: {}",
        stderr(&out)
    );
    let open = sole_open_line(&logged);
    assert_default_layout_argv(&open);
    assert!(!open.contains("--width"), "{open}");
    assert!(!open.contains("--height"), "{open}");
    assert!(!open.contains("HERDR_GITUI_PLACEMENT"), "{open}");
}

#[test]
fn popup_session_delivers_summary_with_zero_close_calls() {
    // "Popup session makes no close call": delivery is unchanged, but the
    // teardown gate makes zero `plugin pane close` and zero `tab close`
    // calls — the popup's carried context points at the caller's agent pane.
    let fx = Fixture::new("popup-session");
    let repo = fx.subdir("repo");
    clean_repo(&repo);
    let (herdr, gitui, log) = fx.stubs();
    let state = fx.subdir("state");

    // Script that commits a file between snapshots.
    let script = fx.dir.join("popup_commit_script.sh");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\ncd '{}'\necho 'hello' > new.txt\ngit -c user.name=t -c user.email=t@example.com \
             --no-pager add new.txt\ngit -c user.name=t -c user.email=t@example.com \
             --no-pager commit -q -m 'add new file'\n",
            repo.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let path = combined_path(gitui.parent().unwrap());
    let tool_path = gitui.canonicalize().unwrap();
    let mut env = pane_env(&repo, &state, &herdr, &tool_path, &log, &path);
    env.extend([
        ("HERDR_GITUI_CALLER_PANE", "caller-1"),
        ("HERDR_GITUI_CALLER_AGENT", "claude"),
        ("HERDR_GITUI_CALLER_TAB", "caller-tab"),
        ("HERDR_GITUI_PLACEMENT", "popup"),
        ("GITUI_SCRIPT", script.to_str().unwrap()),
    ]);
    let out = run_plugin(&env);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("sent session summary to agent pane caller-1"),
        "stderr: {}",
        stderr(&out)
    );
    let logged = read_log(&log);
    assert!(
        logged.contains("agent prompt caller-1 gitui session summary:"),
        "{logged}"
    );
    assert!(
        !logged.contains("plugin pane close"),
        "popup teardown must not close the caller pane: {logged}"
    );
    assert!(!logged.contains("tab close"), "{logged}");
    assert!(!logged.contains("tab focus"), "{logged}");
    // Delivery cleanup is unchanged: the summary file was removed.
    assert!(txt_files(&state.join("sessions")).is_empty());
}

#[test]
fn outside_herdr_errors() {
    let out = run_plugin(&[]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("invoked outside herdr plugin context"),
        "stderr: {}",
        stderr(&out)
    );
}

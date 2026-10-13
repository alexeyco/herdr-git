//! End-to-end tests for the dual-mode plugin binary.
//!
//! Each test spawns the real binary (`CARGO_BIN_EXE_herdr-gitui`) with a
//! hermetic environment, a stub `herdr` callback, and a stub `gitui` child.
//! The stubs append their argv to `$STUB_LOG`; assertions grep that log.
//! Everything runs in a unique temp directory and needs no network or global
//! git config.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

/// Stub `herdr`: logs every call, answers `plugin pane open` with a canned JSON
/// reply, always exits 0.
const HERDR_STUB: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "$STUB_LOG"
if [ "$1" = "plugin" ] && [ "$2" = "pane" ] && [ "$3" = "open" ]; then
  printf '%s\n' '{"result":{"plugin_pane":{"pane":{"pane_id":"pane-stub","tab_id":"tab-stub"}}}}'
fi
exit 0
"#;

/// Stub `gitui`: logs `GITUI <argv>`, runs `$GITUI_SCRIPT` if set (a shell
/// script that mutates the test repo between snapshots), then exits
/// `$GITUI_RC` (default 0).
const GITUI_STUB: &str = r#"#!/bin/sh
printf 'GITUI %s\n' "$*" >> "$STUB_LOG"
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

    /// Writes the `herdr` and `gitui` stubs and returns `(herdr, gitui,
    /// stub_log)` paths.
    fn stubs(&self) -> (PathBuf, PathBuf, PathBuf) {
        let herdr = self.dir.join("herdr");
        let gitui = self.dir.join("gitui");
        write_executable(&herdr, HERDR_STUB);
        write_executable(&gitui, GITUI_STUB);
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

/// Spawns the plugin binary with a cleared environment plus PATH/HOME/git
/// isolation and the caller-supplied env pairs. `stdin` is null so the hold
/// paths return at once instead of hanging.
fn run_plugin(envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_herdr-gitui"));
    cmd.stdin(Stdio::null())
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
    log: &'a Path,
    combined_path: &'a str,
) -> Vec<(&'a str, &'a str)> {
    vec![
        ("HERDR_PLUGIN_ENTRYPOINT_ID", "open"),
        ("HERDR_GITUI_DIR", repo.to_str().unwrap()),
        ("HERDR_PLUGIN_STATE_DIR", state.to_str().unwrap()),
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
    let mut env = pane_env(&repo, &state, &herdr, &log, &path);
    env.extend([
        ("HERDR_GITUI_CALLER_PANE", "caller-1"),
        ("HERDR_GITUI_CALLER_AGENT", "claude"),
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
    // gitui is invoked with bare argv (no positional path); the stub logs
    // "GITUI " followed by its (empty) argument list — no repo dir argument.
    let line = gitui_line(&logged);
    assert!(
        line.starts_with("GITUI") && !line.contains(repo.to_str().unwrap()),
        "expected bare gitui invocation (no dir arg), got: {logged}"
    );
    assert!(txt_files(&state.join("sessions")).is_empty());
}

#[test]
fn pane_no_changes_closes_pane() {
    let fx = Fixture::new("no-changes");
    let repo = fx.subdir("repo");
    clean_repo(&repo);
    let (herdr, gitui, log) = fx.stubs();
    let state = fx.subdir("state");

    let path = combined_path(gitui.parent().unwrap());
    let mut env = pane_env(&repo, &state, &herdr, &log, &path);
    env.extend([
        ("HERDR_GITUI_CALLER_PANE", "caller-1"),
        ("HERDR_GITUI_CALLER_AGENT", "claude"),
    ]);
    let out = run_plugin(&env);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let logged = read_log(&log);
    assert!(!logged.contains("agent prompt"), "{logged}");
    assert!(logged.contains("plugin pane close pane-7"), "{logged}");
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
    let mut env = pane_env(&repo, &state, &herdr, &log, &path);
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
    let mut env = pane_env(&repo, &state, &herdr, &log, &path);
    env.push(("GITUI_SCRIPT", script.to_str().unwrap()));
    // No HERDR_GITUI_CALLER_PANE or HERDR_GITUI_CALLER_AGENT.
    let out = run_plugin(&env);

    // Hold + stdin null → exit 0 (hold returns, no error).
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("no agent in focused pane; summary kept at"),
        "stderr: {}",
        stderr(&out)
    );
    let logged = read_log(&log);
    assert!(!logged.contains("agent prompt"), "{logged}");
    assert_eq!(txt_files(&state.join("sessions")).len(), 1);
}

#[test]
fn action_opens_pane_with_env_and_focus() {
    let fx = Fixture::new("action");
    let repo = fx.subdir("repo");
    let (herdr, _gitui, log) = fx.stubs();
    let ctx = format!(
        r#"{{"workspace_id":"ws-1","focused_pane_id":"p1","focused_pane_agent":"claude","worktree":{{"checkout_path":"{}"}}}}"#,
        repo.display()
    );

    let out = run_plugin(&[
        ("HERDR_PLUGIN_ACTION_ID", "open"),
        ("HERDR_PLUGIN_CONTEXT_JSON", &ctx),
        ("HERDR_BIN_PATH", herdr.to_str().unwrap()),
        ("STUB_LOG", log.to_str().unwrap()),
    ]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("opened gitui pane pane-stub in tab tab-stub"),
        "stderr: {}",
        stderr(&out)
    );
    let logged = read_log(&log);
    let open = logged
        .lines()
        .find(|l| l.starts_with("plugin pane open"))
        .unwrap_or("");
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
        open.contains("--env HERDR_GITUI_CALLER_AGENT=claude"),
        "{open}"
    );
    assert!(open.ends_with("--focus"), "{open}");
    assert!(logged.contains("tab focus tab-stub"), "{logged}");
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

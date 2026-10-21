use super::delivery;
use super::outcome::{self, Outcome};
use super::ports::{MockClock, MockHerdr, MockStateFilesystem, MockTuiProcess, StateFileEntry};
use super::state;
use super::teardown;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, UNIX_EPOCH};

static HERDR_ENV_LOCK: Mutex<()> = Mutex::new(());

struct IsolatedHerdrEnv {
    _lock: MutexGuard<'static, ()>,
    previous: Vec<(&'static str, Option<OsString>)>,
}

impl IsolatedHerdrEnv {
    fn new() -> Self {
        let lock = HERDR_ENV_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let keys = [
            "HERDR_GITUI_PLACEMENT",
            "HERDR_PLUGIN_ENTRYPOINT_ID",
            "HERDR_GITUI_CALLER_TAB",
            "HERDR_PANE_ID",
            "HERDR_TAB_ID",
        ];
        let previous = keys
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect();
        for key in keys {
            unsafe { std::env::remove_var(key) };
        }
        Self {
            _lock: lock,
            previous,
        }
    }
}

impl Drop for IsolatedHerdrEnv {
    fn drop(&mut self) {
        for (key, value) in &self.previous {
            match value {
                Some(value) => unsafe { std::env::set_var(key, value) },
                None => unsafe { std::env::remove_var(key) },
            }
        }
    }
}

#[test]
fn herdr_close_and_focus_failures_are_best_effort_with_tab_fallback() {
    let mut herdr = MockHerdr::new();
    herdr
        .expect_close_pane()
        .withf(|pane| pane == "pane-1")
        .times(1)
        .returning(|_| Err("pane unavailable".to_string()));
    herdr
        .expect_close_tab()
        .withf(|tab| tab == "tab-1")
        .times(1)
        .returning(|_| Err("tab unavailable".to_string()));
    herdr
        .expect_focus_tab()
        .withf(|tab| tab == "tab-1")
        .times(1)
        .returning(|_| Err("focus unavailable".to_string()));

    teardown::close_pane(&herdr, false, Some("pane-1"), Some("tab-1"));
    teardown::restore_caller_tab(&herdr, Some("tab-1"));
}

#[test]
fn successful_pane_close_does_not_close_tab() {
    let mut herdr = MockHerdr::new();
    herdr
        .expect_close_pane()
        .withf(|pane| pane == "pane-1")
        .times(1)
        .returning(|_| Ok(()));

    teardown::close_pane(&herdr, false, Some("pane-1"), Some("tab-1"));
}

#[test]
fn outcome_precedence_covers_kill_failure_no_change_agent_and_delivery() {
    assert_eq!(outcome::decide(None, "summary", true), Outcome::Killed);
    assert_eq!(
        outcome::decide(Some(7), "summary", true),
        Outcome::Failed(7)
    );
    assert_eq!(outcome::decide(Some(0), " \n", true), Outcome::NoChanges);
    assert_eq!(outcome::decide(Some(0), "summary", false), Outcome::NoAgent);
    assert_eq!(outcome::decide(Some(0), "summary", true), Outcome::Deliver);
}

#[test]
fn killed_and_failed_sessions_keep_their_precedence_and_file_policy() {
    let _environment = IsolatedHerdrEnv::new();
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let now = UNIX_EPOCH + Duration::from_millis(1_234);
    let mut clock = MockClock::new();
    clock.expect_now().returning(move || now);
    clock.expect_process_id().returning(|| 42);

    let filesystem = MockStateFilesystem::new();
    let herdr = MockHerdr::new();
    let process = MockTuiProcess::new();

    let killed = delivery::finish(
        delivery::FinishContext {
            tool_id: "gitui",
            code: None,
            sessions_dir: dir,
            summary_text: "summary",
            caller_pane: Some("pane"),
            caller_agent: Some("agent"),
        },
        &herdr,
        &process,
        &filesystem,
        &clock,
    );
    assert_eq!(
        killed.unwrap_err(),
        "gitui terminated without an exit code (killed?)"
    );

    let mut failed_fs = MockStateFilesystem::new();
    failed_fs
        .expect_write()
        .times(1)
        .returning(|path, contents| {
            assert_eq!(path.file_name().unwrap(), "42-1234.txt");
            assert_eq!(contents, b"gitui session summary:\n\nsummary");
            Ok(())
        });
    let failed = delivery::finish(
        delivery::FinishContext {
            tool_id: "gitui",
            code: Some(7),
            sessions_dir: dir,
            summary_text: "summary",
            caller_pane: Some("pane"),
            caller_agent: Some("agent"),
        },
        &herdr,
        &process,
        &failed_fs,
        &clock,
    );
    assert_eq!(failed.unwrap_err(), "gitui failed with exit code 7");
}

#[test]
fn no_change_and_no_agent_outcomes_have_expected_side_effects() {
    let _environment = IsolatedHerdrEnv::new();
    let temp = tempfile::tempdir().unwrap();
    let mut filesystem = MockStateFilesystem::new();
    filesystem.expect_write().times(1).returning(|_, contents| {
        assert_eq!(contents, b"lazygit session summary:\n\nsummary");
        Ok(())
    });
    let mut clock = MockClock::new();
    clock
        .expect_now()
        .returning(|| UNIX_EPOCH + Duration::from_millis(2_000));
    clock.expect_process_id().returning(|| 9);
    let mut process = MockTuiProcess::new();
    process
        .expect_hold_pane()
        .withf(|prompt| prompt == "herdr-gitui: press Enter to close")
        .times(1)
        .returning(|_| ());
    let herdr = MockHerdr::new();

    assert_eq!(outcome::decide(Some(0), "", false), Outcome::NoChanges);
    delivery::finish(
        delivery::FinishContext {
            tool_id: "lazygit",
            code: Some(0),
            sessions_dir: temp.path(),
            summary_text: "summary",
            caller_pane: Some("pane"),
            caller_agent: None,
        },
        &herdr,
        &process,
        &filesystem,
        &clock,
    )
    .unwrap();
}

#[test]
fn successful_delivery_removes_the_persisted_summary() {
    let _environment = IsolatedHerdrEnv::new();
    let temp = tempfile::tempdir().unwrap();
    let sessions = temp.path();
    let summary_file = sessions.join("42-1234.txt");
    let mut filesystem = MockStateFilesystem::new();
    let actual_dir = sessions.to_owned();
    filesystem
        .expect_write()
        .times(1)
        .returning(move |path, contents| {
            fs::write(actual_dir.join(path.file_name().unwrap()), contents)
        });
    let actual_file = summary_file.clone();
    filesystem
        .expect_remove_file()
        .times(1)
        .returning(move |path| {
            assert_eq!(path, actual_file);
            fs::remove_file(path)
        });
    let mut clock = MockClock::new();
    clock
        .expect_now()
        .returning(|| UNIX_EPOCH + Duration::from_millis(1_234));
    clock.expect_process_id().returning(|| 42);
    let mut herdr = MockHerdr::new();
    herdr
        .expect_agent_prompt()
        .withf(|pane, prompt| {
            pane == "pane-1" && prompt == "gitui session summary:\n\nchanged files"
        })
        .times(1)
        .returning(|_, _| Ok(()));
    let process = MockTuiProcess::new();

    // No pane identifiers are present in the test environment, so the success
    // path only exercises delivery and file cleanup.
    delivery::finish(
        delivery::FinishContext {
            tool_id: "gitui",
            code: Some(0),
            sessions_dir: sessions,
            summary_text: "changed files",
            caller_pane: Some("pane-1"),
            caller_agent: Some("agent-1"),
        },
        &herdr,
        &process,
        &filesystem,
        &clock,
    )
    .unwrap();
    assert!(!summary_file.exists());
}

#[test]
fn pruning_removes_only_expired_text_summaries() {
    let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
    let expired = Path::new("/sessions/expired.txt").to_owned();
    let recent = Path::new("/sessions/recent.txt").to_owned();
    let unrelated = Path::new("/sessions/expired.tmp").to_owned();
    let unreadable_time = Path::new("/sessions/unknown.txt").to_owned();
    let mut filesystem = MockStateFilesystem::new();
    filesystem.expect_read_dir().times(1).returning(move |_| {
        Ok(vec![
            StateFileEntry {
                path: expired.clone(),
                modified: Ok(now - Duration::from_secs(7 * 24 * 60 * 60)),
            },
            StateFileEntry {
                path: recent.clone(),
                modified: Ok(now - Duration::from_secs(60)),
            },
            StateFileEntry {
                path: unrelated.clone(),
                modified: Ok(now - Duration::from_secs(8 * 24 * 60 * 60)),
            },
            StateFileEntry {
                path: unreadable_time.clone(),
                modified: Err(std::io::Error::other("unavailable")),
            },
        ])
    });
    filesystem
        .expect_remove_file()
        .withf(|path| path == Path::new("/sessions/expired.txt"))
        .times(1)
        .returning(|_| Ok(()));

    assert!(state::prune_old_sessions(&filesystem, Path::new("/sessions"), now).is_empty());
}

#[test]
fn popup_teardown_skips_callbacks_and_tab_teardown_falls_back_to_tab() {
    let mut popup_herdr = MockHerdr::new();
    popup_herdr.expect_close_pane().times(0);
    popup_herdr.expect_close_tab().times(0);
    popup_herdr.expect_focus_tab().times(0);
    teardown::close_pane(&popup_herdr, true, Some("pane-1"), Some("tab-1"));

    let mut tab_herdr = MockHerdr::new();
    tab_herdr
        .expect_close_tab()
        .withf(|tab| tab == "tab-1")
        .times(1)
        .returning(|_| Ok(()));
    teardown::close_pane(&tab_herdr, false, None, Some("tab-1"));
}

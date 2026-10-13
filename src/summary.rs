//! Pure summary generation from before/after snapshots and git query results.
//!
//! The summary is an English description of what changed during a gitui session.
//! All rules are implemented as a pure function over snapshots and a small
//! trait for the few git queries summary needs, so every rule is testable with
//! synthetic inputs — no git required.

use crate::git::Snapshot;

/// A single commit line: short hash and subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommitLine {
    pub(crate) hash: String,
    pub(crate) subject: String,
}

/// Git queries the summary builder needs. The real implementation calls git;
/// tests provide a mock.
pub(crate) trait GitQueries {
    /// Is `ancestor` an ancestor of `descendant`? `None` on probe failure.
    fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Option<bool>;
    /// Commit log lines for `from..to` range (no merges, reverse). `None` on
    /// probe failure.
    fn log_range(&self, from: &str, to: &str) -> Option<Vec<CommitLine>>;
    /// Whether there are merge commits in `from..to`. `None` on probe failure.
    fn has_merges_in_range(&self, from: &str, to: &str) -> Option<bool>;
    /// Up to `n` commit log lines from a single commit. `None` on probe failure.
    fn log_head(&self, head: &str, n: usize) -> Option<Vec<CommitLine>>;
}

/// Builds the session summary from before/after snapshots and git queries.
/// Returns an empty string when there is nothing to report.
pub(crate) fn summary(before: &Snapshot, after: &Snapshot, q: &dyn GitQueries) -> String {
    // Rule 1: either snapshot not in a repo → empty.
    if !before.in_repo || !after.in_repo {
        return String::new();
    }

    let mut lines: Vec<String> = Vec::new();
    let branch_switched = before.branch != after.branch;

    // Rule 2: branch switch.
    if branch_switched {
        match &after.branch {
            Some(name) => lines.push(format!("- switched to branch {name}")),
            None => {
                let short = after.head.as_deref().map(short_sha).unwrap_or("unknown");
                lines.push(format!("- switched to a detached HEAD at {short}"));
            }
        }
        // On branch switch, list up to 5 commits unique to the new HEAD.
        if let Some(head) = &after.head
            && let Some(commits) = q.log_head(head, 5)
        {
            for c in &commits {
                let branch_desc = after.branch.as_deref().unwrap_or("HEAD");
                lines.push(format!(
                    "- branch {branch_desc} is at commit {}: {}",
                    c.hash, c.subject
                ));
            }
        }
    }

    // Rules 3–5: same-branch commit history changes (skipped on branch switch).
    if !branch_switched
        && let (Some(b_head), Some(a_head)) = (&before.head, &after.head)
        && b_head != a_head
    {
        let ancestor_of = q.is_ancestor(b_head, a_head);
        let reverse_ancestor = q.is_ancestor(a_head, b_head);

        match (ancestor_of, reverse_ancestor) {
            // Rule 3: created commits (before is ancestor of after).
            (Some(true), _) => {
                if let Some(commits) = q.log_range(b_head, a_head) {
                    let cap = 10;
                    let total = commits.len();
                    for c in commits.iter().take(cap) {
                        lines.push(format!("- created commit {}: {}", c.hash, c.subject));
                    }
                    if total > cap {
                        lines.push(format!("- ... and {} more commits", total - cap));
                    }
                }
                // Merge line: if the range contains merge commits.
                if let Some(true) = q.has_merges_in_range(b_head, a_head)
                    && let Some(branch) = &after.branch
                {
                    lines.push(format!("- merged into branch {branch}"));
                }
            }
            // Rule 5: reset (after is ancestor of before).
            (Some(false), Some(true)) => {
                if let Some(branch) = &after.branch {
                    lines.push(format!(
                        "- reset branch {branch}: HEAD moved back from {} to {}",
                        short_sha(b_head),
                        short_sha(a_head)
                    ));
                }
            }
            // Rule 4: amend/rebase (neither is ancestor of the other).
            (Some(false), Some(false)) => {
                if let Some(branch) = &after.branch {
                    lines.push(format!(
                        "- amended or rebased branch {branch}: HEAD moved from {} to {}",
                        short_sha(b_head),
                        short_sha(a_head)
                    ));
                }
            }
            // Probe failures: omit the line.
            _ => {}
        }
    }

    // Rule 6: push (same upstream on both sides, ahead-count decreased).
    if before.upstream.is_some()
        && after.upstream.is_some()
        && before.upstream == after.upstream
        && let (Some(b_ahead), Some(a_ahead)) = (before.ahead, after.ahead)
        && a_ahead < b_ahead
    {
        let upstream = after.upstream.as_deref().unwrap_or("upstream");
        lines.push(format!(
            "- pushed {} commit(s) to {upstream}",
            b_ahead - a_ahead
        ));
    }

    // Rule 7: staging changes (only increases).
    if after.staged > before.staged {
        lines.push(format!(
            "- staged {} more file(s)",
            after.staged - before.staged
        ));
    }
    if after.unstaged > before.unstaged {
        lines.push(format!(
            "- unstaged {} more file(s)",
            after.unstaged - before.unstaged
        ));
    }
    if after.untracked > before.untracked {
        lines.push(format!(
            "- {} more untracked file(s)",
            after.untracked - before.untracked
        ));
    }

    lines.join("\n")
}

/// First 7 characters of a SHA.
fn short_sha(sha: &str) -> &str {
    if sha.len() >= 7 { &sha[..7] } else { sha }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A mock git-queries implementation for unit tests.
    struct MockQueries {
        ancestors: HashMap<(String, String), bool>,
        ranges: HashMap<(String, String), Vec<CommitLine>>,
        merges: HashMap<(String, String), bool>,
        heads: HashMap<String, Vec<CommitLine>>,
    }

    impl MockQueries {
        fn new() -> Self {
            Self {
                ancestors: HashMap::new(),
                ranges: HashMap::new(),
                merges: HashMap::new(),
                heads: HashMap::new(),
            }
        }

        fn ancestor(mut self, a: &str, b: &str, result: bool) -> Self {
            self.ancestors
                .insert((a.to_string(), b.to_string()), result);
            self
        }

        fn range(mut self, from: &str, to: &str, commits: Vec<CommitLine>) -> Self {
            self.ranges
                .insert((from.to_string(), to.to_string()), commits);
            self
        }

        fn merges(mut self, from: &str, to: &str, has: bool) -> Self {
            self.merges.insert((from.to_string(), to.to_string()), has);
            self
        }

        fn head_log(mut self, head: &str, commits: Vec<CommitLine>) -> Self {
            self.heads.insert(head.to_string(), commits);
            self
        }
    }

    impl GitQueries for MockQueries {
        fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Option<bool> {
            self.ancestors
                .get(&(ancestor.to_string(), descendant.to_string()))
                .copied()
        }

        fn log_range(&self, from: &str, to: &str) -> Option<Vec<CommitLine>> {
            self.ranges
                .get(&(from.to_string(), to.to_string()))
                .cloned()
        }

        fn has_merges_in_range(&self, from: &str, to: &str) -> Option<bool> {
            self.merges
                .get(&(from.to_string(), to.to_string()))
                .copied()
        }

        fn log_head(&self, head: &str, n: usize) -> Option<Vec<CommitLine>> {
            self.heads
                .get(head)
                .map(|v| v.iter().take(n).cloned().collect())
        }
    }

    fn commit(h: &str, s: &str) -> CommitLine {
        CommitLine {
            hash: h.to_string(),
            subject: s.to_string(),
        }
    }

    fn repo_snapshot() -> Snapshot {
        Snapshot {
            in_repo: true,
            branch: Some("main".to_string()),
            head: Some("a".repeat(40)),
            upstream: None,
            ahead: None,
            staged: 0,
            unstaged: 0,
            untracked: 0,
        }
    }

    #[test]
    fn not_in_repo_returns_empty() {
        let before = Snapshot::default();
        let after = repo_snapshot();
        let q = MockQueries::new();
        assert_eq!(summary(&before, &after, &q), "");
    }

    #[test]
    fn no_changes_returns_empty() {
        let s = repo_snapshot();
        let q = MockQueries::new();
        assert_eq!(summary(&s, &s, &q), "");
    }

    #[test]
    fn created_single_commit() {
        let before = repo_snapshot();
        let mut after = before.clone();
        after.head = Some("b".repeat(40));
        let q = MockQueries::new()
            .ancestor(&"a".repeat(40), &"b".repeat(40), true)
            .range(
                &"a".repeat(40),
                &"b".repeat(40),
                vec![commit("abc1234", "fix: handle edge case")],
            )
            .merges(&"a".repeat(40), &"b".repeat(40), false);
        let s = summary(&before, &after, &q);
        assert!(
            s.contains("- created commit abc1234: fix: handle edge case"),
            "{s}"
        );
    }

    #[test]
    fn created_commits_capped_at_10() {
        let before = repo_snapshot();
        let mut after = before.clone();
        after.head = Some("b".repeat(40));
        let commits: Vec<CommitLine> = (0..15)
            .map(|i| commit(&format!("h{i:07}"), &format!("commit {i}")))
            .collect();
        let q = MockQueries::new()
            .ancestor(&"a".repeat(40), &"b".repeat(40), true)
            .range(&"a".repeat(40), &"b".repeat(40), commits)
            .merges(&"a".repeat(40), &"b".repeat(40), false);
        let s = summary(&before, &after, &q);
        assert!(s.contains("- ... and 5 more commits"), "{s}");
        // Only 10 commit lines.
        let commit_lines = s
            .lines()
            .filter(|l| l.starts_with("- created commit"))
            .count();
        assert_eq!(commit_lines, 10);
    }

    #[test]
    fn merge_commit_adds_merged_line() {
        let before = repo_snapshot();
        let mut after = before.clone();
        after.head = Some("b".repeat(40));
        let q = MockQueries::new()
            .ancestor(&"a".repeat(40), &"b".repeat(40), true)
            .range(
                &"a".repeat(40),
                &"b".repeat(40),
                vec![commit("abc1234", "merge branch feature")],
            )
            .merges(&"a".repeat(40), &"b".repeat(40), true);
        let s = summary(&before, &after, &q);
        assert!(s.contains("- merged into branch main"), "{s}");
    }

    #[test]
    fn amend_or_rebase() {
        let a_hash = "a".repeat(40);
        let b_hash = "b".repeat(40);
        let before = repo_snapshot();
        let mut after = before.clone();
        after.head = Some(b_hash.clone());
        let q = MockQueries::new()
            .ancestor(&a_hash, &b_hash, false)
            .ancestor(&b_hash, &a_hash, false);
        let s = summary(&before, &after, &q);
        let a_short = short_sha(&a_hash);
        let b_short = short_sha(&b_hash);
        assert!(
            s.contains(&format!(
                "- amended or rebased branch main: HEAD moved from {a_short} to {b_short}"
            )),
            "{s}"
        );
    }

    #[test]
    fn reset_branch() {
        let a_hash = "a".repeat(40);
        let b_hash = "b".repeat(40);
        let before = repo_snapshot();
        let mut after = before.clone();
        after.head = Some(b_hash.clone());
        let q = MockQueries::new()
            .ancestor(&a_hash, &b_hash, false)
            .ancestor(&b_hash, &a_hash, true);
        let s = summary(&before, &after, &q);
        let a_short = short_sha(&a_hash);
        let b_short = short_sha(&b_hash);
        assert!(
            s.contains(&format!(
                "- reset branch main: HEAD moved back from {a_short} to {b_short}"
            )),
            "{s}"
        );
    }

    #[test]
    fn ancestor_probe_failure_omits_rules_3_to_5() {
        let before = repo_snapshot();
        let mut after = before.clone();
        after.head = Some("b".repeat(40));
        // Neither ancestor direction is known (probe failure → None).
        let q = MockQueries::new();
        let s = summary(&before, &after, &q);
        assert!(!s.contains("created commit"), "{s}");
        assert!(!s.contains("amended"), "{s}");
        assert!(!s.contains("reset"), "{s}");
        assert!(s.is_empty(), "expected empty summary, got: {s}");
    }

    #[test]
    fn branch_switch() {
        let mut before = repo_snapshot();
        before.branch = Some("main".to_string());
        let mut after = before.clone();
        after.branch = Some("feature-x".to_string());
        after.head = Some("c".repeat(40));
        let q =
            MockQueries::new().head_log(&"c".repeat(40), vec![commit("c123456", "start feature")]);
        let s = summary(&before, &after, &q);
        assert!(s.contains("- switched to branch feature-x"), "{s}");
        assert!(
            s.contains("- branch feature-x is at commit c123456: start feature"),
            "{s}"
        );
        // Rules 3–5 must be skipped.
        assert!(!s.contains("created commit"), "{s}");
        assert!(!s.contains("amended"), "{s}");
        assert!(!s.contains("reset"), "{s}");
    }

    #[test]
    fn branch_switch_to_detached_head() {
        let mut before = repo_snapshot();
        before.branch = Some("main".to_string());
        let mut after = before.clone();
        after.branch = None;
        after.head = Some("deadbee0deadbee0".to_string());
        let q = MockQueries::new();
        let s = summary(&before, &after, &q);
        assert!(
            s.contains("- switched to a detached HEAD at deadbee"),
            "{s}"
        );
    }

    #[test]
    fn push_detected_by_ahead_decrease() {
        let mut before = repo_snapshot();
        before.upstream = Some("origin/main".to_string());
        before.ahead = Some(3);
        let mut after = before.clone();
        after.ahead = Some(1);
        let q = MockQueries::new();
        let s = summary(&before, &after, &q);
        assert!(s.contains("- pushed 2 commit(s) to origin/main"), "{s}");
    }

    #[test]
    fn push_not_detected_when_ahead_increases() {
        let mut before = repo_snapshot();
        before.upstream = Some("origin/main".to_string());
        before.ahead = Some(1);
        let mut after = before.clone();
        after.ahead = Some(3);
        let q = MockQueries::new();
        let s = summary(&before, &after, &q);
        assert!(!s.contains("pushed"), "{s}");
    }

    #[test]
    fn staging_deltas_only_increases() {
        let mut before = repo_snapshot();
        before.staged = 2;
        before.unstaged = 3;
        before.untracked = 1;
        let mut after = before.clone();
        after.staged = 5; // +3
        after.unstaged = 1; // decrease, not reported
        after.untracked = 4; // +3
        let q = MockQueries::new();
        let s = summary(&before, &after, &q);
        assert!(s.contains("- staged 3 more file(s)"), "{s}");
        assert!(!s.contains("unstaged"), "{s}");
        assert!(s.contains("- 3 more untracked file(s)"), "{s}");
    }

    #[test]
    fn branch_switch_still_reports_push_and_staging() {
        let mut before = repo_snapshot();
        before.branch = Some("main".to_string());
        before.upstream = Some("origin/main".to_string());
        before.ahead = Some(5);
        before.staged = 1;
        let mut after = before.clone();
        after.branch = Some("feature".to_string());
        after.head = Some("f".repeat(40));
        after.ahead = Some(3);
        after.staged = 4;
        let q = MockQueries::new().head_log(&"f".repeat(40), vec![]);
        let s = summary(&before, &after, &q);
        assert!(s.contains("- switched to branch feature"), "{s}");
        assert!(s.contains("- pushed 2 commit(s) to origin/main"), "{s}");
        assert!(s.contains("- staged 3 more file(s)"), "{s}");
    }

    #[test]
    fn short_sha_truncates_to_7() {
        assert_eq!(short_sha("abcdef1234567890"), "abcdef1");
        assert_eq!(short_sha("abc"), "abc");
    }
}

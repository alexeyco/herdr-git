# session-summary

## Purpose

The session-summary capability turns the before/after pair of git snapshots
into a short, human- and agent-readable description of what changed during a
gitui session. It is a pure function over the two snapshots plus a few commit
queries (ancestry checks and commit-log lines), so each rule is testable
without git. The summary is deliberately heuristic: it reports branch switches,
new commits, merges, resets, amend/rebase, pushes, and increases in staged,
unstaged, and untracked files, as counts and short hashes rather than full
file lists. Commit-hash references and subject lines are drawn from git log
output; where a classification cannot be made — an ancestry probe fails, or a
change is not observable from the snapshot fields — the summary omits the line
rather than guessing. When either snapshot falls outside a repository, the
summary is empty.

## Requirements

### Requirement: Summary is empty unless both snapshots are in a repository

The summary SHALL return an empty string whenever either the before or the
after snapshot reports `in_repo` false.

#### Scenario: Either side outside a repo yields empty

- **WHEN** the before snapshot has `in_repo` false, or the after snapshot has
  `in_repo` false
- **THEN** the summary is an empty string and no rule lines are emitted

### Requirement: A branch switch is reported and suppresses same-branch history rules

When `before.branch` differs from `after.branch`, the summary SHALL emit a
switch line, then up to five commit lines describing the newest commits
reachable from `after.head`. A branch switch SHALL skip the same-branch history
classification (created / merged / reset / amend) but SHALL still allow the
push and staging-delta rules to apply.

#### Scenario: Switch to a named branch lists its newest commits

- **WHEN** before branch is `main` and after branch is `feature-x` with a
  resolvable HEAD
- **THEN** the first line is `- switched to branch feature-x` followed by up to
  five `- branch feature-x is at commit <hash>: <subject>` lines, and no
  `created commit`, `amended`, or `reset` line appears

#### Scenario: The commit lines are the newest five, not unique to the new head

- **WHEN** after head resolves and the switch lines are built
- **THEN** the commit lines come from `git log --no-merges --format=%h%x09%s -n
5 <after.head>`, i.e. the newest five non-merge commits, NOT filtered to
  those unique to the new HEAD, and the branch label is `after.branch` or `HEAD` when detached

#### Scenario: Switch to a detached HEAD

- **WHEN** after branch is `None` and after head is present
- **THEN** `- switched to a detached HEAD at <short>` is emitted, where `<short>`
  is the first seven characters of `after.head`, with commit lines labeling the
  branch as `HEAD`

#### Scenario: Detached switch with no head

- **WHEN** after branch is `None` and after head is also `None`
- **THEN** the detached switch line uses `unknown` in place of the short SHA

#### Scenario: Branch switch still reports push and staging deltas

- **WHEN** a branch switch also decreases the ahead count against the same
  upstream and increases the staged count
- **THEN** the push line and the staged line appear alongside the switch lines

### Requirement: New commits on the same branch are listed, capped at ten

When the branch is unchanged and both heads are present and differ and
`before.head` is an ancestor of `after.head`, the summary SHALL emit one
`- created commit <hash>: <subject>` line per new commit, showing at most ten
and appending an overflow line when more exist. When the new range also
contains a merge commit and `after.branch` is present, it SHALL emit a
`- merged into branch <branch>` line.

#### Scenario: A single new commit

- **WHEN** before head is an ancestor of after head and the range contains one
  non-merge commit `abc1234` "fix: handle edge case"
- **THEN** a line `- created commit abc1234: fix: handle edge case` appears,
  the commit lines coming from `git log --no-merges --reverse
--format=%h%x09%s <before>..<after>`

#### Scenario: More than ten new commits are capped

- **WHEN** the range contains fifteen non-merge commits
- **THEN** ten `- created commit ...` lines appear followed by `- ... and 5
more commits` (the count beyond the ten-commit cap)

#### Scenario: A merge in the range adds a merged line

- **WHEN** before head is an ancestor of after head, the range contains a merge
  commit (detected via `git rev-list --merges <before>..<after>` returning a
  non-empty result), and after branch is `main`
- **THEN** `- merged into branch main` is emitted in addition to the created
  commit lines

### Requirement: A backwards or rewritten same-branch HEAD is classified by bidirectional ancestry

When the branch is unchanged, both heads are present and differ, and
`before.head` is NOT an ancestor of `after.head`, the summary SHALL classify
the change by reverse ancestry: after-is-ancestor-of-before yields a reset
line, neither-ancestor yields an amend-or-rebase line, and a failed ancestry
probe SHALL omit the line entirely. Both classifications require
`after.branch` to be present.

#### Scenario: Head moved back to an ancestor

- **WHEN** before head is not an ancestor of after head, but after head is an
  ancestor of before head (via `git merge-base --is-ancestor`), on branch `main`
- **THEN** `- reset branch main: HEAD moved back from <before7> to <after7>` is
  emitted, where each `<short>` is the first seven characters of the full SHA

#### Scenario: Head rewritten to an unrelated commit

- **WHEN** neither head is an ancestor of the other, on branch `main`
- **THEN** `- amended or rebased branch main: HEAD moved from <before7> to
<after7>` is emitted with the same seven-character short hashes, deliberately
  not distinguishing an amend from a rebase

#### Scenario: Ancestry probe failure omits the line

- **WHEN** both ancestry probes return unknown and the branch is unchanged with
  differing heads
- **THEN** no created, reset, or amended line is emitted

### Requirement: A decrease in ahead against an unchanged upstream is reported as a push

When `before.upstream` and `after.upstream` are both present and equal, and
both ahead counts are present, and `after.ahead` is less than `before.ahead`,
the summary SHALL emit `- pushed <delta> commit(s) to <upstream>` where `<delta>`
is `before.ahead - after.ahead`. An increase in the ahead count SHALL NOT be
reported, and a push that occurred in a different clone SHALL be invisible
because only the local ahead count is observed.

#### Scenario: Ahead count decreased

- **WHEN** upstream is `origin/main` on both sides and ahead goes from 3 to 1
- **THEN** `- pushed 2 commit(s) to origin/main` is emitted

#### Scenario: Ahead count increased is not reported

- **WHEN** upstream is `origin/main` on both sides and ahead goes from 1 to 3
- **THEN** no `pushed` line appears

### Requirement: Staging deltas are reported as increases only, as counts not names

The summary SHALL emit `- staged <n> more file(s)` when `after.staged` exceeds
`before.staged`, `- unstaged <n> more file(s)` when `after.unstaged` exceeds
`before.unstaged`, and `- <n> more untracked file(s)` when `after.untracked`
exceeds `before.untracked`, where `<n>` is the positive difference. Decreases
in any of these counts SHALL be suppressed and only counts, never file names,
SHALL be reported.

#### Scenario: Only increases are reported

- **WHEN** staged goes from 2 to 5, unstaged goes from 3 to 1, and untracked
  goes from 1 to 4
- **THEN** `- staged 3 more file(s)` and `- 3 more untracked file(s)` appear,
  and no `unstaged` line is emitted

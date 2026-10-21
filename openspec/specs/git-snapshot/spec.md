# Git snapshot

## Purpose

The git-snapshot capability captures a point-in-time, read-only view of a
repository's state so that a gitui session can be summarized afterward. A
snapshot is a fixed set of fields — repository membership, branch, HEAD,
upstream, ahead count, and staged/unstaged/untracked file counts — gathered by
running read-only git commands in the target directory. It never mutates the
index or the working tree. Two snapshots are taken, one before gitui launches
and one after it exits, and the summary capability diffs them. When a probe
fails unexpectedly, capture degrades gracefully: a fatal failure of the
work-tree check yields a default non-repo snapshot and a single failing count
yields zero for just that field, each with a warning on stderr, so a partial
result still survives.

## Requirements

### Requirement: Snapshot captures a fixed set of read-only fields

The snapshot SHALL capture exactly these fields and no others: `in_repo`,
`branch`, `head`, `upstream`, `ahead`, `staged`, `unstaged`, `untracked`. No
stash, behind, or conflict state exists — there are no such fields.

#### Scenario: Committed repository reports branch and head

- **WHEN** a snapshot is taken of a repository that has commits on branch
  `main`
- **THEN** `in_repo` is true, `branch` is `Some("main")`, and `head` is the
  full SHA of HEAD

#### Scenario: Detached HEAD has no branch

- **WHEN** a snapshot is taken while HEAD points directly at a commit
  (detached)
- **THEN** `branch` is `None` (`symbolic-ref --short -q HEAD` fails) while
  `in_repo` remains true

#### Scenario: Repository without commits has no head

- **WHEN** a snapshot is taken of a freshly initialized repository with no
  commits yet
- **THEN** `head` is `None` (`rev-parse HEAD` fails) and `in_repo` is true

### Requirement: Probes are read-only and scoped to the target directory

Every git command the snapshot runs SHALL execute with the working directory
set to the target directory and SHALL NOT mutate the index or the working tree.
The pane flow SHALL take exactly two snapshots that bracket the gitui session:
one before gitui is spawned and one after it exits.

#### Scenario: Snapshot does not alter repository state

- **WHEN** a snapshot is taken of any repository
- **THEN** the index and working tree are unchanged, because every probe is a
  read-only git command run with `current_dir` set to the repository

#### Scenario: Every probe runs in the target directory

- **WHEN** a snapshot is captured
- **THEN** each of `rev-parse --is-inside-work-tree`, `rev-parse HEAD`,
  `symbolic-ref --short -q HEAD`, `rev-parse --abbrev-ref --symbolic-full-name
@{upstream}`, `rev-list --count @{upstream}..HEAD`, `diff --cached
--name-only`, `diff --name-only`, and `ls-files --others --exclude-standard`
  runs with its working directory set to the target directory

#### Scenario: Two snapshots bracket the gitui session

- **WHEN** a pane session runs gitui in a repository
- **THEN** a `before` snapshot is captured before the gitui process starts and
  an `after` snapshot is captured after it exits

### Requirement: Ahead count is computed only when an upstream exists

The snapshot SHALL compute `ahead` via `rev-list --count @{upstream}..HEAD`
only when the upstream probe returned `Some`; when no upstream is configured,
`ahead` SHALL be `None` and the ahead command SHALL NOT run.

#### Scenario: Upstream present yields a count

- **WHEN** the branch has an upstream and HEAD is 3 commits ahead of it
- **THEN** `upstream` is `Some` and `ahead` is `Some(3)`

#### Scenario: No upstream yields none

- **WHEN** the branch has no configured upstream
- **THEN** `upstream` is `None` and `ahead` is `None`, and no `rev-list` ahead
  probe is attempted

### Requirement: Staged, unstaged, and untracked are counts of non-blank output lines

The snapshot SHALL derive `staged` from `git diff --cached --name-only`,
`unstaged` from `git diff --name-only`, and `untracked` from `git ls-files
--others --exclude-standard`, each as the number of non-blank lines on stdout
— a line consisting only of whitespace does not count. These are counts of
files, not their names.

#### Scenario: File deltas are counted

- **WHEN** one file is staged, one is modified but unstaged, and one is
  untracked
- **THEN** `staged` is 1, `unstaged` is 1, and `untracked` is 1

### Requirement: Probe failures degrade gracefully with a stderr warning

Only the work-tree check is fatal: when `rev-parse --is-inside-work-tree`
cannot be spawned, the snapshot SHALL write
`herdr-gitui: warning: git probe failed (<cmd>): <err>` to stderr and return
a default non-repo snapshot; a check that denies repository membership is
the expected non-repo signal and SHALL NOT warn. Every other
count-command failure SHALL write the same warning form to stderr, zeroing
only that field and keeping the rest of the snapshot
(src/git.rs:63,90-94,31-45,150-161).

#### Scenario: Work-tree check cannot spawn

- **WHEN** the `rev-parse --is-inside-work-tree` command cannot be run (the
  git process fails to spawn)
- **THEN** a `herdr-gitui: warning: git probe failed (...)` line is written to
  stderr and the returned snapshot has `in_repo` false (a default snapshot)

#### Scenario: Work-tree check answers unexpectedly

- **WHEN** `rev-parse --is-inside-work-tree` runs but exits non-zero, or exits
  0 with stdout that is not `true`
- **THEN** a default non-repo snapshot is returned with no warning

#### Scenario: A single failing count zeroes only that field

- **WHEN** one of the staged/unstaged/untracked count commands fails — either
  it cannot spawn or it exits non-zero
- **THEN** a `herdr-gitui: warning: git probe failed (...)` line is written to
  stderr, that field is 0, and the other snapshot fields still reflect the
  repository

#### Scenario: Unavailable optional probe returns none silently

- **WHEN** a quiet probe (`rev-parse HEAD`, `symbolic-ref --short -q HEAD`, the
  upstream probe, or the ahead probe) fails because the value is simply not
  available (no commits, detached HEAD, or no upstream)
- **THEN** that field is `None` and no warning is emitted

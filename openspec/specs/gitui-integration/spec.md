# gitui-integration

## Purpose

This capability covers how the plugin locates the `gitui` binary and launches
it for a repository session. gitui is expected to be installed by the user and
found on `PATH`; the plugin never installs, updates, or version-checks it.
Resolution and launch happen only in pane mode: the action mode opens the pane
and exits without ever touching gitui. Once pane mode starts, it picks the
repository directory, resolves gitui to a canonical absolute path, prints a
single banner line, and spawns gitui as a direct argument-less child confined
to that directory, waiting for it to exit before the session result is handed
to outcome mapping (specified separately under `session-outcomes`).

## Requirements

### Requirement: gitui is resolved from PATH only

The plugin SHALL locate gitui by scanning the `PATH` environment variable
(treated as empty when unset), splitting it into directories, skipping empty
entries, and selecting the first `gitui` candidate that is a regular file with
any executable bit set (Unix mode `& 0o111 != 0`; on non-Unix platforms any
regular file qualifies). There SHALL be no override environment variable, no
configuration, and no minimum gitui version check.
(`src/gitui.rs:16-19,32-41,55-71`)

#### Scenario: First executable entry wins

- **WHEN** two `PATH` directories each contain an executable `gitui` file
- **THEN** the candidate from the earlier directory is selected

#### Scenario: Non-executable candidate is skipped and search continues

- **WHEN** the first `PATH` directory contains a `gitui` file with mode 0644
  (no executable bit) and a later directory contains an executable `gitui`
- **THEN** the non-executable file is skipped and the later executable one is
  selected

#### Scenario: gitui missing from PATH

- **WHEN** no `PATH` entry contains an executable `gitui`
- **THEN** resolution fails with the message
  `gitui not found on PATH; install it from https://github.com/gitui-org/gitui`
  (`src/gitui.rs:12,27`)

### Requirement: The resolved path is canonicalized to an absolute path

The selected candidate SHALL be canonicalized to an absolute path before use.
There SHALL be no working-directory fallback: a candidate that cannot be
canonicalized is a hard error, because the action and pane processes can have
different working directories. Relative `PATH` entries still resolve to an
absolute result. (`src/gitui.rs:43-53`)

#### Scenario: Relative PATH entry resolves to absolute

- **WHEN** a `PATH` entry is relative (e.g. `.`) and contains an executable
  `gitui`
- **THEN** the resolved path is absolute and points at that same file

#### Scenario: Canonicalization failure

- **WHEN** the first executable candidate cannot be canonicalized
- **THEN** resolution fails with the message
  `found gitui at <candidate> but cannot resolve it to an absolute path: <error>`

### Requirement: gitui resolution happens in pane mode before any user-visible output

Pane mode SHALL resolve gitui after checking `HERDR_BIN_PATH`, resolving the
repository directory, and creating/pruning the sessions directory, and before
taking the before-snapshot or printing anything to stdout, so a missing binary
fails without visible output. Action mode SHALL NOT resolve gitui at all.
(`src/session.rs:133-156`, `src/session.rs:95-129`, `src/herdr.rs:38-42`)

#### Scenario: Missing binary fails before any banner

- **WHEN** pane mode starts and no executable `gitui` exists on `PATH`
- **THEN** the plugin returns the not-found error without running a git
  snapshot and without printing the `herdr-gitui: starting gitui ...` line

#### Scenario: Action mode never checks gitui availability

- **WHEN** the binary runs in action mode
- **THEN** it resolves the repository context and asks herdr to open the pane
  without ever calling gitui resolution; a missing gitui cannot fail the
  action process itself

### Requirement: The launch directory is HERDR_GITUI_DIR or the process cwd

Pane mode SHALL use the trimmed, non-blank value of `HERDR_GITUI_DIR` as the
repository directory when present, otherwise the process working directory. A
working-directory read failure SHALL produce the message
`cannot read current directory: <error>`. (`src/session.rs:137-142,426-431`)

#### Scenario: Exported directory wins

- **WHEN** `HERDR_GITUI_DIR` is set to a non-blank value
- **THEN** that value is used as the repository directory regardless of the
  process working directory

#### Scenario: Blank variable falls back to cwd

- **WHEN** `HERDR_GITUI_DIR` is unset, empty, or whitespace-only
- **THEN** the plugin uses the process working directory, and reports
  `cannot read current directory: <error>` if reading it fails

### Requirement: gitui is launched as a direct argument-less child and awaited to exit

Pane mode SHALL spawn the canonicalized gitui binary as an argument-less
direct child with the repository directory as `current_dir`, block on
`Command::status()` with no timeout until it exits, and pass the collected
exit status to outcome mapping, specified under `session-outcomes`.
(`src/session.rs:156-163`)

UNVERIFIED: whether the inherited stdio lands on a real pane tty is herdr's
external behavior; the plugin code only shows that stdio is not redirected.

#### Scenario: Clean launch

- **WHEN** gitui is resolved and the repository directory is set
- **THEN** the plugin prints `herdr-gitui: starting gitui in <dir>` to stdout
  before spawning, then spawns gitui with no arguments and `current_dir` set
  to that directory, and waits indefinitely for it to exit

#### Scenario: Child inherits the pane stdio

- **WHEN** gitui is spawned
- **THEN** the plugin does not reconfigure stdio: the child inherits the pane
  process stdio unchanged

#### Scenario: No positional path and no -d

- **WHEN** gitui is launched for a repository directory
- **THEN** gitui discovers the repo from its working directory; the plugin
  passes no positional path and no `-d` flag, because `-d` means the git dir

UNVERIFIED: the claim that gitui 0.28 rejects a positional path argument comes
from the code comment at `src/session.rs:157-159`; this plugin never exercises
gitui itself to prove it.

#### Scenario: Spawn failure

- **WHEN** spawning the resolved binary fails (e.g. the file disappeared after
  resolution)
- **THEN** the plugin returns `failed to run <gitui_bin> in <dir>: <error>`

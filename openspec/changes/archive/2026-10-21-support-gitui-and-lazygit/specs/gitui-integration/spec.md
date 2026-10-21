# GitUI integration

## MODIFIED Requirements

### Requirement: gitui is resolved from PATH only

The plugin SHALL resolve `gitui` and `lazygit` by scanning `PATH` for the first regular executable candidate of each name, skipping empty entries, and canonicalize each selected path to an absolute path before launching it. There SHALL be no binary override setting and no minimum-version check for either tool. Action mode performs discovery and selection; pane mode launches the selected canonical path.

#### Scenario: First executable entry wins

- **WHEN** multiple `PATH` directories contain executable candidates for either supported tool
- **THEN** the candidate from the earliest directory is selected for that tool

#### Scenario: Non-executable candidate is skipped and search continues

- **WHEN** an earlier `PATH` directory contains a non-executable candidate and a later directory contains an executable candidate for the same supported tool
- **THEN** the non-executable file is skipped and the later executable one is selected

#### Scenario: gitui missing from PATH

- **WHEN** neither supported tool is available on `PATH`
- **THEN** action mode shows setup guidance with installation links for gitui and lazygit and does not open a pane

#### Scenario: Selected tool path is absolute

- **WHEN** the selected tool candidate is found through a relative `PATH` entry
- **THEN** the path passed to pane mode is canonical and absolute

### Requirement: gitui is launched as a direct argument-less child and awaited to exit

Pane mode SHALL launch the selected canonical `gitui` or `lazygit` binary as an argument-less direct child, with the existing repository directory as `current_dir`, inherit pane stdio, and wait for its exit status. Before and after git snapshots SHALL remain read-only and surround the selected TUI process.

#### Scenario: Clean launch

- **WHEN** a selected supported tool is resolved and the repository directory is set
- **THEN** the plugin prints its tool-specific start line before spawning the selected TUI with no arguments in that directory, and waits for it to exit

#### Scenario: Child inherits the pane stdio

- **WHEN** the selected TUI is spawned
- **THEN** the plugin does not reconfigure stdio and the child inherits the pane process stdio unchanged

#### Scenario: No positional path and no -d

- **WHEN** either supported TUI is launched for a repository directory
- **THEN** it discovers the repository from its working directory and receives no positional path or `-d` flag

#### Scenario: Spawn failure

- **WHEN** spawning the resolved selected binary fails
- **THEN** pane mode reports the existing launch error with the selected binary path and directory

## REMOVED Requirements

### Requirement: gitui resolution happens in pane mode before any user-visible output

Pane mode SHALL resolve gitui after checking `HERDR_BIN_PATH`, resolving the
repository directory, and creating/pruning the sessions directory, and before
taking the before-snapshot or printing anything to stdout, so a missing binary
fails without visible output. Action mode SHALL NOT resolve gitui at all.

#### Scenario: Missing binary fails before any banner

- **WHEN** pane mode starts and no executable `gitui` exists on `PATH`
- **THEN** the plugin returns the not-found error without running a git snapshot and without printing the `herdr-gitui: starting gitui ...` line

#### Scenario: Action mode never checks gitui availability

- **WHEN** the binary runs in action mode
- **THEN** it resolves the repository context and asks herdr to open the pane without ever calling gitui resolution; a missing gitui cannot fail the action process itself

## ADDED Requirements

### Requirement: Action mode selects the installed tool before opening a pane

Action mode SHALL apply the `tool-selection` capability before opening a pane.
Pane mode SHALL launch the selected tool and SHALL NOT independently select a
different tool. It SHALL validate the selected path before taking a
before-snapshot or printing the session banner.

#### Scenario: Action mode hands the selected tool to the pane

- **WHEN** action mode selects an available tool and opens its pane
- **THEN** pane mode launches that same tool and does not repeat interactive selection

#### Scenario: Selected binary disappears before pane launch

- **WHEN** the selected binary is unavailable when pane mode starts
- **THEN** pane mode reports a launch error before taking a git snapshot or printing a start banner

#### Scenario: No available tools prevents pane open

- **WHEN** action mode finds neither supported tool
- **THEN** it shows setup guidance and does not open a pane

#### Scenario: Snapshot boundaries are unchanged

- **WHEN** the selected TUI starts and exits
- **THEN** the plugin takes the before-snapshot before launch and the after-snapshot after exit without mutating the git index or working tree

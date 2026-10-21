# gitui-integration

## Purpose

This capability covers discovery and launch of the supported `gitui` and `lazygit` binaries for repository sessions. Both tools are expected to be installed by the user and found on `PATH`; the plugin never installs or version-checks them. Action mode discovers and selects an installed tool before opening a pane. Pane mode validates and launches that selected canonical path in the repository directory, then waits for it to exit before the session result is handed to outcome mapping (specified separately under `session-outcomes`).

## Requirements

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

# Tool selection

## Purpose

This capability selects an installed git TUI when a user invokes the plugin. It avoids an unnecessary choice when only one supported tool is available, offers a repeatable interactive choice when both are installed, and gives setup guidance when neither is available.

## Requirements

### Requirement: Action mode detects supported tools from PATH

Action mode SHALL detect executable `gitui` and `lazygit` binaries from `PATH`, canonicalize each selected candidate to an absolute path, and SHALL NOT install either tool.

#### Scenario: Both supported tools are installed

- **WHEN** executable `gitui` and `lazygit` candidates are available on `PATH`
- **THEN** action mode resolves both tools for selection

#### Scenario: One supported tool is installed

- **WHEN** exactly one of `gitui` or `lazygit` is executable on `PATH`
- **THEN** action mode resolves that tool for direct launch

#### Scenario: No supported tool is installed

- **WHEN** neither `gitui` nor `lazygit` is executable on `PATH`
- **THEN** action mode requests the `setup` popup entrypoint, which shows installation links for both tools, and does not open a tool session pane

### Requirement: Both installed tools are selected with an interactive picker

When both tools are available, action mode SHALL request the `picker` popup
on every invocation, passing canonical paths, saved preference, and caller
context. The picker SHALL list the tools, preselect an available saved
choice, support arrows or `k`/`j`, Enter, Esc, and `q`, and return a choice
or cancellation without opening a pane. It SHALL be borderless, 24 by 6,
with blank rows above and below the list, no help line, reset colors, and
reverse-video selection.

#### Scenario: Picker appears for every invocation with both tools

- **WHEN** both supported tools are available on an action invocation
- **THEN** action mode requests the `picker` popup even when a prior choice is saved, and the popup displays the picker

#### Scenario: Picker request uses its dedicated popup entrypoint

- **WHEN** both tools are available
- **THEN** action mode opens `alexeyco.git` entrypoint `picker` with popup placement and passes each available tool's canonical path, saved preference, repository directory, and resolved caller context

#### Scenario: Saved choice is preselected

- **WHEN** both tools are available and the saved preference names one of them
- **THEN** the picker initially highlights the saved tool

#### Scenario: User confirms a choice

- **WHEN** the user confirms a tool in the picker
- **THEN** the picker UI returns the selected tool; the picker flow saves its id as the last explicit choice and opens the `open` session pane with that tool's id and canonical path

#### Scenario: Popup placement closes picker before opening selected tool

- **WHEN** both tools are available, configured placement is `popup`, and the user confirms a tool
- **THEN** the 24-by-6 picker publishes the tool id, closes, and the action opens the selected tool popup with configured width and height

#### Scenario: Picker presentation follows the terminal theme

- **WHEN** the picker renders its tool rows
- **THEN** it has no help line or border, uses reset foreground and background colors, and fills the active row with reverse video

#### Scenario: User cancels the picker

- **WHEN** the user presses Esc or `q` in the picker
- **THEN** no tool is launched and no preference is saved

#### Scenario: Setup request uses its dedicated popup entrypoint

- **WHEN** neither tool is available
- **THEN** action mode opens `alexeyco.git` entrypoint `setup` with popup placement and the setup popup presents installation guidance for both tools

### Requirement: Popup placement closes picker before opening selected tool

When configured placement is `popup`, the picker flow SHALL publish its result
to the waiting action. On selection, the action SHALL close the compact picker
popup before opening the selected TUI popup with configured dimensions. On
cancellation, it SHALL open no TUI popup. The picker SHALL save confirmed
choices as the last explicit choice.

#### Scenario: Popup picker cancellation releases the waiting action

- **WHEN** configured placement is `popup` and the user cancels the picker
- **THEN** the picker publishes cancellation, the action returns without opening a tool popup, and no preference is saved

### Requirement: Single available tool launches without a picker

When exactly one supported tool is available, action mode SHALL launch that tool directly without showing a picker. Automatic launch SHALL NOT overwrite the saved explicit choice.

#### Scenario: Only gitui is installed

- **WHEN** gitui is available and lazygit is not
- **THEN** gitui launches directly without a picker

#### Scenario: Only lazygit is installed

- **WHEN** lazygit is available and gitui is not
- **THEN** lazygit launches directly without a picker

#### Scenario: Remembered tool disappeared

- **WHEN** both tools were previously available, the saved choice names one of them, and only the other tool is now available
- **THEN** the remaining tool launches directly without a picker and the saved choice is unchanged

### Requirement: Last explicit choice is stored forgivingly

The plugin SHALL store the last explicit tool choice in `$HERDR_PLUGIN_STATE_DIR/last-tool` as a bare tool id followed by a newline. It SHALL write the file through a temporary file and rename, ignore write failures, and treat missing, unreadable, empty, or unknown file contents as no saved choice.

#### Scenario: Preference has the required representation

- **WHEN** a user explicitly chooses lazygit
- **THEN** `last-tool` contains `lazygit` followed by a newline

#### Scenario: Missing or invalid preference

- **WHEN** the preference file is missing, unreadable, empty, or contains an unknown id
- **THEN** action mode continues without a saved choice and still allows the available tool flow

#### Scenario: Preference save fails

- **WHEN** saving an explicit choice fails
- **THEN** the selected tool still launches and the save failure does not abort the action

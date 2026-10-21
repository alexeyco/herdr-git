## MODIFIED Requirements

### Requirement: Both installed tools are selected with an interactive picker

When both tools are available, action mode SHALL request the `picker` popup
entrypoint on every invocation, passing both canonical tool paths, the saved
preference, and resolved repository and caller context. The picker SHALL list
available tools, support arrow keys and `k`/`j`, Enter, and Esc or `q`, and
preselect the saved choice when available. Its UI SHALL return the confirmed
tool or cancellation without opening a pane. The picker popup SHALL be
borderless, 24 columns wide and 6 rows high, show one blank row above and
below the tool rows with no help line, use reset foreground and background
colors, and highlight the active row with reverse video.

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

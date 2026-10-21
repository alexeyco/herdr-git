## ADDED Requirements

### Requirement: Plugin code separates integrations from workflows

The plugin SHALL keep external system interactions in adapters and plugin
workflows in features. Features SHALL depend on explicit ports for the
interactions that need substitution in tests, rather than directly embedding
process spawning, Herdr CLI calls, terminal control, and session-file I/O in
workflow logic.

#### Scenario: Session workflow uses adapter ports

- **WHEN** a maintainer inspects or tests the session workflow
- **THEN** git probing, TUI process execution, Herdr callbacks, and session
  storage are reached through named adapter ports, and tests can substitute
  those boundaries without invoking the real external programs

#### Scenario: Action workflow uses Herdr adapter

- **WHEN** a maintainer tests action and pane-opening workflows
- **THEN** the workflow supplies typed inputs to a Herdr port and verifies
  argv, environment, and reply handling through a mock, without spawning Herdr

### Requirement: Refactoring preserves observable plugin behavior

The refactored plugin SHALL preserve all existing externally observable
behavior defined by the baseline capabilities, including Herdr argv and
environment, tool selection, pane and popup lifecycle, git snapshot and
summary results, session-file semantics, exact user-visible messages, and
process exit codes.

#### Scenario: Existing behavior suites remain unchanged in outcome

- **WHEN** the existing unit and end-to-end suites run against the refactored
  implementation
- **THEN** they pass with the same asserted messages, exit codes, callback
  arguments, environment values, state effects, and pane behavior

#### Scenario: Error context preserves displayed text

- **WHEN** an existing error path propagates through the new error handling
- **THEN** stderr retains the exact existing message text and the process
  retains its existing exit code

### Requirement: Terminal picker owns terminal lifecycle safely

The picker SHALL use Ratatui with its Crossterm backend to initialize and
restore terminal state on normal completion and returned errors, while
preserving the existing choices, ordering, selection behavior, and key
bindings. Its popup SHALL be 24 columns wide and 6 rows high, borderless, and
contain the tool rows with one blank row above and below, without help text.
The active row SHALL use reverse video and the base foreground and background
SHALL use terminal reset colors.

#### Scenario: Picker selection and cancellation

- **WHEN** the user moves through available tools and confirms or cancels
- **THEN** the picker returns the same selected tool or cancellation result as
  before the refactor and restores terminal state

#### Scenario: Picker respects terminal colors

- **WHEN** the picker renders its tool rows
- **THEN** the active row fills the popup width with reverse video and the
  base foreground and background use terminal reset colors

#### Scenario: Picker rendering failure restores terminal

- **WHEN** terminal rendering or event handling returns an error after
  initialization
- **THEN** the picker attempts terminal restoration before returning the
  error, and the error reaches the existing message and exit-code handling

### Requirement: Refactor reduces duplicated logic and clarifies ownership

The completed refactor SHALL reduce duplicated production logic by extracting
shared behavior behind the new module boundaries and SHALL make adapter and
feature ownership clear. Production LOC SHALL be reported against the starting
revision as diagnostic information, but is not a completion threshold. Unit
tests for these boundaries SHALL use `mockall` and
`tempfile` where they replace hand-written fakes or shared temporary paths
with simpler isolated tests.

#### Scenario: Production source size is reported

- **WHEN** production Rust lines are compared between the starting revision
  and the completed implementation, excluding test-only code in either
  revision
- **THEN** both totals and the difference are reported, whether the final
  implementation is smaller or larger

#### Scenario: Boundary tests avoid real integrations

- **WHEN** adapter-dependent feature tests run
- **THEN** they use generated mocks or isolated temporary directories and do
  not require a real Herdr process, Git repository, TUI executable, or shared
  filesystem location

#### Scenario: Boundary test setup becomes simpler

- **WHEN** the refactored boundary tests are compared with the recorded
  pre-refactor test setup for the same cases
- **THEN** the change report identifies concrete setup removed or simplified,
  such as bespoke fakes replaced by `mockall` mocks or manual/shared temporary
  paths replaced by `tempfile`; it does not claim test LOC reduction unless
  that metric is separately measured

### Requirement: README banner uses the current plugin identity

The README banner SHALL identify the plugin as `herdr-git` and describe the
supported Git TUI workflow in its accessible label and visible text, while
preserving the existing artwork and dimensions.

#### Scenario: Banner branding matches the plugin

- **WHEN** a reader views the README banner or uses its accessible label
- **THEN** it identifies `herdr-git` and refers to Git TUI support without the
  obsolete `herdr-gitui` or gitui-only branding

### Requirement: Specification headings use readable labels

Tracked OpenSpec specification files SHALL use readable top-level headings
instead of raw kebab-case capability IDs. Specification file paths, capability
IDs, and requirement content SHALL remain unchanged.

#### Scenario: Specification headings are human-readable

- **WHEN** a maintainer opens a tracked file under `openspec/**/specs/`
- **THEN** any top-level heading based on a kebab-case capability ID uses a
  readable label, such as `# Agent delivery` for `agent-delivery`

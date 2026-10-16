# session-outcomes

## MODIFIED Requirements

### Requirement: Best-effort pane teardown

The plugin's pane teardown SHALL attempt `herdr plugin pane close
$HERDR_PANE_ID` via the herdr binary and, when that id is missing or the call
fails, fall back to `herdr tab close $HERDR_TAB_ID`; all failures are logged
to stderr and never propagated (src/session.rs:359-373,370-382). When the
session runs as a popup — the pane process sees `HERDR_GITUI_PLACEMENT=popup`
in its environment, exported by the action — the teardown SHALL make no herdr
call at all: neither `plugin pane close` nor the tab-close fallback. A popup
is a singleton session resource rather than a Herdr pane: its process does
not receive `HERDR_PANE_ID` for itself, and the plugin context its
environment carries points at the underlying tiled caller pane, so any
close-the-own-pane call would target the user's agent pane; the popup closes
naturally when the session process exits (herdr-side behavior). Summary
delivery is unaffected by the popup gate because it addresses
`HERDR_GITUI_CALLER_PANE`, not the session's own pane id. The pane hold its
outcomes invoke is specified by the mode-dispatch capability (see "Hold waits
for one Enter and never hangs on closed stdin"); the hold mechanism is not
re-specified here.

#### Scenario: Pane close succeeds

- **WHEN** a non-popup teardown runs with a non-blank `HERDR_PANE_ID` and the close call succeeds
- **THEN** no tab close is attempted and no message is logged

#### Scenario: Pane close falls back to tab close

- **WHEN** a non-popup `HERDR_PANE_ID` is missing or the pane close fails
- **THEN** `herdr tab close $HERDR_TAB_ID` is attempted when the tab id is present, and any failure of either call is logged (e.g. `herdr-gitui: failed to close pane {pane}: {e}`) without failing the session

#### Scenario: No ids at all is a silent no-op

- **WHEN** teardown runs with both `HERDR_PANE_ID` and `HERDR_TAB_ID` missing or blank
- **THEN** no herdr call is attempted, nothing is logged, and the session outcome is unchanged (src/session.rs:362-372)

#### Scenario: Close on process exit is herdr-side behavior

- **WHEN** the plugin pane process exits, whether or not teardown ran
- **THEN** this capability requires nothing further of the plugin: closing the pane on process exit is herdr-side behavior, not a plugin call

#### Scenario: Popup session makes no close call

- **WHEN** a session with `HERDR_GITUI_PLACEMENT=popup` ends with a summary that is delivered to the caller agent
- **THEN** delivery happens exactly as today (agent prompt to `HERDR_GITUI_CALLER_PANE`, success message, session file removed) and the plugin makes zero `herdr plugin pane close` and zero `herdr tab close` calls; the popup closes on its own when the process exits

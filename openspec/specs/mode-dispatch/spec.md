# Mode dispatch

## Purpose

The herdr-gitui binary is a single executable that herdr invokes in two
different roles: as the plugin's `open` action and as the entrypoint command
of the gitui plugin pane. This capability describes how the process decides
which role it is running in, using only the entry-point environment variables
herdr sets, and the shared error contract for both modes: every failure is
reported as a single `herdr-gitui:`-prefixed message on stderr with exit code 1, and
a plugin pane additionally blocks on stdin before closing so a short-lived
message stays readable — both on a failure and on the no-agent keep path,
where the session succeeds (exit 0) yet still holds so the user can read
where the summary was kept.

## Requirements

### Requirement: Pane marker wins mode dispatch

The binary SHALL select its run mode from the entry-point environment,
checking the pane marker first: when `HERDR_PLUGIN_ENTRYPOINT_ID` is set it
SHALL run pane mode, otherwise when `HERDR_PLUGIN_ACTION_ID` is set it SHALL
run action mode, otherwise it SHALL treat the invocation as outside herdr.
(src/main.rs:45-55,57-63)

#### Scenario: Entrypoint marker only

- **WHEN** `HERDR_PLUGIN_ENTRYPOINT_ID` is set and `HERDR_PLUGIN_ACTION_ID` is unset
- **THEN** the process runs pane mode (`session::run_pane`)

#### Scenario: Both markers set

- **WHEN** both `HERDR_PLUGIN_ENTRYPOINT_ID` and `HERDR_PLUGIN_ACTION_ID` are set
- **THEN** the process runs pane mode (the entrypoint marker wins, matching herdr's action-spawns-pane nesting)

#### Scenario: Action marker only

- **WHEN** `HERDR_PLUGIN_ENTRYPOINT_ID` is unset or blank and `HERDR_PLUGIN_ACTION_ID` is set
- **THEN** the process runs action mode (`session::run_action`)

### Requirement: A marker counts as set only when non-blank

The dispatch SHALL treat an environment variable as unset when it is absent
or when its value consists only of whitespace; the check trims the value and
rejects the empty result. (src/main.rs:65-69, tests at 136-151)

#### Scenario: Whitespace-only markers

- **WHEN** `HERDR_PLUGIN_ENTRYPOINT_ID` is `"   "` and `HERDR_PLUGIN_ACTION_ID` is `"\t"`
- **THEN** the process dispatches as Outside

### Requirement: Every failure prints a single prefixed message and exits 1

When the entry point returns an error, the binary SHALL print
`herdr-gitui: {error}` to stderr and exit with code 1. The prefix applies to
the first line only: the error text can itself contain interior newlines —
herdr CLI stderr snippets are trimmed and cut at 200 characters but not
newline-stripped — so the contract is a single prefixed message, not a single
physical line. (src/main.rs:23-24,30; src/herdr.rs:17-25,152-158)

#### Scenario: Action-mode failure

- **WHEN** action mode fails (for example an invalid plugin context)
- **THEN** stderr contains a single `herdr-gitui:`-prefixed message and the process exits 1 without any stdin prompt

### Requirement: Outside invocation fails with a fixed error and no hold

When neither marker is set, the binary SHALL fail with the error message
`invoked outside herdr plugin context`, print it under the common prefix,
exit 1, and SHALL NOT hold: `HERDR_PLUGIN_ENTRYPOINT_ID` is unset, so the
pane hold is skipped. (src/main.rs:61,23-31)

#### Scenario: Running the binary from a plain shell

- **WHEN** the binary is executed with neither `HERDR_PLUGIN_ENTRYPOINT_ID` nor `HERDR_PLUGIN_ACTION_ID` set
- **THEN** stderr shows `herdr-gitui: invoked outside herdr plugin context`, the exit code is 1, and the process does not block on stdin

### Requirement: Pane-mode failures hold the pane before exit

If an error occurs while `HERDR_PLUGIN_ENTRYPOINT_ID` is set — the variable
is re-checked at failure time, not only at dispatch — the binary SHALL call
`session::hold_pane()` before exiting with code 1, so an auto-closing plugin
pane stays open until the user has read the message. (src/main.rs:25-30)

#### Scenario: Error inside a plugin pane

- **WHEN** pane mode fails with `HERDR_PLUGIN_ENTRYPOINT_ID` still set
- **THEN** the error is printed, the hold prompt appears, and the process exits 1 only after the hold returns

### Requirement: Hold waits for one Enter and never hangs on closed stdin

`hold_pane` SHALL print `herdr-gitui: press Enter to close` to stderr and
read exactly one line from stdin; a closed stdin (EOF) SHALL return
immediately so the hold never blocks a non-interactive run.
(src/session.rs:25-27,375-382)

#### Scenario: User presses Enter

- **WHEN** the hold prompt is displayed and the user presses Enter
- **THEN** `hold_pane` returns and the process proceeds to exit

#### Scenario: Stdin already closed

- **WHEN** the hold prompt is displayed and stdin is at EOF
- **THEN** `hold_pane` returns without waiting

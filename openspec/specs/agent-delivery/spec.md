# agent-delivery

## Purpose

This capability hands the gitui session summary back to the agent pane that
requested the session. When the session outcome is Deliver, the plugin builds
a prompt from a fixed header plus the trimmed summary, caps it at 64 KiB on a
UTF-8 character boundary with a truncation note pointing at the kept summary
file, and submits it to the caller pane by running
`herdr agent prompt <pane> <prompt>` through the herdr binary provided in
`HERDR_BIN_PATH`. Success removes the kept file and closes the pane; failure
keeps the file, reports the cause, and holds the pane so the user can read
the error.

## Requirements

### Requirement: Prompt composition

The plugin SHALL build the delivered prompt as the fixed header
`gitui session summary:\n\n` followed by the session summary text with
leading and trailing whitespace trimmed
(src/session.rs:33-34,71-73).

#### Scenario: Header plus trimmed body

- **WHEN** the summary is `  - created commit abc: fix\n`
- **THEN** the prompt before capping is `gitui session summary:\n\n- created commit abc: fix`

### Requirement: Byte cap with truncation note

The plugin SHALL cap the delivered prompt at `MAX_PROMPT_BYTES = 65536` bytes,
applied to the whole prompt including the header; a prompt that already fits
SHALL be delivered unchanged
(src/session.rs:30-32,78-92,334).

#### Scenario: Short prompt unchanged

- **WHEN** the composed prompt is at most 65536 bytes
- **THEN** it is delivered byte-for-byte with no note appended

#### Scenario: Oversized prompt truncated with note

- **WHEN** the composed prompt exceeds 65536 bytes
- **THEN** the delivered text is the first at-most-65536 bytes cut back to the nearest UTF-8 character boundary at or below the limit, followed by `\n\n(output truncated; full summary kept at {path})`, where `{path}` is the session file written for this session

#### Scenario: The truncation note is appended after the cut

- **WHEN** a truncated prompt has the note appended
- **THEN** the final string may exceed 65536 bytes by the length of the note — the cut-then-append order is the current contract

#### Scenario: Multibyte characters are never split

- **WHEN** truncation would fall inside a multibyte UTF-8 character
- **THEN** the cut backs up to the preceding character boundary

### Requirement: Delivery via the herdr CLI

The plugin SHALL deliver by running
`<HERDR_BIN_PATH> agent prompt <caller_pane> <capped prompt>`, where
`<caller_pane>` is `HERDR_GITUI_CALLER_PANE`; `HERDR_GITUI_CALLER_AGENT`
gates eligibility but is not passed. The call SHALL capture output and
run without a timeout; a non-zero exit SHALL be an error. HERDR_BIN_PATH
resolution is owned by pane-launch ("Herdr callbacks execute only the
HERDR_BIN_PATH binary") (src/herdr.rs:8-26,147-149; src/session.rs:335).

#### Scenario: Successful call

- **WHEN** the herdr binary exits with status 0 after `agent prompt`
- **THEN** delivery is reported as successful and stdout is discarded

#### Scenario: Non-zero exit from herdr

- **WHEN** `herdr agent prompt ...` exits non-zero
- **THEN** the error names the command, the exit status, and the command's stderr: `{bin} agent prompt {pane} {prompt} failed ({status}): {stderr snippet}` with the stderr snippet trimmed to at most 200 characters (src/herdr.rs:152-158)

#### Scenario: Hung daemon blocks the plugin

- **WHEN** the herdr daemon does not answer the CLI call
- **THEN** the plugin waits indefinitely, since plugin API v1 calls carry no timeout

### Requirement: Deliver flow and success effects

On the Deliver outcome the plugin SHALL write the session file before invoking
the herdr CLI. After a successful delivery the plugin SHALL print
`herdr-gitui: sent session summary to agent pane {target}` to stderr, remove
the session file best-effort (a file already gone counts as success, any
other removal failure only logs
`herdr-gitui: warning: cannot remove {path}: {e}`), run pane teardown, and
exit with code 0 (src/session.rs:327-341,440-452).

#### Scenario: Successful delivery cleans up

- **WHEN** the agent prompt call succeeds
- **THEN** the confirmation line appears on stderr naming the caller pane, the session file is deleted (or its absence is accepted), pane teardown is attempted, and the process exits 0

#### Scenario: File exists before the CLI call

- **WHEN** delivery is attempted
- **THEN** the session file was already written, so the truncation note and any failure message can reference a real path

#### Scenario: Session file write failure aborts delivery as a hard error

- **WHEN** the Deliver outcome reaches `write_session_file` and the write fails
- **THEN** the error `failed to write session file {path}: {e}` is returned, printed with the `herdr-gitui: ` prefix, the pane is held per the mode-dispatch contract, and the process exits 1 — no `herdr agent prompt` call is made

### Requirement: Delivery failure keeps the summary

When the agent prompt call fails, the plugin SHALL produce the error
`failed to send session summary to agent pane {target}: {cause}; summary kept
at {path}`, keep the session file, skip pane teardown, and let main print the
message with the `herdr-gitui: ` prefix, hold the pane per the mode-dispatch
contract, and exit with code 1 (src/session.rs:342-346; src/main.rs:22-31).

#### Scenario: Failed delivery leaves everything readable

- **WHEN** `herdr agent prompt` returns an error
- **THEN** stderr shows the failure message with the cause and the kept file path, the session file remains on disk, the pane is held per the mode-dispatch contract, and the process exits 1

### Requirement: Defensive guard against a missing caller pane at send time

Deliver eligibility is owned by the session-outcomes capability ("Outcome
decision precedence") and is not re-specified here. The delivery path itself
carries only a defensive precondition guard: if the Deliver arm runs without a
caller pane id at send time, it SHALL return
`internal: deliver outcome without a caller pane` and be reported like any
other error (prefixed message, pane held, exit 1). This state is unreachable
through the outcome decision (src/session.rs:329-333).

#### Scenario: Guard fires without a caller pane id

- **WHEN** the Deliver arm is entered with no caller pane id at send time
- **THEN** the error `internal: deliver outcome without a caller pane` is returned, reported with the `herdr-gitui: ` prefix, the pane is held per the mode-dispatch contract, the process exits 1, and no `herdr agent prompt` call is made

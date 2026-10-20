## ADDED Requirements

### Requirement: Manifest identity defines the qualified action name

The plugin manifest SHALL declare plugin id `alexeyco.gitui` and action id `open`, producing the qualified action identifier `alexeyco.gitui.open`. The action id and pane entrypoint id SHALL remain `open`; the plugin SHALL NOT expose `gitui.open` as an alias.

#### Scenario: Qualified action is invoked

- **WHEN** a Herdr setting binds `command = "alexeyco.gitui.open"` or a user runs `herdr plugin action invoke alexeyco.gitui.open`
- **THEN** Herdr invokes the plugin's `open` action

#### Scenario: Old action name is not an alias

- **WHEN** a setting or invocation uses `gitui.open`
- **THEN** this plugin does not resolve that name as an alias for `alexeyco.gitui.open`

## MODIFIED Requirements

### Requirement: Herdr callbacks execute only the HERDR_BIN_PATH binary

Action mode's Herdr callbacks — the settings-layer `plugin config-dir alexeyco.gitui` query, pane open, and the best-effort tab focus — SHALL exec the binary at `HERDR_BIN_PATH` and SHALL fail with a contract error when it is missing or blank. The pane-open callback SHALL identify the plugin as `alexeyco.gitui`; its action entrypoint remains `open`. This requirement is the origin of that contract: every other capability's Herdr call cross-references it rather than restating it, and one shared check guards every Herdr call the plugin makes in either mode (src/herdr.rs:8-13,28-36).

#### Scenario: Bin path absent at pane open

- **WHEN** action mode reaches the pane-open call with `HERDR_BIN_PATH` unset or whitespace-only
- **THEN** the action fails with "herdr did not provide HERDR_BIN_PATH (plugin API v1 contract); cannot call back into herdr"

#### Scenario: Tab focus passes through the same guard

- **WHEN** the best-effort tab-focus call runs with `HERDR_BIN_PATH` missing or blank
- **THEN** the shared check rejects that call too, producing the same contract error as its internal failure (which the best-effort caller ignores)

#### Scenario: Config-dir query passes through the same guard

- **WHEN** the settings layer attempts the `plugin config-dir alexeyco.gitui` fallback query with `HERDR_BIN_PATH` missing or blank
- **THEN** the shared check rejects that call as well, and the settings layer treats the rejection as "config directory unavailable" (a warning, not an action failure)

### Requirement: Pane open passes a fixed argv and requires a pane id in the reply

Action mode SHALL build the pane-open argv in the fixed order shown below;
the `--placement` value comes from the loaded settings (`tab`, `split`,
`overlay`, `zoomed`, or `popup`; default `tab`) instead of a hardcoded
literal, and a `--direction right|down` pair appears immediately after the
placement value only when the effective placement is `split` and the
configured direction is not `auto` — for non-split placements a configured
direction is ignored, and `auto` means the flag is not passed (herdr docs do
not guarantee `--direction` for non-split placements). When the effective
placement is `split` or `zoomed` and the caller pane is resolved,
`--target-pane <caller-pane-id>` SHALL appear immediately after the
`--direction` position (after the pair when present, otherwise directly
after the placement value); the caller pane is `focused_pane_id` from the
plugin context with the existing `HERDR_PANE_ID` env fallback — the same
value already exported as `HERDR_GITUI_CALLER_PANE`. For `tab`, `overlay`,
and `popup` placements no `--target-pane` is passed. When the effective
placement is `split` or `zoomed` but the caller pane is not resolved, the
action SHALL print a stderr warning and fall back to placement `tab` for the
open — herdr rejects split/zoomed without a target pane, and the user's
explicit choice must never surface as a cryptic `invalid_params` failure
(degradation consistent with the never-fatal settings philosophy).
`--workspace <id>` is passed only when the effective placement is `tab`; for
`split`, `zoomed`, `overlay`, and `popup` the open targets a pane rather
than a workspace and the pair is omitted entirely — the daemon rejects the
combinations: `--workspace` together with `--target-pane` on split/zoomed
fails with `invalid_params` "split and zoomed plugin panes target an
existing pane; use target_pane_id" (even when the target pane is present),
and `--workspace` on overlay or popup fails with `invalid_params` "overlay
and popup plugin panes target the active pane" (overlay and popup target the
daemon-derived active pane without any `--target-pane` from the plugin). For
the `popup` placement, optional `--width <v>` and `--height <v>` pairs
appear immediately after the `--placement popup` value, in that order, only
when the corresponding settings key is valid and set — each pair
independently, so a width-only config emits only `--width` — and an omitted
or invalid key emits no flag (herdr's default half-size popup applies); the
flag value is the configured positive integer or `N%` string passed verbatim
as an argv token. For the `popup` placement the action SHALL additionally
export `--env HERDR_GITUI_PLACEMENT=popup` immediately after the
`HERDR_GITUI_DIR` pair, and for no other placement (non-popup argv stays
byte-identical) so pane mode can gate its teardown on it. Caller `--env`
pairs appear whenever resolved, while `HERDR_GITUI_DIR` and `--focus` are
always present (src/herdr.rs:60-100). For every non-popup placement the
reply SHALL carry a non-blank `result.plugin_pane.pane.pane_id` — a missing
pane id is the failure described in the scenario below; a `popup` open is
exempt: the daemon's popup reply is `{"id":"cli:plugin","result":{"type":"ok"}}`
with no pane id at all (live-verified on herdr 0.9.5, because a popup is a
singleton session resource rather than a Herdr pane), and such a reply SHALL
be treated as success, while a popup reply that is not valid JSON or carries
a non-`ok` result (e.g. `ui_busy` while another Herdr modal is active) fails
through the unchanged pane-open error path. `tab_id` is optional in all
cases and raw reply snippets in errors are truncated at 200 characters
(src/herdr.rs:102-121,151-158).

#### Scenario: Full context

- **WHEN** workspace id `W`, caller pane `P`, and caller agent `A` are all resolved for directory `D` and no placement/direction config is active (defaults)
- **THEN** argv is exactly `plugin pane open --plugin alexeyco.gitui --entrypoint open --placement tab --workspace W --cwd D --env HERDR_GITUI_DIR=D --env HERDR_GITUI_CALLER_PANE=P --env HERDR_GITUI_CALLER_AGENT=A --focus`

#### Scenario: Minimal context

- **WHEN** no workspace id, caller pane, or caller agent resolved for directory `D` and no placement/direction config is active (defaults)
- **THEN** argv is exactly `plugin pane open --plugin alexeyco.gitui --entrypoint open --placement tab --cwd D --env HERDR_GITUI_DIR=D --focus`

#### Scenario: Split placement with explicit direction

- **WHEN** the config sets `placement = "split"` and `direction = "down"` for directory `D` and caller pane `P` is resolved
- **THEN** argv contains `--placement split --direction down --target-pane P` in the placement position, no other placement or direction flags, and no `--workspace` token

#### Scenario: Split placement with auto direction

- **WHEN** the config sets `placement = "split"` and `direction = "auto"` for directory `D` and caller pane `P` is resolved
- **THEN** argv contains `--placement split --target-pane P`, no `--direction` flag at all, and no `--workspace` token

#### Scenario: Direction ignored for non-split placements

- **WHEN** the config sets `placement = "zoomed"` (or `overlay`, or `tab`) and `direction = "down"` for directory `D` and caller pane `P` is resolved
- **THEN** argv contains `--placement zoomed` (or the respective value) and no `--direction` flag; `zoomed` additionally carries `--target-pane P` while `overlay` and `tab` carry no `--target-pane`; `zoomed` and `overlay` also carry no `--workspace` token (only `tab` keeps `--workspace` when a workspace id resolves)

#### Scenario: Split targets the caller pane

- **WHEN** the effective placement is `split` with direction `down` and caller pane `P` is resolved
- **THEN** argv reads `--placement split --direction down --target-pane P` followed by the remaining fixed-order arguments and contains no `--workspace` token even when a workspace id is resolved

#### Scenario: Zoomed targets the caller pane

- **WHEN** the effective placement is `zoomed` and caller pane `P` is resolved
- **THEN** argv reads `--placement zoomed --target-pane P` with no `--direction` flag and no `--workspace` token

#### Scenario: Overlay omits workspace targeting

- **WHEN** the effective placement is `overlay` and a workspace id `W` is resolved for directory `D`
- **THEN** argv contains `--placement overlay` and no `--workspace` token and no `--target-pane` token — the daemon targets the active pane itself

#### Scenario: Unresolvable caller pane falls back to tab with warning

- **WHEN** the configured placement is `split` or `zoomed` but no caller pane resolves from `focused_pane_id` or the `HERDR_PANE_ID` env fallback
- **THEN** the action prints a `herdr-gitui:`-prefixed stderr warning and opens with `--placement tab` (no `--direction`, no `--target-pane`) instead of letting herdr fail with `invalid_params`; the fallback uses the tab argv shape, so `--workspace` appears when a workspace id is resolved

#### Scenario: Popup passes size flags

- **WHEN** the effective placement is `popup` with resolved `width = 80` and `height = "80%"` for directory `D` and caller pane `P` resolved, and a workspace id `W` is resolved
- **THEN** argv is exactly `plugin pane open --plugin alexeyco.gitui --entrypoint open --placement popup --width 80 --height 80% --cwd D --env HERDR_GITUI_DIR=D --env HERDR_GITUI_PLACEMENT=popup --env HERDR_GITUI_CALLER_PANE=P --focus`, with no `--workspace`, no `--target-pane`, and no `--direction` token; a width-only config (no valid `height`) is identical minus the `--height 80%` pair

#### Scenario: Reply without pane id

- **WHEN** a non-popup pane-open reply lacks a non-blank `result.plugin_pane.pane.pane_id` or is not valid JSON
- **THEN** the action fails with an error naming the missing path (or the parse error) and a raw snippet of at most 200 characters; a popup reply is exempt from this rule (see "Popup opens without pane id reply" under the success-reporting requirement)

#### Scenario: Reply without tab id

- **WHEN** the reply carries a valid `pane_id` but no non-blank `tab_id`
- **THEN** the open succeeds with no tab id reported

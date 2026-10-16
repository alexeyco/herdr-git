# Design

## Approach

A new `src/settings.rs` owns everything about settings: directory discovery,
file reading, parsing, per-key validation, and warnings. It exposes one
entry point — `Settings::load()` returning `{ placement, direction, width,
height }` — that
`run_action()` calls once between context parse (src/session.rs:102) and
pane open (src/session.rs:112). `herdr::pane_open_args()` gains two
parameters and stops hardcoding `tab`.

## Files to touch

- `Cargo.toml` — add `toml = "1"`, `serde = { version = "1", features =
["derive"] }`.
- `src/main.rs` — `mod settings;`.
- `src/settings.rs` (new) — `Placement`/`Direction` enums with `Default`
  (`Tab`, `Right`); config-dir resolution; `config.toml` reading; validation
  with per-key fallback; stderr warnings prefixed `herdr-gitui:`.
- `src/herdr.rs` — new `plugin_config_dir()` using the guarded `exec()`
  (:8-26) calling `plugin config-dir gitui`; `pane_open_args()` (:60-100)
  takes placement + optional direction + optional caller pane id, emits
  `--workspace` only for placement `tab` (see the compatibility matrix),
  inserts `--direction <d>` right after the placement value and
  `--target-pane <id>` right after the direction position for split/zoomed,
  and degrades split/zoomed without a caller pane id to `tab` with a stderr
  warning; for popup emits `--width`/`--height` right after the placement
  value and exports `--env HERDR_GITUI_PLACEMENT=popup`; `parse_pane_open`
  becomes placement-aware (ok-without-pane-id for popup); update doc comment
  :57-59 and the manifest-drift comment :74-76.
- `src/session.rs` — `run_action()` loads settings and passes them together
  with the resolved caller pane to `plugin_pane_open()`; on the default path
  the argv is byte-identical to today's, so defaults keep every existing
  assertion green; popup success prints the popup notice and skips the tab
  focus re-assert; in pane mode `close_pane()` (called from `finish()` at
  :324/:348, defined :370-382) returns immediately when
  `HERDR_GITUI_PLACEMENT=popup` — no pane close, no tab close fallback.
- `tests/e2e.rs` — stub herdr answers `plugin config-dir`; tests set
  `HERDR_PLUGIN_CONFIG_DIR` (run_plugin already `.env_clear()`s,
  tests/e2e.rs:86-101).
- `README.md`, `docs/how-it-works.md`, `AGENTS.md` — document the config file.

## Parsing strategy: two-phase, not strict derive

Serde-derive into an enum-backed struct would reject the whole file on one
bad value ("invalid value: zigzag"), which contradicts the agreed per-key
fallback. Instead:

1. Parse the file into a flat `toml::Table` (or `BTreeMap<String,
toml::Value>`); a parse error is "corrupt TOML" → all defaults + warning.
2. Walk the table: keys outside `{placement, direction, width, height}` →
   unknown-key
   warning; known keys validated against their value sets — a bad or
   non-string value falls back to that key's default with a warning (for
   `width`/`height` the default is "unset", i.e. no flag), every
   other key keeps its effect.

The `config-dir` reply needs no JSON or serde parsing at all: the confirmed
bare-path shape (see below) is handled by trimming stdout and checking for a
non-blank line; `serde_json` stays only where it is used today. The `toml`
crate's own `from_str::<Table>` needs serde only for value types.

## Config-directory dual resolution

`env_non_blank("HERDR_PLUGIN_CONFIG_DIR")` (helper: src/session.rs:426-431)
wins outright. Otherwise call `herdr plugin config-dir gitui` through the
same HERDR_BIN_PATH-guarded `exec()`; any failure of that call — missing
`HERDR_BIN_PATH`, non-zero exit, empty or blank stdout — is a settings-layer
warning ("config directory unavailable"), never fatal, and the action
proceeds with defaults (it will fail later at pane open with the unchanged
contract error if the binary path is really missing). If both yield nothing
→ defaults + warning. `<dir>/config.toml` missing is silent defaults per the
herdr-reviewr precedent; an existing-but-unreadable file warns like corrupt
content.

Trade-off accepted: the CLI fallback costs one extra herdr exec per action
launch when the env var is unset. Action mode is a one-shot process, so the
cost is one subprocess (~ms) and it keeps the plugin working if herdr ever
stops injecting the variable.

## Direction and target-pane gating

`--direction` is emitted only when `placement == split && direction !=
auto`. herdr 0.9.5 documents `--direction right|down` on `plugin pane open`
but does not guarantee acceptance for non-split placements, so a configured
`direction` is silently ignored (no warning) for
`tab`/`overlay`/`zoomed`/`popup`;
`auto` means "do not pass the flag at all". The socket schema types
direction as a nullable `SplitDirection`, consistent with split-only use.

Placement `split`/`zoomed` additionally emits `--target-pane
<caller-pane>` immediately after the `--direction` position (directly after
the placement value when no direction is passed). Live smoke tests on herdr
0.9.5 proved the daemon's targeting contract both ways: `plugin pane open
--placement split --direction right --workspace w3E ...` fails with
`{"error":{"code":"invalid_params","message":"split and zoomed plugin panes
target an existing pane; use target_pane_id"}}` — including when
`--target-pane` is also passed, the offender then being `--workspace` (see
the compatibility matrix below); upstream docs corroborate
("Split and zoomed panes target an existing pane; tab panes can target a
workspace"). The caller pane is the value `run_action()` already resolves —
`focused_pane_id` from the plugin context with the `HERDR_PANE_ID` env
fallback (src/session.rs:114-116), the same one exported as
`HERDR_GITUI_CALLER_PANE`. `tab`/`overlay`/`popup` never carry
`--target-pane`.

When split/zoomed is configured but the caller pane does not resolve, the
open falls back to placement `tab` with a stderr warning rather than firing
a guaranteed-herdr-failure. The settings layer is deliberately never-fatal
(lenient validation, warn-and-degrade), and a raw daemon `invalid_params`
error would be unmappable by the user to their own config choice; `tab`
still opens the pane and the warning names the degraded knob.

## Placement/flag compatibility matrix

Live verification against the herdr 0.9.5 daemon (5 probe runs, all opened
panes closed again) proved the contract stricter than the docs suggest — the
targeting flags are mutually exclusive per placement:

| placement | `--workspace`                         | `--target-pane`                               | `--direction`                      | `--width`/`--height`                                                                                             |
| --------- | ------------------------------------- | --------------------------------------------- | ---------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `tab`     | allowed (today's behavior, unchanged) | never                                         | never                              | never                                                                                                            |
| `split`   | **forbidden**                         | required                                      | `right`/`down` per the gating rule | never                                                                                                            |
| `zoomed`  | **forbidden**                         | required                                      | never                              | never                                                                                                            |
| `overlay` | **forbidden**                         | never (daemon derives the active pane itself) | never                              | never                                                                                                            |
| `popup`   | **forbidden**                         | never (daemon derives the active pane itself) | never                              | optional, immediately after the placement value; reply `ok` carries **no pane id** (singleton, not a Herdr pane) |

Probe evidence:

- Probe 1: `--workspace` + `--target-pane` + `--direction` on split →
  `invalid_params` "split and zoomed plugin panes target an existing pane;
  use target_pane_id" — the daemon rejects the workspace even when the
  target pane **is** present.
- Probe 5: split with `--target-pane` but no `--workspace`, otherwise the
  same flags → OK (isolation of the offender).
- Probe 3: overlay + `--workspace` → `invalid_params` "overlay and popup
  plugin panes target the active pane".
- Probe 4: overlay with only `--cwd`/`--env` → OK.
- Popup probe (live, herdr 0.9.5): `plugin pane open --plugin gitui
--entrypoint open --placement popup --width "80%" --height "80%" --env
...` → reply `{"id":"cli:plugin","result":{"type":"ok"}}` — no
  `result.plugin_pane.pane.pane_id` anywhere in the popup reply.

**argv construction rule:** the `--workspace <id>` pair is emitted only when
the effective placement is `tab`; `--target-pane <caller-pane>` only for
`split`/`zoomed`; `--direction` only for `split` with a non-`auto`
configured direction; `--width`/`--height` only for `popup` with the
corresponding valid setting, each pair independently. `pane_open_args()`
gates the workspace argument on the effective placement up front, so the
fallback-to-tab path emits `--workspace` again exactly as the tab default
does.

## Popup lifecycle and teardown gate

A popup is a centered session-modal terminal that does not change the tiled
layout, receives all terminal input (including Escape), and closes when its
command exits. Action mode: after a successful popup open the reply has no
pane id and no tab id, so the action prints `herdr-gitui: opened gitui
popup` on stderr and skips the best-effort `tab focus` re-assert. Because a
popup is a **singleton session resource**, opening one while another Herdr
modal is active returns `ui_busy` — the plugin treats it as the standard
pane-open failure, no special-casing. Pane mode: the action exports `--env
HERDR_GITUI_PLACEMENT=popup` (popup-only, so every non-popup argv stays
byte-identical); `close_pane()` in pane mode checks that variable first and
returns without any herdr call when it reads `popup`. Without the gate the
teardown could resolve the session's "own pane" from the popup-carried
plugin context, which points at the underlying tiled **caller pane** —
closing it would close the user's agent pane. The popup closes naturally
when gitui exits (herdr-side behavior). Summary delivery is unaffected: it
addresses `HERDR_GITUI_CALLER_PANE`/`HERDR_GITUI_CALLER_AGENT`, never the
session's own pane id. Dimensions smaller than the daemon's popup minimum
are clamped by herdr, not validated by the plugin.

## Focus: unchanged

`--focus` stays the unconditional last argv element (src/herdr.rs:98) for
every placement including `popup`. The best-effort `tab focus` re-assert
stays (src/session.rs:124) for all non-popup placements; it is skipped for
`popup` because the popup reply carries no tab id (see the modified
"Success reports the pane and re-asserts tab focus best-effort" requirement).
The user explicitly dropped a `focus` setting to avoid conflicting with that
requirement.

## Manifest drift

`herdr-plugin.toml` keeps `[[panes]].placement = "tab"`: the manifest value is
declarative documentation, the request `--placement` overrides it at open
time, and the existing code comment (src/herdr.rs:74-76) already acknowledges
the drift. No manifest change is in scope.

## Config-dir reply shape: confirmed

`herdr plugin config-dir gitui` was executed on herdr 0.9.5: it prints a bare
path line (e.g. `/Users/alexeyco/.config/herdr/plugins/config/gitui`) and
exits rc 0 — no JSON envelope. Parse contract: trim stdout; a single non-empty
line is the directory; empty or blank stdout is a resolution failure → all
defaults + the "config directory unavailable" warning. The e2e stub mirrors the
bare path line.

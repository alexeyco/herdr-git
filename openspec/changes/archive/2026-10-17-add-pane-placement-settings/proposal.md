# add-pane-placement-settings

## Why

The pane-open argv hardcodes `--placement tab` (src/herdr.rs:60-99), so every
user gets the gitui pane in a new tab with no way to choose a split, overlay,
or zoomed layout. herdr 0.9.5 accepts `--placement overlay|popup|split|tab|zoomed`
and `--direction right|down` on `plugin pane open` (popup additionally
accepts optional `--width`/`--height`), and the sibling
herdr-reviewr plugin already ships a flat-key `config.toml` for exactly this.
Adding a settings file makes the layout configurable without touching the
plugin manifest.

## What Changes

- A new `config.toml` in the gitui plugin's config directory with flat
  top-level keys: `placement` (`tab` default | `split` | `overlay` | `zoomed`
  | `popup`), `direction` (`right` default | `down` | `auto`), and the
  optional popup size keys `width` and `height` (each a positive integer of
  outer terminal cells or a string percentage like `"80%"`).
- Config directory resolution: prefer `HERDR_PLUGIN_CONFIG_DIR`; when absent
  or blank, query `herdr plugin config-dir gitui` through the existing
  HERDR_BIN_PATH-guarded exec helper; if neither yields a directory, use
  defaults with a stderr warning.
- Action mode reads the config once at action start (no runtime reload) and
  the pane-open argv reflects it: `--placement <value>` replaces the hardcoded
  `tab`, and `--direction <value>` is passed only when placement is `split`
  and direction is not `auto` (herdr docs do not guarantee `--direction` is
  accepted for non-split placements). `auto` means "do not pass
  `--direction`".
- `--target-pane <caller-pane>` is mandatory whenever the effective placement
  is `split` or `zoomed` — herdr rejects these placements without a target
  pane (discovered by manual smoke test on herdr 0.9.5: `invalid_params —
"split and zoomed plugin panes target an existing pane; use
target_pane_id"`); when the caller pane does not resolve, the open degrades
  to `tab` with a stderr warning.
- `--workspace <id>` is passed only when the effective placement is `tab` —
  the same manual smoke test (5 live probe runs against the herdr 0.9.5
  daemon) showed the daemon rejects `--workspace` combined with `split`/
  `zoomed` even when `--target-pane` is present (same "use target_pane_id"
  error) and with `overlay` (`invalid_params — "overlay and popup plugin
panes target the active pane"`), so for split/zoomed/overlay/popup the pair is
  omitted entirely and the open targets the caller or active pane; `tab`
  behavior stays byte-identical to today.
- New placement `popup`: `placement = "popup"` opens a centered session-modal
  terminal popup without changing the tiled layout (herdr docs; live-probed OK
  on herdr 0.9.5). Optional `width`/`height` keys add `--width <v>`
  `--height <v>` pairs immediately after `--placement popup`, each passed
  independently only when valid and set (omitted keys mean herdr's default
  half-size popup); both keys are ignored silently for every other placement.
- The popup open reply carries no pane id (`{"id":"cli:plugin","result":
{"type":"ok"}}`, verified) because a popup is a singleton session resource,
  not a Herdr pane: the strict pane-id rule now applies to non-popup
  placements only, and a successful popup open prints `herdr-gitui: opened
gitui popup` on stderr with no tab-focus re-assert.
- Popup safety rule (critical): the action exports `--env
HERDR_GITUI_PLACEMENT=popup` to the pane process (popup only; every
  non-popup argv stays byte-identical), and in pane mode the plugin MUST NOT
  call `herdr plugin pane close` (nor the tab-close fallback) when that
  variable reads `popup` — the popup's carried plugin context points at the
  underlying tiled caller pane, so closing "its own pane" would close the
  user's agent pane; the popup closes naturally when gitui exits. Summary
  delivery is unaffected (it uses `HERDR_GITUI_CALLER_*`).
- Lenient validation: missing file or omitted key → defaults silently;
  corrupt TOML → all defaults + warning; unknown key → warning, not error;
  invalid value for a known key → that key's default + warning, other keys
  still applied. Settings failures are never fatal to the action.
- New dependencies: `toml` and `serde` (with `derive`); the crate currently
  has only `serde_json`.
- Focus is NOT configurable: `--focus` stays unconditional in the argv for
  every placement (spec pane-launch requirement "Success reports the pane
  and re-asserts tab focus best-effort"); the best-effort `tab focus`
  re-assert stays for all placements except `popup`, which has no tab id to
  focus.

## Out of scope

- A configurable focus setting — `--focus` stays unconditional.
- Per-workspace config overrides — one flat `config.toml` per plugin config
  directory only.
- A settings UI or editor integration.
- Runtime config reload — settings are read once per action launch (see the
  pane-settings "Late edit does not affect the running session" scenario).
- Any change to `herdr-plugin.toml` — the manifest stays `placement = "tab"`.

## Capabilities

- **pane-settings** (new): config-directory discovery, config-file reading,
  key validation, defaults, and warning behavior for the plugin settings —
  including the `popup` placement value and the optional `width`/`height`
  size keys with per-key fallback.
- **pane-launch** (modified): the pane-open argv requirement becomes
  config-driven instead of a fixed `--placement tab` literal, the
  HERDR_BIN_PATH callback guard now also covers the `plugin config-dir`
  query, and the pane-id reply rule becomes placement-aware (a popup reply
  legitimately carries no pane id). The "Herdr callbacks" and "Pane open
  passes a fixed argv" requirements change, as does "Success reports the
  pane and re-asserts tab focus best-effort" (popup notice, no tab focus).
  The baseline pane-launch Purpose (which names `--placement tab`) will need
  a rewording at archive/sync time.
- **session-outcomes** (modified): "Best-effort pane teardown" gains the
  popup gate — a popup session makes zero close calls, protecting the user's
  agent pane.

## Impact

- Code: `src/settings.rs` (new), `src/herdr.rs` (`pane_open_args`,
  `plugin_pane_open`, new `plugin_config_dir`; `parse_pane_open` becomes
  placement-aware so a popup `ok` reply without a pane id parses as
  success), `src/session.rs` (`run_action`, config load between :102 and
  :112, popup notice on success, and the pane-mode `close_pane()` gate on
  `HERDR_GITUI_PLACEMENT=popup`), `Cargo.toml`,
  `src/main.rs` (module registration), `tests/e2e.rs` (stub herdr must
  handle `plugin config-dir`; `run_plugin` must set
  `HERDR_PLUGIN_CONFIG_DIR`), README.md, docs/how-it-works.md, AGENTS.md
  gotcha list.
- Manifest: unchanged — `[[panes]].placement = "tab"` stays declarative only.

ASSUMPTION: manifest `[[panes]].placement = "tab"` stays as-is and may drift from the actual argv; the existing code comment (src/herdr.rs:74-76) already acknowledges the drift and herdr docs say the request `--placement` overrides the manifest.

ASSUMPTION: a popup open rejected with `ui_busy` (another Herdr modal is active — documented for the singleton popup) surfaces through the standard pane-open failure path with the raw-error snippet, with no plugin-side retry or special messaging.

ASSUMPTION: a bare numeric string like `width = "80"` is treated as invalid (the accepted forms are a positive integer value or an `N%` string), warns, and leaves the key unset; herdr's own minimum-size clamping is daemon behavior the plugin does not pre-validate.

CONFIRMED: `herdr plugin config-dir gitui` prints a bare path line (e.g. `/Users/alexeyco/.config/herdr/plugins/config/gitui`) with rc 0 — not a JSON envelope; verified by executing it against herdr 0.9.5. The parse is a stdout trim, not JSON deserialization.

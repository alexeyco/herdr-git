# Pane settings

## Purpose

The plugin's action mode discovers and reads a flat-key `config.toml` in its config directory before opening a pane, turning layout choices (`placement`, `direction`, and popup size keys `width`/`height`) into user-editable settings. Directory discovery prefers `HERDR_PLUGIN_CONFIG_DIR` and falls back to a guarded `herdr plugin config-dir alexeyco.git` callback; every settings failure degrades to documented defaults with a stderr warning rather than aborting the action.

## Requirements

### Requirement: Config directory resolves from env first, then the herdr config-dir callback

Action mode SHALL resolve the settings directory as the value of
`HERDR_PLUGIN_CONFIG_DIR` when non-blank; when it is absent or blank it SHALL
query `herdr plugin config-dir alexeyco.git` through the HERDR_BIN_PATH-guarded
exec helper; when neither path yields a usable directory it SHALL use defaults
for all settings and emit a warning on stderr. The settings layer SHALL never
turn a config-directory failure into an action failure.

#### Scenario: Environment variable preferred

- **WHEN** `HERDR_PLUGIN_CONFIG_DIR` is set to a non-blank directory, even if `HERDR_BIN_PATH` is also set
- **THEN** `config.toml` is read from that directory and no `plugin config-dir` callback is made

#### Scenario: CLI fallback when env absent

- **WHEN** `HERDR_PLUGIN_CONFIG_DIR` is unset or blank and the `plugin config-dir alexeyco.git` query prints a non-blank directory path line
- **THEN** `config.toml` is read from the directory the callback reported

#### Scenario: Neither source yields a directory

- **WHEN** `HERDR_PLUGIN_CONFIG_DIR` is absent or blank and the `plugin config-dir` query fails for any reason (missing `HERDR_BIN_PATH`, non-zero exit, empty or blank stdout, or other output that yields no directory path)
- **THEN** the action continues with default settings and prints a warning on stderr; the only subsequent failure, if any, is the unchanged pane-open contract error

### Requirement: Settings read once per action launch from config.toml

Action mode SHALL read `<config-dir>/config.toml` exactly once, at action
start before the pane-open call, and SHALL apply the parsed values only to
that launch; a config file that does not exist SHALL produce pure defaults
with no warning. Edits made after a launch SHALL take effect on the next
launch, not during a running session.
(design.md: load between src/session.rs:102 and :112)

#### Scenario: Missing file is silent defaults

- **WHEN** the resolved config directory contains no `config.toml`
- **THEN** all settings take their defaults and no warning is printed

#### Scenario: Late edit does not affect the running session

- **WHEN** `config.toml` is modified after the action has opened the pane
- **THEN** the already-running pane mode is unaffected and the next action launch reads the new values

### Requirement: Invalid config content degrades to defaults with warnings

Action mode SHALL parse `config.toml` with flat top-level keys and SHALL
never fail the action over its content: unparseable TOML SHALL fall back to
all defaults with one warning, and a file that exists but cannot be read
(e.g. a permission error) SHALL be treated as unparseable content the same
way — all defaults with one warning; a key outside the known set SHALL
produce a warning while every known key keeps its effect; and an invalid or
non-string value for a known key SHALL fall back to that key's own default
with a warning while the other keys keep their effect.
(design.md: two-phase parsing)

#### Scenario: Corrupt TOML

- **WHEN** `config.toml` is not valid TOML
- **THEN** the action warns on stderr and opens the pane with all default settings (`--placement tab` and no `--direction` flag)

#### Scenario: Existing but unreadable file

- **WHEN** `config.toml` exists but cannot be read (e.g. its permissions deny the plugin process)
- **THEN** the action treats it as unparseable content: one stderr warning and the pane opens with all default settings

#### Scenario: Unknown key warns but does not error

- **WHEN** `config.toml` contains a key the plugin does not know (e.g. `focus = false`) alongside valid known keys
- **THEN** the action warns on stderr about the unknown key and applies the valid known keys normally

#### Scenario: Invalid value falls back per key

- **WHEN** `config.toml` sets `placement = "zigzag"` and `direction = "down"`
- **THEN** the action warns that `placement` is invalid, uses the default placement `tab`, and still accepts `direction = "down"` as valid

### Requirement: Popup size keys width and height

The known key set SHALL include two optional keys, `width` and `height`,
each valid either as a positive integer TOML value (outer terminal-cell
dimensions) or as a string percentage of the form `N%` (e.g. `"80%"`); the
`placement` key's value set SHALL be `tab` (default) | `split` | `overlay` |
`zoomed` | `popup`. An omitted key SHALL stay unset and no size flag is
passed (herdr renders its documented default half-size popup); an invalid
value for either key SHALL fall back to that key's own default — left unset —
with a stderr warning while the other keys keep their effect, per the
per-key fallback policy of "Invalid config content degrades to defaults with
warnings". Resolved size values SHALL reach herdr only when the effective
placement is `popup` (the `--width`/`--height` flags are specified by the
pane-launch capability); for every other placement they SHALL be ignored
silently, with no warning, following the same pattern as `direction` for
non-split placements. Values smaller than herdr's popup minimum are clamped
by the daemon, not validated by the plugin.

#### Scenario: Integer cells and percentage string are valid

- **WHEN** `config.toml` sets `placement = "popup"`, `width = 80`, and `height = "80%"`
- **THEN** both size keys resolve as valid with no warning (their translation into `--width 80 --height 80%` is pinned by the pane-launch capability)

#### Scenario: Invalid size value omits that key with a warning

- **WHEN** `config.toml` sets `placement = "popup"`, `width = 0` (or any value that is neither a positive integer nor an `N%` string, e.g. `"tall"`), and `height = "80%"`
- **THEN** the action warns that `width` is invalid and leaves it unset, while `height = "80%"` still resolves and applies

#### Scenario: Size keys ignored for non-popup placements

- **WHEN** `config.toml` sets `placement = "split"` together with `width = 80` and `height = "80%"`
- **THEN** no warning is printed for the size keys and no size flag reaches the pane-open argv (the pane-launch capability pins the absence of the flags)

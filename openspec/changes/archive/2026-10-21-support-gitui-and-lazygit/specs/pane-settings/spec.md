# Pane settings

## MODIFIED Requirements

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

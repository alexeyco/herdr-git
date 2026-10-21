## 1. Define the new plugin identity

- [x] 1.1 Change `herdr-plugin.toml` plugin id to `alexeyco.gitui` while keeping action id and pane id `open`; update implementation references in `src/herdr.rs` and `src/settings.rs` so pane-open and config-dir callbacks use the new plugin id.
- [x] 1.2 Update `tests/e2e.rs` fixtures and assertions for the plugin id and config-dir callback; verify `alexeyco.gitui.open` invokes the action and `gitui.open` does not resolve as an alias, then run the focused test suite and require it to pass.

## 2. Update public documentation and specifications

- [x] 2.1 Update `README.md`, `docs/development.md`, `docs/how-it-works.md`, and `CHANGELOG.md` examples and references to use `alexeyco.gitui.open` for action bindings/invocation and `alexeyco.gitui` for plugin callbacks.
- [x] 2.2 Update `openspec/specs/pane-launch/spec.md` and `openspec/specs/pane-settings/spec.md` to match the delta requirements, including these scenarios: `Qualified action is invoked`, `Old action name is not an alias`, `Bin path absent at pane open`, `Tab focus passes through the same guard`, `Config-dir query passes through the same guard`, `Full context`, `Minimal context`, `Popup passes size flags`, `Split placement with explicit direction`, `Split placement with auto direction`, `Direction ignored for non-split placements`, `Split targets the caller pane`, `Zoomed targets the caller pane`, `Overlay omits workspace targeting`, `Unresolvable caller pane falls back to tab with warning`, `Reply without pane id`, `Reply without tab id`, `Environment variable preferred`, `CLI fallback when env absent`, `Neither source yields a directory`.

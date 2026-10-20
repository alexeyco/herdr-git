# Design: qualified plugin action identifier

## Approach

Use `alexeyco.gitui` as the Herdr plugin id everywhere Herdr addresses the plugin. Keep `open` as the action and pane entrypoint id. Herdr then exposes the action as `alexeyco.gitui.open`. Do not add a compatibility alias or migrate old settings.

The Herdr callback changes cover both `plugin pane open --plugin` and `plugin config-dir`. Keep `HERDR_GITUI_*` environment keys unchanged because they are runtime data keys, not Herdr plugin identifiers.

## Files to touch

- `herdr-plugin.toml` — declare plugin id `alexeyco.gitui`.
- `src/herdr.rs` — pass `alexeyco.gitui` to pane-open and config-dir callbacks and update identifier comments.
- `src/settings.rs` — update the config-dir identifier comment.
- `tests/e2e.rs` — expect the qualified action and new plugin id in callback assertions and fixtures.
- `README.md` — update binding, invocation, and config-dir examples.
- `docs/development.md` — update plugin commands and action examples.
- `docs/how-it-works.md` — update action identity and callback examples.
- `CHANGELOG.md` — update documented action and config-dir references.
- `openspec/specs/pane-launch/spec.md` — define the manifest/action identity and update pane callback requirements.
- `openspec/specs/pane-settings/spec.md` — update the config-dir callback identity.

## Data and compatibility

The manifest plugin id changes from `gitui` to `alexeyco.gitui`; action id and pane id remain `open`. The resulting user binding is `command = "alexeyco.gitui.open"`. Users must update existing bindings manually. The config-dir fallback now queries the new plugin id, so an old identity's config directory is not discovered by that callback.

## Trade-off

The plugin uses one canonical public identity. Existing references stop resolving until users update them, but the implementation has no alias or migration path to maintain.

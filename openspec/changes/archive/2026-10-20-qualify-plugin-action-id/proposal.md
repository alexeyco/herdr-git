# Qualify the plugin action identifier

## Why now

The repository is now named `herdr-git`, and its public action identifier needs to include the owner's namespace. Herdr forms the qualified action name from the plugin id and action id, so the manifest identity, callback arguments, examples, and tests must agree.

## What changes

Set the manifest plugin id to `alexeyco.gitui` and keep the action id `open`, making the qualified action `alexeyco.gitui.open`. Use that plugin id for Herdr pane-open and config-directory callbacks. Update test expectations and user-facing examples and references to the new identity.

Existing settings that use `gitui.open` and config directories stored under the old plugin identity do not move automatically. Users configure the new action and directory themselves. The old identifier is not retained as an alias.

## Impact

This change updates the `pane-launch` and `pane-settings` capabilities. It does not change the action's behavior, pane entrypoint id, or the `HERDR_GITUI_*` runtime environment variable names.

REJECTED-RISK: Existing bindings and old config-dir will not automatically move to new name — user chose only the new identifier and manual reconfiguration.

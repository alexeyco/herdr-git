# Development

Development happens against a locally linked plugin; no GitHub install needed.

## Dev loop

```
make build                       # produces ./target/release/herdr-gitui
herdr plugin link .              # register the working directory as a plugin
herdr plugin action list --plugin alexeyco.git
herdr plugin action invoke alexeyco.git.open
herdr plugin log list --plugin alexeyco.git
```

`plugin link` does not run build commands, so rebuild after every source change. Unlink with `herdr plugin unlink alexeyco.git`.

The action calls back into herdr through `HERDR_BIN_PATH` (set by herdr), not through a bare `herdr` on `PATH`. That is the binary herdr launched, so callbacks work even when the plugin is linked.

To exercise the session flow, put [gitui](https://github.com/gitui-org/gitui), [lazygit](https://github.com/jesseduffield/lazygit), or both on `PATH`. The plugin resolves the first executable of each tool.

## Checks

```
make lint        # cargo fmt --check + cargo clippy
make test        # cargo test
make build       # release build
make fmt-check   # prettier check
```

`make check` runs all of them. CI runs the same commands verbatim (it does not use make); see [.github/workflows/ci.yml](../.github/workflows/ci.yml). Prettier is pinned to `prettier@3.9.6` in the Makefile and in CI, so the local formatting check matches what CI enforces.

`cargo test --locked` runs the unit tests plus the integration suite in `tests/`; the integration tests drive the real binary with stub `herdr` and `gitui` shell scripts against real git repos, so they need no network and no herdr install.

## E2E tests

The integration suite in `tests/e2e.rs` is hermetic. It uses:

- A **stub `herdr`** shell script that logs its argv and returns canned JSON for `plugin pane open` (including a `pane_id` and `tab_id`).
- A **stub `gitui`** shell script that logs its argv, runs `$GITUI_SCRIPT` if set (a shell script that mutates the test repo between snapshots — e.g. commits a file), and exits `$GITUI_RC` (default 0).

Real git repos are created per test. Coverage:

- Action mode pane-open args (correct `--plugin`, `--entrypoint`, `--env` pairs).
- Happy path: commit in gitui → agent prompt with `gitui session summary:` and `created commit`.
- No change → no prompt, `no repository changes detected` logged.
- Non-zero rc → hold/error.
- No agent → summary kept file + hold message.
- Outside herdr → exit 1.

`stdin` is null in spawned runs so holds return at once.

## Manual smoke test

```
make build
herdr plugin link .
herdr plugin list                     # shows alexeyco.git 0.2.0
herdr plugin action invoke alexeyco.git.open
```

1. Bind `prefix+shift+g` in `~/.config/herdr/config.toml`:

   ```toml
   [[keys.command]]
   key = "prefix+shift+g"
   type = "plugin_action"
   command = "alexeyco.git.open"
   ```

2. Happy path: focus an agent pane, open gitui, make a commit, and quit; the summary arrives in the agent as one prompt prefixed `gitui session summary:`.
3. Quit gitui without making changes → nothing is sent.
4. Switch branch in gitui → the branch switch is reported.
5. Push with an upstream configured → the push is reported (ahead-count decrease).
6. Close the `gitui` tab early → the session is cancelled; nothing is sent.
7. Invoke the action from a pane with no agent → the summary is kept on disk, the path is printed in the plugin log, and the pane waits for Enter before closing.
8. Inspect the log: `herdr plugin log list --plugin alexeyco.git`.

## Version sync

The version lives in two files that must match:

- `Cargo.toml`
- `herdr-plugin.toml`

Bump both in the same commit. A release is cut by pushing a tag `v<version>` (e.g. `v0.1.0`); the release workflow fails if the tag does not match both files.

CI enforces this on every PR: a `version-sync` job fails when the two `version = "..."` values diverge, so drift is caught before a release tag, not only at tag time. An `msrv` job reads `rust-version` from `Cargo.toml`, installs that exact toolchain, and runs `cargo check --locked`.

The release workflow first verifies the tag against both version files, then runs the full CI suite (via `workflow_call`) and only then creates the release and builds artifacts, so a release is gated on the same lint and test matrix as a PR. It ships binaries for four targets: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, and `aarch64-apple-darwin`. The macOS binaries are unsigned and not notarized. For release steps, see [CONTRIBUTING.md](../CONTRIBUTING.md).

## Notes

- herdr validates `min_herdr_version` against the running herdr binary; linking with an older herdr is refused.
- Usage and requirements are documented in the [README](../README.md); behavior details in [how-it-works.md](how-it-works.md).

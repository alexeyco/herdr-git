# Compare Herdr Git plugins

## Why

The README compares `herdr-git` with the upstream Git TUIs it launches, which
does not help readers compare Herdr plugins. A source-based comparison also
needs a recurring freshness check so release links and public sources do not
quietly become stale.

## What Changes

- Replace the current table with a compact feature matrix and a collapsible
  detailed comparison, following the presentation in `herdr-review`. Compare
  `herdr-git` with Crokily/herdr-lazygit, JacquesvanWyk/herdr-lazygit,
  beomjungil/herdr-lazygit-overlay, and alexarthurs/herdr-sidebar. Identify the
  last as an adjacent Git workflow plugin, not a Git TUI launcher; use short,
  linked names in the table headers.
- Describe only capabilities documented by each peer README. Use
  “Not stated in README” when a capability is undocumented; do not present
  that as evidence the capability is absent. Include the latest release for
  each peer when one exists, state when no GitHub release exists, include
  source links, and add a human-maintained date for the content review.
- Add a Node-based comparison checker and an offline test suite. A monthly
  GitHub Actions workflow checks release tags, repository availability, and
  external README links. It opens or updates a rolling PR for release-tag drift,
  files or updates one issue for other documented-source drift, and fails the
  run without opening a PR or issue when sources are unavailable.
- Keep the checker out of the regular CI gates. Add no npm dependencies or
  `package.json`.

## Capabilities

### New Capabilities

- `plugin-comparison`: a source-linked comparison of Herdr Git plugins and its
  monthly freshness and drift-reporting behavior.

### Modified Capabilities

None.

## Impact

Changes `README.md`, adds `scripts/check-comparison.mjs`,
`.github/workflows/comparison-freshness.yml`, and
`tests/comparison.test.mjs`. The scheduled workflow needs GitHub Actions
permissions for contents, pull requests, and issues; drift may create or
update a comparison-fix PR or a `comparison-drift` issue. The workflow never
updates the default branch directly. Production plugin code, Cargo dependencies,
`Makefile`, and existing CI workflows remain unchanged.

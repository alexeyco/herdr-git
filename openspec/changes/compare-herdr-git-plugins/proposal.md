# Compare Herdr Git TUI plugins

## Why now

The README currently compares `herdr-git` with the upstream `gitui` and
`lazygit` applications. Those applications are tools that plugins launch, so
the table does not help readers compare alternatives in the Herdr plugin
category.

## What changes

Replace the upstream-app comparison with a table comparing `herdr-git` to
three Herdr plugins that launch a Git TUI: [Crokily/herdr-lazygit](https://github.com/Crokily/herdr-lazygit),
[JacquesvanWyk/herdr-lazygit](https://github.com/JacquesvanWyk/herdr-lazygit),
and [beomjungil/herdr-lazygit-overlay](https://github.com/beomjungil/herdr-lazygit-overlay).
Also include [alexarthurs/herdr-sidebar](https://github.com/alexarthurs/herdr-sidebar)
as an adjacent Git workflow plugin, clearly distinguished from Git TUI
launchers. Compare documented placement and behavior, agent summary, and
distinctive functions. Link each peer to its repository and describe only
capabilities its README documents. Mark a feature as “Not stated in README”
when its documentation does not establish it; do not imply that the plugin
lacks it.

## Capabilities

- Adds a `plugin-comparison` requirement for a source-linked, documentation-
  based comparison in the README.
- Leaves plugin behavior and comparison of raw upstream applications,
  `herdr-file-viewer`, and review plugins out of scope.

## Assumptions and rejected risks

None. The peer set, comparison dimensions, evidence sources, and wording
constraint are confirmed by the user.

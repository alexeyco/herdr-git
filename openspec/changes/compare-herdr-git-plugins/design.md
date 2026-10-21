# Design

## Approach

Replace the current comparison table in `README.md` with a table whose columns
are `herdr-git`, the three selected Herdr Git TUI launchers, and
`alexarthurs/herdr-sidebar`. Label the sidebar as an adjacent Git workflow
plugin, not a Git TUI launcher. Compare placement and open behavior, agent
summary, and distinctive functions; identify the Git TUI for launcher peers.
Include a repository link in each peer's column heading.

Use each peer's public README as the source for its table entries:

- `Crokily/herdr-lazygit`: lazygit in a narrow sidebar, a key to expand it,
  and optional AI-written commit messages.
- `JacquesvanWyk/herdr-lazygit`: lazygit in a split pane or tab, a
  press-to-open/focus/close toggle, and a `jq` requirement for the toggle.
- `beomjungil/herdr-lazygit-overlay`: lazygit in a session-modal popup and
  forwarding the active pane working directory.
- `alexarthurs/herdr-sidebar`: a dockable file explorer/source control pane;
  stages, unstages, discards, commits, inspects diffs, and syncs; browses
  commits, file history, branches, worktrees, remotes, stashes, and tags; can
  draft a commit message through local `claude` CLI with a filename fallback.
  Its README does not claim to launch gitui or lazygit.
- `alexeyco/herdr-git`: gitui or lazygit, configured Herdr pane placement,
  and a Git session summary delivered to the focused agent.

For capabilities not established by a peer README, write `Not stated in
README`. Do not use a blank, cross, or “no” that could be read as a claim that
the capability is absent. Link the peers directly to their GitHub
repositories. The marketplace page says listings are automatically indexed
from public repositories with the `herdr-plugin` topic and valid manifest,
and are not reviewed; the comparison is a documentation-based feature guide,
not a review or exhaustive market survey.

## Files

- `README.md` — replace only the existing comparison section and its
  introductory copy.

## Sources

- [Crokily/herdr-lazygit README](https://github.com/Crokily/herdr-lazygit)
- [JacquesvanWyk/herdr-lazygit README](https://github.com/JacquesvanWyk/herdr-lazygit)
- [beomjungil/herdr-lazygit-overlay README](https://github.com/beomjungil/herdr-lazygit-overlay)
- [alexarthurs/herdr-sidebar README](https://github.com/alexarthurs/herdr-sidebar)
- [Herdr plugin marketplace](https://herdr.dev/plugins/)
- Current `README.md`

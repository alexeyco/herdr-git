# Design

## Context

The current `README.md` compares `herdr-git` with raw Git TUIs. The approved
peer set is three Git TUI launchers and `herdr-sidebar`, an adjacent Git
workflow plugin. The sibling `herdr-review` provides the presentation and
monthly freshness-check pattern to adapt.

## Goals / Non-Goals

**Goals:**

- Present the comparison in a compact summary matrix followed by a collapsible
  detailed table, with concise linked header labels.
- Keep peer behavior claims traceable to public READMEs and distinguish an
  undocumented feature from an absent feature.
- Automatically detect stale release tags and dead or moved external links;
  route machine-fixable and human-review findings separately.

**Non-Goals:**

- Do not compare raw `gitui` or `lazygit` applications as Herdr competitors.
- Do not automate review of editorial feature claims or update the human-review
  date.
- Do not add the checker to `make check` or existing push/pull-request gates.
- Do not change plugin behavior, production Rust code, Cargo dependencies, or
  add `package.json`.

## Decisions

### Comparison layout and sources

Follow the `herdr-review` README structure: a short introduction, a status
legend, a compact capability matrix using documented and partial/different
statuses with explanatory footnotes, a dated source-review line, and a
`<details><summary>Detailed comparison</summary>` table. Use short column
labels linked to the source repositories: `herdr-git`, `Crokily`, `Jacques`,
`beomjungil`, and `herdr-sidebar`. Mark `herdr-sidebar` as adjacent Git
workflow tooling. The legend may use positive and partial status symbols, but
undocumented cells must literally say `Not stated in README`; never use a
negative mark to imply absence.

The detailed table compares surface and placement, TUI selection, documented
Git capabilities, repeat-call behavior, and latest release. Add one
manual-review date for the feature claims. The release row links the latest
release for each peer, or states when the repository has no GitHub release.
Resolve current release tags during implementation and list only peers that
publish GitHub releases in the workflow's `EXPECT_REPOS`; the link check still
covers every peer source. Do not add a separate agent-integration row: it is
too sparse to help users choose among these plugins.

Use each peer README as the feature source:

- `Crokily/herdr-lazygit`: narrow lazygit sidebar, `U` expands to the full
  layout, and `C` offers optional AI commit-message generation.
- `JacquesvanWyk/herdr-lazygit`: split pane or tab, repeated action opens,
  focuses, and closes, and `jq` is required for the toggle.
- `beomjungil/herdr-lazygit-overlay`: session-modal popup over the active pane
  and forwarding of the focused pane's working directory.
- `alexarthurs/herdr-sidebar`: dockable Explorer and Source Control pane;
  stages, unstages, discards, commits, inspects diffs, syncs, browses Git
  history and refs, and drafts commit messages with local `claude` plus a
  filename fallback. Its README does not describe launching a Git TUI.
- `alexeyco/herdr-git`: user-selected gitui or lazygit, configured Herdr pane
  placement, and a Git session summary delivered to the focused agent.

### Freshness checker

Adapt the `herdr-review` `scripts/check-comparison.mjs` contract to this
README and peer set. Implement `scripts/check-comparison.mjs` with Node built-in
modules only; keep it independently runnable with `node --test` fixtures and
do not introduce npm installation or dependencies.

Check mode reads the README once and verifies three fact classes:

1. For each `owner/repo@tag` in `EXPECT_REPOS`, query
   `GET /repos/{owner}/{repo}/releases/latest`. A matching tag is clean; a
   mismatch is machine-fixable release drift; a 404 is issue-only drift.
2. Check each distinct external HTTP(S) URL in the README with HEAD and
   redirects enabled. A final 404 is a dead link; a changed final URL is moved
   link drift. Other unsuccessful statuses or transport failures are
   unavailable sources.
3. Verify each configured release tag has a corresponding release-cell URL in
   the README. A stale tag with a matching tagless URL prefix is machine-fixable;
   a missing cell is issue-only drift.

Use exit 0 for clean checks, 1 for drift, and 2 for any unavailable source;
unavailable dominates drift. Give requests a 10-second timeout and retry
transport errors, 429, and 5xx once after 2 seconds. Support
`COMPARISON_GITHUB_API` for fixture testing and `GITHUB_TOKEN` for authenticated
GitHub requests.

Support the reference modes and their narrow write sets:

- Default check mode accepts `--readme`, `--report`, and `--summary`.
- `--fix` accepts `--readme` and `--workflow`; it updates only matching release
  cells and the workflow's single `EXPECT_REPOS` line. Compute all changes
  before writing. Leave 404 entries and unmatchable cells untouched so the
  issue route can report them. Never change feature cells, footnotes, or the
  manual-review date.
- `--report-issue <path>` creates or updates a single `comparison-drift` issue
  using the GitHub REST API, not the `gh` CLI. Ensure the label before issue
  creation and treat an already-existing label response as success.
- `--summary <path>` appends an audit of release, README-cell, and link checks
  for clean, drift, and unavailable runs. Do not include credentials.

Print each checker message with the `comparison-check:` prefix. In check mode,
write an issue report only for issue-only drift; machine-fixable release drift
uses the PR route. A failed summary write is unavailable.

### Monthly workflow and drift routing

Add `.github/workflows/comparison-freshness.yml` with cron `0 6 1 * *` and
`workflow_dispatch`. Serialize runs with a concurrency group. Grant only the
GitHub Actions permissions needed for contents, pull requests, and issues. Use
`ubuntu-latest`, Node 22, and `GITHUB_TOKEN`; do not install npm packages.

Capture check exit status: clean ends green, drift continues through the
routing steps, and unavailable sources fail the run without opening a PR or
issue. On drift, run `--fix`, format `README.md` with the repository's pinned
Prettier version, and open or update the rolling
`chore/comparison-freshness` pull request against `master`. The PR may contain
only the README release cells and workflow expectations; maintainers merge it.
If the check report contains issue-only drift, create or update one issue
labeled `comparison-drift`. A machine-fixable-only run creates no issue.

The workflow writes a run audit to `$GITHUB_STEP_SUMMARY`. Leave `Makefile` and
existing CI workflows unchanged; the Node tests run locally only.

### Tests

Add `tests/comparison.test.mjs` using `node:test` and a local `node:http`
fixture server. Cover check classifications, retry and timeout behavior,
release-cell matching, atomic and selective fix behavior, issue deduplication,
summary output and credential exclusion, and workflow guards. Tests must not
depend on live GitHub, Herdr, or credentials.

## Risks / Trade-offs

- [GitHub rate limits or transient failures] → authenticate workflow API calls
  with `GITHUB_TOKEN`, retry limited transient failures once, and fail closed
  with exit 2 instead of filing a misleading drift issue.
- [HEAD behavior differs across hosts] → follow redirects and report
  unsupported or unreachable URLs as unavailable for human review.
- [A generated fix PR becomes stale before merge] → keep one rolling PR branch;
  the next monthly run updates it, and the maintainer reviews and merges it.
- [Feature claims become stale even when links remain live] → retain the dated
  human-review line; automation never rewrites editorial claims or that date.
- [GitHub Actions default token behavior changes] → the workflow may open PRs
  only from a dedicated branch and never pushes the default branch directly.

## Sources

- [herdr-review comparison README](https://github.com/alexeyco/herdr-review/blob/master/README.md)
- [herdr-review freshness workflow](https://github.com/alexeyco/herdr-review/blob/master/.github/workflows/comparison-freshness.yml)
- [herdr-review checker](https://github.com/alexeyco/herdr-review/blob/master/scripts/check-comparison.mjs)
- [Crokily/herdr-lazygit README](https://github.com/Crokily/herdr-lazygit)
- [JacquesvanWyk/herdr-lazygit README](https://github.com/JacquesvanWyk/herdr-lazygit)
- [beomjungil/herdr-lazygit-overlay README](https://github.com/beomjungil/herdr-lazygit-overlay)
- [alexarthurs/herdr-sidebar README](https://github.com/alexarthurs/herdr-sidebar)
- [Herdr plugin marketplace](https://herdr.dev/plugins/)

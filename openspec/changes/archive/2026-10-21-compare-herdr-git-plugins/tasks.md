# Tasks

## 1. README comparison

- [x] 1.1 Replace the upstream Git TUI comparison in `README.md` with a
      concise summary matrix and a collapsible `Detailed comparison` table,
      following `herdr-review`; use short linked column labels and identify
      `herdr-sidebar` as adjacent Git workflow tooling. Verify the headings,
      documented dimensions, and table layout in the rendered Markdown.
- [x] 1.2 Verify every peer feature cell against its public README, use
      “Not stated in README” for undocumented behavior, and include the latest
      release row with tag links where releases exist and an explicit no-release
      state otherwise. Read current tags from the
      GitHub release pages and verify the human-review date and source links.
- [x] 1.3 Run `npx --yes prettier@3.9.6 --write README.md` and
      `npx --yes prettier@3.9.6 --check "**/*.md"`; confirm no upstream
      `gitui`/`lazygit` applications remain as comparison columns.

## 2. Freshness checker and tests

- [x] 2.1 Add `scripts/check-comparison.mjs` using Node built-ins only. Check
      configured GitHub latest-release tags, release-cell presence, and
      external README link liveness; report clean, drift, and unavailable
      results as exit codes 0, 1, and 2, with unavailable taking precedence.
      Verify request timeout, retry, and `COMPARISON_GITHUB_API` test seam.
- [x] 2.2 Implement `--fix`, `--report`, `--report-issue`, and `--summary`.
      Verify that fix mode updates release cells and the workflow expectation
      line atomically, issue mode deduplicates one `comparison-drift` issue,
      and the audit summary contains no credentials.
- [x] 2.3 Add `tests/comparison.test.mjs` using `node:test` and an offline HTTP
      fixture server. Run `node --test tests/comparison.test.mjs` and cover
      clean/drift/unavailable results, fix and issue routing, summary output,
      and credential exclusion.

## 3. Monthly GitHub Actions workflow

- [x] 3.1 Add `.github/workflows/comparison-freshness.yml` with a monthly
      schedule and manual dispatch. Verify serialized runs, required token
      permissions, the machine-drift PR route, issue-only drift route, and
      unavailable-source failure route; confirm it never writes to `master`.
- [x] 3.2 Verify the workflow writes the run audit to the job summary and
      leaves `Makefile`, `.github/workflows/ci.yml`, and
      `.github/workflows/release.yml` unchanged; the checker must not become
      a push or pull-request CI gate.

## 4. Integrated verification

- [x] 4.1 Run the checker against the current public sources, the comparison
      fixture tests, `openspec validate compare-herdr-git-plugins`,
      `git diff --check`, and `gtimeout -k 5 300 make check`; resolve any
      failures and confirm only the planned comparison, checker, tests, and
      monthly workflow files changed.

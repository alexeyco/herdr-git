# plugin-comparison Specification

## Purpose

Helps readers compare Herdr Git plugins using their public documentation and
keeps machine-checkable facts and source links fresh over time.

## Requirements

### Requirement: README compares Herdr Git plugins using documented capabilities

The README SHALL present a compact status matrix and a collapsible detailed
comparison of `herdr-git` and the selected Herdr Git plugins. The matrix and
detailed comparison SHALL use the same four criteria: flexible launch
placement, `gitui` support, `lazygit` support, and feedback to the agent. It
SHALL link each peer, identify `herdr-sidebar` as adjacent workflow tooling, and
avoid claims beyond the linked public documentation.

#### Scenario: Reader sees the selected peers in both comparison views

- **WHEN** a reader opens the README comparison section
- **THEN** its summary and detailed table compare `herdr-git` with
  Crokily/herdr-lazygit, JacquesvanWyk/herdr-lazygit,
  beomjungil/herdr-lazygit-overlay, and alexarthurs/herdr-sidebar using short,
  linked header names; the sidebar is labeled as adjacent Git workflow tooling

#### Scenario: Comparison follows the herdr-review presentation

- **WHEN** a reader scans the comparison section
- **THEN** it has an introductory sentence, a concise status legend, a compact
  matrix, source footnotes, and a collapsible `Detailed comparison` table; the
  matrix uses documented, partial/different, and not-found-in-public-sources
  statuses; both tables compare flexible launch placement, `gitui` support,
  `lazygit` support, and feedback to the agent; the detailed table also records
  the latest release or an explicit no-release state

#### Scenario: A capability is not documented

- **WHEN** a peer's linked public sources do not describe a comparison
  capability
- **THEN** the cell uses the not-found status defined by the legend and does
  not claim that the capability is impossible or absent from the implementation

#### Scenario: Git TUI launcher behavior follows peer documentation

- **WHEN** a reader compares the Git TUI launchers
- **THEN** Crokily is described as a lazygit sidebar with an expand action and
  optional AI commit messages; JacquesvanWyk as a split or tab launcher with
  an open/focus/close toggle requiring `jq`; and beomjungil as a session-modal
  popup that forwards the active pane working directory

#### Scenario: Adjacent Git workflow plugin is described accurately

- **WHEN** a reader compares `herdr-sidebar`
- **THEN** it is described as a dockable file explorer and source-control pane
  with its documented Git operations, history browsing, and local `claude`
  commit-message drafting with filename fallback, without calling it a Git TUI
  launcher

#### Scenario: Human-reviewed content has a source date

- **WHEN** a reader checks the comparison's documented feature claims
- **THEN** the section links its public sources and gives a date for the human
  review; the automated updater does not change that date or feature claims

### Requirement: Monthly freshness checks report comparison drift

The freshness checker SHALL compare configured peer release tags, release-cell
presence, and README external-link liveness with GitHub sources. It SHALL
distinguish drift from unavailable sources and preserve a report for every run.

#### Scenario: Current sources pass the check

- **WHEN** every configured latest release matches its expected tag, release
  cells are present, and README external links resolve without moving
- **THEN** the checker reports a clean result and exits 0

#### Scenario: Release tag or cell is stale

- **WHEN** GitHub reports a newer release tag or the README release cell has
  an older configured tag
- **THEN** the checker reports machine-fixable drift and exits 1

#### Scenario: Repository or link needs human review

- **WHEN** a release endpoint returns 404, a README URL is dead or moved, or a
  configured release cell is missing
- **THEN** the checker reports issue-only drift and exits 1

#### Scenario: A source cannot be checked

- **WHEN** GitHub or a README link returns an unavailable status, a request
  fails, or the checker cannot parse its required configuration
- **THEN** the checker reports the unavailable source and exits 2, even if it
  also found drift

### Requirement: Release fixes preserve human-authored comparison content

The fix mode SHALL update release tags in the README and the workflow's
expected-tag list together. It SHALL leave feature claims, status cells,
source notes, and the human-review date unchanged.

#### Scenario: Release drift is fixed on both surfaces

- **WHEN** a configured peer has a newer latest release and the README has a
  matching release cell
- **THEN** fix mode updates that cell and its expected-tag entry to the live
  tag in the same run

#### Scenario: Fix mode cannot repair a missing release cell

- **WHEN** a configured release cell is absent from the README
- **THEN** fix mode leaves the README and expected-tag entry unchanged and
  preserves the missing-cell finding for issue reporting

#### Scenario: Unavailable release source causes no partial update

- **WHEN** any required release source is unavailable or the expected-tag
  workflow entry is missing
- **THEN** fix mode exits 2 and leaves both files unchanged

### Requirement: Monthly workflow routes drift without changing CI gates

The GitHub Actions workflow SHALL run monthly and on manual dispatch, route
machine-fixable drift to a rolling pull request and issue-only drift to one
deduplicated issue, and fail on unavailable sources. The checker SHALL remain
outside push and pull-request CI gates.

#### Scenario: Scheduled and manual runs execute the checker

- **WHEN** the monthly schedule fires or a maintainer dispatches the workflow
- **THEN** the workflow runs the checker and writes its audit report to the job
  summary

#### Scenario: Machine-fixable drift opens or updates a pull request

- **WHEN** the check reports only machine-fixable release drift
- **THEN** fix mode updates the release data and the workflow opens or updates
  a pull request on a dedicated branch; it does not push the default branch

#### Scenario: Issue-only drift creates or updates one issue

- **WHEN** the check reports repository, link, or missing-cell drift
- **THEN** the workflow creates or updates one issue labeled
  `comparison-drift` with the findings

#### Scenario: Unavailable source fails without a pull request or issue

- **WHEN** the check exits 2
- **THEN** the workflow fails and opens neither a pull request nor an issue

#### Scenario: Regular CI gates remain unchanged

- **WHEN** a contributor runs `make check` or pushes a change
- **THEN** the monthly checker and its Node tests are not added to the existing
  CI gates

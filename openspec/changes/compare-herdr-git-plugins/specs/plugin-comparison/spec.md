## ADDED Requirements

### Requirement: README compares Herdr Git plugins using documented capabilities

The README SHALL compare `herdr-git` with the selected Git TUI launchers and
the adjacent `herdr-sidebar` Git workflow plugin. It SHALL distinguish the
sidebar from launchers and cover Git TUI, placement and open behavior, agent
summary, and distinctive functions. Undocumented capabilities SHALL NOT be
presented as absent.

#### Scenario: Reader compares the selected Herdr plugins

- **WHEN** a reader views the README comparison section
- **THEN** it compares `herdr-git` with Crokily/herdr-lazygit,
  JacquesvanWyk/herdr-lazygit, and beomjungil/herdr-lazygit-overlay as Git TUI
  launchers, and alexarthurs/herdr-sidebar as an adjacent Git workflow plugin,
  with each peer linked to its repository

#### Scenario: Peer feature is not documented

- **WHEN** a peer README does not document a capability in a comparison
  dimension
- **THEN** the table says that the capability is not stated in that README
  and does not claim that the peer lacks the capability

#### Scenario: Distinctive behavior follows peer documentation

- **WHEN** a reader compares distinctive functions
- **THEN** the table describes Crokily's sidebar, expand action, and optional
  AI commit messages; JacquesvanWyk's split/tab toggle and its `jq`
  requirement; and beomjungil's session-modal popup and active-pane working
  directory forwarding, with source links to their repositories

#### Scenario: Adjacent Git workflow plugin is distinguished from launchers

- **WHEN** a reader compares `herdr-sidebar` with the Git TUI launchers
- **THEN** the table identifies it as a dockable file explorer/source control
  pane and describes its documented Git workflow and commit-message drafting
  functions without implying that it launches gitui or lazygit

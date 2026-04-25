## MODIFIED Requirements

### Requirement: User can open a dedicated project metrics screen
The application SHALL provide a dedicated `Project metrics` workspace in `codex-session-explorer`
whose title and global controls live in the app shell header rather than in an additional nested
screen header.

#### Scenario: Opening project metrics screen
- **WHEN** the user switches to project metrics mode from the main explorer UI
- **THEN** the application shows a dedicated project-level analytics workspace instead of embedding
  the same content inside the session metrics card
- **THEN** the workspace title and global controls MUST be presented through the app shell header
  without an extra local screen header above the chart

### Requirement: User can query metrics for a selected project and time window
The project metrics screen SHALL let the user select a project and time window, and control
`scope` plus `includeSpawnAgents`, using the existing project metrics backend query.

#### Scenario: Loading project metrics for a project
- **WHEN** the user selects a project, a time window, a `scope`, and an `includeSpawnAgents` mode
- **THEN** the application requests project metrics for those exact parameters
- **THEN** the screen renders the returned chronological session series without changing the backend
  contract of the project metrics query

### Requirement: The workspace is optimized for normalized multi-metric correlation analysis
The project metrics screen SHALL use its main chart as a normalized multi-metric comparison surface
for correlation analysis rather than as a one-metric-at-a-time viewer.

#### Scenario: Loading a project workspace with data
- **WHEN** the query returns project metrics for a selected project and range
- **THEN** the workspace MUST render multiple supported metric families together on the main chart
- **THEN** the chart MUST use one shared normalization method for all visible metrics
- **THEN** the workspace MUST keep raw absolute values available in inspection UI without making raw
  values the main comparison scale

### Requirement: User can drill down from a chart point to the source session
The project metrics screen SHALL let the user pin a chart point and inspect the full selected
session in the side panel without losing graph context, while still allowing an explicit transition
to the existing session detail workflow when needed.

#### Scenario: Pinning a session from the chart
- **WHEN** the user clicks a chart point representing a specific session
- **THEN** the explorer MUST pin that session as the active project-metrics selection
- **THEN** the side panel MUST show the selected session details inside the `Pinned session` tab
  without leaving the project metrics screen

#### Scenario: Opening the pinned session in the existing session workflow
- **WHEN** the user explicitly chooses to open the pinned session from the side panel
- **THEN** the explorer MUST open that session in the existing session detail workflow
- **THEN** pinning itself MUST NOT automatically navigate away from the project metrics screen

## ADDED Requirements

### Requirement: Side panel starts with a window pulse overview
The project metrics screen SHALL expose `Window pulse` as the first side-panel tab and use it for
the current window summary rather than rendering the same summary as a permanent block above the chart.

#### Scenario: Viewing the default side-panel overview
- **WHEN** the user opens a loaded project metrics workspace
- **THEN** the first side-panel tab MUST be `Window pulse`
- **THEN** that tab MUST show the current visible-window summary without requiring a permanent
  overview block in the main chart area

### Requirement: Pinned session details remain the primary truth for point inspection
The project metrics screen SHALL treat the `Pinned session` tab as the full truth source for the
selected session, including metric values, `agent_role`, session identifiers, state flags and the
available start request text.

#### Scenario: Viewing a pinned session in the side panel
- **WHEN** a chart point is pinned
- **THEN** the `Pinned session` tab MUST show all available metric values for that session
- **THEN** the tab MUST show `agent_role`, the full session identifier, session timing and flags
  such as `anomaly`, `partial`, `unknown` and `degraded`
- **THEN** the tab MUST show the start user request when the existing data paths can provide it, or
  an explicit unavailable state otherwise

### Requirement: Pinned session request text loads through a dedicated lazy-fetch detail endpoint
The project metrics workspace SHALL load request/task text for the `Pinned session` tab through a
dedicated session-detail endpoint rather than expanding the main `query_project_metrics` payload.

#### Scenario: Loading pinned session detail after click
- **WHEN** the user pins a chart point or explicitly activates a session from the side panel
- **THEN** the client MUST request a dedicated session-detail payload by `session_id`
- **THEN** the request MUST NOT require a second `query_project_metrics` round-trip

#### Scenario: Detail endpoint serves materialized request text
- **WHEN** the backend returns pinned session detail
- **THEN** the payload MUST include the best available start request or task summary text together
  with enough metadata to distinguish available data from explicit `unavailable`
- **THEN** the detail payload MUST be served from versioned materialized storage when present, with
  on-demand recomputation only for missing or stale entries

### Requirement: Side panel exposes a dedicated anomalies tab
The project metrics screen SHALL expose a dedicated `Anomalies` side-panel tab with an indicator in
the tab header and MUST NOT rely on persistent anomaly panels inside the main chart area.

#### Scenario: Opening anomaly review from the side panel
- **WHEN** the loaded workspace contains anomaly items for the current window or pinned context
- **THEN** the `Anomalies` tab header MUST show a compact indicator of anomaly presence or count
- **THEN** anomaly details MUST be available inside that tab instead of as standalone panels above
  or below the chart

### Requirement: Side panel separates series-local controls from chart-global controls
The project metrics screen SHALL use different side-panel tabs for per-series configuration and
chart-global configuration.

#### Scenario: Configuring series and chart behavior
- **WHEN** the user works with the `Series` and `Chart` tabs
- **THEN** the `Series` tab MUST own series-local actions such as visibility, `primary`,
  emphasis, per-series analytical overrides and per-series `raw values`
- **THEN** the `Chart` tab MUST own only chart-global settings such as normalization description,
  global default analytical mode and outlier handling
- **THEN** the workspace MUST NOT expose a free chart-type switch as a separate control surface

### Requirement: Near-graph summary shows compact anomaly indicators
The project metrics screen SHALL expose anomaly and data-quality state in the near-graph summary as
compact indicators without expanding them into explanatory blocks in the main area.

#### Scenario: Viewing a pinned summary with anomaly context
- **WHEN** the pinned point or current window contains anomaly or data-quality signals
- **THEN** the near-graph summary MUST show compact indicators for those signals
- **THEN** the indicators MUST act as summary status only and MUST NOT turn the summary into a
  multi-line anomaly list

## ADDED Requirements

### Requirement: User can open a dedicated project metrics screen
The application SHALL provide a dedicated project metrics screen in `codex-session-explorer`
separate from the session-level metrics panel.

#### Scenario: Opening project metrics screen
- **WHEN** the user switches to project metrics mode from the main explorer UI
- **THEN** the application shows a dedicated screen for project-level metrics instead of embedding
  the same content inside the session metrics card

### Requirement: User can query metrics for a selected project and time window
The project metrics screen SHALL let the user select a project, choose a time window and toggle
whether spawn-agent contribution is included, then load data through the existing project metrics
backend query.

#### Scenario: Loading project metrics for a project
- **WHEN** the user selects a project, a time window and a spawn-agent mode
- **THEN** the application requests project metrics for those exact parameters
- **THEN** the screen renders the returned aggregate payload and chronological session series

### Requirement: Screen shows chronological charts for key project metrics
The project metrics screen SHALL render chronological charts derived from the returned session list
for at least total duration, total tokens, error or failed-operation counts, and tool-call volume.

#### Scenario: Rendering metric trends
- **WHEN** the backend returns multiple sessions for the selected project and time window
- **THEN** the screen shows time-ordered charts for the supported metrics
- **THEN** each plotted point is associated with the contributing session that produced it

### Requirement: Coverage semantics remain visible in the charts
The project metrics screen SHALL preserve `known`, `partial` and `unknown` coverage semantics from
the metrics contract and MUST NOT coerce `unknown` values to zero.

#### Scenario: Rendering unknown and partial values
- **WHEN** a metric point has `coverage = unknown`
- **THEN** the chart shows a gap or explicit unknown marker instead of plotting `0`
- **THEN** the tooltip or legend explains the coverage state

#### Scenario: Rendering partial values
- **WHEN** a metric point has `coverage = partial` and a numeric value
- **THEN** the chart shows the numeric value
- **THEN** the tooltip or legend marks that value as partial

### Requirement: User can drill down from a chart point to the source session
The project metrics screen SHALL let the user inspect contributing sessions and open the session
that corresponds to a selected chart point or list row.

#### Scenario: Opening a session from a chart
- **WHEN** the user activates a chart point or contributing-session row
- **THEN** the explorer opens that session in the existing session detail workflow
- **THEN** the user can continue investigation without manually searching for the session id

### Requirement: Screen handles empty and degraded project states explicitly
The project metrics screen SHALL show explicit UI states for no data, backend errors and degraded
project identity instead of presenting an empty chart area as valid data.

#### Scenario: No sessions in selected range
- **WHEN** the query succeeds but returns no sessions for the selected project and range
- **THEN** the screen shows an empty-state explanation for the chosen filters

#### Scenario: Degraded project identity
- **WHEN** the selected project belongs to a degraded project bucket
- **THEN** the screen shows a visible explanation that project identity is incomplete
- **THEN** the user can still inspect available metrics without mixing them with normal projects

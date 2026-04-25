## MODIFIED Requirements

### Requirement: User can inspect project metrics with interactive charts
The system SHALL render project metrics as interactive charts where hover is reserved for tooltip
inspection and click pins the active session for persistent analysis.

#### Scenario: Opening interactive charts
- **WHEN** the user opens project metrics for a selected project and range
- **THEN** the screen MUST show interactive charts with hover tooltip details for the supported metrics
- **THEN** clicking a chart point MUST pin that session as the active analytical context

### Requirement: The chart compares multiple metric families at once
The system SHALL treat the project-metrics chart as a correlation surface and render multiple
supported metrics from different families simultaneously rather than reducing the viewport to a
single active series.

#### Scenario: Opening a loaded correlation chart
- **WHEN** the user opens project metrics for a selected project and range
- **THEN** the chart MUST render multiple supported metric series together on the same viewport
- **THEN** the comparison model MUST support cross-family inspection such as duration, tokens,
  failures and other available metrics without forcing one-at-a-time chart switching

### Requirement: All visible chart values are normalized by one shared method
The system SHALL normalize every visible metric series by one shared chart-level normalization
method before rendering them together.

#### Scenario: Comparing different metric families
- **WHEN** the chart renders multiple visible metric series with different raw units
- **THEN** every visible series MUST use the same chart-level normalization rule
- **THEN** the normalization rule MUST NOT vary per metric series
- **THEN** tooltip and detail inspection MUST still expose raw absolute values without treating raw
  values as the primary comparison scale

### Requirement: User can zoom and narrow the visible chart window
The system SHALL allow the user to reduce or shift the visible portion of the session series by
chart-native interaction, without separate window-size or window-position controls outside the chart.

#### Scenario: Narrowing the chart window
- **WHEN** the user selects a region on a loaded metric series
- **THEN** the screen MUST update the visible chart window to the selected subset of points
- **AND** the underlying loaded dataset MUST remain tied to the current project and time-range filters

#### Scenario: Using quick chart navigation controls
- **WHEN** the user uses overlay chart controls for zoom or left/right navigation
- **THEN** the screen MUST update the visible chart window directly from the chart viewport
- **THEN** those controls MUST live as compact overlays in the top-left corner of the chart

### Requirement: User can control metric visibility with toggles
The system SHALL provide curated visibility toggles for supported metric series through a dedicated
`Series` side-panel tab instead of persistent controls above the chart.

#### Scenario: Hiding and showing metric series
- **WHEN** the user turns a supported metric series on or off
- **THEN** the summary chart MUST update the visible set of series without reinterpreting unknown values as zero
- **THEN** the control surface MUST remain inside the `Series` tab

### Requirement: The chart uses a fixed primary-secondary visual hierarchy
The system SHALL keep a fixed visual grammar for project-metrics comparison: one `primary` metric
dominates the chart as a bar layer, while all other visible metrics remain muted secondary lines.

#### Scenario: Reading the chart with many series
- **WHEN** the chart renders a primary metric together with multiple secondary metrics
- **THEN** the primary metric MUST remain visually dominant as a bar-series
- **THEN** secondary metrics MUST remain visible as muted lines instead of competing dominant layers
- **THEN** chart type MUST NOT be exposed as a free user-facing toggle

### Requirement: Secondary metrics can be emphasized without changing chart type
The system SHALL let the user temporarily or persistently increase the visual weight of secondary
metrics while keeping the fixed chart grammar intact.

#### Scenario: Emphasizing a secondary metric
- **WHEN** the user hovers a metric in the series control surface or explicitly emphasizes it
- **THEN** that secondary metric MUST gain stronger visual emphasis than other muted secondaries
- **THEN** the chart MUST keep the same primary bar plus secondary line grammar
- **THEN** the system MUST NOT rely on a hard numeric cap for emphasized metrics as the primary way
  to preserve readability

### Requirement: Analytical modes use a global default with per-series overrides
The system SHALL provide one chart-level default analytical mode and allow per-series overrides for
supported metrics.

#### Scenario: Setting the default chart mode
- **WHEN** the user changes the chart-level default analytical mode
- **THEN** every visible metric series without an explicit override MUST immediately use that mode
- **THEN** the chart MUST preserve the current project, range, pin and window context

#### Scenario: Overriding one metric series
- **WHEN** the user assigns `trend`, `moving average` or `moving median` to one metric series from
  the `Series` tab
- **THEN** only that metric series MUST use the override
- **THEN** the override control MUST remain compact and series-local rather than becoming a global
  chart-type switch

### Requirement: Raw values can be enabled per metric series
The system SHALL allow raw-value rendering per metric series while preserving the analytical line as
the primary reading for that series.

#### Scenario: Enabling raw values for one metric
- **WHEN** the user enables raw values for a metric series
- **THEN** the chart MUST render raw values as a secondary visual layer for that metric
- **THEN** the enabled raw series MUST coexist with the shared normalization and current outlier mode
- **THEN** the raw-value toggle MUST remain series-local in the `Series` tab

## ADDED Requirements

### Requirement: The chart shows a compact pinned summary above the viewport
The system SHALL render a single-line pinned summary above the chart when a session is pinned.

#### Scenario: Viewing a pinned summary
- **WHEN** the user has pinned a chart point
- **THEN** the left side of the summary MUST show `agent_role`, compact flags for `anomaly`,
  `baseline`, `partial`, `unknown` and `degraded`, the point timestamp and a short session identifier
- **THEN** the right side of the summary MUST show `duration`, `tokens`, `calls` and `failures`

### Requirement: Hover tooltip does not override pinned context
The system SHALL keep hover tooltip inspection separate from the pinned session context.

#### Scenario: Hovering after a session is pinned
- **WHEN** the user hovers other points after pinning a session
- **THEN** the tooltip MUST reflect the hovered point
- **THEN** the pinned summary and side panel MUST continue to show only the pinned session until the
  user changes or clears the pin

### Requirement: The chart remains free from persistent anomaly markers
The system SHALL keep the chart viewport visually focused on metric exploration and MUST NOT render
persistent anomaly badges, chips or grouped anomaly lists directly on top of the chart.

#### Scenario: Reviewing anomalies in a loaded chart
- **WHEN** the current window contains metric, baseline, session or data-quality anomalies
- **THEN** the chart MUST remain readable without persistent anomaly markers layered over the lines
  or points
- **THEN** anomaly exploration MUST happen through the side-panel `Anomalies` tab and compact
  summary indicators near the chart

### Requirement: Anomalies are classified before being presented
The system SHALL classify anomaly signals into distinct classes before presenting them in the
project metrics workspace.

#### Scenario: Building anomaly review content
- **WHEN** the screen derives anomaly data for the current chart window
- **THEN** each anomaly item MUST belong to one of the classes `Metric`, `Baseline`, `Session` or
  `Data issue`
- **THEN** `change-point` and `baseline shift` signals MUST belong to the `Baseline` class rather
  than being merged with simple point outliers or data-quality flags

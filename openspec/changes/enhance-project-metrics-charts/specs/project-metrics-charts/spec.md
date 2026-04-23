## ADDED Requirements

### Requirement: User can inspect project metrics with interactive charts
The system SHALL render project metrics as interactive charts that support richer point inspection
than the current static SVG overview.

#### Scenario: Opening interactive charts
- **WHEN** the user opens project metrics for a selected project and range
- **THEN** the screen MUST show interactive charts with hover or focus details for the supported metrics

### Requirement: User can overlay multiple metric series on a summary chart
The system SHALL provide a summary chart where multiple supported metric series can be displayed at
the same time.

#### Scenario: Viewing multiple metrics together
- **WHEN** the user enables several supported metric series
- **THEN** the screen MUST render them together on the summary chart for the same session chronology

### Requirement: User can zoom and narrow the visible chart window
The system SHALL allow the user to reduce the visible portion of the time series without changing
the underlying project query filters.

#### Scenario: Narrowing the chart window
- **WHEN** the user uses the chart zoom control on a loaded metric series
- **THEN** the screen MUST update the visible chart window to the selected subset of points
- **AND** the underlying loaded dataset MUST remain tied to the current project and time-range filters

### Requirement: User can control metric visibility with toggles
The system SHALL provide curated visibility toggles for supported metric series so the user can
focus on the most relevant project signals without losing the summary-chart context.

#### Scenario: Hiding and showing metric series
- **WHEN** the user turns a supported metric series on or off
- **THEN** the summary chart MUST update the visible set of series without reinterpreting unknown values as zero

### Requirement: Coverage semantics remain explicit in interactive charts
The system SHALL preserve `known`, `partial` and `unknown` coverage semantics in chart rendering and
point details.

#### Scenario: Viewing a point with partial or unknown coverage
- **WHEN** the user inspects a chart point whose metric coverage is `partial` or `unknown`
- **THEN** the chart and tooltip MUST show the coverage state explicitly
- **AND** an `unknown` point MUST remain a gap or explicit unknown marker instead of a plotted zero value

### Requirement: User can drill down from an interactive chart to the source session
The system SHALL keep chart-driven navigation to the contributing session after the chart renderer is
enhanced.

#### Scenario: Opening a session from a chart point
- **WHEN** the user activates a chart point representing a specific session
- **THEN** the explorer MUST open that session in the current workflow context

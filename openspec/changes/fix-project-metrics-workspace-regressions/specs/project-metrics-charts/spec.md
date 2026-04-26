## ADDED Requirements

### Requirement: Every rendered chart point remains directly pinnable
The system SHALL provide a deterministic direct `click -> pin` path for every rendered project
metrics point, including the `primary` bar-layer.

#### Scenario: Pinning a session from the primary metric layer
- **WHEN** the user clicks the `primary` bar that represents a specific session
- **THEN** the chart MUST pin that exact session as the active analytical context
- **THEN** this activation MUST NOT depend on hover being the only selection source

#### Scenario: Pinning a session from a secondary metric layer
- **WHEN** the user clicks a visible point or dot on a secondary metric series
- **THEN** the chart MUST pin that exact session as the active analytical context
- **THEN** the interaction contract MUST remain equivalent to clicking the `primary` layer

### Requirement: Primary bar remains dominant without obscuring secondary lines
The system SHALL keep the fixed `primary bar + muted secondary lines` hierarchy, while ensuring the
primary bar does not visually drown out the secondary analytical lines.

#### Scenario: Reading primary and secondary metrics together
- **WHEN** the chart renders one `primary` bar-layer together with visible secondary lines
- **THEN** the primary layer MUST remain visually dominant
- **THEN** its fill opacity MUST still leave secondary lines readable in the same viewport

### Requirement: Shared normalization remains readable for sparse series
The system SHALL preserve one shared chart-level normalization for all visible series, while using a
compression step that prevents sparse operational metrics from collapsing against the lower bound of
the shared scale when more extreme series are present.

#### Scenario: Rendering sparse metrics together with large-range metrics
- **WHEN** the chart renders sparse series such as `failures` or `tool calls` together with more
  extreme visible metrics
- **THEN** the normalization pipeline MUST still use one shared chart-level scale
- **THEN** it MUST apply a monotonic compression step before the final global `min-max` mapping
- **THEN** sparse series MUST remain visually readable without switching to per-series normalization

### Requirement: Pinned session stays visible on the chart viewport
The system SHALL reflect the currently pinned session directly on the chart viewport with a compact
persistent visual affordance.

#### Scenario: Inspecting a pinned point on the chart
- **WHEN** the user pins a session from the chart
- **THEN** the corresponding point or bar MUST remain visibly marked on the chart viewport
- **THEN** this pinned affordance MUST persist independently of transient hover state
- **THEN** the affordance MUST stay compact and MUST NOT turn into a noisy overlay layer

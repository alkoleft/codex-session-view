## ADDED Requirements

### Requirement: Workspace preserves compatible chart context across query refresh
The `Project metrics` workspace SHALL preserve compatible user chart configuration when the loaded
dataset changes because of a new `project`, `period`, `scope` or `includeSpawnAgents` query.

#### Scenario: Refreshing project metrics after filter change
- **WHEN** the user changes `project`, `period`, `scope` or `includeSpawnAgents`
- **THEN** the workspace MUST keep compatible chart-global settings such as active tab, default
  analytical mode and outlier mode
- **THEN** the workspace MUST keep compatible per-series settings such as visibility, `primary`,
  emphasis, per-series override and `raw values`
- **THEN** the workspace MUST clamp zoom and pinned selection to the new dataset instead of
  resetting the whole workspace to its initial state

### Requirement: Pinned session survives refresh when still present
The `Project metrics` workspace SHALL preserve the pinned analytical context across query refresh
when the same contributing session still exists in the refreshed dataset.

#### Scenario: Refreshing a dataset that still contains the pinned session
- **WHEN** the user already has a pinned session and the refreshed project-metrics dataset still
  contains the same `session_id`
- **THEN** the workspace MUST keep that session pinned
- **THEN** the pinned summary and `Pinned session` tab MUST continue to show that same session
  without a forced fallback selection

### Requirement: Window pulse includes duration and token drift
The `Project metrics` workspace SHALL keep `Window pulse` compact, while exposing `duration` and
`token drift` as part of its quick current-window summary.

#### Scenario: Reading the current visible window
- **WHEN** the user opens the `Window pulse` tab for a loaded visible chart window
- **THEN** the quick summary MUST include a duration-oriented signal for that window
- **THEN** the quick summary MUST include a token-drift signal for that window
- **THEN** these signals MUST remain part of the compact window-level summary instead of becoming a
  separate overview block above the chart

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

### Requirement: Enabled skills stay tied to the first session skill list
The `Project metrics` workspace SHALL treat `Enabled skills` as the count of skills connected to the
session at startup, based on the first session message that enumerates available skills, rather than
on later explicit skill-usage markers.

#### Scenario: Counting connected skills from the first session message
- **WHEN** the first session message contains the available-skills list for that session
- **THEN** `skills_count` MUST equal the number of skills in that startup list
- **THEN** later skill-usage evidence MUST NOT rewrite that connected-skills count

#### Scenario: Explicit skill marker appears later in the session
- **WHEN** the session later contains an explicit `<skill>...</skill>` message for a concrete skill
- **THEN** the system MUST treat that marker as skill usage/load evidence
- **THEN** that marker MUST remain separate from the `Enabled skills` count shown for the session

#### Scenario: Quoted available-skills block does not become skill usage
- **WHEN** the session contains a quoted or repeated `skills_instructions` / `### Available skills`
  block without an explicit `<skill>...</skill>` marker
- **THEN** the system MUST treat that content as session context text rather than as concrete skill
  usage/load evidence
- **THEN** this quoted block MUST NOT increment or rewrite `used_skills` and MUST NOT rewrite
  `Enabled skills`

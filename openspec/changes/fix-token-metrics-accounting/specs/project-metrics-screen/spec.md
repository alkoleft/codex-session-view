## ADDED Requirements

### Requirement: Screen communicates metric completeness explicitly
The project metrics screen SHALL make it clear when a metric is corrected, partial or unsupported
with the current sources.

#### Scenario: Rendering an audited metric group
- **WHEN** the screen shows a metric whose backend coverage is `partial` or `unknown`
- **THEN** the UI MUST preserve that coverage state in summaries, pinned details or series metadata
- **AND** it MUST NOT present that metric as a fully trusted value just because the field exists in
  the payload

### Requirement: Screen summaries use explicit token labels
The project metrics screen SHALL distinguish `all tokens` from `cached tokens` in aggregate and
pinned-session summaries.

#### Scenario: Rendering token summaries
- **WHEN** the screen renders project-level summary cards or a pinned-session summary
- **THEN** the UI MUST label the overall token total as `all tokens`
- **AND** the UI MUST expose `cached tokens` as a separate metric instead of folding it into the
  generic `tokens` label

## MODIFIED Requirements

### Requirement: Screen shows chronological charts for key project metrics
The project metrics screen SHALL render chronological charts derived from the returned session list
for at least total duration, `all tokens`, `cached tokens`, error or failed-operation counts, and
tool-call volume.

#### Scenario: Rendering metric trends
- **WHEN** the backend returns multiple sessions for the selected project and time window
- **THEN** the screen shows time-ordered charts for the supported metrics
- **THEN** `all tokens` and `cached tokens` are available as distinct metric series rather than one
  ambiguous `tokens` signal
- **THEN** metric series that remain only `partial` or `unknown` after the completeness audit MUST
  preserve that status in the rendered screen
- **THEN** each plotted point is associated with the contributing session that produced it

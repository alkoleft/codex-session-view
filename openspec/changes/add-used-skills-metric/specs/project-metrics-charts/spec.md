## ADDED Requirements

### Requirement: User can chart used skills count as a first-class metric
The system SHALL expose `Used skills` as a supported project-metrics series derived from
`session.used_skills.count`, while keeping the existing used-skills rollup list for inspection.

#### Scenario: Enabling the used-skills series
- **WHEN** project metrics are loaded for a selected project and range
- **THEN** chart controls MUST offer a `Used skills` metric series
- **THEN** each chart point for that series MUST be derived from the corresponding
  `session.used_skills.count`
- **AND** the series MUST remain separate from `Enabled skills`

#### Scenario: Used-skills coverage is partial or unknown
- **WHEN** a session has `used_skills.count` with `coverage = partial` or `coverage = unknown`
- **THEN** the chart and point details MUST preserve that coverage state explicitly
- **AND** an `unknown` point MUST remain a gap or explicit unknown marker instead of `0`
- **AND** the UI MUST NOT substitute `Enabled skills` or project-level rollup length for that
  session point

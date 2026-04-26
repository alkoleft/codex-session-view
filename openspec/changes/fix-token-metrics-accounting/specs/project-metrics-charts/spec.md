## MODIFIED Requirements

### Requirement: User can overlay multiple metric series on a summary chart
The system SHALL provide a summary chart where multiple supported metric series can be displayed at
the same time, including distinct `all tokens` and `cached tokens` series when those values are
available.

#### Scenario: Viewing multiple metrics together
- **WHEN** the user enables several supported metric series
- **THEN** the screen MUST render them together on the summary chart for the same session chronology
- **THEN** `all tokens` and `cached tokens` MUST remain independently inspectable instead of sharing
  one generic token label
- **THEN** metric series with audited `partial` or `unknown` coverage MUST remain visibly
  distinguishable from fully known series

### Requirement: User can control metric visibility with toggles
The system SHALL provide curated visibility toggles for supported metric series so the user can
focus on the most relevant project signals without losing the summary-chart context.

#### Scenario: Hiding and showing metric series
- **WHEN** the user turns a supported metric series on or off
- **THEN** the summary chart MUST update the visible set of series without reinterpreting unknown
  values as zero
- **THEN** the visibility controls MUST expose separate entries for `all tokens` and `cached tokens`

## ADDED Requirements

### Requirement: Used skills are derived from explicit skill-load events
The `session-metrics` capability SHALL treat `skill.load` as the canonical event source for
`used_skills`, while preserving `unknown` when no explicit or compatible legacy evidence exists.

#### Scenario: Session contains skill-load events
- **WHEN** the normalized event stream for a session contains one or more `skill.load` events
- **THEN** `used_skills.identifiers` SHALL be derived from those events
- **AND** repeated loads of the same skill within one session MUST be deduplicated in
  `used_skills.identifiers` and `used_skills.count`

#### Scenario: Session does not contain skill-load events
- **WHEN** the session has no `skill.load` events and no compatible legacy fallback evidence
- **THEN** `used_skills` SHALL remain `unknown`
- **AND** the system MUST NOT derive `used_skills` from startup `Available skills`, quoted
  `skills_instructions` or arbitrary shell output

#### Scenario: Legacy shell-input fallback is needed for older logs
- **WHEN** a historical session lacks `skill.load` events but contains explicit shell/tool input
  opening `.../SKILL.md`
- **THEN** the system MAY use that fallback as compatibility evidence for `used_skills`
- **AND** this fallback MUST have lower precedence than real `skill.load` events when both are
  present

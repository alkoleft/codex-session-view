## ADDED Requirements

### Requirement: Session log emits explicit skill-load events
The normalized session log SHALL expose a dedicated `skill.load` event type for factual skill
activation/load evidence instead of inferring this fact only from generic shell/tool payloads.

#### Scenario: Explicit skill marker is present in a session message
- **WHEN** a normalized session message contains an explicit `<skill>...</skill>` block with a
  concrete skill identifier
- **THEN** the reader SHALL emit a separate `skill.load` event for that identifier
- **AND** the emitted event MUST remain distinct from the surrounding `message.*` event text

#### Scenario: Quoted available-skills text is present without a skill marker
- **WHEN** a message contains quoted `skills_instructions`, `### Available skills` or other session
  context text without an explicit `<skill>...</skill>` marker
- **THEN** the reader MUST NOT emit `skill.load` from that text
- **AND** the system MUST treat such text as context, not as factual skill activation evidence

### Requirement: Skill-load fallback is limited to explicit shell input
The system SHALL keep any compatibility fallback for skill loading narrowly scoped to shell/tool
input that explicitly opens a skill file, and MUST NOT derive `skill.load` from arbitrary output.

#### Scenario: Shell command explicitly opens a skill file
- **WHEN** a normalized shell/tool input explicitly references `.../SKILL.md` as the target being
  opened or read
- **THEN** the reader MAY emit `skill.load` or an equivalent compatibility signal for that skill
- **AND** this fallback MUST preserve the same identifier that would be used by the explicit marker

#### Scenario: Shell output only quotes or lists skill file paths
- **WHEN** shell stdout, aggregated output or search results merely contain one or more quoted
  `.../SKILL.md` paths
- **THEN** the reader MUST NOT emit `skill.load` from that output alone
- **AND** the system MUST avoid treating search hits, documentation excerpts or copied context as
  usage evidence

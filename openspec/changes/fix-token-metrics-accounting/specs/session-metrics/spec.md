## ADDED Requirements

### Requirement: Public metric groups have audited completeness semantics
The system SHALL define explicit completeness semantics for every public metric group in
`SessionMetrics` and `ProjectMetricsResponse`.

#### Scenario: Metric group is backed by current sources
- **WHEN** normalized events, operation projection or indexed metadata provide enough evidence to
  calculate a public metric group
- **THEN** the system MUST return a computed value with the correct `source` and `coverage`
- **AND** the implementation MUST NOT leave that metric group permanently `unknown` without a
  documented source limitation

#### Scenario: Metric group is not backed by current sources
- **WHEN** the current event/model sources do not provide enough evidence to calculate a public
  metric group reliably
- **THEN** the system MUST keep that metric group `partial` or `unknown`
- **AND** the unsupported status MUST be documented rather than hidden behind synthetic zeroes or
  misleading labels

### Requirement: Session token aggregation uses scope-aware snapshots
The system SHALL derive session-level token totals from the latest cumulative token snapshot of
each relevant thread or scope instead of using one globally last `info.tokens` event.

#### Scenario: Multi-thread session exposes multiple token snapshots
- **WHEN** a session contains `info.tokens` snapshots for a root thread and one or more child
  threads
- **THEN** the session token ledger MUST aggregate the latest cumulative snapshot from each relevant
  scope without double-counting older snapshots from the same scope
- **AND** the resulting session totals MUST remain traceable to the same spawn-agent contribution
  model used by task facts

#### Scenario: Scope identity is incomplete
- **WHEN** token events exist but one or more snapshots cannot be assigned to a reliable thread or
  scope boundary
- **THEN** the system MUST preserve known token values it can attribute safely
- **AND** any ambiguous contribution MUST remain `partial` or `unknown` instead of being silently
  merged into confirmed totals

## MODIFIED Requirements

### Requirement: Metric Coverage

Система SHALL сопровождать каждую public metric group состоянием coverage: known, partial или
unknown, причём coverage MUST отражать фактическую полноту реализованного источника, а не только
наличие поля в контракте.

#### Scenario: Metric group is implemented from available evidence

- **WHEN** система действительно умеет вычислить metric group из доступных источников
- **THEN** она SHALL вернуть значение этой группы
- **AND** coverage SHALL отражать полноту используемого источника как `known` или `partial`

#### Scenario: Metric group is declared but not yet supported

- **WHEN** metric group присутствует в публичном контракте, но текущие источники или реализация не
  позволяют посчитать её надёжно
- **THEN** система SHALL вернуть `unknown`
- **AND** не должна подменять этот статус synthetic нулём или выдавать группу как fully supported

### Requirement: Project Token Ledger

Система SHALL предоставлять канонический token ledger для session-level и project-level чтения с
явным общим счётчиком `all tokens` и разрезами input, output, cached tokens, reasoning output, tool
call, task и spawn agent, если такие данные доступны в источниках.

#### Scenario: Project token totals are requested

- **WHEN** потребитель запрашивает token ledger проекта за временное окно
- **THEN** система SHALL вернуть `all tokens` как общий total и отдельные разрезы input, output,
  cached tokens, reasoning output, tool call, task и spawn agent
- **AND** каждый разрез SHALL указывать source и coverage

#### Scenario: Token breakdown is partially unavailable

- **WHEN** источник содержит только общий `tokens_used` без детального breakdown
- **THEN** система SHALL вернуть `all tokens` как общий total
- **AND** недоступные разрезы SHALL быть неизвестными, а не нулевыми

## ADDED Requirements

### Requirement: Task Metrics Use A Stable Analytic Grain
Система SHALL строить task-level метрики как отдельный аналитический grain, а не только как session-level totals.

#### Scenario: Task identity is available
- **WHEN** нормализованные события содержат достаточные task boundaries и identifiers
- **THEN** система SHALL создать отдельную task metrics запись
- **AND** запись SHALL иметь стабильный аналитический ключ, пригодный для повторного расчёта

#### Scenario: Task identity is incomplete
- **WHEN** лог не даёт достаточных данных для устойчивой task identity
- **THEN** система SHALL не синтезировать произвольный task fact
- **AND** зависимые task metrics SHALL оставаться unavailable или unknown

### Requirement: Task Metrics Capture Raw Task Signals
Система SHALL сохранять raw task signals, которые описывают контекст задачи и пригодны для последующей классификации.

#### Scenario: Raw semantic signals are present
- **WHEN** task events содержат `agent_role`, `requested_agent_type`, `receiver_role`, `collaboration_mode_kind` или аналогичные поля
- **THEN** task metrics SHALL сохранить эти raw signals отдельно
- **AND** система MUST NOT подменять их финальной semantic classification в рамках этого capability

### Requirement: Task Metrics Capture Operational Breakdown
Система SHALL считать по каждой задаче token, duration, tool, MCP и spawn-agent breakdown, если источники это позволяют.

#### Scenario: Task aggregates are available
- **WHEN** task boundaries и operation/token events позволяют посчитать breakdown
- **THEN** task metrics SHALL вернуть token dimensions, duration, tool/MCP counts и spawn-agent contribution
- **AND** каждая metric group SHALL иметь `coverage` и `source`

#### Scenario: Task aggregates are partially unavailable
- **WHEN** часть breakdown signals отсутствует
- **THEN** система SHALL вернуть доступные task metrics
- **AND** недоступные task metrics SHALL быть `unknown`, а не `0`

### Requirement: Task Metrics Are Readable By Session And Project
Система SHALL поддерживать чтение task metrics как минимум в разрезе session и project.

#### Scenario: Session task metrics are requested
- **WHEN** потребитель запрашивает task metrics для session
- **THEN** система SHALL вернуть task facts, относящиеся к этой session
- **AND** порядок task facts SHALL быть устойчивым по task lifecycle или timestamp

#### Scenario: Project task metrics are requested
- **WHEN** потребитель запрашивает task metrics для project и окна времени
- **THEN** система SHALL вернуть task facts matching sessions проекта
- **AND** эти task facts SHALL быть пригодны для последующих аналитических агрегатов

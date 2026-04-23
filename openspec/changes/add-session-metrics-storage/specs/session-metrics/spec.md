## ADDED Requirements

### Requirement: Session Metrics Projection

Система SHALL строить метрики сессии из канонических `EventRecord`, session metadata и operation projection, не выполняя повторный независимый парсинг raw Codex-логов.

#### Scenario: Metrics are computed from normalized events

- **WHEN** session replay возвращает нормализованные события и operation metadata
- **THEN** система создаёт session metrics из этих нормализованных данных
- **AND** raw log parsing не используется как отдельный источник истины для метрик

#### Scenario: Operation metadata is partially unavailable

- **WHEN** сессия содержит старые события без operation metadata
- **THEN** система SHALL возвращать доступные метрики, не зависящие от operation metadata
- **AND** operation-dependent метрики SHALL помечаться как неизвестные, а не как нулевые

### Requirement: Metric Coverage

Система SHALL сопровождать каждую metric group состоянием coverage: known, partial или unknown.

#### Scenario: Metric source is complete

- **WHEN** источник содержит достаточные данные для расчёта metric group
- **THEN** система SHALL вернуть значение метрики
- **AND** coverage SHALL быть `known`

#### Scenario: Metric source is incomplete

- **WHEN** источник содержит только часть данных для metric group
- **THEN** система SHALL вернуть доступное значение или breakdown
- **AND** coverage SHALL быть `partial`

#### Scenario: Metric source is missing

- **WHEN** источник не содержит данных для metric group
- **THEN** система SHALL вернуть `unknown`
- **AND** система MUST NOT подставлять ноль вместо отсутствующих данных

### Requirement: Project Identity

Система SHALL привязывать session metrics к project identity, чтобы метрики можно было агрегировать и сравнивать в разрезе проектов.

#### Scenario: Project metadata is available

- **WHEN** session metadata содержит `cwd` и git metadata
- **THEN** система SHALL вычислить стабильный `project_key`
- **AND** сохранить исходные `cwd`, `git_origin_url`, `git_branch` и `git_sha` рядом с записью метрик

#### Scenario: Project metadata is incomplete

- **WHEN** session metadata не содержит достаточных project fields
- **THEN** система SHALL сохранить session metrics в degraded project bucket
- **AND** такой bucket SHALL быть явно отличим от нормальных project aggregates

### Requirement: Materialized Metrics Storage

Система SHALL хранить materialized session metrics так, чтобы повторное чтение и project-level сравнение не требовали полного replay каждой сессии.

#### Scenario: Session metrics are stored

- **WHEN** система рассчитала metrics для сессии
- **THEN** она SHALL сохранить одну актуальную запись metrics для этой `session_id`
- **AND** запись SHALL содержать `metrics_schema_version`, timestamps, project identity и source metadata

#### Scenario: Metrics are recomputed

- **WHEN** metrics schema или projection logic меняется
- **THEN** система SHALL иметь возможность пересчитать materialized metrics
- **AND** stale records SHALL быть отличимы по версии схемы или projection version

### Requirement: Degradation Signals

Система SHALL предоставлять базовые показатели, достаточные для ручного поиска деградаций по проектам и временным окнам.

#### Scenario: Baseline metrics are available

- **WHEN** метрики сессии запрошены для проекта или временного окна
- **THEN** система SHALL вернуть duration, event count, thread count, message count, operation count, tool breakdown, failure count, error count, abort count и token totals при наличии данных

#### Scenario: Token source is ambiguous

- **WHEN** token usage доступен из нескольких источников
- **THEN** система SHALL указать источник token metric
- **AND** система MUST NOT суммировать разные token sources без явного правила приоритета

### Requirement: Outcome and Duration Breakdown

Система SHALL хранить outcome и duration breakdown для session-level и operation-level метрик, когда timestamps и operation metadata позволяют это сделать.

#### Scenario: Duration breakdown is available

- **WHEN** session events содержат timestamps и operation boundaries
- **THEN** система SHALL вернуть total duration, generation/model duration, tool/shell/MCP duration и spawn-agent duration при наличии
- **AND** неатрибутируемый остаток SHALL быть сохранён как unknown или idle gap

#### Scenario: Operation outcome is available

- **WHEN** operation metadata содержит terminal status или error information
- **THEN** система SHALL классифицировать outcome как completed, failed, aborted, interrupted или unknown
- **AND** error outcome SHALL сохранять low-cardinality error type, если он доступен

### Requirement: Project Token Ledger

Система SHALL предоставлять итоговый счётчик потребления токенов по проекту с разрезами input, output, cached input, tool call, task и spawn agent, если такие данные доступны в источниках.

#### Scenario: Project token totals are requested

- **WHEN** потребитель запрашивает token ledger проекта за временное окно
- **THEN** система SHALL вернуть total tokens и разрезы input, output, cached input, tool call, task и spawn agent
- **AND** каждый разрез SHALL указывать source и coverage

#### Scenario: Token breakdown is partially unavailable

- **WHEN** источник содержит только общий `tokens_used` без детального breakdown
- **THEN** система SHALL вернуть общий total
- **AND** недоступные разрезы SHALL быть неизвестными, а не нулевыми

### Requirement: Session Chronology

Система SHALL предоставлять метрики сессий в хронологическом порядке и поддерживать переключатель учёта spawn-agent вклада.

#### Scenario: Sessions are listed chronologically

- **WHEN** потребитель запрашивает session metrics для проекта
- **THEN** система SHALL вернуть сессии в хронологическом порядке по времени начала или доступному session timestamp
- **AND** каждая запись SHALL содержать основные totals и факторы влияния

#### Scenario: Spawn agent contribution is excluded

- **WHEN** потребитель запрашивает метрики с `include_spawn_agents=false`
- **THEN** система SHALL исключить spawn-agent contribution из session totals
- **AND** сохранить отдельные поля, показывающие исключённый вклад

### Requirement: Factor Metadata

Система SHALL сохранять факторы влияния, которые помогают объяснить деградации и улучшения AI-agent workflow.

#### Scenario: Model and reasoning metadata are available

- **WHEN** session metadata содержит model, reasoning effort, agent role, CLI version, sandbox или approval mode
- **THEN** система SHALL сохранить эти значения в metrics record
- **AND** project/session aggregate SHALL позволять группировать или фильтровать метрики по этим факторам

#### Scenario: Context and integration metadata are available

- **WHEN** события или metadata содержат размер стартового контекста, количество skills или MCP-интеграции
- **THEN** система SHALL сохранить start context size, skills count, MCP server count и MCP call count
- **AND** отсутствующие значения SHALL быть неизвестными, а не нулевыми

### Requirement: Context Growth and Compaction Metrics

Система SHALL учитывать рост контекста и compaction-события как факторы деградаций, если эти данные присутствуют в нормализованных событиях.

#### Scenario: Context size is available

- **WHEN** events или metadata содержат start context size, token window или context-size snapshots
- **THEN** система SHALL сохранить стартовый размер контекста и последующие context-size indicators
- **AND** metrics SHALL позволять сравнивать context growth между сессиями

#### Scenario: Compaction events are available

- **WHEN** session events содержат context compaction markers
- **THEN** система SHALL посчитать количество compaction events
- **AND** metrics SHALL сохранять связь compaction с token usage и session timeline при наличии timestamps

### Requirement: Task-Level Metrics

Система SHALL считать метрики внутри сессии в разрезе task/turn/agent-work item, когда такие границы можно определить из нормализованных событий.

#### Scenario: Task boundaries are available

- **WHEN** normalized events содержат task, turn или agent-work boundaries
- **THEN** система SHALL вернуть metrics breakdown по этим boundaries
- **AND** суммы task-level metrics SHALL быть трассируемы к session-level totals

#### Scenario: Task boundaries are unavailable

- **WHEN** session events не содержат надёжных task boundaries
- **THEN** система SHALL вернуть session-level metrics без synthetic task split
- **AND** task-level breakdown SHALL быть помечен как unavailable

### Requirement: Tool Command Taxonomy

Система SHALL классифицировать tool usage по типам команд, пригодным для анализа деградаций.

#### Scenario: Tool commands are classified

- **WHEN** session metrics рассчитываются по operation metadata
- **THEN** система SHALL классифицировать операции минимум в категории search, edit, web_search, test, build, git, filesystem, mcp, collaboration и other
- **AND** category totals SHALL включать counts, failures и duration/token contribution при наличии

#### Scenario: Shell command category is ambiguous

- **WHEN** shell command невозможно надёжно классифицировать
- **THEN** система SHALL отнести операцию в `other`
- **AND** не должна подменять ambiguity произвольной категорией

### Requirement: Business Review Metrics

Система SHALL считать business metrics, связанные с review workflow, чтобы оценивать качество работы AI agents по сессиям.

#### Scenario: Review markers are available

- **WHEN** session events содержат распознанные review cycles, review findings или comments
- **THEN** система SHALL вернуть количество review cycles и количество замечаний
- **AND** эти значения SHALL быть доступны в session-level и project-level aggregates

#### Scenario: Review markers are unavailable

- **WHEN** review workflow не распознан в session events
- **THEN** система SHALL пометить business review metrics как unknown
- **AND** не должна возвращать ноль без подтверждённого источника markers

### Requirement: Feedback and Evaluator Metrics

Система SHALL поддерживать отдельные metric groups для feedback, evaluator и guardrail signals, не смешивая их с техническими ошибками.

#### Scenario: Quality markers are available

- **WHEN** session events содержат feedback score, evaluator result, guardrail triggered или handoff markers
- **THEN** система SHALL сохранить эти значения как отдельные quality metrics
- **AND** project aggregates SHALL позволять группировать сессии по этим quality metrics

#### Scenario: Quality markers are unavailable

- **WHEN** explicit quality markers отсутствуют
- **THEN** система SHALL вернуть quality metrics как unknown
- **AND** система MUST NOT выводить качество сессии из одного факта отсутствия технических ошибок

### Requirement: Regression and Efficiency Metrics

Система SHALL предоставлять derived metrics для поиска деградаций и улучшений только поверх достаточно покрытых базовых метрик.

#### Scenario: Baseline comparison is requested

- **WHEN** потребитель запрашивает сравнение текущего окна с baseline
- **THEN** система SHALL вернуть differences для token usage, duration, error rate, outcome rate и tool category distribution
- **AND** каждая difference SHALL наследовать минимальный coverage своих входных метрик

#### Scenario: Efficiency metric inputs are covered

- **WHEN** базовые metrics для success outcome, token usage, accepted task или review findings имеют достаточный coverage
- **THEN** система SHALL посчитать efficiency metrics вроде tokens per successful session, tokens per accepted task и review findings per 1k tokens
- **AND** при недостаточном coverage derived metric SHALL быть unknown

### Requirement: Consumer API

Система SHALL предоставить стабильный read contract для потребителей метрик, включая `codex-session-explorer` и будущий CLI/export.

#### Scenario: Explorer requests session metrics

- **WHEN** `codex-session-explorer` открывает выбранную сессию
- **THEN** backend SHALL вернуть metrics payload по стабильному контракту
- **AND** frontend SHALL отображать неизвестные значения отдельно от нулевых

#### Scenario: Project metrics are requested

- **WHEN** потребитель запрашивает метрики проекта за временное окно
- **THEN** система SHALL вернуть агрегированные показатели проекта и список contributing sessions
- **AND** ответ SHALL позволять сравнить текущее окно с другим окном без повторного чтения raw logs

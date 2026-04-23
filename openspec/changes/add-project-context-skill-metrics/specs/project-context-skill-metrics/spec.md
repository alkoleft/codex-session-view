## ADDED Requirements

### Requirement: Start Context Size Uses Stable Source Precedence
Система SHALL вычислять размер стартового контекста сессии по фиксированной цепочке источников, чтобы project metrics сравнивали одно и то же значение между сессиями.

#### Scenario: Explicit start context is present
- **WHEN** нормализованные события содержат явный `start_context_size` в `runtime.context` или эквивалентном payload field
- **THEN** система SHALL использовать это значение как `start_context_size`
- **AND** она MUST NOT заменять его значением из более поздних token snapshots

#### Scenario: Start context falls back to the first non-empty token snapshot
- **WHEN** явный `start_context_size` отсутствует
- **AND** в сессии есть `info.tokens` события с непустыми `input_tokens` или `total_tokens`
- **THEN** система SHALL использовать первое непустое token значение как размер стартового контекста
- **AND** более поздние cumulative token snapshots MUST NOT перезаписывать это значение

#### Scenario: Start context source is missing
- **WHEN** ни явный start-context field, ни пригодные token snapshots не найдены
- **THEN** `start_context_size` SHALL иметь coverage `unknown`
- **AND** система MUST NOT возвращать `0` как замену отсутствующих данных

### Requirement: Session Metrics Capture Enabled Skills and MCP Counts
Система SHALL сохранять для каждой сессии количество skills и MCP servers, включённых в runtime context, чтобы эти факторы можно было выводить и агрегировать по проекту.

#### Scenario: Runtime context contains enabled integrations
- **WHEN** нормализованный `runtime.context` payload содержит массивы `skills` и `mcp_servers`
- **THEN** система SHALL сохранить `skills_count` и `mcp_server_count` для сессии
- **AND** эти значения SHALL быть доступны в session metrics и project metrics response

#### Scenario: Runtime context does not contain enabled integrations
- **WHEN** в сессии отсутствуют массивы `skills` и `mcp_servers`
- **THEN** `skills_count` и `mcp_server_count` SHALL иметь coverage `unknown`
- **AND** система MUST NOT подставлять ноль вместо отсутствующих runtime arrays

### Requirement: Project Metrics Expose Full Token Ledger
Система SHALL выводить по проекту token metrics не только как `total`, но и как отдельные dimensions `input`, `output`, `cached_input`, `reasoning_output`, `task` и `spawn_agent`.

#### Scenario: Project token metrics are requested
- **WHEN** потребитель запрашивает project metrics для выбранного окна
- **THEN** ответ SHALL содержать project-level token ledger с полями `total`, `input`, `output`, `cached_input`, `reasoning_output`, `task` и `spawn_agent`
- **AND** каждая token metric SHALL сохранять собственные `coverage` и `source`

#### Scenario: Token dimensions are partially unavailable
- **WHEN** лог содержит только часть token dimensions
- **THEN** система SHALL вернуть доступные dimensions
- **AND** недостающие dimensions SHALL иметь `unknown`, а не `0`

### Requirement: Spawn-Agent Aggregation Is Explicit
Система SHALL уметь считать project/session metrics как с учётом `spawn agents`, так и без него, без двойного счёта.

#### Scenario: Spawn-agent contribution is included
- **WHEN** запрос metrics выполнен с `include_spawn_agents=true`
- **THEN** session-level и project-level totals SHALL суммировать вклад root session и дочерних `spawn agents`
- **AND** token, duration и счётчики операций SHALL отражать суммарное значение по выбранному окну

#### Scenario: Spawn-agent contribution is excluded
- **WHEN** запрос metrics выполнен с `include_spawn_agents=false`
- **THEN** session-level и project-level totals SHALL исключать вклад `spawn agents`
- **AND** система SHALL сохранять отдельный breakdown, показывающий исключённый вклад

### Requirement: Project Metrics Expose Session Task and Spawn Counts
Система SHALL выводить по проекту агрегированные session-level `task_count` и `spawn_agent_calls`, чтобы можно было сравнивать orchestration complexity между сессиями и окнами.

#### Scenario: Session task and spawn metrics are available
- **WHEN** session metrics рассчитаны для сессий проекта
- **THEN** project metrics response SHALL включать `task_count` и `spawn_agent_calls` как агрегируемые показатели
- **AND** session chronology SHALL сохранять эти значения для каждой сессии

#### Scenario: Task or spawn counts are unavailable
- **WHEN** источник не даёт надёжного `task_count` или `spawn_agent_calls`
- **THEN** эти metrics SHALL иметь `unknown`
- **AND** система MUST NOT выводить synthetic zero вместо отсутствующих данных

### Requirement: Session Metrics Capture Used Skills Conservatively
Система SHALL считать used skills как отдельную metric group только по явным usage markers, а не по произвольным эвристикам над свободным текстом.

#### Scenario: Explicit used-skill markers are present
- **WHEN** нормализованные события содержат явный marker использования skill
- **THEN** система SHALL сохранить уникальный список использованных skill identifiers для сессии
- **AND** metric group SHALL включать coverage/source и не дублировать один и тот же skill identifier внутри сессии

#### Scenario: Used-skill markers are absent
- **WHEN** сессия не содержит явных markers использования skill
- **THEN** used skills SHALL иметь coverage `unknown`
- **AND** система MUST NOT выводить used skills только из configured/enabled skills list

### Requirement: Project Metrics Expose Context, Token and Integration Factors
Система SHALL выводить по проекту временной ряд и агрегаты для стартового контекста, token breakdown, session-level task/spawn counts, enabled skills, enabled MCP servers и used skills в выбранном окне.

#### Scenario: Project metrics are requested
- **WHEN** потребитель запрашивает project metrics для `project_key` и временного окна
- **THEN** ответ SHALL содержать session chronology с `start_context_size`, `skills_count`, `mcp_server_count`, token metrics, `task_count` и `spawn_agent_calls`
- **AND** ответ SHALL содержать project-level rollup по used skills для matching sessions

#### Scenario: Project metrics include unknown factor values
- **WHEN** часть сессий не имеет данных по start context, token dimensions, session-level task/spawn counts, enabled integrations или used skills
- **THEN** project output SHALL сохранять coverage этих метрик
- **AND** unknown values MUST NOT искажаться в project summaries как нули

### Requirement: Project UI Shows New Metrics Without Hiding Unknown States
Система SHALL выводить новые project/session metrics на project metrics экране так, чтобы пользователь видел и динамику по сессиям, и неизвестные значения.

#### Scenario: Numeric factor metrics are available
- **WHEN** project metrics screen получает данные с `start_context_size`, `skills_count`, `mcp_server_count`, token breakdown, `task_count` и `spawn_agent_calls`
- **THEN** экран SHALL позволять смотреть эти метрики по сессиям в рамках project analytics
- **AND** пользователь SHALL видеть, какие точки имеют `known`, `partial` или `unknown` coverage

#### Scenario: Used skills are available for the selected window
- **WHEN** project metrics response содержит used skills rollup
- **THEN** экран SHALL вывести отдельный список или таблицу использованных skills
- **AND** для каждого skill SHALL быть видно хотя бы identifier и количество связанных сессий или usage events

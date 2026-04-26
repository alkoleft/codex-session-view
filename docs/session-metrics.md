# Метрики сессий

## Назначение

Слой `session_metrics` в `crates/codex-log` фиксирует агрегированные показатели Codex-сессий для
аналитики по проектам, временным окнам и факторам влияния. Он строится поверх уже
нормализованных `EventRecord`, `EventTree`, operation projection и indexed session metadata.
Отдельного parser raw Codex-логов для метрик нет.

## Версии

- `metrics_schema_version` описывает публичную форму metrics payload.
- `source_projection_version` описывает версию логики расчёта.
- Версии сохраняются в materialized payload для диагностики и миграций storage-формы.
- Изменение правил `task_class` не приводит к version negotiation в read path: уже сохранённые
  materialized metrics остаются источником истины до явного recompute.

## Project identity

`ProjectIdentity` хранит:

- `project_key`;
- состояние `normal` или `degraded`;
- исходные `cwd`, `git_origin_url`, `git_branch`, `git_sha`;
- нормализованный `cwd`, если он доступен.

Нормальный project bucket строится из нормализованного `cwd` и доступной git metadata. Если
`cwd` отсутствует, сессия попадает в явный degraded bucket. Такие сессии не смешиваются с
нормальными project aggregates.

## Coverage model

Каждая группа метрик возвращает `CoveredMetric<T>`:

- `value` содержит значение или `null`;
- `coverage` принимает `known`, `partial` или `unknown`;
- `source` фиксирует источник: `normalized_events`, `operation_projection`, `event_tree`,
  `indexed_session_metadata`, `session_metadata`, `derived` или `unavailable`.

`unknown` не равен нулю. Если источник отсутствует, значение остаётся `null`.

## Outcome taxonomy

Session outcome принимает значения:

- `completed`;
- `failed`;
- `aborted`;
- `interrupted`;
- `unknown`.

При наличии error metadata сохраняется низкокардинальный `error_type`.

## Token ledger

Token ledger содержит:

- `total`;
- `input`;
- `output`;
- `cached_input`;
- `reasoning_output`;
- `tool_call`;
- `task`;
- `spawn_agent`.

Для session-level totals слой метрик использует scope-aware схему:

- если `info.tokens` привязаны к `thread_id`, берётся последний накопительный snapshot на каждый
  thread/scope и затем значения суммируются по всем подтверждённым scope;
- `spawn_agent` считается как сумма child-thread snapshots с подтверждённым `parent_thread_id`;
- `task` считается как `total - spawn_agent`, когда иерархия scope известна достаточно надёжно;
- если одновременно есть scoped и unscoped token snapshots, подтверждённые scoped значения
  сохраняются, но coverage по этим полям становится `partial`;
- если scoped snapshots нет вообще, используется последний unscoped snapshot с `partial`
  coverage;
- если есть только `tokens_used` из indexed metadata, заполняется общий `total` с `partial`
  coverage, а недоступные разрезы остаются `unknown`.

`tool_call` пока не вычисляется из текущих источников и остаётся `unknown`.

## Duration breakdown

Duration breakdown содержит:

- `total_ms`;
- `generation_ms`;
- `tool_ms`;
- `shell_ms`;
- `mcp_ms`;
- `spawn_agent_ms`;
- `idle_unknown_ms`.

Общая длительность считается по timestamp первого и последнего нормализованного события.
Operation-level разрезы считаются только при наличии start/terminal boundaries в operation
projection. Неатрибутируемый остаток сохраняется как `idle_unknown_ms`.

## Tool taxonomy

Операции классифицируются в категории:

- `search`;
- `edit`;
- `web_search`;
- `test`;
- `build`;
- `git`;
- `filesystem`;
- `mcp`;
- `collaboration`;
- `other`.

Ambiguous shell-команды попадают в `other`.

## Task classification

Task-level facts (`task_facts[]`) теперь дополнительно хранят:

- `task_class`;
- `task_class_source`;
- `task_class_confidence`.

Стабильные semantic classes:

- `implementation`;
- `review`;
- `analysis`;
- `planning`;
- `approval`;
- `unknown`.

Текущие materialized mappings поверх raw task signals:

- `collaboration_mode_kind=planning|plan` -> `planning`;
- `collaboration_mode_kind=review|review_loop` -> `review`;
- `collaboration_mode_kind=approval` -> `approval`;
- `agent_role|requested_agent_type|receiver_role=worker|implementation|implementer` -> `implementation`;
- `agent_role|requested_agent_type|receiver_role=reviewer|review` -> `review`;
- `agent_role|requested_agent_type|receiver_role=explorer|docs_researcher|analysis|research|researcher` -> `analysis`;
- `agent_role|requested_agent_type|receiver_role=planner|planning|plan` -> `planning`;
- `agent_role|requested_agent_type|receiver_role=approver|approval` -> `approval`.

Confidence semantics:

- `confident` для однозначного materialized mapping;
- `partial` для конфликтующих сигналов, когда система сохраняет `task_class=unknown`;
- `unknown` когда пригодных сигналов нет.

Амбивалентные кейсы не получают synthetic уверенную classification. Если raw signals ведут к разным
semantic classes, система оставляет `task_class=unknown` и фиксирует `task_class_source=ambiguous_signals`.

## Business, context и quality groups

Business review metrics содержат `review_cycles` и `review_findings`. Значения считаются только
по распознанным markers; при отсутствии markers возвращается `unknown`.

Context metrics содержат стартовый размер контекста, рост контекста и количество compaction
events. Для стартового контекста используется первое непустое событие `info.tokens` (с приоритетом
`input_tokens`, затем `total_tokens`), если в runtime metadata нет более явного поля.
Если доверенного источника нет, соответствующая метрика остаётся `unknown`.

Quality groups для feedback, evaluator, guardrail и handoff зарезервированы как отдельные группы
и не выводятся из факта отсутствия технических ошибок.

## Derived metrics и baseline

Derived efficiency metrics считаются только при достаточном coverage входных метрик. Если входы
не покрыты, derived metric возвращается как `unknown`.

Baseline comparison сейчас представлен в контракте как отдельная группа с inherited coverage.
Заполнение фактических deltas должно выполняться только поверх materialized metrics с покрытыми
базовыми значениями.

## Storage

Первый materialized storage boundary находится вне Codex-owned `state_*.sqlite`.
`codex-session-explorer` пишет метрики в:

```text
CODEX_HOME/codex-session-explorer/session-metrics.sqlite
```

Таблица хранит одну актуальную запись на `session_id`, project key, timestamps, версии схемы и
полный JSON payload метрик. Upsert обновляет запись только при явной materialization/recompute
операции.

## Consumer API

Backend/Tauri contract предоставляет:

- session metrics при загрузке полной сессии;
- отдельную команду чтения metrics выбранной сессии;
- project metrics query по `project_key`, временному окну и `include_spawn_agents`.
- явную команду `recompute_metrics` для полного или project-scoped пересчёта materialized metrics.

Operational expectations:

- read path использует уже materialized payload и не пересчитывает существующие session metrics
  только из-за обновления task-classification rules;
- если запись для сессии отсутствует, она материализуется при первом чтении;
- после изменения правил classification нужно один раз запустить `recompute_metrics`, прежде чем
  полагаться на project/task analytics;
- `recompute_metrics` можно ограничить конкретным `project_key`, если полный rebuild не нужен.

Frontend показывает backend-provided metrics, если они есть. Локальная агрегация в React
сохраняется только как compatibility path для сессий без backend metrics.

## Аудит покрытия публичных метрик

### SessionMetrics

| Группа | Источник | Семантика coverage | Статус |
| --- | --- | --- | --- |
| `event_count`, `message_count`, `error_count`, `abort_count`, `failure_count` | normalized events | `known`, если события есть в нормализованной ленте | implemented |
| `outcome` | terminal session events | `known` для явного terminal marker, иначе `unknown` | implemented |
| `thread_count` | `EventTree` или distinct `thread_id` | `known` через tree, `partial` через fallback distinct count | implemented |
| `factors.model`, `reasoning_effort`, `cli_version`, `sandbox_policy_kind`, `approval_mode`, `agent_role` | indexed session metadata | `known`, если поле есть в summary; иначе `unknown` | implemented |
| `factors.skills_count` | первый session message с `### Available skills` -> fallback `runtime_context.skills` | `known` при найденном стартовом списке skills, иначе `unknown` | implemented |
| `factors.mcp_server_count` | `runtime_context.mcp_servers` | `known` по первому доступному списку, иначе `unknown` | implemented |
| `factors.mcp_call_count` | normalized events | `known` по числу `mcp.call` events | implemented |
| `factors.start_context_size`, `context.start_context_size` | `runtime_context` -> first non-empty `info.tokens` -> legacy event field | `known` при найденном доверенном источнике, иначе `unknown` | implemented |
| `operations.*` | operation projection | `known`, если есть projection snapshots; иначе `unknown` | implemented |
| `duration.total_ms` | timestamps первого и последнего события | `known`, если обе границы распознаны | implemented |
| `duration.tool_ms`, `shell_ms`, `mcp_ms`, `spawn_agent_ms`, `idle_unknown_ms` | operation projection + derived remainder | `partial`, потому что покрывают только атрибутируемую часть длительности | partial-by-source |
| `duration.generation_ms` | нет источника в текущей нормализации | всегда `unknown` | unsupported-with-current-sources |
| `token_ledger.total`, `input`, `output`, `cached_input`, `reasoning_output` | scope-aware `info.tokens`; fallback `tokens_used` | `known` для полностью scoped snapshots, `partial` при unscoped/fallback cases | implemented |
| `token_ledger.task`, `spawn_agent` | derived from scoped token snapshots и thread hierarchy | `known` при подтверждённой иерархии, `partial` при ambiguous scope, `unknown` без snapshot | implemented |
| `token_ledger.tool_call` | нет отдельного token source per tool category | всегда `unknown` | unsupported-with-current-sources |
| `tool_breakdown.count`, `failures` | operation projection | `known` по counted snapshots | implemented |
| `tool_breakdown.duration_ms` | operation projection durations | `partial` | partial-by-source |
| `tool_breakdown.token_contribution` | нет per-tool token ledger в текущих событиях | всегда `unknown` | unsupported-with-current-sources |
| `task_metrics.task_count`, `turn_count` | normalized events | `known`, если есть соответствующие markers; иначе `unknown` | implemented |
| `task_metrics.agent_work_item_count` | `EventTree` или distinct `thread_id` | `known` через tree, `partial` через fallback distinct count | implemented |
| `task_facts[]` operations/duration/token ledger | task boundaries + operation projection + scoped token snapshots | `known` для закрытых интервалов, `partial`/`unknown` для open boundaries | implemented |
| `used_skills` | explicit `skill_identifiers` signals | `known`, если есть явные usage markers; иначе `unknown` | implemented |
| `business_review.review_cycles`, `review_findings` | markers в normalized events и `EventTree` | `partial`, потому что счётчики marker-based | partial-by-source |
| `context.context_growth` | нет канонического delta source | всегда `unknown` | unsupported-with-current-sources |
| `context.compaction_events`, `context.context_compression` | normalized events | `known` по числу compaction markers | implemented |
| `quality.*` | нет поддержанного источника | всегда `unknown` | unsupported-with-current-sources |
| `baseline.*` | требует materialized baseline window, пока не реализован | всегда `unknown` | unsupported-with-current-sources |
| `derived_efficiency.tokens_per_successful_session` | derived from session total + outcome | `known` только для completed session с известным total | implemented |
| `derived_efficiency.review_findings_per_1k_tokens` | derived from review findings + total tokens | `partial`, потому что зависит от marker-based review group | partial-by-source |
| `derived_efficiency.tokens_per_accepted_task` | нет accepted-task source of truth в текущем payload | всегда `unknown` | unsupported-with-current-sources |

### ProjectMetricsResponse

| Группа | Источник | Семантика coverage | Статус |
| --- | --- | --- | --- |
| `session_count`, `contributing_session_ids`, `scope_filter`, `available_scope_counts`, `sessions[]` | materialized session rows | структурные поля без отдельного coverage | implemented |
| `token_ledger.*` | сумма session-level token ledger | `known`, если все contributing sessions `known`; иначе `partial` | implemented |
| `duration_ms` | сумма `sessions[].duration.total_ms` | `known`/`partial` по session coverage | implemented |
| `factors.start_context_size`, `skills_count`, `mcp_server_count` | сумма session rollups | `known`/`partial` по session coverage | implemented |
| `operations.spawn_agent_calls` | сумма session rollups | `known`/`partial` по session coverage | implemented |
| `task_metrics.task_count` | сумма session rollups | `known`/`partial` по session coverage | implemented |
| `task_facts[]` | объединение session task facts без доп. вычислений | зависит от coverage каждого факта | implemented |
| `used_skills` | derived union по session `used_skills` | `known`, если все contributing sessions дали known usage; иначе `partial` | implemented |
| `baseline.*` | project baseline ещё не материализован | всегда `unknown` | unsupported-with-current-sources |
| `derived_efficiency.*` | project-level derived rollups пока не собраны поверх materialized sessions | всегда `unknown` | unsupported-with-current-sources |

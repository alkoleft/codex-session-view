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
- `tool_call`;
- `task`;
- `spawn_agent`.

При наличии `info.tokens` используются детальные значения из последнего непустого
нормализованного token snapshot. Эти поля в логе уже накопительные, поэтому слой метрик не
суммирует несколько `info.tokens` между собой. Если есть только `tokens_used` из indexed
metadata, заполняется общий `total` с `partial` coverage, а недоступные разрезы остаются
`unknown`.

Дополнительно token ledger теперь выделяет `reasoning_output` как отдельный разрез.

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

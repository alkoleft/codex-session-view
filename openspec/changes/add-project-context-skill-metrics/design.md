## Context

В `crates/codex-log/src/session_metrics.rs` уже есть базовый слой materialized session metrics:

- `build_factor_metadata(...)` считает `skills_count`, `mcp_server_count`, `mcp_call_count` и `start_context_size`;
- `start_context_size` уже использует fallback к первому непустому `info.tokens` snapshot через `first_nonempty_tokens_start_context(...)`;
- `TokenLedger` уже содержит `total`, `input`, `output`, `cached_input`, `reasoning_output`, `tool_call`, `task` и `spawn_agent`, но project-level aggregation сейчас доводит только `total`;
- `TaskMetrics` уже содержит `task_count`, а `OperationMetrics` уже содержит `spawn_agent_calls`, но эти значения не выведены как project-level aggregates и не участвуют в project UI;
- `aggregate_project_metrics(...)` сейчас агрегирует по проекту только `token_ledger.total` и `duration.total_ms`;
- `ProjectMetricsScreen` строит project charts только для duration/tokens/failures/tool calls/derived metrics и не выводит factor metrics;
- доменная модель не хранит used skills ни на уровне сессии, ни на уровне проекта.

Следствие: часть нужных источников уже есть в коде, но они не оформлены как явный project-level контракт и не покрывают usage конкретных skills. Этот change должен остаться узким: расширить session/project metrics и не смешивать этот слой с task-level grain, task classification или сменой storage backend.

Ограничения проекта:

- пояснительная документация пишется по-русски;
- если меняется нормализация событий, `event_type`, `payload` или правила dedup/merge, обязательно обновляется `docs/log-events.md`;
- текущий `session_metrics` store уже хранит payload как JSON, поэтому additive поля в модели не требуют немедленной физической SQL-миграции.

## Goals / Non-Goals

**Goals:**

- Довести start-context/MCP/skills metrics до project-level чтения и вывода.
- Довести до project-level полный token ledger: `input`, `output`, `cached_input`, `reasoning_output`, `task`, `spawn_agent` и `total`.
- Зафиксировать, как project/session metrics считаются при `include_spawn_agents=true`, включая суммирование вкладов дочерних агентов.
- Довести до project-level output агрегаты `task_count` и `spawn_agent_calls`.
- Зафиксировать единый source precedence для стартового контекста сессии.
- Добавить отдельный контракт для used skills с `unknown`, если явного источника нет.
- Сделать новые поля пригодными для session view, project view и последующих агрегатов без подмены отсутствующих данных нулями.

**Non-Goals:**

- Не вводить task-level grain или task classification в рамках этого change.
- Не менять storage backend и не делать storage abstraction в рамках этого change.
- Не угадывать used skills по свободному тексту без строгого распознаваемого marker-а.
- Не менять существующие token/cost правила за пределами стартового контекста.

## Decisions

### 1. Расширяем текущий session/project metrics contract без смены grain

Решение: оставить `SessionMetrics` источником истины для factor metrics и расширить `ProjectMetricsResponse` агрегатами/rollup-полями, которые вычисляются из уже materialized session entries.

Причина: `SessionMetricsStore::list_project_sessions(...)` уже читает сессии проекта в хронологическом порядке, а storage-слой хранит весь payload целиком. Новая ветка вычислений не требует отдельной task fact model.

Альтернатива: одновременно вводить task-level grain и session/project rollups. Это размоет scope и усложнит проверку change.

### 2. Project-level token ledger обязан агрегировать все доступные token dimensions

Решение: `ProjectMetricsResponse` должен агрегировать не только `token_ledger.total`, но и `input`, `output`, `cached_input`, `reasoning_output`, `task` и `spawn_agent`, сохраняя `coverage/source` для каждого поля.

Причина: project contract должен отвечать на вопрос про полный token breakdown без ручного обхода каждой session row.

Альтернатива: оставить project-level только `total`, а breakdown читать из отдельных session rows. Это неудобно для project window analytics.

### 3. `include_spawn_agents` трактуется как режим агрегации, а не только UI-фильтр

Решение: при `include_spawn_agents=true` project/session aggregates должны суммировать вклад root session и дочерних `spawn agents`; при `false` root totals и project rollups должны исключать `spawn_agent` contribution, но показывать исключённый вклад отдельным полем.

Причина: для стоимости и нагрузки пользователь хочет уметь считать метрики с учётом и без учёта дочерних агентов.

Альтернатива: всегда показывать только root totals или всегда включать дочерних агентов. Оба варианта скрывают важный аналитический срез.

### 4. Источник стартового контекста фиксируется как explicit precedence chain

Решение: считать `start_context_size` в таком порядке:

1. явный `runtime.context.context.start_context_size`;
2. первое непустое `info.tokens.input_tokens`;
3. первое непустое `info.tokens.total_tokens`;
4. legacy payload fields `start_context_size` / `context_size`;
5. `unknown`, если ни один источник не найден.

Причина: такой порядок уже частично отражён в `build_factor_metadata(...)` и тестах `compute_session_metrics`, где стартовый контекст берётся из первого непустого token snapshot.

Альтернатива: использовать только явный `start_context_size`. Это потеряет часть исторических логов.

### 5. Enabled skills и MCP считаются из runtime context, а used skills остаются консервативной метрикой

Решение: counts включённых `skills` и `mcp_servers` продолжают извлекаться из `runtime.context`, а used skills хранятся отдельно и считаются только по явным usage markers.

Причина: enabled integrations и used skills отвечают на разные вопросы и не должны смешиваться в одну метрику.

Альтернатива: выводить только enabled skills count. Это не отвечает на вопрос, какие skills реально участвовали в сессиях.

### 6. Project UI строится на существующем response shape и добавляет session/project series

Решение: `ProjectMetricsScreen` получает новые series keys как минимум для `start_context_size`, `skills_count`, `mcp_server_count`, `task_count`, `spawn_agent_calls` и token breakdown series (`input/output/cached/reasoning`), а used skills отображаются отдельным блоком.

Причина: числовые factor metrics естественно ложатся в уже существующий chart pipeline, а used skills являются категориальным набором и требуют отдельного представления.

Альтернатива: свалить всё в summary cards. Это не даёт динамики по сессиям.

## Risks / Trade-offs

- [Risk] Исторические логи могут не содержать ни `runtime.context`, ни пригодных token snapshots. -> Mitigation: `start_context_size`, enabled counts и used skills возвращаются как `unknown`, без synthetic нулей.
- [Risk] Не все логи содержат полный token breakdown (`input/output/cached/reasoning/task/spawn`). -> Mitigation: project ledger агрегирует каждый dimension независимо и сохраняет `unknown`, если источник по dimension отсутствует.
- [Risk] Суммирование `spawn agents` может задвоить вклад, если смешать root totals и дочерние session totals без правила. -> Mitigation: зафиксировать явный режим агрегации `include_spawn_agents` и покрыть его тестами на session/project level.
- [Risk] Usage markers skills могут отсутствовать в текущей нормализации. -> Mitigation: сначала проверить live payloads/tests; если marker-а нет, добавить его в `codex-log` и синхронно обновить `docs/log-events.md`.
- [Risk] Project screen перегрузится дополнительными series. -> Mitigation: новые factor series делать secondary по умолчанию, а used skills выводить отдельно от основных chart toggles.

## Migration Plan

1. Расширить доменную модель `SessionMetrics`/`ProjectMetricsResponse` полным project token ledger, агрегатами `task_count`/`spawn_agent_calls`, factor rollups и used-skill metrics с backward-compatible defaults.
2. Обновить extraction logic в `codex-log`: source precedence для стартового контекста, project rollups для token/task/spawn/context counts и used skills extraction.
3. При необходимости добавить normalized usage marker в readers/projector и обновить `docs/log-events.md`.
4. Расширить backend types и transport для `codex-session-explorer`.
5. Добавить project UI series/cards/table для token breakdown, task/spawn counts, start context, enabled skills, enabled MCP и used skills.
6. Обновить unit/UI tests и прогнать recompute path для metrics rows через существующий stale/rebuild flow.

## Open Questions

- Есть ли в реальных логах уже стабильный marker skill usage, или change должен сам вводить его в нормализацию?
- Нужен ли для project rollup `usage_count`, `session_count`, или оба показателя по used skills?
- Должен ли project screen показывать distinct enabled skills по проекту, или достаточно distinct used skills плюс session-level enabled counts?

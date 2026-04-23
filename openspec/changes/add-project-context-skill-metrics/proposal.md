## Why

Текущий слой `session_metrics` уже умеет считать часть нужных показателей для отдельной сессии, но project-level аналитика всё ещё неполная: нет полного token breakdown по окнам и проектам, не доведены счётчики `task` и `spawn agents`, а факторы стартового контекста, MCP и skills не оформлены как единый устойчивый контракт. Из-за этого по проекту нельзя надёжно сравнивать, как меняются стоимость и структура запуска: `input/output/cached/reasoning` tokens, вклад `spawn agents`, размер стартового контекста, число tasks, MCP/skills и реально использованные skills.

## What Changes

- Добавить в project-level metrics контракт агрегаты и вывод по размеру стартового контекста сессии, где источником считается первое непустое token-событие, если явный `start_context_size` отсутствует.
- Добавить project/session token ledger с отдельными метриками `input`, `output`, `cached_input`, `reasoning_output`, а также режимом агрегации с учётом `spawn agents`.
- Зафиксировать project/session metrics для количества MCP-серверов и skills, включённых в сессию, без подмены `unknown` нулями.
- Довести до project-level контракта агрегаты `task_count` и `spawn_agent_calls`, чтобы по проекту были видны и временной ряд, и totals по окну.
- Добавить учёт использованных skills как отдельного metric group: список уникальных skills, их session/project counts и coverage.
- Довести новые project/session metrics до materialized storage, backend API и project metrics UI.
- Зафиксировать правила source precedence и деградированных состояний для cases, где session log не содержит явных runtime arrays или usage markers.

## Capabilities

### New Capabilities
- `project-context-skill-metrics`: project/session metrics для стартового контекста, полного token ledger, агрегатов `task`/`spawn agent`, включённых MCP/skills и использованных skills.

### Modified Capabilities

Нет существующих capability specs в `openspec/specs/`.

## Impact

- `crates/codex-log`: расширение модели `SessionMetrics`/`ProjectMetricsResponse`, extraction logic и materialized aggregation.
- `crates/codex-session-explorer-backend`: чтение и выдача новых project metrics полей.
- `apps/codex-session-explorer`: project/session metrics UI, filters/cards/tables для новых факторов.
- `docs/log-events.md`: обновление обязательно, если реализация вводит новые нормализованные event fields, usage markers или меняет mapping payload.

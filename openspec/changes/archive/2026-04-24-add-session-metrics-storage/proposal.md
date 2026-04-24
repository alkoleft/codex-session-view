## Why

Сейчас `codex-session-view` умеет читать, нормализовать и показывать отдельные Codex-сессии, но не фиксирует агрегированные метрики по проектам. Из-за этого сложно увидеть деградации, рост стоимости, изменение длительности, частоты ошибок или влияния отдельных факторов между сериями запусков.

## What Changes

- Добавить слой сбора и хранения метрик по сессиям с обязательной привязкой к проекту.
- Считать метрики из уже нормализованной модели логов, не вводя второй независимый parser сырых Codex-файлов.
- Поддержать сравнение метрик между проектами, временными окнами и отдельными сессиями.
- Сохранять базовые показатели деградаций: длительность, количество событий, tool/shell/MCP активность, ошибки, abort/failure-состояния, token/cost-индикаторы при наличии данных.
- Вести итоговый token ledger по проекту: input, output, cached input, tool call, task-level и spawn-agent consumption, если источники позволяют выделить эти разрезы.
- Показывать метрики сессий в хронологическом порядке с переключателем включения/исключения spawn-agent вкладов.
- Фиксировать факторы влияния: model, reasoning effort, количество skills, количество MCP-серверов/вызовов и размер стартового контекста при наличии данных.
- Поддержать метрики внутри сессии в разрезе task/turn/agent-work item.
- Классифицировать tool usage по типам команд: поиск, редактирование, web search, тесты, сборка, git, файловые операции, MCP и прочие категории.
- Добавить business metrics для agent workflow: количество review циклов и замечаний по сессиям.
- Для каждой метрики хранить состояние достоверности: known, partial или unknown, чтобы не смешивать отсутствие данных с нулём.
- Добавить outcome taxonomy для сессий и операций: completed, failed, aborted, interrupted, unknown.
- Добавить duration breakdown: total, model/generation, tool/shell/MCP, spawn-agent contribution и idle/unknown gap при наличии данных.
- Учитывать context growth и compaction: стартовый контекст, изменение контекста, события compaction и их влияние на token usage.
- Зарезервировать слой feedback/evaluator/guardrail metrics для будущих явных markers качества.
- Поддержать derived efficiency metrics: tokens per successful session, tokens per accepted task, review findings per 1k tokens.
- Сделать метрики пригодными для будущего UI-дашборда и CLI/export-сценариев.

## Capabilities

### New Capabilities

- `session-metrics`: сбор, хранение и чтение агрегированных метрик Codex-сессий в разрезе проектов, временных окон и факторов влияния.

### Modified Capabilities

Нет существующих OpenSpec capabilities: `openspec/specs/` пока пуст.

## Impact

- `crates/codex-log`: расширение доменной модели поверх `EventRecord`, session catalog, replay и projection.
- `apps/codex-session-explorer`: будущий потребитель метрик для project/session analytics UI.
- `docs/log-events.md`: должен обновляться при изменении нормализации событий, payload, dedup/merge правил или маппинга event types.
- Возможная новая persistence-граница для materialized метрик, если реализации недостаточно in-memory projection.

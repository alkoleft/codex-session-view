## Context

Сейчас доменная модель метрик заканчивается на `SessionMetrics` и содержит только session-level `task_count`/`turn_count`, но не факт-записи по каждой задаче. При этом нормализованные события уже несут полезные task signals: `task.started`, `task.completed`, `task_id`, `turn_id`, `agent_role`, `requested_agent_type`, `receiver_role`, `collaboration_mode_kind`, operation breakdown и token snapshots.

Следствие: прежде чем обсуждать task classification, storage backend или UI-аналитику по типам задач, нужно зафиксировать базовый grain данных: что именно считается task entity и какие поля входят в её raw факт-модель.

## Goals / Non-Goals

**Goals:**

- Ввести отдельную task-level факт-модель метрик.
- Зафиксировать устойчивую task identity и правила grouping task events.
- Считать по task duration, token ledger, tool/MCP activity, spawn-agent contribution и outcome.
- Сохранять raw signals, нужные для будущей классификации task semantics.
- Не ломать существующий session-level metrics contract.

**Non-Goals:**

- Не вводить semantic `task_class` в рамках этого change.
- Не менять storage backend в рамках этого change.
- Не проектировать финальный UI для task analytics.

## Decisions

### 1. Task metrics вводятся как отдельный grain, а не как вложенный JSON-комментарий внутри session totals

Решение: считать `task_metrics_fact` логическим новым слоем модели, где одна запись соответствует одной аналитической задаче внутри session/project.

Причина: token/duration/tool breakdown по task плохо выражается через один session summary и нужен как отдельная единица агрегации.

Альтернатива: хранить только session-level totals и иногда восстанавливать task breakdown на лету. Это делает аналитику хрупкой и дорогой.

### 2. Task grain строится из нормализованных `task.started` / `task.completed` и связанных event boundaries

Решение: task identity формируется из доступных raw identifiers (`task_id`, `turn_id`, `thread_id`, session scope) и стабильного аналитического ключа, который можно восстановить при повторном расчёте.

Причина: эти сигналы уже есть в текущем event model и позволяют опираться на факты, а не на текстовые эвристики.

Альтернатива: строить task grain по свободному тексту сообщений или только по thread roots. Это слишком неоднозначно.

### 3. Task fact хранит raw semantic signals, но не финальную классификацию

Решение: task-level модель должна сохранять `agent_role`, `requested_agent_type`, `receiver_role`, `collaboration_mode_kind` и другие raw markers отдельно, не превращая их в `task_class` в рамках этого change.

Причина: классификация задач должна жить отдельным change и иметь собственные правила пересчёта.

Альтернатива: сразу смешать grain и classification. Это усложнит проверку correctness и размоет boundaries.

### 4. Token и spawn breakdown по task считаются first-class fields

Решение: task fact включает token dimensions (`input`, `output`, `cached_input`, `reasoning`, `total`), duration, tool/MCP counts и spawn-agent contribution.

Причина: именно эти поля потом нужны для медиан, rolling averages и сравнения типов задач.

Альтернатива: ограничиться только task count. Это не даёт аналитической ценности.

## Risks / Trade-offs

- [Risk] В старых логах task boundaries могут быть неполными. -> Mitigation: task fact возвращает `unknown` coverage для зависимых полей и не синтезирует фиктивные задачи.
- [Risk] Один `task_id` может не быть достаточным уникальным ключом сам по себе. -> Mitigation: строить composite analytic key из session/task/turn/thread context.
- [Risk] Слишком ранняя попытка включить classification размоет scope. -> Mitigation: оставить raw semantic signals в task fact, а classification вынести в отдельный change.

## Migration Plan

1. Добавить доменную task-level модель метрик и backward-compatible transport types.
2. Зафиксировать task identity и extraction rules из нормализованных событий.
3. Добавить расчёт token/duration/tool/MCP/spawn breakdown по task.
4. Подключить чтение task metrics для session/project queries.
5. Добавить unit tests на task grouping и task-level aggregates.

## Open Questions

- Какой analytic key считать canonical: `session_id + task_id`, `session_id + turn_id` или composite с `thread_id`?
- Нужен ли отдельный task outcome taxonomy, отличный от session outcome, уже в первом шаге?
- Нужно ли сразу materialize task facts отдельно от session metrics, или допустим общий store с двумя grains?

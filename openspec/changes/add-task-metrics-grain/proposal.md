## Why

Session-level aggregates перестают быть достаточными, если хочется анализировать работу агентов на уровне отдельных задач: где именно выросли токены, какие типы задач дольше всего живут и какой вклад вносят дочерние агенты. Без отдельного task grain любая такая аналитика превращается в эвристику поверх session totals.

## What Changes

- Добавить отдельную task-level модель метрик поверх уже нормализованных событий и operation projection.
- Зафиксировать устойчивый task grain для аналитики: identity, timestamps, outcome, token/duration/tool/MCP/spawn breakdown.
- Сохранять raw task signals, пригодные для последующей классификации, не смешивая их с итоговой semantic classification.
- Поддержать чтение task metrics в разрезе session и project без изменения существующего session-level контракта.

## Capabilities

### New Capabilities
- `task-metrics-grain`: task-level факт-модель метрик поверх нормализованных событий Codex-сессий.

### Modified Capabilities

Нет существующих capability specs в `openspec/specs/`.

## Impact

- `crates/codex-log`: новая task-level metrics model и extraction logic.
- `crates/codex-session-explorer-backend`: чтение task metrics по session/project.
- Будущие changes на task classification и advanced analytics будут опираться на этот grain как на источник истины.

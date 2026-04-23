## Why

Контракт metrics уже вырос из простого session cache: есть project/session aggregates, планируется task-level grain и статистические вычисления по разным срезам. Если прибить этот слой к `SQLite`-специфичной реализации, дальнейший переход к более удобному analytics backend станет дорогим и рискованным.

## What Changes

- Выделить backend-agnostic abstraction для materialized metrics storage.
- Зафиксировать, что domain/model/read contracts не зависят от конкретного backend-а хранения.
- Оставить текущий `SQLite`-адаптер первым compatibility backend, но не делать его единственно допустимым вариантом.
- Подготовить границу для последующего добавления альтернативного analytics backend, не переписывая metrics model.

## Capabilities

### New Capabilities
- `metrics-storage-abstraction`: backend-agnostic contract для materialized metrics storage.

### Modified Capabilities

Нет существующих capability specs в `openspec/specs/`.

## Impact

- `crates/codex-log`: storage interfaces и adapters для materialized metrics.
- Будущие changes на analytics backend смогут опираться на стабильную storage boundary.
- Session/project/task metrics перестанут быть жёстко привязаны к текущему `SQLite` query path.

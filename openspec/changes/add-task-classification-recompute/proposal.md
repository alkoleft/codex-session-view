## Why

После появления task-level grain одной raw факт-модели недостаточно: нужно уметь различать задачи разработки, review и анализа, но без загрязнения read path версионированием классификатора. Если правила классификации меняются, система должна пересчитывать materialized task metrics явной командой, а не усложнять каждое чтение.

## What Changes

- Добавить semantic classification для task metrics: минимум `implementation`, `review`, `analysis`, `planning`, `approval`, `unknown`.
- Разделить raw task signals и итоговый `task_class`, чтобы аналитика работала по устойчивому semantic полю.
- Добавить для классификации поля происхождения и достоверности (`source`, `confidence`), не вводя classifier versioning в read path.
- Добавить отдельную команду пересчёта materialized task/session metrics, которую запускают после изменения правил классификации.

## Capabilities

### New Capabilities
- `task-classification-recompute`: semantic classification task metrics и явный recompute workflow после смены правил.

### Modified Capabilities

Нет существующих capability specs в `openspec/specs/`.

## Impact

- `crates/codex-log`: classifier logic, materialized task/session metrics rebuild.
- CLI/backend tooling: отдельная команда пересчёта metrics.
- Любая аналитика по task types и dashboard breakdown будет зависеть от этого capability.

## 1. Task Classification Model

- [x] 1.1 Добавить в task metrics поля `task_class`, `task_class_source` и `task_class_confidence`.
- [x] 1.2 Зафиксировать mappings из raw task signals в semantic classes `implementation`, `review`, `analysis`, `planning`, `approval`, `unknown`.
- [x] 1.3 Сохранить правило, что ambiguous cases не получают synthetic уверенную classification.

## 2. Recompute Workflow

- [x] 2.1 Добавить отдельную команду пересчёта materialized task/session metrics после изменения classification rules.
- [x] 2.2 Обновить materialization flow так, чтобы read path работал с уже пересчитанными task classes без classifier versioning.
- [x] 2.3 Зафиксировать operational expectations для recompute workflow в документации/контракте change.

## 3. Verification

- [x] 3.1 Добавить tests для task classification mappings и confidence semantics.
- [x] 3.2 Добавить tests для recompute command и обновления materialized metrics.
- [x] 3.3 Проверить change через `openspec validate add-task-classification-recompute --strict`.

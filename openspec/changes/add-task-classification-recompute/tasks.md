## 1. Task Classification Model

- [ ] 1.1 Добавить в task metrics поля `task_class`, `task_class_source` и `task_class_confidence`.
- [ ] 1.2 Зафиксировать mappings из raw task signals в semantic classes `implementation`, `review`, `analysis`, `planning`, `approval`, `unknown`.
- [ ] 1.3 Сохранить правило, что ambiguous cases не получают synthetic уверенную classification.

## 2. Recompute Workflow

- [ ] 2.1 Добавить отдельную команду пересчёта materialized task/session metrics после изменения classification rules.
- [ ] 2.2 Обновить materialization flow так, чтобы read path работал с уже пересчитанными task classes без classifier versioning.
- [ ] 2.3 Зафиксировать operational expectations для recompute workflow в документации/контракте change.

## 3. Verification

- [ ] 3.1 Добавить tests для task classification mappings и confidence semantics.
- [ ] 3.2 Добавить tests для recompute command и обновления materialized metrics.
- [ ] 3.3 Проверить change через `openspec validate add-task-classification-recompute --strict`.

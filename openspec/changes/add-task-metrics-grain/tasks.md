## 1. Task Grain Model

- [ ] 1.1 Добавить доменную task-level модель метрик и transport types без смешивания с финальной task classification.
- [ ] 1.2 Зафиксировать canonical analytic key для task fact и правила grouping task events.
- [ ] 1.3 Добавить raw task signals (`agent_role`, `requested_agent_type`, `receiver_role`, `collaboration_mode_kind` и аналоги) в task fact model.

## 2. Task Aggregation

- [ ] 2.1 Реализовать расчёт token, duration, tool/MCP и spawn breakdown по task.
- [ ] 2.2 Поддержать чтение task facts в разрезе session и project.
- [ ] 2.3 Сохранить `unknown` semantics для task metrics, где raw signals недостаточно.

## 3. Verification

- [ ] 3.1 Добавить unit tests на task identity, grouping и task-level aggregates.
- [ ] 3.2 Добавить backend contract tests на чтение task metrics по session/project.
- [ ] 3.3 Проверить change через `openspec validate add-task-metrics-grain --strict`.

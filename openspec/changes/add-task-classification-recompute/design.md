## Context

Task-level grain сам по себе хранит raw task signals, но не отвечает на вопрос, является ли задача `implementation`, `review` или `analysis`. При этом versioning classifier-а в read path усложняет чтение и делает API тяжелее. Пользовательский выбор зафиксирован: при смене правил классификации выполняется явный пересчёт отдельной командой, а не вводятся classifier versions в каждом read-запросе.

## Goals / Non-Goals

**Goals:**

- Ввести устойчивое поле `task_class` для аналитики.
- Сохранить разницу между raw signals и итоговой semantic classification.
- Сохранять происхождение и достоверность classification.
- Ввести явную команду пересчёта materialized metrics после изменения правил classification.

**Non-Goals:**

- Не вводить classifier versioning в read path.
- Не смешивать classification rules со storage backend choice.
- Не требовать автоматического recompute при каждом старте приложения.

## Decisions

### 1. `task_class` материализуется, а не вычисляется заново на каждом чтении

Решение: classification сохраняется в materialized task metrics и используется downstream analytics как обычное поле fact model.

Причина: task-level аналитика требует быстрых фильтров и group-by по semantic class.

Альтернатива: каждый раз вычислять `task_class` на чтении. Это усложнит queries и сделает результаты зависимыми от runtime classifier.

### 2. Никакого classifier versioning в read path

Решение: система не хранит и не требует classifier version для обычного чтения metrics.

Причина: это усложняет read contract и не соответствует зафиксированному пользователем решению.

Альтернатива: хранить `classifier_version` и разруливать stale reads на лету. Это тяжелее и для чтения, и для UX.

### 3. Изменение правил классификации требует явного recompute command

Решение: после изменения classification rules пользователь или automation запускает отдельную команду пересчёта materialized task/session metrics.

Причина: recompute — это операционный акт, а не обязанность каждого read path.

Альтернатива: lazy recompute на чтении. Это делает производительность и результаты менее предсказуемыми.

### 4. `task_class_source` и `task_class_confidence` обязательны

Решение: вместе с `task_class` сохраняются source/confidence, чтобы различать explicit mappings, strong role-based inference и uncertain heuristics.

Причина: semantic analytics без уровня доверия быстро превратится в ложную точность.

Альтернатива: хранить только `task_class`. Это скрывает качество классификации.

### 5. Recompute идёт отдельной командой `recompute_metrics`

Решение: backend/transport contract использует явную команду `recompute_metrics` с опциональным
`project_key`, а обычный read path читает уже materialized payload без classifier versioning.

Причина: пользователю нужен предсказуемый operational step после изменения classification rules,
а не скрытый lazy refresh на каждом чтении.

Альтернатива: пересчитывать существующие записи при любом read или пытаться договориться о версии
classifier-а в каждом query. Это усложняет контракт и делает результаты менее детерминированными.

## Risks / Trade-offs

- [Risk] Classification rules со временем изменятся и потребуют полного rebuild. -> Mitigation: сделать recompute command first-class частью capability.
- [Risk] Слабые heuristics дадут ложную точность. -> Mitigation: хранить `task_class_confidence` и оставлять ambiguous cases в `unknown`.
- [Risk] Пользователь забудет запустить recompute после смены правил. -> Mitigation: задокументировать этот operational step в tasks и CLI contract.

## Migration Plan

1. Добавить domain fields `task_class`, `task_class_source`, `task_class_confidence`.
2. Реализовать classifier rules поверх raw task signals.
3. Добавить команду явного пересчёта materialized metrics.
4. Добавить tests для classifier и recompute workflow.

## Operational Expectations

- Изменение task classification rules само по себе не обновляет уже сохранённые materialized metrics.
- `load_session_metrics` и `query_project_metrics` материализуют только отсутствующие записи.
- После изменения mappings automation или пользователь должны вызвать `recompute_metrics` до
  интерпретации project/task analytics.
- Если нужен узкий rebuild, recompute можно ограничить одним `project_key`.

## Open Questions

- Какие mappings считаются `known`, а какие только `partial`?
- Нужен ли отдельный dry-run/report режим для recompute command?

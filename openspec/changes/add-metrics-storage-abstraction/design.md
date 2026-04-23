## Context

Сейчас materialized `session_metrics` хранятся через конкретную `SQLite`-реализацию: таблица, индексы и query path живут рядом с доменной моделью. Пока это достаточно для простого cache/store, но будущие project/task analytics и статистические вычисления могут потребовать другой backend. Пользователь уже зафиксировал, что storage choice нужно отделить от самого metrics contract.

## Goals / Non-Goals

**Goals:**

- Ввести backend-agnostic интерфейс materialized metrics storage.
- Оставить текущий `SQLite` адаптер рабочим первым backend-ом.
- Отделить domain model и read/write contracts от конкретного backend-а.
- Подготовить безопасную границу для последующего добавления alternative analytics backend.

**Non-Goals:**

- Не мигрировать на новый backend в рамках этого change.
- Не смешивать storage abstraction с task classification rules.
- Не обещать немедленный `DuckDB`/`Parquet` swap в этом же change.

## Decisions

### 1. Storage abstraction появляется раньше backend migration

Решение: сначала вводится `MetricsStore`/`MetricsRepository` boundary, а уже потом при необходимости добавляется второй backend.

Причина: это позволяет разделить контракт и реализацию, не привязывая metrics model к одному storage path.

Альтернатива: сразу мигрировать на новый backend. Это расширяет scope и усложняет проверку.

### 2. `SQLite` остаётся первым compatibility backend

Решение: существующий `SQLite` store не выбрасывается, а становится первым адаптером абстракции.

Причина: это минимизирует риск и оставляет текущий flow рабочим.

Альтернатива: сначала удалить старый store и только потом проектировать abstraction. Это опасно и не даёт incremental path.

### 3. Domain contracts не должны знать про backend specifics

Решение: `SessionMetrics`, `ProjectMetricsResponse`, будущие task facts и read queries должны зависеть только от storage interface, а не от `SQLite` schema details.

Причина: иначе любой backend swap превратится в переписывание половины metrics stack.

Альтернатива: оставить schema/query details протекать вверх. Это делает abstraction фикцией.

## Risks / Trade-offs

- [Risk] Abstraction окажется слишком общей и ничего не упростит. -> Mitigation: проектировать её под реальные read/write patterns metrics layer.
- [Risk] Появится лишний уровень indirection без немедленной выгоды. -> Mitigation: сохранить change узким и не добавлять лишние adapters сейчас.
- [Risk] Будущие analytics-backend needs окажутся шире первого интерфейса. -> Mitigation: строить abstraction вокруг domain queries, а не вокруг SQLite API.

## Migration Plan

1. Выделить storage interface для materialized metrics.
2. Подключить существующий `SQLite` store как первый adapter.
3. Перевести metrics read/write paths на interface.
4. Добавить tests на adapter contract.

## Open Questions

- Какие queries должны войти в минимальный storage interface уже сейчас: session lookup, project range query, task query, recompute hooks?
- Нужен ли сразу отдельный config surface для выбора backend-а, или пока достаточно internal abstraction?
- Когда будет обоснованным добавить второй adapter: после task grain или только после появления реальных performance pain points?

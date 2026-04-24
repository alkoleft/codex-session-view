## 1. Storage Interface

- [x] 1.1 Выделить backend-agnostic storage interface для materialized metrics.
- [x] 1.2 Зафиксировать минимальный набор operations для session/project/task metrics и rebuild flows.
- [x] 1.3 Убедиться, что domain model и публичные read contracts не зависят от backend-specific schema details.

## 2. SQLite Adapter

- [x] 2.1 Подключить существующий `SQLite` metrics store как первый compatibility adapter.
- [x] 2.2 Перевести текущие read/write paths metrics layer на storage interface.
- [x] 2.3 Оставить change узким: без обязательной миграции на новый backend.

## 3. Verification

- [x] 3.1 Добавить tests на storage interface contract и SQLite adapter.
- [x] 3.2 Проверить, что session/project metrics продолжают читаться через abstraction без изменения публичного контракта.
- [x] 3.3 Проверить change через `openspec validate add-metrics-storage-abstraction --strict`.

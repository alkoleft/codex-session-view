## 1. Storage Interface

- [ ] 1.1 Выделить backend-agnostic storage interface для materialized metrics.
- [ ] 1.2 Зафиксировать минимальный набор operations для session/project/task metrics и rebuild flows.
- [ ] 1.3 Убедиться, что domain model и публичные read contracts не зависят от backend-specific schema details.

## 2. SQLite Adapter

- [ ] 2.1 Подключить существующий `SQLite` metrics store как первый compatibility adapter.
- [ ] 2.2 Перевести текущие read/write paths metrics layer на storage interface.
- [ ] 2.3 Оставить change узким: без обязательной миграции на новый backend.

## 3. Verification

- [ ] 3.1 Добавить tests на storage interface contract и SQLite adapter.
- [ ] 3.2 Проверить, что session/project metrics продолжают читаться через abstraction без изменения публичного контракта.
- [ ] 3.3 Проверить change через `openspec validate add-metrics-storage-abstraction --strict`.

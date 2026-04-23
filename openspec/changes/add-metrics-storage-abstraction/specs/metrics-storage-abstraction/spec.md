## ADDED Requirements

### Requirement: Metrics Storage Uses A Backend-Agnostic Interface
Система SHALL читать и записывать materialized metrics через backend-agnostic storage interface.

#### Scenario: Materialized metrics are written
- **WHEN** metrics layer сохраняет session, project или task metrics
- **THEN** запись SHALL идти через storage interface
- **AND** доменная модель MUST NOT зависеть от backend-specific SQL details

#### Scenario: Materialized metrics are read
- **WHEN** потребитель запрашивает session/project/task metrics
- **THEN** чтение SHALL выполняться через storage interface
- **AND** публичный metrics contract SHALL быть одинаковым независимо от backend-а

### Requirement: SQLite Remains The First Compatibility Backend
Система SHALL сохранить текущий `SQLite` store как первый рабочий adapter новой abstraction.

#### Scenario: Existing metrics flow continues to work
- **WHEN** storage abstraction введена
- **THEN** текущий `SQLite` backend SHALL оставаться рабочим adapter
- **AND** система MUST NOT требовать немедленной миграции на другой backend

### Requirement: Storage Abstraction Supports Future Backend Swap
Система SHALL задавать такую границу, чтобы в будущем можно было добавить новый backend, не переписывая metrics model.

#### Scenario: Alternative backend is introduced later
- **WHEN** проект добавляет второй metrics backend
- **THEN** domain model и публичные read contracts SHALL оставаться совместимыми
- **AND** backend-specific logic SHALL быть локализована внутри adapters

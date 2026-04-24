## ADDED Requirements

### Requirement: Local HTTP viewer application
Система SHALL предоставлять отдельное локальное HTTP приложение для `codex-session-explorer`,
которое запускается без Tauri WebView и открывается в браузере.

#### Scenario: Browser-based viewer startup
- **WHEN** пользователь запускает HTTP вариант `codex-session-explorer`
- **THEN** система MUST поднять локальный HTTP endpoint для viewer
- **THEN** пользователь MUST иметь возможность открыть приложение в браузере

### Requirement: Integrated frontend delivery
Система SHALL раздавать frontend `codex-session-explorer` из HTTP backend как встроенные
статические assets, чтобы browser-based вариант поставлялся как единый application surface.

#### Scenario: Frontend assets served by backend
- **WHEN** браузер открывает root route HTTP приложения
- **THEN** backend MUST отдать встроенный frontend entrypoint и связанные статические assets
- **THEN** frontend MUST работать без отдельного внешнего dev/prod frontend server

### Requirement: Reused viewer API contract
Система SHALL реализовывать HTTP API, совместимый с уже используемым frontend remote viewer
contract, чтобы один и тот же frontend мог работать поверх отдельного Rust backend.

#### Scenario: Session catalog and session loading over HTTP
- **WHEN** frontend запрашивает список сессий, preview, полную сессию или project metrics через
  `/api/viewer/*`
- **THEN** backend MUST вернуть ответы в формате, совместимом с `RemoteViewerBackendClient`
- **THEN** frontend MUST использовать этот API без отдельной ветки UI для HTTP приложения

### Requirement: Shared backend behavior across runtimes
Система SHALL переиспользовать общую Rust viewer backend-логику между Tauri и HTTP приложениями
для каталога сессий, загрузки сессии, preview и project metrics.

#### Scenario: Shared implementation for core viewer operations
- **WHEN** Tauri runtime и HTTP runtime выполняют одну и ту же viewer операцию
- **THEN** обе реализации MUST опираться на общий Rust backend слой
- **THEN** доменные правила загрузки и нормализации данных MUST не дублироваться в двух отдельных
  runtime-specific реализациях

### Requirement: Browser parity for core viewer workflows
Система SHALL сохранять в HTTP приложении основные пользовательские сценарии текущего
`codex-session-explorer`: просмотр каталога, загрузку выбранной сессии, session preview и project
metrics.

#### Scenario: Core workflows available in browser
- **WHEN** пользователь работает в browser-based приложении
- **THEN** он MUST иметь возможность выбрать сессию из каталога
- **THEN** система MUST позволить открыть preview и полное содержимое сессии
- **THEN** система MUST позволить открыть project metrics для выбранного проекта

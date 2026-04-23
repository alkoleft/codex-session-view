## 1. Shared Rust Backend Extraction

- [x] 1.1 Выделить общий Rust crate для viewer backend-логики без зависимости от Tauri runtime.
- [x] 1.2 Перенести в общий crate операции каталога сессий, preview, полной загрузки сессии и
  project metrics.
- [x] 1.3 Переключить `apps/codex-session-explorer/src-tauri` на использование общего backend слоя
  через thin adapter.

## 2. HTTP Application

- [x] 2.1 Создать отдельный Rust binary crate для локального HTTP `codex-session-explorer`.
- [x] 2.2 Реализовать same-origin HTTP routes для frontend assets и API `/api/viewer/*` поверх
  shared backend contract.
- [x] 2.3 Добавить конфигурацию запуска backend, включая выбор порта и стратегию определения
  `CODEX_HOME`.

## 3. Frontend Integration

- [x] 3.1 Подготовить сборку `apps/codex-session-explorer` для работы как встроенного frontend
  HTTP-приложения.
- [x] 3.2 Проверить, что frontend корректно работает в `remote` режиме через same-origin без
  обязательного внешнего `VITE_VIEWER_REMOTE_BASE_URL`.
- [x] 3.3 Явно обработать capability differences между Tauri и HTTP runtime без дублирования UI.

## 4. Validation and Docs

- [x] 4.1 Добавить Rust tests для shared backend и HTTP endpoints на ключевые viewer сценарии.
- [x] 4.2 Прогнать frontend tests для remote backend path и убедиться, что browser workflow не
  ломает существующий Tauri path.
- [x] 4.3 Обновить документацию запуска и production build для второго HTTP приложения.

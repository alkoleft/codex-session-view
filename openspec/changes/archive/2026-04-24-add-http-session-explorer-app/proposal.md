## Why

`codex-session-explorer` уже имеет разделение на frontend и backend contract, но основной рабочий
сценарий по-прежнему завязан на Tauri WebView. Из-за этого нельзя запустить тот же viewer как
обычное локальное HTTP-приложение в браузере, хотя большая часть логики уже пригодна к
переиспользованию.

## What Changes

- Добавить второе приложение `codex-session-explorer` без Tauri WebView: отдельный локальный Rust
  HTTP backend, который сам раздаёт встроенный frontend.
- Вынести общую viewer backend-логику из Tauri-слоя в переиспользуемый Rust-модуль, чтобы Tauri и
  HTTP варианты работали поверх одного доменного ядра.
- Зафиксировать HTTP API, совместимый с уже существующим frontend remote backend contract
  `/api/viewer/*`.
- Подготовить production build flow, в котором frontend собирается один раз и встраивается в
  HTTP-бинарник.
- Сохранить пользовательскую функциональность просмотра каталога сессий, загрузки сессии,
  preview и project metrics в браузерном приложении.

## Capabilities

### New Capabilities

- `http-session-explorer-app`: локальное браузерное приложение для просмотра Codex sessions на
  основе отдельного Rust HTTP backend со встроенным frontend и переиспользованием существующего
  viewer contract.

### Modified Capabilities

Нет существующих OpenSpec capabilities: `openspec/specs/` в этом репозитории пока пуст.

## Impact

- `crates/*`: выделение общего Rust backend-ядра viewer из Tauri-specific слоя.
- `apps/codex-session-explorer/src-tauri/*`: упрощение до thin adapter над общим backend.
- `apps/codex-session-explorer/src/*`: доведение frontend до стабильной работы поверх same-origin
  remote backend.
- `apps/*`: новый HTTP app crate для локального сервера и встраивания frontend assets.
- Сборка и документация запуска: команды build/run для browser-based варианта приложения.

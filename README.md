# codex-session-view

`codex-session-view` это монорепозиторий для работы с логами Codex-сессий. Это не инструмент запуска `codex`; текущий scope ограничен чтением, нормализацией и просмотром логов.

- `crates/codex-log` — Rust-библиотека для чтения, нормализации, replay и проекции логов;
- `apps/codex-session-explorer` — desktop-приложение `codex-session-explorer` для интерактивного просмотра этих логов.

Проект больше не содержит worker-runtime, root bin или task-runner сценариев. Вся активная разработка сосредоточена на логовом ядре и viewer.

## Быстрый старт

```bash
cargo test --workspace
npm --prefix apps/codex-session-explorer test
npm --prefix apps/codex-session-explorer run build
```

## Структура репозитория

- `crates/codex-log` содержит source of truth для `EventRecord`, readers, session catalog, replay и tree/view-model.
- `apps/codex-session-explorer/src-tauri` использует `codex-log` как backend-ядро для desktop viewer.
- `docs/` фиксирует актуальную модель логов, архитектуру и roadmap будущего CLI.

## Что дальше

Следующий архитектурный этап уже зарезервирован в документации: отдельный CLI для чтения логов и отправки их в базу данных. В этом репозитории он пока не реализован и не имеет crate/bin.

# План: `codex-session-view`

Дата: 2026-04-16

## 1. Текущее позиционирование

Репозиторий поддерживает только контур работы с логами Codex-сессий:

- `crates/codex-log` — библиотека для чтения, нормализации, replay и проекции логов;
- `apps/codex-session-explorer` — desktop-приложение `codex-session-explorer`.

Корневой package/bin, orchestration runtime и parity/acceptance-слой в этом репозитории отсутствуют и не должны возвращаться в scope.

## 2. Поддерживаемые части

### 2.1 `codex-log`

Пакет отвечает за:

- session discovery;
- чтение raw session/run файлов;
- нормализацию в `EventRecord`;
- operation stream, replay и tree/view-model;
- backend-утилиты для viewer и будущих логовых потребителей.

### 2.2 `codex-session-explorer`

Приложение отвечает за:

- выбор `CODEX_HOME`;
- список и preview сессий;
- загрузку полной сессии;
- live tail и визуализацию timeline.

### 2.3 CI

Единый CI-контур для workspace называется `workspace-checks` и покрывает Rust workspace, `codex-log` и frontend `codex-session-explorer`.

## 3. Текущие ограничения

В текущем scope не реализуются:

- отдельный CLI/bin для чтения логов;
- отправка логов в базу данных;
- схема БД, миграции и внешние storage-интеграции;
- orchestration runtime и запуск задач.

## 4. Следующий этап

Следующий архитектурный шаг уже зафиксирован: отдельный CLI для чтения логов и отправки их в БД. Он должен переиспользовать `codex-log`, но пока не имеет собственной реализации в workspace.

## 5. Базовые проверки

```bash
cargo test --workspace
cargo test -p codex-log
cargo test -p codex-session-explorer
npm --prefix apps/codex-session-explorer test
npm --prefix apps/codex-session-explorer run build
```

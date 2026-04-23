# codex-session-explorer

`codex-session-explorer` это UI для интерактивного разбора логов Codex-сессий. Один и тот же frontend теперь может работать в двух режимах: как Tauri desktop-приложение с локальным backend и как browser UI поверх отдельного same-origin HTTP backend, который сам раздаёт встроенный frontend и viewer API.

## Что внутри

- React + Vite frontend с Tailwind/shadcn shell.
- Tauri 2 shell с startup flash prevention и thin adapter над общим Rust viewer backend.
- Remote/browser режим через HTTP JSON backend без зависимости от Rust/Tauri API.
- Shared Rust viewer backend, который переиспользуется Tauri и HTTP runtime без дублирования доменной логики.
- `tauri-ui` batteries: external link guard и dev-only debug panel по `Cmd/Ctrl + D`.
- Единый frontend-контракт для каталога сессий, preview и полной загрузки сессии.
- Live tail доступен только в `tauri`-режиме; для `remote` это capability v2.
- Продуктовое имя приложения: `codex-session-explorer`.

## Скриншоты

### Основной экран

![Окно выбора сессии codex-session-explorer](../../docs/screenshots/codex-session-explorer-real-session-picker.png)

### Выбранная сессия

![Выбранная сессия codex-session-explorer](../../docs/screenshots/codex-session-explorer-real-selected-session.png)

## Примечание по scaffold

- Upstream `create-tauri-ui` требует `bun`.
- Этот app адаптирован под текущий repo и npm-based workflow, но использует ключевые runtime-паттерны и батарейки из upstream `tauri-ui`.

## Локальные команды

```bash
npm install
npm run dev
npm run build
npm run tauri:dev
npm run tauri:build
```

Для browser-based production surface frontend собирается один раз и затем встраивается в
`apps/codex-session-explorer-http`:

```bash
npm run build
cargo run -p codex-session-explorer-http -- --port 4321
```

## Режимы backend

### `tauri`

- Значение по умолчанию внутри Tauri runtime.
- Использует текущие Rust-команды `detect_codex_home`, `initialize_codex_home`, `list_indexed_sessions`, `load_session_preview`, `load_session_preview_by_id`, `load_session`, `tail_session`.
- Требует локальный `CODEX_HOME` и показывает локальные filesystem metadata.

### `remote`

- Значение по умолчанию вне Tauri runtime.
- Не требует `CODEX_HOME` и не показывает Tauri-specific path metadata в shell.
- В browser-based приложении обычно запускается same-origin поверх `codex-session-explorer-http`, поэтому `VITE_VIEWER_REMOTE_BASE_URL` не обязателен.
- Поддерживает список сессий, preview, полную загрузку сессии, session metrics и project metrics.
- Live tail и viewer commands остаются capability-gated и в browser runtime по умолчанию недоступны.

## Конфигурация frontend

- `VITE_VIEWER_BACKEND_MODE=tauri|remote` явно выбирает backend-режим.
- `VITE_VIEWER_REMOTE_BASE_URL=https://host` задаёт базовый URL внешнего viewer backend для режима `remote`.
- Если `VITE_VIEWER_BACKEND_MODE` не задан, frontend выбирает `tauri` внутри Tauri runtime и `remote` вне Tauri runtime.
- `VITE_VIEWER_REMOTE_BASE_URL` применяется только когда выбран `remote` backend; в `tauri`-режиме он игнорируется.
- Если frontend открыт из того же origin, что и HTTP backend, `RemoteViewerBackendClient` автоматически использует `window.location.origin`.

Пример browser/dev запуска со stub backend:

```bash
VITE_VIEWER_BACKEND_MODE=remote \
VITE_VIEWER_REMOTE_BASE_URL=http://127.0.0.1:4010 \
npm run dev
```

## Remote API v1

Frontend ожидает `POST` endpoint’ы:

- `/api/viewer/list_indexed_sessions`
- `/api/viewer/load_session_preview_by_id`
- `/api/viewer/load_session_preview`
- `/api/viewer/load_session`
- `/api/viewer/load_session_metrics`
- `/api/viewer/query_project_metrics`
- `/api/viewer/tail_session`

Требования к контракту:

- Request body отправляется в JSON со `snake_case` полями.
- Response body должен совпадать с текущими viewer-типами `IndexedSessionCatalogPage`, `SessionPreview`, `LoadedSession`.
- Ошибки возвращаются как `non-2xx`; желательно JSON вида `{ "message": "..." }`, но frontend также понимает plain text body.

## Ограничения browser/remote v1

- Нет live tail polling по умолчанию и подписки на viewer commands из Tauri window events.
- Внешние ссылки в browser-режиме открываются через browser-safe fallback.
- Remote backend должен сам поставлять уже нормализованные события во внутреннем формате viewer API; дополнительный mapping во frontend не добавляется.

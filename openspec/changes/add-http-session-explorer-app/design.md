## Context

В репозитории уже есть `apps/codex-session-explorer` с React/Vite frontend и Tauri backend на
Rust. Во frontend также присутствует `RemoteViewerBackendClient`, который умеет работать через
HTTP endpoints `/api/viewer/*`, а значит формат браузерного взаимодействия уже частично
зафиксирован в коде. При этом фактический Rust backend остаётся внутри `src-tauri`, поэтому второй
вариант приложения пока не существует как самостоятельный локальный HTTP server.

Пользовательский запрос жёстко задаёт архитектурную рамку:

- это должно быть именно второе приложение, а не Tauri WebView;
- backend должен быть отдельным Rust HTTP server;
- frontend должен быть максимально переиспользован и встроен в backend;
- основной сценарий запуска: локально открыть браузер и работать с тем же viewer.

## Goals / Non-Goals

**Goals:**

- Дать отдельный browser-based вариант `codex-session-explorer` без Tauri WebView.
- Переиспользовать существующий React frontend и текущий backend contract вместо новой UI/API
  линии.
- Вынести общую Rust viewer-логику из Tauri backend в независимый переиспользуемый модуль.
- Раздавать frontend assets из HTTP backend, чтобы приложение запускалось как один локальный
  сервис.
- Сохранить user-facing сценарии каталога сессий, preview, полной загрузки сессии и project
  metrics.

**Non-Goals:**

- Не заменять и не удалять Tauri приложение: оно остаётся отдельным runtime target.
- Не вводить новый frontend framework, новый API protocol или отдельный Node-based production
  server.
- Не добиваться byte-identical parity для Tauri-specific возможностей вроде viewer command bridge,
  если они завязаны на desktop runtime.
- Не менять event semantics, формат session artifacts или contract `codex-log`.

## Decisions

### 1. Общая доменная логика viewer выносится из Tauri app в отдельный Rust crate

Решение: извлечь `ViewerBackend` и связанную доменную логику чтения каталога, загрузки сессий,
project metrics и tail в отдельный crate, который не зависит от Tauri.

Причина: сейчас именно Tauri backend является единственной реальной реализацией viewer операций.
Если поверх него строить HTTP server без выделения общего слоя, получится дублирование логики и
расхождение поведения между двумя приложениями.

Альтернатива: оставить всю логику в `src-tauri` и вызывать её через внутренние адаптеры. Это
делает HTTP приложение зависимым от Tauri crate и ломает границу между runtime-specific и shared
кодом.

### 2. HTTP приложение реализуется как отдельный Rust binary crate

Решение: добавить новое приложение, например `apps/codex-session-explorer-http`, как отдельный
workspace member с собственным `main.rs`.

Причина: пользователь просит именно второе приложение. Отдельный binary target лучше отражает
границы поставки, упрощает сборку и не смешивает HTTP runtime с Tauri packaging.

Альтернатива: прятать HTTP mode за feature flag внутри `src-tauri`. Это уменьшает число crate, но
смешивает разные runtime-обязанности и усложняет release surface.

### 3. HTTP API закрепляется на уже существующем remote contract `/api/viewer/*`

Решение: новый backend реализует те же операции, которые уже ожидает `RemoteViewerBackendClient`:
`list_indexed_sessions`, `load_session_preview`, `load_session`, `load_session_metrics`,
`query_project_metrics`, `load_session_preview_by_id`, а поддержку `tail_session` фиксирует явно по
фактической готовности.

Причина: этот contract уже живёт во frontend коде, значит это самый короткий путь к
переиспользованию UI без разворота нового API.

Альтернатива: спроектировать новый REST API и потом переделывать frontend adapter. Это не даёт
пользы на данном этапе и увеличивает объём change.

### 4. Frontend собирается Vite build-ом и встраивается в HTTP бинарник

Решение: production build нового приложения должен включать шаг сборки `apps/codex-session-explorer`
и встраивание `dist/` в Rust binary как статических assets.

Причина: пользователь прямо потребовал, чтобы front был интегрирован в back. Это означает единый
локальный deliverable, а не два раздельных процесса.

Альтернатива: раздавать frontend отдельно через dev server или внешний nginx. Это удобно для
разработки, но не соответствует целевой форме приложения.

### 5. Same-origin browser режим становится базовым для remote frontend

Решение: frontend должен уметь работать без обязательного `VITE_VIEWER_REMOTE_BASE_URL`, если
открыт из того же origin, который отдаёт HTTP backend.

Причина: интегрированный backend+frontend должен работать по zero-config сценарию. Уже существующая
логика `resolveRemoteBaseUrl` этому соответствует, значит change должен сохранить и закрепить это
как целевой путь.

Альтернатива: требовать явный base URL даже для встроенного режима. Это ухудшает DX и создаёт
лишнюю конфигурацию там, где origin уже известен.

### 6. Tauri и HTTP runtime делят один контракт возможностей, но различаются capability flags

Решение: сохранить единый TypeScript contract `ViewerBackendClient`, а runtime-specific различия
отражать через `ViewerBackendCapabilities` и явное поведение адаптеров.

Причина: это уже принятая точка расширения в frontend. Благодаря этому один и тот же UI может
честно реагировать на отсутствие live tail или desktop command bridge, не разветвляясь в отдельные
экраны.

Альтернатива: делать отдельный frontend под HTTP runtime. Это противоречит требованию
максимального переиспользования.

## Risks / Trade-offs

- [Risk] Вынесение общего backend-слоя затронет существующий Tauri код и может вызвать регрессии.
  -> Mitigation: оставить Tauri слой thin adapter-ом и покрыть shared crate тестами на загрузку
  каталога, session preview и project metrics.
- [Risk] Интеграция frontend assets в Rust binary усложнит build pipeline.
  -> Mitigation: зафиксировать явный build step и отдельную команду/скрипт для production сборки.
- [Risk] HTTP runtime не сможет один-в-один повторить Tauri-specific возможности.
  -> Mitigation: закрепить parity для пользовательских viewer-сценариев и отдельно пометить
  desktop-only функции как non-goal или capability-gated behavior.
- [Risk] Same-origin режим может скрыть ошибки конфигурации при внешнем reverse proxy.
  -> Mitigation: сохранить явную поддержку `VITE_VIEWER_REMOTE_BASE_URL` для development и
  нестандартных развёртываний.

## Migration Plan

1. Выделить общий Rust crate viewer backend и переключить Tauri backend на его использование.
2. Создать новый HTTP app crate и реализовать same-origin API `/api/viewer/*` поверх shared crate.
3. Подготовить frontend build/output и встраивание `dist/` в HTTP бинарник.
4. Проверить browser workflow against local HTTP server и существующий Tauri workflow against shared
   backend.
5. Обновить документацию запуска и сборки для второго приложения.

Rollback: удалить новый HTTP app crate и вернуть Tauri к прежней in-app реализации, если общий
backend extraction окажется нестабильным. Frontend remote adapter при этом можно оставить, так как
он уже изолирован и не ломает desktop runtime.

## Open Questions

- Нужно ли в первой версии делать live tail по HTTP, или допустимо зафиксировать его как явное
  последующее расширение capability?
- Какой способ встраивания assets предпочтителен в этом репозитории: compile-time embed или
  упаковка рядом с бинарником с единым runner script?
- Нужен ли отдельный CLI/flag для выбора `CODEX_HOME` в HTTP приложении, или достаточно текущей
  стратегии autodetect плюс явный параметр запуска?

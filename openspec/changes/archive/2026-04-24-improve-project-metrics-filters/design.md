## Context

Текущий `ProjectMetricsScreen` уже умеет загружать `ProjectMetricsResponse`, но project selector
строится во frontend из `catalogSessions`, которые заполняются только текущей страницей
`listIndexedSessions`. При `SESSIONS_PAGE_SIZE` selector видит лишь часть каталога, поэтому проект
может отсутствовать в выборе, даже если backend уже знает его сессии и умеет посчитать по ним
метрики.

Вторая проблема в том, что project-level contract не несёт явной классификации сессии как
основной или дочерней. В `IndexedSessionSummary.source` уже встречается structured metadata с
`subagent.thread_spawn.parent_thread_id`, а в UI есть локальный парсер этого поля для экрана
деталей сессии, но `ProjectMetricsResponse.sessions[]` такого признака не содержит. Из-за этого
экран `Project metrics` не может честно фильтровать тренды по main/subsession и в текущем виде
смешивает корневые прогоны с вложенными subagent runs.

## Goals / Non-Goals

**Goals:**

- Сделать project selector полным для экрана `Project metrics`, независимо от пагинации основного
  session catalog.
- Ввести единый contract для классификации сессий по scope/type, пригодный и для project catalog,
  и для `query_project_metrics`.
- Дать пользователю фильтр `all` / `main` / `subsession`, который одинаково влияет на charts,
  summary cards и contributing sessions.
- Сохранить текущую семантику coverage и degraded project identity без подмены отсутствующих
  данных.

**Non-Goals:**

- Не переписывать основной session list UI и его пагинацию ради этой задачи.
- Не строить произвольную иерархию из всех thread depth на экране `Project metrics`; в первой
  версии нужен именно пользовательский срез `main` против `subsession`.
- Не менять существующую формулу `project_key` и не вводить новую project-level storage схему,
  если достаточно расширения существующих response/model.

## Decisions

### 1. Полный project selector выносится в отдельный backend catalog API

Решение: добавить отдельный backend endpoint/command для project catalog, который проходит по
всему индексированному каталогу и возвращает deduped project options для `Project metrics`.

Причина: автодогрузка всех страниц `listIndexedSessions` из UI привязывает selector к состоянию
основного списка, усложняет orchestration и плохо масштабируется на большом каталоге. Для экрана
метрик нужен отдельный источник истины: список проектов и связанный metadata summary.

Альтернатива: загружать все страницы `listIndexedSessions` во frontend и потом строить selector
локально. Это решает симптом, но оставляет экран зависимым от paginated session list и лишний раз
таскает весь session payload.

### 2. Классификация scope вводится как отдельная metadata-модель

Решение: в backend добавить явную классификацию сессии, например `main`, `subsession`, `unknown`,
которая вычисляется из indexed metadata. Сигналом `subsession` считается наличие structured
`source.subagent.thread_spawn.parent_thread_id`; отсутствие такого сигнала трактуется как `main`,
а неразбираемые/неполные кейсы допускаются как `unknown` для честного degraded-поведения.

Причина: фильтр должен опираться на один и тот же признак во всех слоях. Локальный UI-парсер
`source` в `App.tsx` уже показывает, что нужная информация присутствует, но она не доведена до
project-level contracts.

Альтернатива: определять subsession только по `agent_path` или по имени файла. Это слишком
хрупко и не является принятым источником истины.

### 3. `query_project_metrics` расширяется фильтром scope и возвращает scope в каждой сессии

Решение: расширить `SessionMetricsQuery` параметром фильтра scope и включить classification в
каждую `SessionMetrics`, чтобы backend мог отфильтровать набор contributing sessions до
агрегации, а frontend не пересчитывал totals поверх лишних данных.

Причина: пользователь ожидает, что фильтр main/subsession влияет на весь экран, а не только на
видимость строк. Поэтому summary cards, charts и contributing list должны строиться из одного
отфильтрованного набора сессий.

Альтернатива: загружать все project sessions и фильтровать их только во frontend. Это ломает
инвариант единого ответа backend для totals и series и усложняет честную агрегацию.

### 4. Session metrics storage обновляется через bump projection/schema version

Решение: добавить session-scope metadata в `SessionMetrics` и пересчитывать устаревшие записи
через существующий механизм stale-check (`METRICS_SCHEMA_VERSION` / `METRICS_PROJECTION_VERSION`).

Причина: project metrics store уже materializes `SessionMetrics`, и именно из него backend собирает
`ProjectMetricsResponse`. Чтобы не делать дорогие ad-hoc joins в рантайме для каждого запроса,
нужный scope должен лежать рядом с остальными session metrics и обновляться тем же путём.

Альтернатива: вычислять scope только при чтении project metrics через повторный проход по
session index. Это усложняет query path и создаёт расхождение между stored metrics и UI payload.

### 5. UI фильтр остаётся трёхсостоянием `all` / `main` / `subsession`

Решение: на экране `Project metrics` добавить отдельный control рядом с project/window filters.
Если backend сообщает presence `unknown` classification, экран показывает пояснение, что часть
сессий не попала в узкий срез и видна только в `all`.

Причина: пользователю нужен простой повседневный сценарий отделения корневых прогонов от
subagent-сессий, без перегрузки интерфейса внутренними состояниями классификации.

Альтернатива: показывать явный четвёртый режим `unknown`. Это полезно для отладки, но перегружает
первый UX и не было запрошено как основной сценарий.

## Risks / Trade-offs

- [Risk] Новый project catalog endpoint продублирует часть dedupe-логики из frontend. -> Mitigation:
  держать dedupe и scope-classification в backend helper, а frontend использовать только готовый
  payload.
- [Risk] Старые записи в metrics store сначала не будут содержать session scope. -> Mitigation:
  bump projection/schema version и использовать уже существующее rematerialization поведение.
- [Risk] Часть реальных сессий останется `unknown`, если `source` metadata неполная или старая. ->
  Mitigation: не прятать это молча, а показывать banner/hint и включать такие записи в `all`.
- [Risk] Полный project catalog может быть тяжёлым на очень большом `CODEX_HOME`. -> Mitigation:
  возвращать compact project rows, а не целые session summaries; при необходимости позже добавить
  backend-side search/pagination без изменения UI semantics.

## Migration Plan

1. Добавить backend helper для построения полного project catalog из indexed session metadata.
2. Ввести session-scope classification model и расширить backend/frontend contracts.
3. Обновить materialization `SessionMetrics`, подняв schema/projection version при необходимости.
4. Расширить `query_project_metrics` фильтром scope и пересчётом aggregates по отфильтрованному
   набору.
5. Переключить `ProjectMetricsScreen` на новый project catalog endpoint и новый session-scope
   filter.
6. Добавить тесты для full selector coverage, scope classification, filtered aggregates и empty
   states.

Rollback: вернуть UI к старому selector и убрать scope filter; stored metrics с новым полем
остаются совместимыми как более богатый payload, а старый API-путь можно сохранить или откатить
вместе с frontend.

## Open Questions

- Нужен ли project catalog search уже в этой задаче, или достаточно полного списка без отдельной
  строки поиска?
- Стоит ли сразу показывать counts `main/subsession` в строке selector, или достаточно общего
  `sessionCount` и type filter на самом экране?
- Нужно ли переиспользовать новый project catalog и session-scope metadata в других экранах
  `codex-session-explorer`, или пока ограничить change только `Project metrics`?

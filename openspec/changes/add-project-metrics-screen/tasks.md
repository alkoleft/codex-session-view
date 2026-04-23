## 1. Project Metrics Screen Shell

- [x] 1.1 Добавить в `codex-session-explorer` отдельный режим/экран project metrics без смешивания с `SessionMetricsPanel`.
- [x] 1.2 Подготовить frontend state для выбранного проекта, временного окна и флага `include_spawn_agents`.
- [x] 1.3 Реализовать состояния `loading`, `error`, `empty` и degraded project identity для нового экрана.

## 2. Project Selection and Query Flow

- [x] 2.1 Собрать project selector из доступного session catalog без дублей и с понятным display label.
- [x] 2.2 Подключить вызов `query_project_metrics` по выбранному проекту, диапазону и spawn-agent toggle.
- [x] 2.3 Синхронизировать project metrics query с lifecycle экрана так, чтобы устаревшие ответы не перетирали более новый выбор пользователя.

## 3. Chronological Charts and Drill-Down

- [x] 3.1 Добавить хронологические графики для total duration, total tokens, errors or failed operations и tool-call volume.
- [x] 3.2 Показать summary cards и список contributing sessions рядом с графиками.
- [x] 3.3 Сделать точки графиков и строки contributing sessions кликабельными для перехода к исходной сессии в текущем explorer workflow.
- [x] 3.4 Отобразить `known`, `partial` и `unknown` coverage без приведения неизвестных значений к нулю.

## 4. Validation

- [x] 4.1 Добавить UI/tests для project selector, query lifecycle, empty/error/degraded states и spawn-agent toggle.
- [x] 4.2 Добавить tests для рендера графиков, coverage markers и перехода к contributing session.
- [x] 4.3 Прогнать `npm --prefix apps/codex-session-explorer test` и обновить связанные docs только если меняется публичный contract или event semantics.

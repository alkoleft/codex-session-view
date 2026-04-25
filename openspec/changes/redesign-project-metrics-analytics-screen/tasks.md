## 1. Shell and layout simplification

- [x] 1.1 Упростить shell/header composition для режима `project_metrics`, оставив в app shell header только title view и глобальные controls `project`, `period`, `scope`, `includeSpawnAgents`
- [x] 1.2 Удалить локальные header-блоки `ProjectMetricsScreen` и stage-level summary/toolbars, сохранив split-view и независимые scroll owners для main area и side panel
- [x] 1.3 Пересобрать main area в layout `near-graph pinned summary + chart viewport`, убрав `Used skills`, `Contributing sessions`, summary cards и постоянные informational alerts из normal loaded state
- [x] 1.4 Реализовывать change как full-rebuild path без compatibility-слоя со старым экраном; если чище, удалить текущую реализацию `ProjectMetricsScreen` и собрать новый workspace заново

## 2. Chart interaction and pinned inspection

- [x] 2.1 Перевести chart interaction на модель `hover tooltip + pinned by click`, чтобы hover не менял side panel и near-graph summary
- [x] 2.2 Реализовать single-line near-graph pinned summary с двумя блоками: слева `agent_role`, flags, timestamp и short session id; справа `duration`, `tokens`, `calls`, `failures`
- [x] 2.3 Перенести zoom/navigation на chart-native interaction: zoom выделением региона и overlay-кнопки `+`, `-`, `←`, `→` в левом верхнем углу графика

## 3. Side panel tabs

- [x] 3.1 Заменить текущий inspector на tabbed side panel с tabs `Window pulse`, `Anomalies`, `Series`, `Chart`, `Pinned session`, где `Window pulse` идёт первым
- [x] 3.2 Перенести текущий `Window pulse` в первый tab side panel, сохранив quick summary активного window без возврата overview-блока в main area
- [x] 3.3 Реализовать отдельный tab `Anomalies` с indicator в заголовке вкладки, группировкой `Metric / Baseline / Session / Data issue`, collapse/expand по группам и сортировкой `severity -> recency`
- [x] 3.4 Не показывать anomaly panels или persistent markers в main area/chart viewport; оставить только compact indicators в near-graph summary и вкладке `Anomalies`
- [x] 3.5 Реализовать для baseline anomalies (`change-point`, `baseline shift`) range-level actions `focus range`, не смешивая их с point-level outliers
- [x] 3.6 Реализовать tab `Series` как correlation-control surface: список visible/available series, группировка `Operational / Tokens / Factors / Derived`, counters `known / partial / unknown`, назначение `primary`, actions `emphasize`, per-series compact badges override (`trend` / `moving average` / `moving median`) и per-series toggle `raw values`
- [x] 3.7 Реализовать tab `Chart`, оставив в нём только глобальные chart settings: active normalization description, global default analytical mode и `outlier mode`, без chart-type switch
- [x] 3.8 Реализовать tab `Pinned session` как полный detail выбранной pinned-сессии: все доступные значения метрик, `agent_role`, full session id, timing, compact flags и стартовый пользовательский запрос или явный `unavailable`

## 4. Data wiring and state contract

- [x] 4.1 Подготовить view-model/state слой для pinned session, near-graph summary, side tabs, anomaly classes и compact flags без изменения backend contract `query_project_metrics`
- [x] 4.2 Добавить correlation chart model: mandatory normalization для всех видимых series одним глобальным методом, fixed visual roles `primary bar + muted secondary lines`, hover/persistent emphasis и отсутствие hard cap на `raw values` / emphasized series
- [x] 4.3 Реализовать модель `global default + per-series override` для analytical modes, чтобы серии без override наследовали общий режим графика, а overrides управлялись compact badges в tab `Series`
- [x] 4.4 Разделить anomaly signals на классы `Metric`, `Baseline`, `Session`, `Data issue`, не смешивая data-quality flags с metric/session anomalies
- [x] 4.5 Добавить отдельный lazy-fetch endpoint `load_project_metrics_session_detail_by_id` и materialized storage в `session-metrics.sqlite` для стартового пользовательского запроса / task summary, не расширяя `query_project_metrics`
- [x] 4.6 Убедиться, что `partial`, `unknown`, `degraded` и anomaly indicators отображаются как compact flags в summary, tab header `Anomalies`, tooltip и pinned detail вместо крупных explanatory blocks
- [x] 4.7 Разделить state/model на query layer, stable workspace model, workspace state, transient interaction state и lazy detail state вместо одного смешанного screen-state
- [x] 4.8 Разрезать selection source-of-truth на `hoveredSessionId` и `pinnedSessionId`, чтобы hover управлял только tooltip/transient emphasis, а side panel и near-graph summary зависели только от pinned selection
- [x] 4.9 Отделить metadata каталога series от пользовательской series/chart configuration (`visible`, `primary`, `emphasis`, per-series override, per-series `raw values`), не сохраняя старую inspector-oriented view-model структуру

## 5. Validation

- [x] 5.1 Обновить frontend tests под новый layout, pinned selection model, tabs side panel, anomaly tab indicators/grouping, chart overlay controls и отсутствие removed blocks
- [x] 5.2 Прогнать `npm --prefix apps/codex-session-explorer test` и `npm --prefix apps/codex-session-explorer run build`
- [x] 5.3 Выполнить UAT-проверку актуального экрана через Playwright, исправить найденные замечания и повторить проверку до зелёного результата
- [x] 5.4 Прогнать `openspec validate redesign-project-metrics-analytics-screen --strict`

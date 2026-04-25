## Why

Текущий `Project metrics` уже поддерживает интерактивный график, zoom, выбор рядов и inspector,
но экран всё ещё ощущается как набор связанных виджетов, а не как быстрый аналитический workspace
вокруг одного основного инструмента. После серии UX-обсуждений зафиксирован более строгий контракт:
главный рабочий объект экрана — график, а все остальные элементы должны поддерживать анализ точки и
настройку графика, не перетягивая на себя внимание.

## What Changes

- Перестроить экран `Project metrics` в компактный аналитический split-view, где основная площадь
  отдана графику, а правая панель становится вторичной tabbed side panel.
- Зафиксировать, что change реализуется как полный redesign экрана без сохранения совместимости со
  старой внутренней UI/state-композицией; если это чище по реализации, текущий
  `project-metrics-screen` может быть удалён и собран заново, а не адаптирован по месту.
- Зафиксировать график как correlation-first workspace: одновременно показывать несколько метрик из
  разных семейств, потому что основной сценарий экрана — поиск корреляций и совместных сдвигов, а
  не просмотр одной серии в изоляции.
- Включить обязательную normalization для всех видимых метрик одним глобальным методом, чтобы
  серии разных семейств оставались сопоставимыми на одном графике; нормализация должна быть общей
  для всего chart state, а не настраиваться отдельно по каждой метрике.
- Оставить в глобальном header только `project`, `period`, `scope` и `includeSpawnAgents`, убрав
  локальные screen/stage headers и лишний верхний chrome.
- Перевести модель выбора точки на `pinned by click`: hover работает только для tooltip на графике,
  а near-graph summary и side panel показывают только pinned session.
- Сохранить `Window pulse` из текущего view как первую вкладку правой панели для quick summary
  активного window, а anomaly-данные вынести в отдельную вкладку `Anomalies`.
- Заменить текущие summary cards, `Used skills` и `Contributing sessions` на компактный
  near-graph pinned summary и tab `Pinned session` с полной информацией по выбранной сессии.
- Зафиксировать anomaly model с классами `Metric`, `Baseline`, `Session`, `Data issue`, где
  `change-point / baseline shift` входит в отдельный baseline-класс и не смешивается с data-quality
  flags.
- Показывать anomaly status только как compact indicators в near-graph summary и в заголовке tab
  `Anomalies`; сам chart не должен быть размечен persistent anomaly markers.
- Реализовать tab `Anomalies` как группированный collapsible list по классам аномалий с сортировкой
  по severity и recency без возврата anomaly panels в main area.
- Зафиксировать фиксированную visual grammar графика: одна `primary` metric рисуется как доминирующий
  bar-layer, а остальные `secondary` series — как приглушённые lines; display type графика не
  переключается пользователем и не превращается в отдельную настройку.
- Перенести управление сериями в tab `Series`: выбор `primary`, временное/явное emphasis,
  per-metric override аналитического режима (`trend`, `moving average`, `moving median`) и toggle
  `raw values` с компактными badges вместо тяжёлой матрицы настроек.
- Перенести в tab `Chart` только глобальные настройки графика: default analytical mode для серий
  без override, единый режим обработки выбросов и явное описание текущей normalization; не
  добавлять отдельный chart-type switch.
- Упростить zoom/navigation UX: убрать отдельные элементы `window size` / `window position` вне
  графика, оставить zoom выделением региона и overlay-кнопки `+`, `-`, `←`, `→` поверх графика в
  левом верхнем углу.
- Сохранить существующий backend contract `query_project_metrics`, но добавить отдельный
  lightweight lazy-fetch endpoint для `Pinned session` detail и materialized storage для стартового
  пользовательского запроса / task summary без расширения project-level query payload.

## Capabilities

### New Capabilities

<!-- none -->

### Modified Capabilities

- `project-metrics-screen`: меняется контракт экрана как отдельного workspace для проектной
  аналитики, включая структуру header/main/sidebar и способ drill-down по выбранной сессии.
- `project-metrics-layout-focus`: уточняется chart-first layout contract, progressive disclosure и
  вторичная роль side panel по отношению к графику.
- `project-metrics-charts`: меняются правила multi-metric correlation rendering, mandatory
  normalization, fixed primary/secondary visual hierarchy, per-metric analytical overrides,
  point inspection, zoom/navigation controls, anomaly signaling и размещение series/chart controls.

## Impact

- `apps/codex-session-explorer/src/App.tsx`: упрощение shell header для режима `project_metrics` и
  передача заголовка view без локальных вложенных header-блоков.
- `apps/codex-session-explorer/src/components/project-metrics-screen.tsx`: основной экранный
  рефакторинг или полная замена текущего screen-контракта: layout, selection model, tabs side
  panel, near-graph summary и chart overlay controls без compatibility-слоя со старой реализацией.
- `apps/codex-session-explorer/src/components/project-metrics.ts`: пересборка domain/view-model
  слоя под новый workspace contract вместо сохранения структуры текущего inspector-oriented экрана.
- `apps/codex-session-explorer/src/components/project-metrics-chart.ts`: переработка chart-analysis
  слоя под global normalization, primary/secondary emphasis и default mode + per-series overrides.
- `apps/codex-session-explorer/src/components/project-metrics-screen.test.tsx`: обновление
  контрактных UI tests под новый layout и interaction model.
- `openspec/specs/project-metrics-screen/spec.md`,
  `openspec/specs/project-metrics-layout-focus/spec.md`,
  `openspec/specs/project-metrics-charts/spec.md`: delta specs для нового UX-контракта.

## Why

Текущий `ProjectMetricsScreen` уже показывает project-level хронологию, но использует фиксированный
набор самописных SVG line charts без zoom, без richer tooltip и без возможности гибко переключать
набор показателей. Из-за этого экран подходит для базового обзора, но плохо помогает исследовать
спайки, деградации и длинные временные ряды по реальным рабочим проектам.

## What Changes

- Сделать графики project metrics более информативными: добавить richer tooltip/legend, явное
  отображение coverage/source и удобное отображение ключевых значений по точке.
- Поддержать zoom и навигацию по длинным временным рядам, чтобы пользователь мог исследовать
  недельные и месячные окна без потери читаемости.
- Расширить набор поддерживаемых series для practically useful показателей поверх уже существующего
  `ProjectMetricsResponse`, включая duration, tokens, failures, tool calls и derived metrics с
  явным `unknown`/`partial` semantics.
- Сделать основной режим экранa сводным графиком, на который можно одновременно вывести несколько
  метрик и управлять их видимостью через выключатели в legend/control panel.
- Зафиксировать переход на готовую chart-библиотеку `Recharts` как основной renderer для
  summary chart в Tauri/web сборке; развитие текущего кастомного `MetricChart` не входит в
  активный implementation path этого change.
- Сохранить drill-down из графика к исходной сессии и не ломать существующий contract
  `query_project_metrics`.

## Capabilities

### New Capabilities
- `project-metrics-charts`: интерактивный сводный график project metrics с zoom, richer point
  details, одновременным отображением нескольких series, выключателями видимости и решением по
  использованию готового chart component stack.

### Modified Capabilities

Нет существующих OpenSpec capabilities: `openspec/specs/` в этом репозитории пока пуст.

## Impact

- `apps/codex-session-explorer/src/components/project-metrics-screen.tsx`: переработка рендера
  графиков, controls и point interaction.
- `apps/codex-session-explorer/src/components/project-metrics.ts`: расширение view-model для новых
  metric series, multi-series legend/tooltip payload и zoom window.
- `apps/codex-session-explorer/package.json`: возможное добавление chart dependency, если выбран
  готовый компонент вместо развития текущего SVG renderer.
- `apps/codex-session-explorer/src/components/project-metrics.test.ts` и смежные UI tests:
  покрытие новых charts, selectors, zoom и session drill-down.

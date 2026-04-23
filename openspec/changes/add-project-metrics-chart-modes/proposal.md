## Why

Текущий `ProjectMetricsScreen` уже умеет показывать исходные значения, trend и median как
одновременные overlay, но не даёт пользователю понятную модель "основная аналитическая линия +
вспомогательные слои". Из-за этого экран хуже подходит для последовательного анализа шума,
сглаженных рядов и baseline по проекту, хотя нужные вычислительные примитивы уже частично есть в
коде.

## What Changes

- Добавить в `Project metrics` отдельные аналитические режимы summary chart вместо жёсткой пары
  toggle для overlay: `trend`, `moving-average`, `moving-median`.
- Зафиксировать `trend` как глобальную robust trend line по методу `Theil-Sen`, а
  `moving-average` и `moving-median` использовать как локальные режимы сглаживания.
- Оставить `raw values` не отдельным режимом, а независимым overlay-переключателем поверх активной
  аналитической линии.
- Добавить отдельный флаг для project-wide median, чтобы пользователь мог при необходимости
  наложить общую медиану проекта поверх активного режима; median считается по полному query window,
  а `unknown` точки игнорируются.
- Сделать конфигурацию режимов расширяемой, чтобы позже можно было безопасно добавить новые
  derived chart modes вроде `deviation-from-median` без переписывания экранного state и
  tooltip/legend contract.
- Сохранить текущие zoom, coverage semantics, series visibility и drill-down к source session для
  всех поддержанных режимов.

## Capabilities

### New Capabilities
- `project-metrics-chart-modes`: аналитические режимы summary chart для project metrics с
  переключением между разными представлениями ряда и опциональной project-wide median overlay

### Modified Capabilities
- Нет существующих OpenSpec capabilities: `openspec/specs/` в этом репозитории пока пуст.

## Impact

- `apps/codex-session-explorer/src/components/project-metrics-screen.tsx`: новый screen state для
  chart mode и overlays, обновлённый toolbar, legend/tooltip copy и рендер отдельных режимов.
- `apps/codex-session-explorer/src/components/project-metrics.ts` или выделенный chart helper:
  registry режимов, вычисление `Theil-Sen` trend line, moving-average и moving-median рядов,
  metadata для tooltip и overlays.
- `apps/codex-session-explorer/src/components/project-metrics.test.tsx` и смежные UI tests:
  покрытие mode switching, raw values / project median overlays и сохранения current
  zoom/drill-down поведения.
- Возможное затрагивание локальных helper-функций аналитики в `ProjectMetricsScreen`, если вычисления
  rolling median и mode registry будут вынесены из монолитного компонента.

## Context

В `apps/codex-session-explorer` уже существует `ProjectMetricsScreen`, который строит view-model из
`ProjectMetricsResponse` и рисует фиксированный набор графиков через самописный SVG renderer в
`project-metrics-screen.tsx`. Текущая реализация покрывает базовый обзор, но не даёт zoom/pan по
длинным рядам, не поддерживает полноценный tooltip/legend слой и плохо масштабируется при
добавлении новых показателей и richer interaction.

Техническое ограничение в том, что экран должен одинаково работать в web и Tauri-сборке, не ломая
существующий backend contract `query_project_metrics` и не перенося расчёт chart series на сервер.
В `package.json` сейчас нет chart-библиотеки, поэтому это change может добавить новую frontend
dependency, если выигрыш по UX и сложности интеграции оправдан.

Дополнительно пользователь явно попросил изучить готовые компоненты для графиков. По результатам
исследования:

- `Recharts` уже предоставляет `LineChart`, `Tooltip`, `Legend`, `ResponsiveContainer` и `Brush`
  для zoom/pan по оси X, а также синхронизацию нескольких графиков через `syncId`.
- `visx` — это low-level набор компонентов; он гибкий, но потребует почти с нуля собирать chart
  primitives, tooltip и zoom interaction.
- `Apache ECharts` даёт самый богатый интерактивный слой (`dataZoom`, `brush`, готовые toolbox
  controls), но потребует более тяжёлой imperative integration и отдельной обвязки вокруг React.

## Goals / Non-Goals

**Goals:**

- Сделать project metrics charts пригодными для реального анализа длинных временных рядов.
- Добавить zoom/navigation, richer tooltips, legend и одновременное отображение нескольких
  показателей на одном сводном графике.
- Сохранить `unknown`/`partial` semantics и drill-down к исходной сессии.
- Выбрать и зафиксировать реалистичный chart stack вместо расплывчатого "потом решим".
- Не менять backend contract и не переносить business logic построения series на сервер.

**Non-Goals:**

- Не превращать экран в BI-конструктор с произвольными формулами и пользовательскими dashboard
  layouts.
- Не вводить новый API для bucketed/downsampled aggregates в рамках первого change.
- Не переписывать весь metrics domain model или materialized storage.
- Не делать multi-project compare в первой итерации.

## Decisions

### 1. Готовый chart component stack предпочтительнее развития текущего самописного SVG

Решение: для реализации change перейти с самописного `MetricChart` на готовый React chart stack,
если интеграция проходит без конфликтов с Tauri/web сборкой; current custom SVG renderer не
сохраняется как основной путь.

Причина: нужно добавить zoom, legend, tooltip, hover/selection state и расширяемый набор серий.
Все эти возможности в текущей реализации уже начали накапливать UI-сложность, но остаются
недостаточными. Развитие кастомного SVG дальше даст больше bespoke-кода, чем пользы.

Альтернатива: доработать текущий SVG собственным zoom-window и tooltip state. Это сэкономит
зависимость, но снова оставит нас с самописным chart framework внутри приложения.

### 2. Предпочтительный кандидат для внедрения: `Recharts`

Решение: проектировать change под `Recharts` как основной кандидат.

Причина: библиотека декларативная для React, хорошо подходит к текущему UI-стеку, закрывает
нужные сценарии `Tooltip`/`Legend`/`ResponsiveContainer` и даёт `Brush` для zoom/pan по оси X без
написания собственного interaction layer. Для набора линейных time-series project metrics это
практичнее, чем low-level `visx`, и легче в интеграции, чем `Apache ECharts`.

Альтернатива: `visx`. Плюс — минимальный и гибкий low-level API. Минус — zoom, tooltip, legend и
composed chart behavior всё равно придётся собирать вручную.

Альтернатива: `Apache ECharts`. Плюс — сильный встроенный интерактивный слой и `dataZoom`. Минус —
более тяжёлая dependency, imperative wrapper и избыточность для сравнительно простого экрана.

### 3. Источник данных остаётся прежним: frontend view-model поверх `ProjectMetricsResponse`

Решение: новый chart layer получает уже подготовленные frontend points из
`buildProjectMetricsViewModel`, а не вызывает новый backend endpoint.

Причина: сервер уже возвращает `sessions[]`, `token_ledger`, `duration_ms`, baseline и derived
metrics. Для текущего объёма change важнее улучшить presentation и interaction, чем усложнять
contract.

Альтернатива: отдельный backend response с готовыми chart series и zoom metadata. Это может
понадобиться позже, если появятся тысячи точек и downsampling, но пока преждевременно.

### 4. Основной режим экрана — один сводный график с curated multi-series overlays

Решение: выделить registry/config для supported metric series и рисовать один основной сводный
chart, в котором пользователь может одновременно включать и выключать curated series:
`duration`, `total tokens`, `failures`, `tool calls`, а также derived metrics вроде
`tokens per successful session` и `review findings per 1k tokens`, если у них есть достаточный
coverage.

Причина: пользователь явно просит сводный график, на который можно вывести все нужные метрики и
управлять ими через выключатели. Такой режим лучше подходит для сопоставления всплесков между
несколькими сигналами, чем набор разрозненных одиночных charts.

Альтернатива: один переключаемый график "по одной серии за раз". Это проще, но заставляет
пользователя постоянно переключаться и мешает визуально сопоставлять коррелирующие деградации.

Альтернатива: всегда рисовать все metric groups. Это перегружает экран и ухудшает читаемость, если
не дать пользователю control over visibility.

### 5. Coverage semantics должны быть видны и в линии, и в tooltip

Решение: unknown points остаются `null`/gap на графике, partial points отображаются отдельным
marker style, а tooltip показывает coverage и formatted value без приведения неизвестных значений к
нулю.

Причина: contract metrics уже различает `known`, `partial`, `unknown`. Скрывать это внутри chart
renderer означает ломать смысл данных.

Альтернатива: приводить все unknown к `0` для гладкости графика. Это создаёт ложные выводы.

### 6. Zoom window, видимость series и выбранная точка становятся частью screen state

Решение: состояние выбранного диапазона графика, набора видимых series и активной сессии хранить в
frontend screen state рядом с project/range filters, чтобы zoom, legend toggles и drill-down
переживали повторный render.

Причина: сводный график требует общего "окна анализа" и стабильной модели видимости линий, а не
локального состояния отдельных виджетов.

Альтернатива: локальное состояние внутри графика или legend. Это делает поведение несогласованным
и усложняет тестирование.

## Risks / Trade-offs

- [Risk] Новая chart dependency увеличит размер frontend bundle. -> Mitigation: ограничиться одним
  кандидатом, проверить `build`/`test`, не тянуть несколько chart stacks одновременно.
- [Risk] `Brush`/zoom может быть неудобным на узких экранах. -> Mitigation: предусмотреть mobile
  fallback с горизонтальным скроллом и компактным набором графиков.
- [Risk] Derived metrics часто будут `unknown` и визуально "пустыми". -> Mitigation: показывать их
  только при достаточном coverage или давать явное empty-state объяснение.
- [Risk] Переход на готовую библиотеку усложнит кастомные unknown/partial markers. -> Mitigation:
  использовать custom dot/tooltip renderers, а не пытаться подогнать contract под библиотеку.

## Migration Plan

1. Выделить chart-series registry и расширить view-model данными для multi-series tooltip, legend,
   visible toggles и zoom.
2. Подключить `Recharts` и заменить текущий `MetricChart` на summary chart c `ResponsiveContainer`,
   `LineChart`, `Tooltip`, `Brush` и curated overlays.
3. Добавить legend/control toggles для multi-series overlays, shared zoom state и
   синхронизированный drill-down к сессии.
4. Проверить desktop/mobile layout, тесты и отсутствие regressions по `unknown`/`partial`
   semantics.

Rollback: удалить `Recharts`-integration и вернуть экран к предыдущему SVG renderer; backend и
storage слой при этом не требуют миграции.

## Open Questions

- Достаточно ли для пользователя `Brush`-style zoom по X-axis, или нужен ещё wheel/pinch zoom?
- Нужно ли показывать derived metrics в отдельной вкладке/группе, чтобы не смешивать их с базовыми
  operational metrics?

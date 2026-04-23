## Context

В `apps/codex-session-explorer/src/components/project-metrics-screen.tsx` summary chart уже
поддерживает curated series selection, zoom window, outlier mode и два аналитических overlay:
`showTrend` и `showMedian`. При этом текущая модель экрана смешивает базовый ряд и аналитические
слои в одном рендере: пользователь видит исходные значения всегда, а trend/median лишь включает
или выключает поверх них.

Текущее поведение полезно для общего обзора, но не закрывает сценарий "смотреть на график в
отдельном аналитическом режиме". В коде уже есть `computeRollingAverage`, `computeQuantile` и
`pickTrendWindowSize`, однако они жёстко привязаны к текущему overlay-подходу и не оформлены как
расширяемая модель chart modes. По результатам обсуждения основная ось взаимодействия должна быть
не "какой один ряд показать", а "какой аналитической линией интерпретировать ряд": `trend`,
`moving-average`, `moving-median`, плюс независимые overlay `raw values` и `project-wide median`.

## Goals / Non-Goals

**Goals:**

- Перевести аналитическое поведение summary chart на явный `chartMode` вместо пары
  `showTrend`/`showMedian`.
- Поддержать как минимум три аналитических режима: `trend`, `moving-average`, `moving-median`.
- Оставить `raw values` и `project median` независимыми overlays поверх активного режима.
- Добавить независимый флаг `showProjectMedian`, который накладывает общую медиану проекта поверх
  активного режима.
- Зафиксировать `trend` как robust global direction line по методу `Theil-Sen`.
- Сохранить существующие zoom, series visibility, coverage semantics и drill-down в session
  explorer.
- Сделать архитектуру режимов расширяемой, чтобы будущие derived curves добавлялись через registry,
  а не через новые флаги в корневом компоненте.

**Non-Goals:**

- Не менять backend contract `query_project_metrics` и не переносить вычисление аналитических рядов
  на сервер.
- Не добавлять полнофункциональный BI-конструктор с несколькими одновременными аналитическими
  режимами.
- Не добавлять `per-series custom window` и не превращать сглаживание в индивидуально настраиваемый
  chart builder.
- Не пересматривать выбранный chart stack, legend model или текущую curated series taxonomy.
- Не менять semantics `known` / `partial` / `unknown` и не нормализовать `unknown` в `0`.

## Decisions

### 1. Summary chart получает один основной аналитический режим и отдельные overlay

Решение: заменить независимые `showTrend` и `showMedian` на единый screen state `chartMode` со
значениями `trend`, `moving-average`, `moving-median`, а `raw values` и project-wide median хранить
отдельными булевыми overlay-флагами.

Причина: текущая пара toggle плохо масштабируется. Она уже даёт частично пересекающееся поведение,
но не отвечает на вопрос "какая аналитическая линия сейчас активна". Одновременно стало понятно,
что `raw values` лучше живут не как mode, а как слой доверия поверх интерпретирующей линии. Для
moving median текущая модель совсем не подходит: добавление ещё одного toggle превратит toolbar в
набор слабосвязанных флагов.

Альтернатива: оставить базовые toggle и добавить ещё `showMovingAverage`/`showMovingMedian`.
Минус: конфликтующие комбинации, усложнение tooltip copy и рост ветвлений в chart renderer.

### 2. `trend` фиксируется как `Theil-Sen`, а не как ещё одно сглаживание

Решение: режим `trend` рассчитывается как robust line по методу `Theil-Sen` на полном query
window для каждой visible series, игнорируя `unknown` точки.

Причина: текущий код уже использует rolling average под именем `trend`, но после выделения
отдельного режима `moving-average` это стало бы дублированием. Для operational и derived metrics
этого экрана характерны выбросы, разреженные значения и bursty-паттерны, поэтому обычная linear
regression слишком чувствительна к единичным всплескам. `Theil-Sen` лучше соответствует смыслу
"общего направления ряда", не превращаясь в ещё один smoothing mode.

Альтернатива: ordinary least squares regression. Плюс: проще вычисление и привычная статистическая
линия. Минус: один spike может заметно исказить наклон и сделать trend misleading именно в тех
сериях, для которых пользователь и открывает project metrics.

### 3. Для всех режимов используется единый analytic-series registry

Решение: выделить registry или helper-слой, который для каждой visible series умеет вычислять:
`rawValues`, `trendValues`, `movingAverageValues`, `movingMedianValues`, `projectMedian` и mode
metadata для tooltip/legend.

Причина: сейчас аналитика частично вычисляется внутри `buildChartAnalysis`, что затрудняет
добавление новых режимов и повторное использование в tooltip, domain calculation и тестах. Единый
registry делает режимы декларативными: UI выбирает режим, а helper возвращает нужный массив точек и
подсказки для подписи режима.

Альтернатива: вычислять каждый режим inline в JSX. Это оставит логику разбросанной между chart
domain, tooltip, inspector и empty-state checks.

### 4. Сглаживание применяется после outlier processing и на основе доступных значений

Решение: analytic modes и overlays строятся поверх `processedValues`, то есть после применения
текущего `outlierMode` (`keep` / `clamp` / `exclude`), а окна сглаживания игнорируют `null`
значения, но не подменяют их нулями.

Причина: пользователь уже управляет тем, как обрабатываются выбросы. Если trend и moving modes
считать от "сырых" значений, chart mode начнёт противоречить выбранному outlier policy. Сохранение
`null` также нужно, чтобы не искажать `unknown` coverage.

Альтернатива: считать сглаживание от исходных raw values. Это сделает derived line независимой от
выбранного outlier mode и визуально удивит пользователя.

### 5. Rolling median использует то же adaptive window sizing, что и moving average

Решение: `moving-median` рассчитывается через новое `computeRollingMedian(values, windowSize)`, где
`windowSize` берётся из уже существующего `pickTrendWindowSize` и всегда остаётся нечётным.

Причина: пользователю важнее согласованное поведение режимов, чем отдельная настройка окна для
каждого режима. Общий adaptive window делает сравнительный анализ между `moving-average` и
`moving-median` предсказуемым.

Альтернатива: выделить отдельный slider под median window или per-series custom window. Это даёт
больше контроля, но заметно расширяет scope и усложняет UX до того, как станет понятно, что это
реально нужно. По итогам обсуждения `per-series custom window` в текущий change не входит.

### 6. Project-wide median остаётся независимым overlay и считается по полному project query window

Решение: `showProjectMedian` рисует горизонтальную линию по всей видимой области активного ряда для
каждой visible series, используя медиану по значениям полного project query window после outlier
processing; `unknown` точки при расчёте медианы игнорируются.

Причина: пользователь явно просит "флагом вывод общей медианы по проекту". Это означает не новый
режим вместо линии, а самостоятельный baseline overlay, который можно наложить на `values`,
`trend`, `moving-average` или `moving-median`.

Альтернатива: делать median отдельным `chartMode`. Тогда её нельзя сравнивать с активным режимом, а
пользователь теряет baseline overlay.

### 7. Tooltip, inspector и chart domain используют активный mode как первичный ряд

Решение: tooltip, inspector snapshot и `computeSeriesDomain` должны ориентироваться на активные
mode values, а overlays `raw values` и `project median` добавляются в rendering и domain только
если соответствующие toggle включены.

Причина: сегодня tooltip одновременно сообщает raw/chart/trend/median. После введения отдельных
режимов и overlays основным должен стать активный mode, а дополнительные слои нужно показывать
только когда они реально включены, иначе UI останется когнитивно перегруженным и не даст понять,
что именно сейчас анализируется.

Альтернатива: продолжать выводить все derived values всегда. Это размоет смысл mode switch и
оставит прежнюю перегруженность интерфейса.

## Risks / Trade-offs

- [Risk] Переход с overlay toggle на selector режима и overlays может сломать привычный workflow
  текущего пользователя. -> Mitigation: оставить `raw values` включёнными по умолчанию и сделать
  labels/help text предельно явными.
- [Risk] Rolling median на длинных рядах добавит вычислительную нагрузку на render. -> Mitigation:
  считать аналитику один раз на изменение входов и держать helper-линии в общем `buildChartAnalysis`
  проходе, не дублируя вычисления в tooltip.
- [Risk] `Theil-Sen` может оказаться дороже простой linear regression на очень длинных рядах. ->
  Mitigation: сначала зафиксировать корректную semantics trend, а оптимизацию вычисления решать
  отдельно только при реальном bottleneck.
- [Risk] Пользователь может ожидать, что project median считается по видимому zoom-window, а не по
  полному project query window. -> Mitigation: явно зафиксировать semantics в tooltip/help text и
  покрыть тестом.
- [Risk] Новые mode labels и toolbar controls могут перегрузить узкие экраны. -> Mitigation:
  использовать компактную segmented control для режима и компактные toggle для overlays.

## Migration Plan

1. Вынести вычисление analytic series в helper/registry, добавить `Theil-Sen` trend line, rolling
   median и metadata для mode labels, overlays и project median.
2. Заменить в `ProjectMetricsScreen` toggle `Trend` / `Median` на selector режима и отдельные
   toggle `Raw values` / `Project median`.
3. Обновить chart renderer, tooltip, inspector и domain calculation так, чтобы они использовали
   активный `chartMode` и включённые overlays.
4. Обновить UI tests и `openspec validate add-project-metrics-chart-modes --strict`.

Rollback: вернуть `showTrend` / `showMedian` toggle и убрать mode registry/overlay model; backend и
persisted data при этом не требуют миграции.

## Open Questions

- Нужен ли отдельный label или badge, который явно сообщает размер окна сглаживания для
  `moving-average` / `moving-median`?
- Когда придёт время для следующих режимов, что полезнее первым: `deviation-from-median` или
  другой derived analytical mode?

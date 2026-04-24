## 1. Chart mode analytics foundation

- [x] 1.1 Вынести или упорядочить вычисление analytic series так, чтобы `buildChartAnalysis` умел возвращать данные для `trend`, `moving-average`, `moving-median`, `raw values` и `project median`.
- [x] 1.2 Добавить helper для `Theil-Sen` trend line на полном query window и helper для rolling median поверх `processedValues`, игнорируя `unknown` точки.
- [x] 1.3 Описать mode metadata, overlay metadata и labels в одном месте, чтобы UI мог рендерить selector, help text и tooltip без ручного ветвления по строковым литералам.

## 2. Project metrics chart mode UI

- [x] 2.1 Заменить в `ProjectMetricsScreen` toggle `Trend` / `Median` на selector активного `chart mode` для `trend`, `moving-average`, `moving-median`.
- [x] 2.2 Добавить независимые toggle `Raw values` и `Project median`, подключив их к overlay-слоям для всех видимых series.
- [x] 2.3 Обновить summary chart renderer и domain calculation так, чтобы основной линией и empty-state логикой управлял активный `chart mode`, а overlays добавлялись независимо, сохраняя текущие zoom-window и series visibility controls.

## 3. Inspection and interaction consistency

- [x] 3.1 Обновить tooltip, inspector и explanatory copy так, чтобы они показывали активный mode как primary chart value, а `raw values` и `project median` только при включённых overlays.
- [x] 3.2 Сохранить coverage semantics `known` / `partial` / `unknown` для всех новых режимов и не допустить подмены `unknown` значений нулями.
- [x] 3.3 Проверить, что drill-down к source session, focus selection и alignment с contributing sessions продолжают работать в режимах `trend`, `moving-average` и `moving-median`.

## 4. Validation

- [x] 4.1 Обновить или добавить UI tests для mode switching, `raw values` / `project median` overlays и поведения tooltip/inspector в аналитических режимах.
- [x] 4.2 Прогнать `npm --prefix apps/codex-session-explorer test` и при необходимости точечные frontend checks для `ProjectMetricsScreen`.
- [x] 4.3 Прогнать `openspec validate add-project-metrics-chart-modes --strict`.

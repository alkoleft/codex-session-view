## ADDED Requirements

### Requirement: Пользователь может переключать аналитический режим summary chart
Экран `Project metrics` SHALL предоставлять явный selector аналитического режима summary chart как
минимум для `trend`, `moving-average` и `moving-median`.

#### Scenario: Переключение между режимами графика
- **WHEN** пользователь выбирает другой поддержанный `chart mode`
- **THEN** summary chart MUST перерисовать активную линию по выбранному режиму без повторного
  запроса project metrics
- **AND** текущие выбранные series, project/range filters и zoom-window MUST сохраниться

### Requirement: Режим trend использует robust line по методу Theil-Sen
Система SHALL рассчитывать `trend` как глобальную robust trend line по методу `Theil-Sen` на полном
query window для каждой видимой series, игнорируя `unknown` точки.

#### Scenario: Построение trend для шумной series
- **WHEN** пользователь включает режим `trend`
- **THEN** summary chart MUST использовать `Theil-Sen` trend line, а не ordinary least squares или
  ещё одну форму rolling average
- **AND** единичные spikes MUST не переопределять общий наклон ряда так же сильно, как в обычной
  linear regression

### Requirement: Пользователь может включать raw values как отдельный overlay
Экран `Project metrics` SHALL поддерживать независимый toggle для отображения raw values поверх
текущего активного `chart mode`.

#### Scenario: Включение raw values поверх аналитического режима
- **WHEN** пользователь включает overlay `raw values`
- **THEN** summary chart MUST показать исходные значения поверх активной аналитической линии без
  смены текущего `chart mode`
- **AND** tooltip и inspector MUST показывать raw value только пока overlay включён

### Requirement: Активный chart mode определяет основные значения в chart inspection UI
Система SHALL использовать активный `chart mode` как основной источник значений для summary chart,
tooltip и point inspector.

#### Scenario: Просмотр точки в аналитическом режиме
- **WHEN** пользователь наводится или выбирает точку при активном режиме `trend`,
  `moving-average` или `moving-median`
- **THEN** tooltip и inspector MUST показывать значение активного режима как основной chart value
- **AND** `unknown` точки MUST оставаться `gap` или явным unknown marker вместо подстановки `0`

### Requirement: Пользователь может включать общую медиану проекта отдельным флагом
Экран `Project metrics` SHALL поддерживать независимый toggle для отображения общей медианы проекта
поверх текущего активного `chart mode`.

#### Scenario: Включение project-wide median
- **WHEN** пользователь включает флаг общей медианы проекта
- **THEN** summary chart MUST показать median baseline для каждой видимой series поверх активного
  режима
- **AND** baseline MUST вычисляться из значений полного project query window без нового backend
  запроса, игнорируя `unknown` точки

### Requirement: Аналитические режимы сохраняют текущие chart invariants
Система SHALL сохранять существующие semantics series visibility, zoom, coverage и drill-down для
всех поддержанных `chart mode`.

#### Scenario: Drill-down из сглаженного режима
- **WHEN** пользователь работает в режиме `moving-average` или `moving-median` и открывает сессию из
  chart point
- **THEN** explorer MUST открыть исходную session в текущем workflow context
- **AND** zoom-window и набор видимых series MUST остаться согласованными с выбранной точкой

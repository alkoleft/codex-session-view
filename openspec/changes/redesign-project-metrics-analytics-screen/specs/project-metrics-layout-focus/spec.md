## MODIFIED Requirements

### Requirement: Project metrics screen prioritizes the chart viewport
Экран `Project metrics` SHALL визуально и композиционно делать график главным рабочим инструментом,
оставляя над ним только compact near-graph summary и overlay controls, не конкурирующие с chart
viewport.

#### Scenario: Opening project metrics with loaded data
- **WHEN** пользователь открывает экран `Project metrics` для проекта, по которому уже загружены метрики
- **THEN** основной chart viewport MUST быть главным визуальным фокусом экрана
- **THEN** над графиком MUST оставаться только компактная pinned summary строка и overlay controls
- **THEN** статические summary, explanatory blocks и крупные secondary sections MUST не занимать
  больше визуального веса, чем сам график

### Requirement: Secondary information uses progressive disclosure
Экран `Project metrics` SHALL переводить вторичную информацию в compact or on-demand presentation
через правую tabbed side panel вместо постоянного развернутого потока карточек и списков.

#### Scenario: Viewing summary and help information
- **WHEN** пользователь работает с уже загруженным графиком
- **THEN** secondary information MUST быть доступна через compact side panel tabs
- **THEN** первый tab `Window pulse` MUST сохранять quick summary текущего visible window
- **THEN** anomaly exploration MUST жить в отдельном tab `Anomalies`
- **THEN** эта secondary information MUST не занимать постоянную основную площадь над графиком,
  если не является активным состоянием ошибки или пустого результата

### Requirement: Metric-series controls remain available without dominating the screen
Экран `Project metrics` SHALL сохранять управление видимостью metric series, но размещать его в
tab `Series` правой панели, а не в крупной постоянной grid или toolbar над графиком.

#### Scenario: Changing visible metric series
- **WHEN** пользователь включает или выключает series на экране `Project metrics`
- **THEN** экран MUST позволять это сделать через side panel без ухода с основного chart context
- **THEN** control surface MUST занимать вторичную роль по отношению к графику

#### Scenario: Adjusting per-series analytics without toolbar bloat
- **WHEN** пользователь меняет `primary` metric, усиливает secondary series, задаёт override
  аналитического режима или включает `raw values`
- **THEN** эти controls MUST жить в compact `Series` tab и использовать короткие badges/actions
- **THEN** main area MUST не получать отдельную матрицу per-series настроек над графиком

### Requirement: Global chart controls stay compact and fixed in purpose
Экран `Project metrics` SHALL выносить в tab `Chart` только глобальные настройки графика и не
делать type switching самостоятельной частью layout.

#### Scenario: Opening the chart settings tab
- **WHEN** пользователь открывает tab `Chart`
- **THEN** он MUST видеть только общие chart-level controls: active normalization description,
  global default analytical mode и `outlier mode`
- **THEN** экран MUST не предлагать свободное переключение `chart type` между разными визуальными
  грамматиками

### Requirement: Point details and contributing sessions form one inspector flow
Экран `Project metrics` SHALL связывать point inspection с одной pinned session detail flow и MUST
не показывать отдельный постоянный список contributing sessions на этом экране.

#### Scenario: Inspecting a selected chart point
- **WHEN** пользователь кликает на точку графика
- **THEN** экран MUST зафиксировать эту точку как pinned session
- **THEN** side panel MUST показывать её детали в `Pinned session` tab без дополнительного
  contributing sessions list

### Requirement: Exceptional states stay explicit without permanent clutter
Экран `Project metrics` SHALL явно показывать `loading`, `error` и `empty` состояния, но для
`partial`, `unknown`, `degraded` и anomaly indicators использовать compact flags вместо постоянных
заметных explanatory blocks в normal loaded state.

#### Scenario: Returning to normal loaded state
- **WHEN** экран находится в обычном состоянии с загруженными данными и без блокирующих ошибок
- **THEN** служебные explanatory blocks MUST не занимать постоянную заметную область экрана
- **THEN** `partial`, `unknown`, `degraded` и anomaly markers MUST отображаться компактными flags
  рядом с pinned summary, tab header `Anomalies` и session detail

## ADDED Requirements

### Requirement: Anomaly review stays grouped and collapsible in the side panel
Экран `Project metrics` SHALL показывать anomaly review внутри side panel как сгруппированный
collapsible list, а не как один длинный flat list или набор отдельных panel-блоков в main area.

#### Scenario: Reviewing anomaly groups
- **WHEN** пользователь открывает tab `Anomalies`
- **THEN** аномалии MUST быть сгруппированы по классам `Metric`, `Baseline`, `Session`,
  `Data issue`
- **THEN** каждая группа MUST поддерживать collapse/expand
- **THEN** `Baseline` anomalies MUST поддерживать range-level navigation, а не притворяться
  обычными point-level alerts

### Requirement: Main area and side panel keep independent scroll ownership
Экран `Project metrics` SHALL сохранять split-view contract, в котором main area и tabbed side
panel остаются независимыми vertical scroll owners.

#### Scenario: Scrolling loaded analytics workspace
- **WHEN** пользователь прокручивает графиковую область или правую панель с tabs
- **THEN** main area MUST скроллиться независимо от side panel
- **THEN** side panel MUST скроллиться независимо от main area

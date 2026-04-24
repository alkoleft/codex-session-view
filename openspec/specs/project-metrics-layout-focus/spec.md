# project-metrics-layout-focus Specification

## Purpose
TBD - created by archiving change declutter-project-metrics-screen. Update Purpose after archive.
## Requirements
### Requirement: Project metrics screen prioritizes the chart viewport
Экран `Project metrics` SHALL визуально ставить summary chart на первое место и выделять ему
основную площадь экрана по сравнению со secondary information blocks.

#### Scenario: Opening project metrics with loaded data
- **WHEN** пользователь открывает экран `Project metrics` для проекта, по которому уже загружены метрики
- **THEN** основной chart viewport MUST быть главным визуальным фокусом экрана
- **THEN** статические summary и explanatory blocks MUST не занимать больше визуального веса, чем сам график

### Requirement: Secondary information uses progressive disclosure
Экран `Project metrics` SHALL переводить вторичную информацию в compact or on-demand presentation
вместо постоянного развернутого потока карточек и пояснений.

#### Scenario: Viewing summary and help information
- **WHEN** пользователь работает с уже загруженным графиком
- **THEN** summary cards, coverage help и secondary controls MUST быть доступны без удаления функциональности
- **THEN** эти блоки MUST отображаться в компактном или раскрываемом виде, если они не являются активным состоянием ошибки или пустого результата

### Requirement: Metric-series controls remain available without dominating the screen
Экран `Project metrics` SHALL сохранять управление видимостью metric series, но размещать его в
более компактной control surface, чем крупная постоянная grid из карточек.

#### Scenario: Changing visible metric series
- **WHEN** пользователь включает или выключает series на экране `Project metrics`
- **THEN** экран MUST позволять это сделать без ухода с основного chart context
- **THEN** control surface MUST занимать вторичную роль по отношению к графику

### Requirement: Point details and contributing sessions form one inspector flow
Экран `Project metrics` SHALL связывать active point details и contributing sessions в единый
inspection flow, синхронизированный с выбором точки на графике.

#### Scenario: Inspecting a selected chart point
- **WHEN** пользователь наводится или кликает на точку графика
- **THEN** экран MUST показать связанные детали этой точки и соответствующую сессию в одном связанном inspector flow
- **THEN** contributing sessions list MUST синхронизироваться с выбранной точкой без разрыва контекста

### Requirement: Responsive layout protects chart readability
Экран `Project metrics` SHALL на desktop и узких экранах сохранять читаемость графика как
приоритетную задачу layout.

#### Scenario: Viewing project metrics on a narrow screen
- **WHEN** ширина экрана недостаточна для desktop layout
- **THEN** secondary information MUST сворачиваться, переноситься или открываться по требованию
- **THEN** график MUST оставаться читаемым и не должен уходить ниже длинной последовательности статических блоков

### Requirement: Exceptional states stay explicit without permanent clutter
Экран `Project metrics` SHALL явно показывать `loading`, `error`, `empty` и `degraded` состояния,
но не держать их explanatory blocks постоянно в layout, когда эти состояния не активны.

#### Scenario: Returning to normal loaded state
- **WHEN** экран находится в обычном состоянии с загруженными данными и без ошибок
- **THEN** служебные explanatory blocks MUST не занимать постоянную заметную область экрана
- **THEN** явные alerts/banners MUST появляться только для реально активных exceptional states


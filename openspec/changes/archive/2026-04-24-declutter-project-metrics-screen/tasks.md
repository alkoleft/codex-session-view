## 1. Chart-First Layout Shell

- [x] 1.1 Перестроить `ProjectMetricsScreen` на зоны `toolbar`, `chart stage` и `inspector` без изменения backend query flow
- [x] 1.2 Перераспределить размеры и spacing так, чтобы chart viewport получил приоритетную высоту и ширину на desktop layout
- [x] 1.3 Оставить `loading`, `error`, `empty` и `degraded` состояния явными, но встроить их в новый layout без постоянного служебного clutter

## 2. Compact Secondary Controls

- [x] 2.1 Сжать project/window/spawn controls в компактный toolbar с понятной visual hierarchy
- [x] 2.2 Перевести summary cards и coverage/help блоки в compact или раскрываемый режим вместо постоянной доминирующей сетки
- [x] 2.3 Заменить крупный grid metric-series toggles на более плотный control surface, сохранив доступ ко всем поддерживаемым series

## 3. Unified Inspector Flow

- [x] 3.1 Объединить active point details и contributing sessions в один синхронизированный inspector
- [x] 3.2 Сохранить связь chart hover/click с выбранной сессией и drill-down в существующий explorer workflow
- [x] 3.3 Подготовить mobile/tablet поведение inspector, чтобы secondary details открывались без потери читаемости графика

## 4. Verification

- [x] 4.1 Обновить `project-metrics-screen` tests под новый layout, progressive disclosure и inspector synchronization
- [x] 4.2 Проверить responsive и interaction сценарии для chart-first композиции на целевых breakpoints
- [x] 4.3 Прогнать `npm --prefix apps/codex-session-explorer test` и `openspec validate declutter-project-metrics-screen --strict`

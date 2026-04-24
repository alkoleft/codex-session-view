## Why

Текущий `ProjectMetricsScreen` показывает много полезных данных, но композиционно перегружен:
значительная часть видимой области занята summary cards, legend/toggle-grid, zoom-toolbar,
служебными пояснениями и отдельной карточкой active point. В результате основной объект экрана,
хронологический график, конкурирует за внимание со статической информацией и хуже помогает
исследовать спайки, деградации и смену тренда.

Это нужно исправить сейчас, потому что экран уже стал рабочим инструментом анализа project-level
метрик, и следующая UX-итерация должна не добавлять новые показатели, а сделать существующие
графики быстрее для чтения и расследования.

## What Changes

- Перестроить экран `Project metrics` в chart-first layout, где основной график становится главным
  визуальным фокусом и получает больше вертикального и горизонтального пространства.
- Сжать фильтры, project context и status/meta-пояснения в компактный верхний toolbar без потери
  доступности и явных empty/error/degraded сигналов.
- Перевести summary cards, coverage/help text и secondary controls в progressive disclosure:
  компактные строки, collapsible sections или вспомогательную боковую панель вместо постоянного
  доминирования в основном полотне.
- Объединить active point context и contributing sessions в единый inspection flow, чтобы
  пользователь сначала читал график, а затем уточнял детали выбранной точки в связанном инспекторе.
- Зафиксировать responsive layout для desktop и узких экранов так, чтобы chart viewport не
  деградировал в набор мелких блоков над графиком.

## Capabilities

### New Capabilities
- `project-metrics-layout-focus`: chart-first компоновка экрана `Project metrics` с прогрессивным
  раскрытием вторичной информации и связанным инспектором деталей.

### Modified Capabilities

Нет существующих OpenSpec capabilities в `openspec/specs/`, которые нужно модифицировать.

## Impact

- `apps/codex-session-explorer/src/components/project-metrics-screen.tsx`: переразметка экрана,
  новый layout hierarchy и перераспределение secondary UI.
- `apps/codex-session-explorer/src/components/project-metrics.ts`: возможное уточнение view-model
  для compact summary и inspector-friendly grouping.
- `apps/codex-session-explorer/src/components/project-metrics-screen.test.tsx`: обновление
  сценариев layout, progressive disclosure и chart-focused interaction.
- `apps/codex-session-explorer/src/index.css` и/или локальные component styles: корректировка
  spacing, adaptive breakpoints и визуальной иерархии без изменения backend contract.

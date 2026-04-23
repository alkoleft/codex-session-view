## Why

Слой project/session metrics уже умеет отдавать агрегаты и хронологический список сессий по
`project_key`, но в `codex-session-explorer` нет отдельного экрана, где можно увидеть тренды по
проекту. Из-за этого пользователь вынужден смотреть метрики только внутри одной сессии и не может
быстро заметить рост длительности, токенов, ошибок или провалов между последовательными запусками.

## What Changes

- Добавить в `codex-session-explorer` отдельный экран метрик проекта поверх уже существующего
  backend contract `query_project_metrics`.
- Показать хронологические графики по ключевым метрикам проекта: длительность, суммарные токены,
  ошибки/failed operations, количество tool calls и сессий по времени.
- Добавить project selector, выбор временного окна и переключатель включения/исключения
  spawn-agent вклада.
- Сделать графики кликабельными для перехода к сессии-источнику и синхронизировать выбранную
  точку графика со списком contributing sessions.
- Явно показывать `unknown`/`partial` coverage, чтобы неполные данные не выглядели как нулевые.
- Добавить пустые и degraded-state сценарии: нет project key, нет данных за диапазон, метрики
  доступны только частично.

## Capabilities

### New Capabilities
- `project-metrics-screen`: отдельный экран просмотра project-level метрик с хронологическими
  графиками, фильтрами диапазона и переходом к contributing sessions.

### Modified Capabilities

Нет существующих OpenSpec capabilities: `openspec/specs/` по-прежнему пуст.

## Impact

- `apps/codex-session-explorer/src/App.tsx`: новый экран/режим и загрузка project-level данных.
- `apps/codex-session-explorer/src/components/*`: новые графики, legend, project selector,
  состояния загрузки/empty/error.
- `apps/codex-session-explorer/src/backend*.ts`: возможное уточнение frontend contract вокруг
  project metrics query и навигации к сессии.
- `apps/codex-session-explorer/src/*.test.ts*`: UI и contract tests для графиков, фильтров и
  coverage-state рендера.

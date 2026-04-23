## Why

Экран `Project metrics` уже умеет строить хронологию по `query_project_metrics`, но список
проектов в selector сейчас собирается только из загруженной части `IndexedSessionSummary[]` и
поэтому не отражает весь доступный каталог. Кроме того, экран не различает основную сессию и
subsession/subagent-сессии, из-за чего пользователь не может отделить корневые прогоны от
вложенных запусков и честно анализировать project-level тренды.

## What Changes

- Обеспечить полноту project selector для экрана `Project metrics`, чтобы пользователь видел все
  доступные проекты, а не только проекты из текущей загруженной страницы session catalog.
- Добавить явную классификацию project-metrics сессий на основные и дочерние
  (`main` / `subsession`) на основе доступного session metadata.
- Добавить фильтр по scope/type сессии на экране `Project metrics`, чтобы можно было отдельно
  смотреть только основные сессии, только subsessions или обе категории вместе.
- Синхронизировать selector, project metrics query и contributing sessions list так, чтобы
  выбранные фильтры одинаково влияли на summary, charts и список contributing sessions.
- Зафиксировать empty-state и degraded-state поведение для случаев, когда по выбранному проекту
  есть только дочерние или только основные сессии, либо часть каталога не может быть
  классифицирована.

## Capabilities

### New Capabilities
- `project-metrics-filters`: полнота project selector и фильтрация project-level метрик по типу
  сессии (`main` / `subsession` / `all`) без потери честной coverage-semantics.

### Modified Capabilities

Нет существующих OpenSpec capabilities в `openspec/specs/`, которые нужно модифицировать.

## Impact

- `apps/codex-session-explorer/src/App.tsx`: загрузка project selector data и orchestration
  project-level фильтров.
- `apps/codex-session-explorer/src/components/project-metrics-screen.tsx`: новый UI-фильтр типа
  сессии и обновлённые empty/degraded состояния.
- `apps/codex-session-explorer/src/components/project-metrics.ts`: построение selector options и
  aggregation/view-model с учётом main/subsession classification.
- `apps/codex-session-explorer/src/backend-types.ts` и backend client contract: возможное
  расширение payload для session classification или project catalog.
- `crates/codex-log/src/session.rs` и/или `crates/codex-log/src/session_metrics.rs`: извлечение и
  передача metadata, достаточного для различения root/subsession и для полного project catalog.
- `apps/codex-session-explorer/src/components/project-metrics.test.ts` и backend tests: покрытие
  полноты selector и фильтрации main/subsession.

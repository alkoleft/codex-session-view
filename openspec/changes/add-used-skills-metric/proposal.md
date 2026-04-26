## Why

Сейчас `project metrics` умеют показывать `Enabled skills` как числовую series и отдельно
`used_skills` как rollup-список identifiers, но не умеют считать числовую метрику фактически
использованных skills по сессии. Из-за этого нельзя сравнивать skill-usage intensity между
сессиями, накладывать её на токены/ошибки и быстро видеть, где агент действительно задействовал
навыки, а где они были лишь доступны в runtime context.

## What Changes

- Добавить числовую метрику `used_skills.count`, вычисляемую только из явных
  `skill_identifiers` markers и не зависящую от `Enabled skills`.
- Сохранить существующий `used_skills` rollup-список identifiers, но расширить session/project
  contract companion-count полем с той же conservative coverage semantics.
- Вывести `Used skills` как поддерживаемую metric series и summary signal на `project metrics`
  surfaces без подмены unknown значений нулями или `skills_count`.
- Обновить документацию и тесты, чтобы различие между `Enabled skills` и `Used skills` было
  закреплено на уровне контракта.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `session-metrics`: session/project metrics contract меняется так, чтобы `used_skills` содержал не
  только identifiers rollup, но и числовой count, рассчитанный по тем же explicit usage markers.
- `project-metrics-charts`: chart surfaces меняются так, чтобы `Used skills` можно было смотреть
  как отдельную metric series с сохранением `known` / `partial` / `unknown` semantics.

## Impact

- `crates/codex-log`: структуры metrics payload, materialization/versioning, session/project
  aggregation и unit-тесты для `used_skills`.
- `crates/codex-session-explorer-backend`: backend contract для project/session metrics.
- `apps/codex-session-explorer`: frontend types, project metrics series и view-model tests.
- `docs/session-metrics.md`: фиксация semantics новой числовой метрики `used_skills.count`.

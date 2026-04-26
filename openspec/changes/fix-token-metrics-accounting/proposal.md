## Why

Текущая модель `session_metrics` покрывает не все публичные метрики одинаково надёжно: token
ledger теряет часть usage в multi-thread и `spawn-agent` сценариях, а несколько других metric
groups всё ещё возвращают `unknown` по умолчанию или держатся на частичных эвристиках, хотя
контракт уже обещает их как часть общей аналитики. Основная проблема находится не в рендеринге
экрана, а в том, что логика сбора, агрегации и materialization метрик не везде доведена до
канонического расчёта; UI лишь показывает последствия этих расхождений.

## What Changes

- Провести completeness-аудит всех публичных metric groups в `SessionMetrics` и
  `ProjectMetricsResponse`, разделив их на: корректно считаемые, требующие исправления по текущим
  данным и пока неподдерживаемые без нового источника событий.
- Исправить session/project token accounting так, чтобы итоговые метрики считались по всем
  релевантным `info.tokens` источникам в пределах сессии, а не по последнему встреченному
  snapshot.
- Проверить и довести до корректного контракта остальные metric families: operations, duration,
  context, business review, quality, baseline, derived efficiency, task/factor rollups и
  tool-breakdown contribution там, где текущие источники уже позволяют это сделать.
- Зафиксировать для неподдерживаемых или эвристических метрик явные semantics `known/partial/unknown`,
  чтобы read path и downstream consumers не создавали видимость точного расчёта там, где источника
  ещё нет.
- Переименовать текущую пользовательскую метрику `tokens` в `all tokens`, чтобы она явно означала
  общий token ledger, и добавить отдельную пользовательскую метрику `cached tokens`.
- Синхронизировать backend contract, materialized metrics, документацию и экран project/session
  metrics, чтобы названия, coverage и реальные вычисления совпадали во всех слоях.

## Capabilities

### New Capabilities

Нет.

### Modified Capabilities

- `session-metrics`: меняются правила полноты и расчёта для token ledger и других публичных metric
  groups, включая явное различение реализованных, частичных и пока неподдерживаемых значений.
- `project-metrics-screen`: экран уточняется как consumer скорректированного metrics contract, а не
  как место вычисления метрик.
- `project-metrics-charts`: chart surfaces синхронизируются с обновлённым metrics contract только в
  части отображения и верификации.

## Impact

- `crates/codex-log`: аудит и корректировка session/project metrics logic, materialization version,
  coverage/source semantics и derived groups.
- `crates/codex-session-explorer-backend`: API-контракт project/session metrics и выдача
  скорректированных metric groups.
- `apps/codex-session-explorer`: верификация и минимальная синхронизация consumer labels/coverage с
  уже исправленным backend contract.
- `docs/session-metrics.md` и при необходимости `docs/log-events.md`: фиксация того, какие metric
  groups реально считаются из текущих источников и какие поля остаются неподдержанными.

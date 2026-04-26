## 1. Аудит полноты метрик

- [ ] 1.1 Инвентаризовать все публичные metric groups в `SessionMetrics` и `ProjectMetricsResponse` и зафиксировать для каждой текущий источник, coverage semantics и статус: implemented, partial-by-source или unsupported-with-current-sources.
- [ ] 1.2 Проверить, какие нетокеновые группы обязаны считаться уже сейчас из текущих источников (`operations`, `duration`, `context`, `business_review`, `quality`, `baseline`, `derived`, `tool_breakdown`, `factor/task rollups`) и отметить конкретные разрывы.
- [ ] 1.3 Обновить `docs/session-metrics.md` по итогам аудита, чтобы контракт и документация совпадали по всем metric families.

## 2. Исправление доменной модели метрик

- [ ] 2.1 Перевести session-level token aggregation с одного глобального `latest_token_snapshot` на scope-aware расчёт по последнему накопительному snapshot каждого релевантного thread/scope.
- [ ] 2.2 Согласовать `all tokens`, `cached_input`, `reasoning_output`, `tool_call`, `task` и `spawn_agent` между session totals, task facts и project aggregation без двойного счёта и без подмены `unknown` нулями.
- [ ] 2.3 Исправить или явно зафиксировать нетокеновые metric groups, которые по текущим источникам должны считаться корректнее, чем сейчас.
- [ ] 2.4 Добавить unit tests на multi-thread, `spawn-agent`, ambiguous-scope token scenarios и найденные нетокеновые regressions.

## 3. Materialization и backend contract

- [ ] 3.1 Повысить `METRICS_PROJECTION_VERSION` или эквивалентную materialization version и проверить, что recompute пересобирает corrected metrics payload, а не только token fields.
- [ ] 3.2 Обновить backend transport/view-model contract так, чтобы downstream consumers честно различали corrected, partial и unsupported metric groups.
- [ ] 3.3 Довести backend/read-model contract до явной token semantics: `tokens = all tokens - cached tokens`, `all tokens = полный total`, без локального пересчёта этой пары в UI consumer'ах.
- [ ] 3.4 Обновить `docs/log-events.md`, если реализация меняет обработку `info.tokens`, mapping payload или другие правила обработки событий.

## 4. Consumer sync

- [ ] 4.1 Синхронизировать consumer labels: переименовать текущую пользовательскую метрику `tokens` в `all tokens` там, где это нужно для чтения скорректированного contract.
- [ ] 4.2 Довести `cached tokens` до отдельной consumer-метрики в тех surfaces, где уже показывается token analytics.
- [ ] 4.3 Обновить consumer surfaces так, чтобы по audited metric groups было ясно, где значение corrected, partial или unsupported, не добавляя локальной логики вычисления.
- [ ] 4.4 Обновить frontend tests только как проверку consumer-sync после исправления backend/materialization.

## 5. Проверка и UAT

- [ ] 5.1 Прогнать релевантные Rust и frontend тесты для session/project metrics.
- [ ] 5.2 Выполнить Playwright UAT для актуального project metrics экрана как consumer-проверку после исправления логики сбора, исправить замечания и повторить UAT после каждого найденного UI-дефекта.
- [ ] 5.3 Проверить change через `openspec validate fix-token-metrics-accounting --strict`.

## Context

`crates/codex-log/src/session_metrics.rs` уже формирует широкий публичный контракт:
`operations`, `duration`, `token_ledger`, `tool_breakdown`, `task_metrics`, `used_skills`,
`business_review`, `context`, `quality`, `baseline` и `derived_efficiency`, а
`apps/codex-session-explorer` использует эти поля как consumer. Но по факту
реализация неравномерна: часть групп считается из реальных источников, а часть пока возвращает
`unknown` почти всегда (`generation_ms`, `context_growth`, quality-группа, baseline deltas,
`tokens_per_accepted_task`, `tool_call` contribution, `tool_breakdown.token_contribution`), либо
держится на упрощённых эвристиках (`review_findings`, часть context/review signals).

Отдельно token path действительно содержит явный дефект: `build_token_ledger()` берёт только
последний встреченный `info.tokens` snapshot по сессии. Для сессий с несколькими thread/scope,
особенно с `spawn-agent`, это означает потерю части usage: итоговый ledger зависит от последнего
события, а не от полного набора thread-local накопителей.

Проблема становится заметной на project metrics экране, но первопричина находится ниже: session
totals материализуются с дефектами, а UI лишь показывает уже искажённые значения. Дополнительно
здесь есть риск ложного отрицательного результата после фикса: materialized payload в
`session-metrics.sqlite` продолжит отдавать старые значения, пока не сменится projection version
или не будет выполнен recompute.

## Goals / Non-Goals

**Goals:**

- Провести системный аудит всех публичных metric groups и закрепить для каждой группы один из
  статусов: implemented, partial-by-source, unsupported-with-current-sources.
- Сделать session token ledger инвариантно полным для multi-thread и `spawn-agent` сессий.
- Согласовать session totals, task facts и project aggregates вокруг одной модели `all tokens`.
- Проверить и, где возможно, довести до корректного расчёта нетокеновые metric groups без
  расширения raw event sources.
- Свести UI-изменения к минимальной синхронизации consumer labels и coverage после исправления
  логики сбора.
- Сохранить coverage/source semantics и режим `include_spawn_agents`.
- Гарантировать, что после релиза corrected totals действительно попадут в read path через version
  bump/recompute.

**Non-Goals:**

- Не менять формат сырых `info.tokens` событий в логах Codex.
- Не вводить денежные cost-метрики и price lookup.
- Не обещать вычисление тех metric groups, для которых в текущих нормализованных событиях нет
  достаточного источника истины.
- Не рассматривать UI как primary fix path для текущей проблемы.
- Не перерабатывать весь project metrics UX вне необходимых consumer-sync изменений.
- Не переименовывать внутренние storage keys без явной необходимости, если достаточно UI labels и
  контрактной семантики.

## Decisions

### 1. Session token ledger больше не вычисляется по одному глобальному последнему snapshot

Решение: считать session-level token ledger из последнего накопительного snapshot на каждый
relevant thread/scope, а затем агрегировать эти thread-level значения в итоговый session ledger с
той же логикой, что и для task/spawn contributions.

Причина: `info.tokens` already cumulative внутри scope, поэтому суммировать все snapshots по ленте
нельзя, но и брать только последний snapshot по всей сессии тоже нельзя. Корректный слой должен
использовать последний snapshot каждого thread и затем складывать независимые вклады.

Альтернатива: оставить текущий `latest_token_snapshot(events)` и пытаться объяснить расхождение
только через UI. Это не исправляет неполный счётчик и закрепляет ложные totals в project analytics.

### 2. Completeness-аудит проходит по всем metric groups, а не только по token ledger

Решение: change сначала инвентаризует каждую публичную metric group в `SessionMetrics` и
`ProjectMetricsResponse`, затем для каждой группы выбирает одно из действий: исправить расчёт,
оставить `partial/unknown` с явным source-based объяснением или сузить ожидания в
документации/consumer contract.

Причина: сейчас проблема шире токенов. Например, `generation_ms`, `context_growth`, quality,
baseline и часть derived/tool contribution полей формально присутствуют в контракте, но не имеют
полноценной реализации. Без такого аудита можно починить только token totals и оставить остальные
ошибочные ожидания нетронутыми.

Альтернатива: ограничиться token fixes и считать остальные `unknown` “нормой”. Это не решает
запрос пользователя проверить все метрики и оставляет contract drift между кодом, materialized
payload и docs.

### 3. `all tokens` становится consumer-label для канонического общего ledger

Решение: consumer-facing label `tokens` заменить на `all tokens`, сохранив за доменной моделью
смысл общего token total для session и project reads.

Причина: после добавления отдельного `cached tokens` label `tokens` становится неоднозначным и не
показывает, что речь идёт о полном расходе, включающем все доступные token dimensions.

Альтернатива: оставить `tokens` как есть и просто добавить ещё одну cached-series. Это повышает
риск неверного чтения consumer surfaces и не помогает различить aggregate total от cached slice.

### 4. `cached tokens` продвигается из breakdown в first-class consumer metric

Решение: `cached_input` в доменной модели сохранить как source field, а в consumer surfaces
подавать как явную метрику `cached tokens` с теми же coverage/source semantics, что и у других
основных token series.

Причина: нужное поле уже существует в `TokenLedger`, но пользовательская задача требует видеть его
наравне с общим total, а не прятать под техническим названием `Cached input`.

Альтернатива: добавить только tooltip/detail field без отдельной series. Это не решает задачу про
метрику, которую можно сравнивать по хронологии и по summary surfaces.

### 5. Для нетокеновых групп приоритет у честной coverage semantics, а не у synthetic значений

Решение: если metric group нельзя надёжно посчитать из текущих normalized events, operation
projection или indexed metadata, система оставляет её `partial/unknown` и делает это явно в docs и
consumer outputs, вместо synthetic zeros или фиктивных “готовых” labels.

Причина: часть текущих групп специально инициализируется `unknown()`, и это лучше, чем ложная
точность. Но change должен проверить, что такое поведение осознанно зафиксировано и одинаково
интерпретируется на read path и в materialized payload.

Альтернатива: заполнять пробелы эвристиками “на глаз”. Это быстро повышает число заполненных полей,
но ломает доверие к метрикам.

### 6. Spawn-agent режим остаётся read-time представлением, а не отдельным persisted total

Решение: материализованный token ledger хранит полные `all tokens` и отдельный `spawn_agent`
contribution, а `include_spawn_agents=false` продолжает вычисляться на read path вычитанием
дочернего вклада.

Причина: так сохраняется один persisted source of truth и не появляется две конкурирующие версии
session/project totals.

Альтернатива: сохранять два итоговых totals (`with_spawn` и `without_spawn`). Это дублирует данные
и усложняет миграцию без необходимости.

### 7. Исправление требует явной invalidation materialized metrics

Решение: bump `METRICS_PROJECTION_VERSION` или эквивалентную materialization version и включить
recompute как обязательную часть реализации/валидации change.

Причина: без invalidation старый JSON payload в `session-metrics.sqlite` продолжит отдавать
неполные token totals, даже если формула в Rust уже исправлена.

Альтернатива: надеяться на lazy recompute только для новых сессий. Это оставит исторические project
charts в смешанном состоянии.

## Risks / Trade-offs

- [Risk] Thread-local snapshots могут приходить без надёжного `thread_id`. -> Mitigation: такие
  случаи оставлять `unknown/partial`, не смешивая их с подтверждёнными thread totals.
- [Risk] Completeness-аудит выявит группы, которые сейчас вообще не из чего считать. -> Mitigation:
  для таких групп явно закрепить `unsupported-with-current-sources` в docs/specs вместо скрытого
  product debt.
- [Risk] Root thread может уже включать часть дочернего usage в некоторых логах. -> Mitigation:
  закрепить один источник агрегации, покрыть mixed parent/spawn cases unit-тестами и не
  суммировать root snapshot с дочерним вкладом без подтверждённой границы scope.
- [Risk] Переименование consumer-label может сломать UI tests и пользовательские привычки. ->
  Mitigation: обновить consumer tests, но не позволять label-work подменить основной fix в
  `codex-log`.
- [Risk] Version bump приведёт к полному recompute и удлинит первый запуск. -> Mitigation:
  использовать существующий recompute flow и ограниченный project-scoped rebuild там, где это
  возможно.

## Migration Plan

1. Инвентаризовать все публичные metric groups и зафиксировать для каждой текущий источник,
   coverage semantics и статус реализации.
2. Исправить session-level token aggregation в `codex-log` и добавить тесты на multi-thread и
   `spawn-agent` token scenarios.
3. Проверить нетокеновые metric groups и довести до корректного расчёта те, что уже должны
   считаться из текущих источников.
4. Обновить project aggregation, derived reads, docs и consumer labels так, чтобы `all tokens`,
   `cached tokens` и остальные audited groups использовали единый корректный контракт.
5. Повысить materialization version и прогнать recompute path в тестах/валидации.
6. Проверить consumer surfaces после recompute только как подтверждение исправленной логики сбора.

## Open Questions

- Нужно ли в публичном TypeScript contract переименовывать ключ `tokens` в `allTokens`, или для
  этого change достаточно пользовательского label при сохранении обратной совместимости ключей?
- Есть ли реальные historical logs, где root snapshot уже включает child usage, и нужен ли для них
  отдельный degraded/partial режим вместо прямой суммы по thread snapshots?
- Какие metric groups в текущем change считаем допустимо остающимися `unknown`, если live-source
  действительно отсутствует: quality, baseline, generation duration, tool token contribution или
  только их часть?

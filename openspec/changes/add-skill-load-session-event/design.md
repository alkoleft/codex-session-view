## Context

После уточнения semantics вокруг `Enabled skills` и `Used skills` стало видно, что текущий
session-log contract слишком неявный: факт загрузки skill не имеет собственного event type и
восстанавливается через `skill_identifiers`, которые readers дописывают в shell/tool payload.
Такой подход плохо отделяет signal от контекста:

- quoted `skills_instructions` и `Available skills` block могут содержать пути `.../SKILL.md`,
  хотя это не usage/load signal;
- shell `output` и результаты `rg` тоже могут нести те же строки;
- explicit `<skill>...</skill>` marker, напротив, уже является фактом загрузки skill, но живёт как
  обычный message text, а не как first-class event.

Проблема уже затрагивает несколько слоёв: readers, session metrics, materialized storage,
документацию event mapping и любые consumer-ы `used_skills`.

## Goals / Non-Goals

**Goals:**

- Ввести отдельный canonical event type `skill.load` в нормализованном session log.
- Сделать источник `used_skills` в `session_metrics` явным и трассируемым к этому event type.
- Отделить startup context / available skills listing от фактической загрузки skill.
- Сохранить возможность controlled compatibility с legacy session logs, где нового event type ещё
  нет.

**Non-Goals:**

- Не перерабатывать весь event taxonomy вокруг skills beyond `skill.load`.
- Не менять значение `skills_count` / `Enabled skills`; оно остаётся привязанным к startup list.
- Не вводить новый пользовательский UI surface сам по себе; change описывает event contract и его
  metrics consumers.

## Decisions

### 1. `skill.load` становится отдельным normalized event type

Решение: readers SHALL порождать отдельный event type `skill.load`, а не прятать signal внутри
payload fields shell/tool events.

Почему так:

- consumers смогут читать skill-load как first-class событие, не сканируя чужие payloads;
- session log станет более объяснимым и пригодным для UAT/debug;
- уменьшается риск ложноположительных usage markers из quoted text.

Альтернатива: оставить только `skill_identifiers` enrichment внутри shell/tool payload. Отклонено,
потому что такой enrichment не выражает отдельный доменный факт и уже показал склонность смешивать
контекст и usage evidence.

### 2. Primary source для `skill.load` — explicit `<skill>...</skill>` marker

Решение: основным источником `skill.load` считается explicit message marker `<skill>...</skill>`
с как минимум `name`, а quoted `skills_instructions` / `Available skills` text MUST NOT порождать
этот event.

Почему так:

- это наиболее точный сигнал фактической загрузки skill;
- он already semantically distinct from startup context;
- он не зависит от особенностей shell/stdout formatting.

Альтернатива: выводить `skill.load` из любого сообщения или output, где встретился путь к
`SKILL.md`. Отклонено как слишком шумный и неустойчивый heuristic.

### 3. Shell/tool чтение `.../SKILL.md` остаётся только controlled fallback

Решение: если explicit `<skill>` marker отсутствует, readers MAY поддерживать controlled fallback
по shell/tool input, который явно открывает `.../SKILL.md`, но не по stdout/output/results. Такой
fallback должен либо напрямую порождать `skill.load`, либо переводиться в совместимый
intermediate signal, из которого `session_metrics` может восстановить usage для legacy logs.

Почему так:

- часть старых логов могла фиксировать только чтение `SKILL.md` через command/tool input;
- это сохраняет совместимость без возврата к шумному сканированию stdout.

Альтернатива: полностью удалить fallback. Отклонено, потому что это может обнулить `used_skills`
для исторических сессий, где explicit `<skill>` marker ещё не нормализовался.

### 4. `session_metrics` читает `skill.load` как canonical source

Решение: `extract_used_skills(...)` SHALL строить список `used_skills.identifiers` из
`skill.load`, а legacy fallback использовать только когда таких events нет.

Почему так:

- появляется чёткая precedence: first-class event > legacy shell evidence > unknown;
- metrics semantics перестают зависеть от структуры произвольных tool payloads;
- materialized storage и project rollups остаются совместимыми по response shape.

## Risks / Trade-offs

- [Risk] Появится двойной счёт, если одна и та же загрузка skill даст и explicit marker, и shell
  fallback. → Mitigation: дедуплицировать по identifier внутри session metrics и задать explicit
  precedence в extraction.
- [Risk] Старые materialized payloads в `session-metrics.sqlite` продолжат отдавать прежнюю
  семантику. → Mitigation: bump `METRICS_PROJECTION_VERSION`, чтобы новый reader/metrics contract
  инвалидировал старый cache.
- [Risk] Часть исторических логов не содержит ни explicit marker, ни надёжный shell input. →
  Mitigation: оставлять `used_skills` как `unknown`, а не подменять эвристическим count.
- [Risk] Введение нового event type потребует синхронного обновления docs/tests/projector logic.
  → Mitigation: включить docs/log-events и reader/session-metrics regression tests в обязательный
  task bundle.

## Migration Plan

1. Добавить `skill.load` в event taxonomy и readers normalization.
2. Переключить `session_metrics` на canonical read of `skill.load` с legacy fallback.
3. Обновить `docs/log-events.md` и `docs/session-metrics.md`.
4. Поднять projection version и прогнать recompute/materialization tests.
5. Проверить UAT на реальной сессии, где quoted `Available skills` раньше раздувал `used_skills`.

## Open Questions

- Нужно ли materialize-ить дополнительные поля `skill.load` кроме identifier/path/source, или для
  текущих consumers достаточно identifier?
- Следует ли projector/timeline surface показывать `skill.load` как отдельный пользовательский
  timeline node, или пока достаточно иметь его только в raw event stream и metrics extraction?

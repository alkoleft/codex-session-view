## Context

Текущая реализация `used_skills` в `crates/codex-log/src/session_metrics.rs` уже консервативно
извлекает explicit skill-usage evidence: `extract_used_skills(...)` берёт только
`payload.skill_identifiers`, дедуплицирует identifiers в рамках сессии и формирует
`UsedSkillsMetrics`. На project уровне `aggregate_used_skills(...)` строит rollup-список skills по
matching sessions. Это даёт корректный список использованных навыков, но не даёт числового
показателя, пригодного для chronologic charts и summary cards.

Одновременно `apps/codex-session-explorer/src/components/project-metrics.ts` уже поддерживает
numeric series вроде `Enabled skills`, `MCP servers`, `Tasks` и `Spawn calls`, но `used_skills`
выводится только отдельным списком в `ProjectMetricsViewModel`. В итоге пользователь видит, какие
skills встречались в окне, но не может сравнивать количество реально использованных skills между
сессиями и коррелировать эту величину с токенами, отказами или длительностью.

## Goals / Non-Goals

**Goals:**

- Добавить числовой показатель `used_skills.count` для session-level и project-level metrics.
- Считать этот показатель строго по тем же explicit markers, что и текущий `used_skills` list.
- Сохранить conservative semantics: отсутствие явных usage markers остаётся `unknown`, а не `0`.
- Вывести новую метрику в `project metrics` как series/summary, не ломая текущий list rollup.
- Обновить materialized payload, документацию и тесты так, чтобы новый контракт был устойчивым.

**Non-Goals:**

- Не выводить usage из текста `skills_instructions`, `### Available skills` или `runtime_context`.
- Не заменять `used_skills` count на event-count по каждому повторному marker внутри одной сессии.
- Не менять текущую семантику `Enabled skills` / `skills_count`.
- Не перестраивать весь `project metrics` UX за пределами новой series и связанного summary signal.
- Не менять нормализацию raw events и не обновлять `docs/log-events.md`, если набор signals
  остаётся прежним.

## Decisions

### 1. `used_skills.count` живёт внутри существующей metric group, а не в `factors`

Решение: расширить `UsedSkillsMetrics` и `UsedSkillsRollup` полем `count: CoveredMetric<u64>`,
вместо добавления нового top-level factor field.

Причина: числовая метрика и identifiers-list описывают один и тот же semantic layer: explicit
skill usage. Их coverage/source semantics должны двигаться вместе, а не расходиться между
разными ветками payload.

Альтернатива: добавить `used_skills_count` в `FactorMetadata` и `ProjectFactorRollups`. Это
смешивает startup-context factors с observed runtime usage и повышает риск путаницы с
`skills_count`.

### 2. Session count считается как число уникальных identifiers внутри сессии

Решение: session-level `used_skills.count` равен размеру дедуплицированного множества
`skill_identifiers`, которое уже используется для `used_skills.identifiers`.

Причина: пользователь запрашивает метрику использованных skills, а существующий rollup уже
опирается на уникальные identifiers. Повторный marker того же skill в одной сессии должен
сигнализировать usage evidence, но не превращать метрику в event counter.

Альтернатива: считать каждое появление skill marker как отдельное usage event. Это дало бы другую
метрику с иной семантикой и плохо сочетается с уже существующим списком unique identifiers.

### 3. Project count считается как число уникальных skills в выбранном окне

Решение: `response.used_skills.count` на project уровне равен размеру итогового уникального rollup
списка skills для matching sessions, а не сумме session counts.

Причина: проектный summary должен отвечать на вопрос "сколько разных skills реально использовалось
в окне", тогда как per-session chart series отвечает на вопрос "сколько skills использовалось в
этой конкретной сессии". Эти два представления дополняют друг друга и не дублируют значения.

Альтернатива: суммировать session counts в project summary. Это дало бы кумулятивный usage volume,
но вводило бы в заблуждение рядом с существующим identifiers rollup и затрудняло бы quick reading.

### 4. Unknown остаётся unknown и не подменяется ни нулём, ни `Enabled skills`

Решение: если explicit usage markers не найдены, `used_skills.count` остаётся `unknown`, даже если
для той же сессии известен `skills_count`.

Причина: текущая contract line уже разделяет "skills были подключены" и "skills есть явные
доказательства использования". Новая метрика не должна ослаблять эту границу.

Альтернатива: возвращать `0`, когда usage markers нет. Это создавало бы ложную точность и ломало
сопоставимость с текущим `used_skills` rollup, который в таком случае тоже остаётся неизвестным.

### 5. Изменение требует version bump и recompute materialized metrics

Решение: повысить версию materialized metrics и прогнать recompute, чтобы старые payload без
`used_skills.count` не считались актуальными только из-за `serde(default)`.

Причина: новый numeric field должен быть гарантированно доступен и в исторических сессиях после
пересчёта, иначе `project metrics` будут смешивать старые payload без count и новые payload с ним.

Альтернатива: опираться только на backward-compatible defaults. Это сохранило бы чтение старых
записей, но оставило бы project analytics в полусырых состояниях до ручного rebuild.

### 6. UI показывает count как chart series и summary signal, но сохраняет skills-list отдельно

Решение: добавить `Used skills` в supported project metric series и summary surfaces, не убирая
существующий list/table used skills.

Причина: count и список отвечают на разные вопросы. Count удобен для quick comparison и chart
correlation, список нужен для конкретного состава skills и inspection.

Альтернатива: заменить список одной numeric метрикой. Это ухудшит объяснимость аналитики и сделает
невозможным понять, какие именно skills стояли за числом.

## Risks / Trade-offs

- [Risk] Пользователь воспримет `used_skills.count` как usage-event volume, а не unique-skill
  count. -> Mitigation: закрепить semantics в docs/specs и назвать surface просто `Used skills`
  только там, где tooltip/description явно говорят о unique skills.
- [Risk] Старые materialized payload без recompute дадут mixed coverage и нестабильные charts. ->
  Mitigation: version bump + recompute как обязательная часть change.
- [Risk] Frontend может случайно подставить `response.used_skills.skills.length` в per-session
  series. -> Mitigation: отдельные tests на session rows и explicit selectMetric из
  `session.used_skills.count`.
- [Risk] Частичные/unknown cases окажутся визуально неотличимы от нуля. -> Mitigation: сохранить
  existing chart gap behavior и проверять его в Vitest/Playwright.

## Migration Plan

1. Расширить Rust metrics structs и serde defaults для `used_skills.count`.
2. Добавить session/project aggregation rules, unit-tests и version bump materialized metrics.
3. Протянуть новое поле через backend/frontend contracts и включить `Used skills` в chart series.
4. Обновить `docs/session-metrics.md` и consumer tests под новую семантику.
5. Выполнить recompute-aware validation, затем проверить `project metrics` UI через Playwright.

## Open Questions

- Нужен ли отдельный session-level summary card `Used skills` в деталях одной сессии, или на этом
  change достаточно project analytics surfaces?
- Стоит ли делать `Used skills` series видимой по умолчанию, или оставить её opt-in рядом с
  другими factor-like signals?

## Why

Текущая логика `used_skills` опирается на косвенные markers вроде `skill_identifiers`, которые могут
появляться из shell/tool payload и смешивать реальную загрузку skill с quoted `Available skills`,
stdout search results и другими упоминаниями `SKILL.md`. Из-за этого session log не различает
явный факт загрузки skill и контекстные тексты, а consumer-ы вроде `session_metrics` вынуждены
догадываться о семантике задним числом.

## What Changes

- Добавить отдельный нормализованный event type `skill.load` в session log для явного факта
  загрузки/активации skill.
- Зафиксировать, какие raw signals могут порождать `skill.load`, и запретить порождать его из
  quoted `skills_instructions`, `Available skills` и произвольного stdout/output.
- Перевести `used_skills` extraction в `session_metrics` на чтение `skill.load` как canonical
  source, сохранив при необходимости контролируемый fallback для legacy evidence.
- Обновить документацию event mapping и metrics semantics, чтобы различие между `Enabled skills` и
  `Used skills` было зафиксировано в контракте, а не только в коде.

## Capabilities

### New Capabilities
- `session-log-skill-load-event`: нормализация и публикация отдельного session-log event для
  явной загрузки skill без смешения с quoted context text.

### Modified Capabilities
- `session-metrics`: `used_skills` и связанные consumers меняют источник истины и должны
  трактовать `skill.load` как canonical usage/load evidence.

## Impact

- `crates/codex-log/src/events/readers*.rs`: выделение и нормализация `skill.load`.
- `crates/codex-log/src/session_metrics.rs`: extraction `used_skills` из отдельного event type.
- `docs/log-events.md` и `docs/session-metrics.md`: фиксация нового event contract и источника для
  `used_skills`.
- Тесты readers/metrics и, при необходимости, backend/frontend consumers, завязанные на semantics
  `used_skills`.

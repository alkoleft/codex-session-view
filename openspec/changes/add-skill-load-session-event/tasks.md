## 1. Event Taxonomy

- [ ] 1.1 Добавить `skill.load` в event taxonomy и определить минимальный payload contract для
      identifier/source/path.
- [ ] 1.2 Обновить readers normalization так, чтобы explicit `<skill>...</skill>` marker порождал
      отдельный `skill.load` event.
- [ ] 1.3 Ограничить shell/tool fallback только explicit input/command references на `SKILL.md`,
      исключив stdout/output/search-result false positives.

## 2. Metrics Integration

- [ ] 2.1 Перевести `extract_used_skills(...)` на canonical чтение `skill.load` с controlled legacy
      fallback и дедупликацией identifiers.
- [ ] 2.2 Поднять `METRICS_PROJECTION_VERSION`, чтобы materialized cache инвалидировался после
      смены semantics.
- [ ] 2.3 Проверить, что project/session aggregates и response shape `used_skills` не ломаются для
      existing consumers.

## 3. Docs and Validation

- [ ] 3.1 Обновить `docs/log-events.md` под новый `skill.load` event и правила fallback.
- [ ] 3.2 Обновить `docs/session-metrics.md`, чтобы `used_skills` ссылался на `skill.load` как на
      canonical source.
- [ ] 3.3 Добавить regression tests на explicit skill marker, quoted `Available skills` block и
      stdout-only `SKILL.md` mentions.
- [ ] 3.4 Прогнать `openspec validate add-skill-load-session-event --strict`.

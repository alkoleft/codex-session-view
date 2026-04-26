## 1. Metrics Contract

- [x] 1.1 Расширить `UsedSkillsMetrics` и `UsedSkillsRollup` полем `count: CoveredMetric<u64>` с
      backward-compatible `serde(default)` поведением.
- [x] 1.2 Посчитать session-level `used_skills.count` по unique explicit `skill_identifiers` и
      project-level `used_skills.count` по unique rollup в выбранном окне, не подменяя unknown
      значениями из `skills_count`.
- [x] 1.3 Повысить версию materialized metrics и покрыть Rust-тестами cases: repeated markers,
      missing markers, project aggregation и coverage propagation.

## 2. Consumer Surfaces

- [x] 2.1 Протянуть `used_skills.count` через backend/frontend contract и обновить
      `docs/session-metrics.md` под новую семантику.
- [x] 2.2 Добавить `Used skills` в `project metrics` series/summary surfaces без замены
      существующего used-skills rollup списка.
- [x] 2.3 Обновить Vitest-покрытие для chart rows, coverage gaps и различия между
      `Enabled skills` и `Used skills`.

## 3. Validation

- [x] 3.1 Выполнить `cargo test --workspace`,
      `npm --prefix apps/codex-session-explorer test` и
      `npm --prefix apps/codex-session-explorer run build`.
- [x] 3.2 Выполнить `openspec validate add-used-skills-metric --strict`.
- [x] 3.3 Проверить `project metrics` экран через Playwright и исправить все замечания до
      завершения change.

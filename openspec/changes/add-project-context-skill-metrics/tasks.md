## 1. Session Metrics Contract

- [ ] 1.1 Расширить Rust-модель `SessionMetrics`/`ProjectMetricsResponse` полями для полного project token ledger, session-level `task_count`/`spawn_agent_calls`, factor rollups и used skills с backward-compatible `serde` defaults.
- [ ] 1.2 Зафиксировать в `codex-log` нормативную цепочку source precedence для `start_context_size`, включая fallback к первому непустому `info.tokens`.
- [ ] 1.3 Довести `input/output/cached_input/reasoning_output/task/spawn_agent` token metrics до project-level aggregation без подмены `unknown` нулями.
- [ ] 1.4 Довести `skills_count`, `mcp_server_count`, session-level `task_count` и `spawn_agent_calls` до project-level aggregation без подмены `unknown` нулями.
- [ ] 1.5 Зафиксировать и покрыть тестами поведение `include_spawn_agents=true|false` для session-level и project-level агрегации.

## 2. Skill Usage Extraction

- [ ] 2.1 Проверить текущие нормализованные payloads и тестовые фикстуры на наличие явного marker-а использования skill.
- [ ] 2.2 Если marker уже есть, добавить extraction used skills в materialized session metrics; если marker отсутствует, ввести additive normalization в `codex-log`.
- [ ] 2.3 Обновить `docs/log-events.md`, если change добавляет новый usage marker, payload field или меняет правила нормализации событий.

## 3. Backend and UI Output

- [ ] 3.1 Расширить backend transport/types для выдачи новых factor metrics, полного token ledger, session-level task/spawn aggregates и used-skills rollup в `codex-session-explorer`.
- [ ] 3.2 Добавить в project metrics view model и charts series для `start_context_size`, `skills_count`, `mcp_server_count`, token breakdown, session-level `task_count` и `spawn_agent_calls`.
- [ ] 3.3 Добавить на project metrics экран отдельный блок used skills с identifier и project-level usage/session counts за выбранное окно.

## 4. Verification

- [ ] 4.1 Добавить unit tests для source precedence стартового контекста, project token aggregation, `include_spawn_agents`, session-level task/spawn counts и unknown semantics used skills.
- [ ] 4.2 Добавить backend/UI tests для project response и project metrics screen с token breakdown, session-level task/spawn metrics, factor metrics и used-skills output.
- [ ] 4.3 Прогнать релевантные Rust и frontend тесты, затем проверить change через `openspec validate add-project-context-skill-metrics --strict`.

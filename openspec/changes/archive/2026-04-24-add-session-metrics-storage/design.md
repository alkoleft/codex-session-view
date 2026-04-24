## Context

`codex-session-view` сейчас сфокусирован на чтении, нормализации, replay и просмотре отдельных Codex-сессий. Каноническая модель событий находится в `crates/codex-log`: `EventRecord`, session catalog, operation stream projection и tree/view-model. `codex-session-explorer` уже может показывать локальные метрики выбранной сессии во frontend, но эти вычисления не являются общим доменным контрактом, не агрегируются по проектам и не дают устойчивого слоя для поиска деградаций.

Для отслеживания деградаций нужен слой, который связывает session-level показатели с project identity, временным окном и факторами влияния: модель, reasoning effort, стартовый контекст, context growth/compaction, skills, MCP, ветка, версия CLI, sandbox/approval mode, agent role, типы tool/MCP/shell операций, ошибки, review cycles, замечания, feedback/evaluator/guardrail markers, abort/failure и token usage. Этот слой должен строиться поверх нормализованных событий и существующего indexed catalog, а не парсить raw Codex-логи повторно.

## Goals / Non-Goals

**Goals:**

- Ввести каноническую модель метрик сессии в `codex-log`.
- Привязать каждую запись метрик к проекту и session metadata.
- Поддержать materialized storage для повторного чтения, сравнения и будущего dashboard/export.
- Дать frontend и будущему CLI один контракт чтения метрик.
- Поддержать project-level token ledger, session chronology, task-level breakdown, tool command taxonomy и business metrics для review workflow.
- Для каждой метрики хранить coverage/confidence, outcome и duration breakdown.
- Поддержать regression baseline и efficiency proxies поверх materialized metrics.
- Сохранить существующую модель `EventRecord` как источник истины для событий и operation status.

**Non-Goals:**

- Не строить полноценный observability backend или distributed tracing систему.
- Не вводить отдельный raw-log parser для метрик.
- Не рассчитывать денежную стоимость без явного источника цен или cost-полей в логах.
- Не менять правила нормализации событий в рамках первого шага, если для метрик достаточно существующих projected fields.
- Не делать проектные метрики единственным источником для transcript/timeline UI.

## Decisions

### 1. Метрики считаются в `codex-log`, а UI только отображает

Решение: вынести расчёт session metrics в Rust-ядро рядом с replay/tree/projector, а frontend использовать как потребителя готового контракта.

Причина: `codex-log` уже владеет нормализованными событиями, operation metadata и session catalog. Если оставить расчёт только в React, появятся разные трактовки метрик для UI, CLI и будущего storage.

Альтернатива: продолжать считать всё во frontend. Это быстрее для одного экрана, но не даёт project-level агрегации, повторного использования и стабильного тестируемого контракта.

### 2. Project identity строится из metadata session catalog

Решение: первичный ключ проекта формируется из нормализованного `cwd` и, если доступны, `git_origin_url`/`git_branch`. Запись метрик хранит исходные metadata-поля отдельно, чтобы можно было пересчитать project identity без потери данных.

Причина: текущий indexed catalog уже читает `cwd`, `git_sha`, `git_branch`, `git_origin_url`, `cli_version`, `model`, `reasoning_effort`, `sandbox_policy`, `approval_mode`, `agent_role` и `tokens_used` из `state_*.sqlite`.

Альтернатива: считать проектом только `cwd`. Это просто, но плохо переносит перемещение checkout и не отличает forks/remotes.

### 3. Storage хранит materialized session metrics, а не копию transcript

Решение: хранить одну materialized запись на session id и набор агрегатов по проекту/окну. Transcript, tree и raw события остаются в существующих источниках.

Причина: метрики нужны для быстрых сравнений и trend UI, но не должны дублировать весь лог и создавать второй источник истины по событиям.

Альтернатива: хранить все нормализованные события в новой БД. Это полезно для будущего полнотекстового поиска и исторического API, но шире текущего change.

### 4. Деградации выражаются как сравнение снимков метрик

Решение: первый слой фиксирует raw aggregates и derived rates: duration, event count, thread count, message count, operation count, tool/shell/MCP/collab breakdown, task breakdown, review/comment counts, success/failure/error/abort counts, token totals при наличии. Правила alerting/anomaly detection остаются следующим слоем.

Причина: без стабильного набора базовых метрик alerting быстро станет набором неявных эвристик.

Альтернатива: сразу внедрить thresholds и regression scoring. Это преждевременно без исторической базы и согласованных baseline.

### 5. Spawn-agent вклад является измерением, а не отдельной сессией по умолчанию

Решение: metrics model хранит root-session показатели и spawn-agent contribution отдельно, а read API поддерживает режимы `include_spawn_agents=true|false`.

Причина: для оценки стоимости проекта spawn-agent вклад нужен в итоговом счётчике, но для анализа деградации основной сессии часто важно увидеть картину без дочерних агентных запусков.

Альтернатива: всегда включать spawn agents в session totals. Это проще, но скрывает источник роста затрат и длительности.

### 6. Tool taxonomy строится как отдельная нормализация поверх operation metadata

Решение: добавить классификатор command/tool categories: search, edit, web_search, test, build, git, filesystem, mcp, collaboration, other. Классификатор использует `event_type`, `tool_name`, shell parsed commands и projected fields.

Причина: raw tool names недостаточно для анализа деградаций: `shell.result` может быть тестом, поиском, сборкой или git-операцией, а для трендов нужен стабильный бизнесовый разрез.

Альтернатива: показывать только `tool_name`. Это теряет важный сигнал и плохо сравнивает сессии между собой.

### 7. Business metrics извлекаются из workflow markers, а неизвестное не подменяется нулём

Решение: review cycles, review comments/findings и related business metrics считаются только там, где в логах есть распознанные markers: review tool calls, plan/review text structure, artifact names или будущие явные event fields.

Причина: эти показатели полезны для оценки качества agent workflow, но эвристики по свободному тексту легко дают ложную точность.

Альтернатива: считать review comments регулярками по transcript. Это можно добавить как экспериментальный слой, но не как каноническую метрику первого уровня.

### 8. Каждая метрика несёт coverage/confidence

Решение: публичный metrics contract должен различать `known`, `partial` и `unknown`, а также хранить source evidence для групп, где это возможно.

Причина: OpenTelemetry GenAI metrics рекомендует не репортить token usage, если его нельзя получить надёжно. Для локального post-hoc анализа это означает, что неизвестное значение нельзя превращать в ноль.

Альтернатива: возвращать `0` для отсутствующих разрезов. Это упрощает UI, но ломает анализ деградаций и занижает стоимость старых/частичных логов.

### 9. Внутренняя модель следует span-like иерархии

Решение: метрики агрегируются по иерархии `project -> session/trace -> agent/spawn-agent -> task/turn -> generation/tool/mcp/review`.

Причина: современные agent tracing системы представляют agent run как вложенные spans: agent, generation/response, tool/function, guardrail, handoff, evaluator. Такая модель лучше объясняет деградации, чем плоский список событий.

Альтернатива: хранить только session totals. Это быстрее для MVP, но не позволяет локализовать рост стоимости или ошибок до task/tool/model слоя.

### 10. Duration и outcome являются first-class metrics

Решение: каждая session/operation aggregate должна иметь outcome и, где возможно, duration breakdown: total, generation/model, tool/shell/MCP, spawn-agent и unknown/idle gap.

Причина: latency/error rate/cost являются стандартными monitoring разрезами для LLM приложений. Для локальных Codex-сессий duration breakdown нужен, чтобы понять, деградация вызвана моделью, инструментами, тестами, MCP или orchestration gaps.

Альтернатива: хранить только timestamp начала/конца. Это не даёт причины деградации.

### 11. Feedback/evaluator/guardrail metrics зарезервированы как явный слой качества

Решение: feedback score, evaluator result, guardrail triggered и review outcome должны быть отдельными metric groups с `unknown`, если явного источника нет.

Причина: LangSmith-подобные observability системы используют feedback scores как отдельный monitoring сигнал, а OpenAI Agents SDK и OpenInference выделяют guardrail/evaluator как отдельные span kinds. Эти сигналы нельзя смешивать с техническими errors.

Альтернатива: считать качество по отсутствию ошибок. Это не отражает реального качества agent workflow.

### 12. Derived metrics строятся только поверх покрытых базовых метрик

Решение: efficiency metrics вроде tokens per successful session, tokens per accepted task и review findings per 1k tokens считаются только если все входные базовые метрики имеют достаточный coverage.

Причина: derived метрики полезны для поиска улучшений, но при частичных исходных данных создают ложную точность.

Альтернатива: считать derived значения best-effort. Это опасно для сравнения проектов и периодов.

## Risks / Trade-offs

- [Risk] Project identity может быть неоднозначным для сессий без `cwd` или git metadata. -> Mitigation: хранить `project_key="unknown"` только как явное degraded state и не смешивать такие сессии с нормальными project aggregates.
- [Risk] Старые логи могут не иметь operation metadata. -> Mitigation: метрики, зависящие от operation stream, возвращают `null`/`n/a`, а независимые показатели вроде event/message/error count остаются доступными.
- [Risk] `tokens_used` из SQLite и `info.tokens` из событий могут расходиться. -> Mitigation: хранить источник token metric явно и не суммировать разные источники без правила приоритета.
- [Risk] Разрезы token usage `tool call`, `task` и `spawn agent` могут быть доступны не во всех логах. -> Mitigation: хранить unknown отдельно от нуля и показывать coverage по каждой группе метрик.
- [Risk] Tool taxonomy может ошибочно классифицировать shell-команды. -> Mitigation: начинать с явных projected fields и проверяемого классификатора; ambiguous команды относить в `other`.
- [Risk] Business metrics по review замечаниям могут зависеть от формата промптов/артефактов. -> Mitigation: считать только распознанные markers и сохранять source evidence/count coverage.
- [Risk] Derived efficiency metrics могут выглядеть точными при частичных исходных данных. -> Mitigation: считать derived metrics только поверх covered inputs и возвращать `unknown` при недостаточном coverage.
- [Risk] Span-like иерархия может не полностью восстанавливаться из старых логов. -> Mitigation: хранить доступную иерархию без synthetic parent-child связей, если source evidence недостаточно.
- [Risk] Materialized storage может устареть после изменения projection logic. -> Mitigation: хранить `metrics_schema_version` и `source_projection_version`, поддержать принудительный reindex/recompute.

## Migration Plan

1. Добавить Rust-модель session metrics и unit tests на synthetic `EventRecord`/`EventTree`.
2. Добавить project identity extractor из `IndexedSessionSummary`/session metadata.
3. Добавить coverage/confidence model, outcome taxonomy и duration breakdown.
4. Добавить token ledger, spawn-agent attribution, task breakdown, factor metadata, tool taxonomy и business review metrics.
5. Добавить context growth/compaction metrics и зарезервировать feedback/evaluator/guardrail groups.
6. Добавить storage-адаптер для materialized metrics с версионированием схемы.
7. Подключить чтение метрик в Tauri/backend контракт `codex-session-explorer`.
8. Перевести frontend panel на backend-provided metrics, оставив локальный расчёт только как временный compatibility fallback при отсутствии нового API.
9. Обновить `docs/log-events.md`, если реализация меняет projected fields, event mapping, payload или operation dedup/merge правила.

## Open Questions

- Нужен ли storage сразу в SQLite рядом с Codex `state_*.sqlite`, или отдельная БД приложения должна быть жёстко отделена от Codex HOME?
- Какой минимальный набор project aggregate windows нужен первым: день, неделя, месяц или произвольный range?
- Нужно ли в первом change считать cost, если в текущих источниках есть только token usage без price table?
- Какие review markers считаем каноническими для business metrics: review tools, artifact names, markdown sections или будущие явные event fields?
- Какие context-size источники считаем доверенными: metadata, `info.tokens`, runtime context events или future explicit fields?

## Source-Backed Practices

- OpenTelemetry GenAI metrics фиксирует `gen_ai.client.token.usage`, `gen_ai.client.operation.duration` и `error.type`, а также требует не репортить token usage без надёжного источника. Это поддерживает решения про token ledger, duration breakdown, outcome/error taxonomy и `unknown != 0`: https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-metrics/
- OpenTelemetry GenAI spans описывает tool execution spans и предупреждает, что input/output/system instruction content может быть большим и чувствительным. Это поддерживает split между агрегированными метриками, source evidence и осторожным хранением payload: https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-spans/
- OpenAI Agents SDK tracing по умолчанию трассирует run, agent, generation, function tool, guardrail и handoff spans. Это поддерживает span-like иерархию `session -> agent -> generation/tool/guardrail/handoff`: https://openai.github.io/openai-agents-js/guides/tracing/
- OpenAI Agents Python tracing model содержит agent/function/guardrail/handoff/response spans и usage на response span. Это поддерживает task/turn/generation/tool attribution и usage-source tracking: https://openai.github.io/openai-agents-python/ref/tracing/
- LangSmith observability показывает project-level monitoring по trace count, latency, error rate, feedback scores и costs, а также grouping по metadata вроде model. Это поддерживает factor metadata, feedback/evaluator metrics и project trend comparison: https://docs.langchain.com/langsmith/observability-llm-tutorial
- OpenInference semantic conventions выделяет span kinds `LLM`, `TOOL`, `AGENT`, `GUARDRAIL`, `EVALUATOR`, `PROMPT`, `RETRIEVER`. Это поддерживает аналитическую taxonomy поверх технических event types: https://arize-ai.github.io/openinference/spec/semantic_conventions.html

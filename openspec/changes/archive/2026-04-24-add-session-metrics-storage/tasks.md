## 1. Domain Model

- [x] 1.1 Add `session_metrics` module to `crates/codex-log` with public structs for session metrics, project identity, metric source and schema/projection versions.
- [x] 1.2 Implement project identity extraction from indexed session metadata with explicit degraded bucket handling for incomplete metadata.
- [x] 1.3 Implement metric aggregation from normalized `EventRecord`/`EventTree` and operation metadata without raw-log reparsing.
- [x] 1.4 Add coverage model with known, partial and unknown states for every metric group.
- [x] 1.5 Add outcome taxonomy for completed, failed, aborted, interrupted and unknown.
- [x] 1.6 Add factor metadata model for model, reasoning effort, CLI version, sandbox/approval mode, agent role, skills count, MCP server/call counts and start context size.
- [x] 1.7 Add Rust unit tests for duration, event count, thread count, message count, tool/shell/MCP/collab breakdown, errors, failures, aborts and token source handling.

## 2. Token and Attribution

- [x] 2.1 Implement project token ledger with input, output, cached input, tool call, task and spawn-agent breakdown where source data exists.
- [x] 2.2 Track token source and coverage for every token metric group.
- [x] 2.3 Implement spawn-agent attribution and `include_spawn_agents` aggregation modes.
- [x] 2.4 Implement duration breakdown for generation/model, tool/shell/MCP, spawn-agent and idle/unknown gaps where boundaries allow.
- [x] 2.5 Add tests proving unknown token and duration breakdown values do not collapse to zero.

## 3. Breakdowns and Taxonomy

- [x] 3.1 Implement chronological session metrics listing for a project/time window.
- [x] 3.2 Implement task/turn/agent-work item breakdown when reliable boundaries exist.
- [x] 3.3 Implement tool command taxonomy for search, edit, web_search, test, build, git, filesystem, mcp, collaboration and other.
- [x] 3.4 Implement business review metrics for review cycles and review findings/comments with explicit unknown state.
- [x] 3.5 Implement context growth and compaction metrics from trusted normalized markers.
- [x] 3.6 Implement feedback, evaluator, guardrail and handoff metric groups with explicit unknown state.
- [x] 3.7 Implement baseline comparison and efficiency metrics only when input coverage is sufficient.
- [x] 3.8 Add tests for spawn-agent toggles, task boundaries, taxonomy classification, review metric extraction, context compaction, quality metrics and derived metric coverage.

## 4. Storage

- [x] 4.1 Choose and implement the first materialized metrics storage boundary without mutating Codex-owned `state_*.sqlite` in place.
- [x] 4.2 Store one current metrics record per `session_id` with `metrics_schema_version`, projection version, project metadata, factor metadata, coverage states, outcome and timestamps.
- [x] 4.3 Add recompute/upsert behavior for stale metrics records when schema or projection version changes.
- [x] 4.4 Add storage tests for insert, update, stale-version detection, degraded project buckets and factor metadata.

## 5. Backend Contract

- [x] 5.1 Expose session metrics read API through the existing viewer backend/Tauri contract.
- [x] 5.2 Expose project metrics query by project key and time window, including contributing session ids and chronological ordering.
- [x] 5.3 Add request option for including or excluding spawn-agent contributions.
- [x] 5.4 Ensure unknown metric values are serialized distinctly from zero values.
- [x] 5.5 Expose baseline comparison and derived efficiency metrics with inherited coverage.
- [x] 5.6 Update TypeScript backend types and contract tests for the new payloads.

## 6. Explorer UI

- [x] 6.1 Switch `SessionMetricsPanel` to backend-provided metrics where available.
- [x] 6.2 Preserve local frontend aggregation only as a compatibility path for sessions without backend metrics.
- [x] 6.3 Add project-level token ledger and chronological session metrics surface with spawn-agent toggle.
- [x] 6.4 Add UI affordances for factor metadata, outcome, duration breakdown, task breakdown, tool taxonomy, review metrics, context metrics and quality metrics.
- [x] 6.5 Add baseline comparison and efficiency metric views with visible coverage state.
- [x] 6.6 Add UI tests that verify unknown values, zero values, danger states, loaded backend metrics rendering, spawn-agent toggle behavior and coverage display.

## 7. Documentation and Validation

- [x] 7.1 Update `docs/log-events.md` if implementation changes event mapping, projected fields, payload structure, deduplication or merge rules.
- [x] 7.2 Document the session metrics storage contract, token ledger, coverage model, outcome taxonomy, duration breakdown, tool taxonomy, context metrics, quality metrics and business review metrics in project docs.
- [x] 7.3 Run `cargo test --workspace`.
- [x] 7.4 Run `npm --prefix apps/codex-session-explorer test`.
- [x] 7.5 Run `openspec validate add-session-metrics-storage --strict`.

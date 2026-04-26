import { describe, expect, it } from "vitest";

import type {
  CoveredMetric,
  MetricCoverage,
  ProjectMetricsResponse,
  SessionMetrics,
} from "@/backend";
import {
  aggregateProjectMetricsResponses,
  buildProjectMetricsViewModel,
  createInitialProjectMetricsRange,
  getProjectMetricPoint,
  resolveProjectMetricsRange,
} from "@/components/project-metrics";

function covered(
  value: number | null,
  coverage: MetricCoverage = value == null ? "unknown" : "known",
): CoveredMetric<number> {
  return {
    value,
    coverage,
    source: coverage === "unknown" ? "unavailable" : "normalized_events",
  };
}

function makeSession(overrides: Partial<SessionMetrics> = {}): SessionMetrics {
  return {
    session_id: "session-1",
    metrics_schema_version: 1,
    source_projection_version: 2,
    computed_at: "2026-04-23T10:00:00Z",
    started_at: "2026-04-23T09:00:00Z",
    ended_at: "2026-04-23T09:10:00Z",
    project: {
      project_key: "project:test",
      state: "normal",
      cwd: "/repo/project-alpha",
      normalized_cwd: "/repo/project-alpha",
      git_origin_url: "https://example.com/repo.git",
      git_branch: "main",
      git_sha: "abc",
    },
    session_scope: "main",
    factors: {
      model: "gpt-5.4",
      reasoning_effort: "medium",
      cli_version: "1.0",
      sandbox_policy_kind: "workspace-write",
      approval_mode: "never",
      agent_role: "default",
      skills_count: covered(0),
      mcp_server_count: covered(0),
      mcp_call_count: covered(0),
      start_context_size: covered(0),
    },
    outcome: {
      outcome: "completed",
      coverage: "known",
      error_type: null,
    },
    event_count: covered(10),
    thread_count: covered(1),
    message_count: covered(4),
    error_count: covered(0),
    abort_count: covered(0),
    failure_count: covered(0),
    operations: {
      operation_count: covered(10),
      successful_operations: covered(10),
      failed_operations: covered(0),
      tool_calls: covered(3),
      shell_calls: covered(1),
      mcp_calls: covered(0),
      collaboration_calls: covered(0),
      spawn_agent_calls: covered(1),
    },
    duration: {
      total_ms: covered(600000),
      generation_ms: covered(1000),
      tool_ms: covered(2000),
      shell_ms: covered(3000),
      mcp_ms: covered(0),
      spawn_agent_ms: covered(120000),
      idle_unknown_ms: covered(0),
    },
    token_ledger: {
      total: covered(4000),
      input: covered(1000),
      output: covered(2000),
      cached_input: covered(100),
      reasoning_output: covered(500),
      tool_call: covered(150),
      task: covered(3000),
      spawn_agent: covered(1000),
    },
    tool_breakdown: [],
    task_metrics: {
      task_count: covered(1),
      turn_count: covered(1),
      agent_work_item_count: covered(1),
    },
    task_facts: [],
    used_skills: {
      identifiers: [],
      count: covered(null),
      coverage: "unknown",
      source: "unavailable",
    },
    business_review: {
      review_cycles: covered(0),
      review_findings: covered(0),
    },
    context: {
      start_context_size: covered(0),
      context_growth: covered(0),
      compaction_events: covered(0),
      context_compression: covered(0),
    },
    quality: {
      feedback_score: covered(0),
      evaluator_result_count: covered(0),
      guardrail_trigger_count: covered(0),
      handoff_count: covered(0),
    },
    baseline: {
      coverage: "known",
      token_usage_delta: covered(0),
      duration_delta_ms: covered(0),
      error_rate_delta: covered(0),
      outcome_rate_delta: covered(0),
    },
    derived_efficiency: {
      tokens_per_successful_session: covered(100),
      tokens_per_accepted_task: covered(100),
      review_findings_per_1k_tokens: covered(0.5),
    },
    ...overrides,
  };
}

function makeResponse(sessions: SessionMetrics[]): ProjectMetricsResponse {
  return {
    project_key: "project:test",
    session_count: sessions.length,
    contributing_session_ids: sessions.map((session) => session.session_id),
    scope_filter: "all",
    available_scope_counts: { main: sessions.length, subsession: 0, unknown: 0 },
    sessions,
    token_ledger: {
      total: covered(8000),
      input: covered(2000),
      output: covered(4000),
      cached_input: covered(200),
      reasoning_output: covered(1000),
      tool_call: covered(300),
      task: covered(6000),
      spawn_agent: covered(2000),
    },
    duration_ms: covered(1200000),
    factors: {
      start_context_size: covered(0),
      skills_count: covered(0),
      mcp_server_count: covered(0),
    },
    operations: {
      spawn_agent_calls: covered(2),
    },
    task_metrics: {
      task_count: covered(2),
    },
    task_facts: sessions.flatMap((session) => session.task_facts),
    used_skills: {
      skills: [],
      count: covered(null),
      coverage: "unknown",
      source: "unavailable",
    },
    baseline: {
      coverage: "partial",
      token_usage_delta: covered(10),
      duration_delta_ms: covered(0),
      error_rate_delta: covered(0),
      outcome_rate_delta: covered(0),
    },
    derived_efficiency: {
      tokens_per_successful_session: covered(100),
      tokens_per_accepted_task: covered(100),
      review_findings_per_1k_tokens: covered(0.75),
    },
  };
}

describe("resolveProjectMetricsRange", () => {
  it("builds preset and custom query windows", () => {
    const now = new Date("2026-04-23T12:00:00Z");
    expect(resolveProjectMetricsRange(createInitialProjectMetricsRange(), now)).toMatchObject({
      start_ts: "2026-04-09T12:00:00.000Z",
      end_ts: "2026-04-23T12:00:00.000Z",
    });

    expect(
      resolveProjectMetricsRange(
        {
          preset: "custom",
          start: "2026-04-01T10:00",
          end: "2026-04-02T11:30",
        },
        now,
      ),
    ).toEqual({
      start_ts: new Date("2026-04-01T10:00").toISOString(),
      end_ts: new Date("2026-04-02T11:30").toISOString(),
    });
  });
});

describe("buildProjectMetricsViewModel", () => {
  it("builds summary-chart rows, derived toggles, and explicit unknown coverage gaps", () => {
    const response = makeResponse([
      makeSession({
        session_id: "session-1",
        started_at: "2026-04-23T09:00:00Z",
        used_skills: {
          identifiers: ["openspec-apply-change", "shadcn"],
          count: covered(2),
          coverage: "known",
          source: "normalized_events",
        },
      }),
      makeSession({
        session_id: "session-2",
        started_at: "2026-04-23T10:00:00Z",
        session_scope: "unknown",
        project: {
          project_key: "project:test",
          state: "degraded",
          cwd: null,
          normalized_cwd: null,
          git_origin_url: null,
          git_branch: null,
          git_sha: null,
        },
        token_ledger: {
          total: covered(null),
          input: covered(0),
          output: covered(0),
          cached_input: covered(0),
          reasoning_output: covered(0),
          tool_call: covered(0),
          task: covered(0),
          spawn_agent: covered(0),
        },
        operations: {
          operation_count: covered(10),
          successful_operations: covered(9),
          failed_operations: covered(null),
          tool_calls: covered(5, "partial"),
          shell_calls: covered(1),
          mcp_calls: covered(0),
          collaboration_calls: covered(0),
          spawn_agent_calls: covered(1),
        },
        error_count: covered(2, "partial"),
      }),
    ]);
    response.used_skills = {
      skills: [
        { identifier: "openspec-apply-change", session_count: 1, usage_count: 1 },
        { identifier: "shadcn", session_count: 1, usage_count: 1 },
      ],
      count: covered(2, "partial"),
      coverage: "partial",
      source: "derived",
    };

    const viewModel = buildProjectMetricsViewModel(response, false);

    expect(viewModel.degraded).toBe(true);
    expect(viewModel.hasUnknownScope).toBe(false);
    expect(viewModel.summaryCards.find((card) => card.label === "Tokens")?.value).toBe("5 800");
    expect(viewModel.summaryCards.find((card) => card.label === "All tokens")?.value).toBe("6 000");
    expect(viewModel.summaryCards.find((card) => card.label === "Cached tokens")?.value).toBe("200");
    expect(viewModel.chartSeries.find((series) => series.key === "startContext")).toMatchObject({
      category: "factors",
    });
    expect(viewModel.chartSeries.find((series) => series.key === "usedSkillsCount")).toMatchObject({
      category: "factors",
    });
    expect(viewModel.chartSeries.find((series) => series.key === "tokenInput")).toMatchObject({
      category: "tokens",
    });
    expect(getProjectMetricPoint(viewModel.chartRows[0], "usedSkillsCount")).toMatchObject({
      coverage: "known",
      value: 2,
    });
    expect(getProjectMetricPoint(viewModel.chartRows[1], "usedSkillsCount")).toMatchObject({
      coverage: "unknown",
      value: null,
    });
    expect(getProjectMetricPoint(viewModel.chartRows[1], "tokens")).toMatchObject({
      coverage: "unknown",
      value: null,
    });
    expect(viewModel.chartSeries.find((series) => series.key === "tokensPerSuccess")).toMatchObject({
      category: "derived",
      defaultVisible: false,
    });
    expect(viewModel.defaultVisibleSeriesKeys).toEqual(["duration", "tokens", "failures", "toolCalls"]);
    expect(viewModel.initialZoomWindow).toEqual({ startIndex: 0, endIndex: 1 });
    expect(viewModel.sessions[0].tokens).toBe("2 900");
    expect(viewModel.sessions[1].sessionScope).toBe("unknown");
    expect(viewModel.sessions[1].failures).toContain("partial");
    expect(viewModel.summaryCards.find((card) => card.label === "Used skills")?.value).toBe("2 (partial)");
    expect(viewModel.usedSkillsCoverage).toBe("partial");
  });
});

describe("aggregateProjectMetricsResponses", () => {
  it("merges multiple backend buckets into one cwd-level response", () => {
    const first = makeResponse([
      makeSession({
        session_id: "session-1",
        started_at: "2026-04-23T09:00:00Z",
      }),
    ]);
    const second = makeResponse([
      makeSession({
        session_id: "session-2",
        started_at: "2026-04-23T10:00:00Z",
      }),
    ]);

    const merged = aggregateProjectMetricsResponses("cwd:/repo/project-alpha", [first, second]);

    expect(merged.project_key).toBe("cwd:/repo/project-alpha");
    expect(merged.session_count).toBe(2);
    expect(merged.contributing_session_ids).toEqual(["session-1", "session-2"]);
    expect(merged.scope_filter).toBe("all");
    expect(merged.token_ledger.total.value).toBe(8000);
    expect(merged.token_ledger.input.value).toBe(2000);
    expect(merged.factors.skills_count.value).toBe(0);
    expect(merged.task_metrics.task_count.value).toBe(2);
    expect(merged.duration_ms.value).toBe(1200000);
    expect(merged.used_skills.count.value).toBeNull();
  });
});

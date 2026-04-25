// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

vi.mock("@/components/ui/scroll-area", () => ({
  ScrollArea: ({
    children,
    className,
    ...props
  }: {
    children: ReactNode;
    className?: string;
  } & React.HTMLAttributes<HTMLDivElement>) => (
    <div className={className} {...props}>
      {children}
    </div>
  ),
}));

vi.mock("recharts", async (importOriginal) => {
  const actual = await importOriginal<typeof import("recharts")>();
  return {
    ...actual,
    ResponsiveContainer: ({ children }: { children: ReactNode }) => (
      <div style={{ width: 1200, height: 400 }}>{children}</div>
    ),
  };
});

import type {
  CoveredMetric,
  MetricCoverage,
  ProjectMetricsResponse,
  SessionMetrics,
} from "@/backend";
import { buildProjectMetricsViewModel, createInitialProjectMetricsRange } from "@/components/project-metrics";
import {
  buildChartAnalysis,
  ProjectMetricsScreen,
  resolveSelectionWindow,
  SeriesDot,
} from "@/components/project-metrics-screen";

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

function makeSession(index: number): SessionMetrics {
  return {
    session_id: `session-${index}`,
    metrics_schema_version: 1,
    source_projection_version: 2,
    computed_at: `2026-04-23T${String(index).padStart(2, "0")}:15:00Z`,
    started_at: `2026-04-23T${String(index).padStart(2, "0")}:00:00Z`,
    ended_at: `2026-04-23T${String(index).padStart(2, "0")}:10:00Z`,
    project: {
      project_key: "project:test",
      state: index === 2 ? "degraded" : "normal",
      cwd: "/repo/project-alpha",
      normalized_cwd: "/repo/project-alpha",
      git_origin_url: "https://example.com/repo.git",
      git_branch: "main",
      git_sha: "abc",
    },
    session_scope: index % 2 === 0 ? "subsession" : "main",
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
    event_count: covered(10 + index),
    thread_count: covered(1),
    message_count: covered(4),
    error_count: covered(index === 3 ? 2 : 0, index === 3 ? "partial" : "known"),
    abort_count: covered(0),
    failure_count: covered(0),
    operations: {
      operation_count: covered(10),
      successful_operations: covered(10 - (index === 3 ? 1 : 0)),
      failed_operations: covered(index === 3 ? 1 : 0, index === 3 ? "partial" : "known"),
      tool_calls: covered(index === 5 ? null : 2 + index, index === 5 ? "unknown" : "known"),
      shell_calls: covered(1),
      mcp_calls: covered(0),
      collaboration_calls: covered(0),
      spawn_agent_calls: covered(1),
    },
    duration: {
      total_ms: covered(120000 * index),
      generation_ms: covered(1000),
      tool_ms: covered(2000),
      shell_ms: covered(3000),
      mcp_ms: covered(0),
      spawn_agent_ms: covered(10000),
      idle_unknown_ms: covered(0),
    },
    token_ledger: {
      total: covered(index === 4 ? null : 3000 * index, index === 4 ? "unknown" : "known"),
      input: covered(1000),
      output: covered(2000),
      cached_input: covered(100),
      reasoning_output: covered(500),
      tool_call: covered(150),
      task: covered(3000),
      spawn_agent: covered(500),
    },
    tool_breakdown: [],
    task_metrics: {
      task_count: covered(1),
      turn_count: covered(1),
      agent_work_item_count: covered(1),
    },
    task_facts: [],
    used_skills: {
      identifiers: index % 3 === 0 ? ["openspec-apply-change"] : [],
      coverage: index % 3 === 0 ? "known" : "unknown",
      source: index % 3 === 0 ? "normalized_events" : "unavailable",
    },
    business_review: {
      review_cycles: covered(0),
      review_findings: covered(index),
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
      coverage: "partial",
      token_usage_delta: covered(10),
      duration_delta_ms: covered(0),
      error_rate_delta: covered(0),
      outcome_rate_delta: covered(0),
    },
    derived_efficiency: {
      tokens_per_successful_session: covered(index === 6 ? null : 100 + index, index === 6 ? "unknown" : "known"),
      tokens_per_accepted_task: covered(100),
      review_findings_per_1k_tokens: covered(index === 7 ? null : index / 10, index === 7 ? "unknown" : "known"),
    },
  };
}

function makeResponse(count: number): ProjectMetricsResponse {
  const sessions = Array.from({ length: count }, (_, index) => makeSession(index + 1));
  return {
    project_key: "project:test",
    session_count: sessions.length,
    contributing_session_ids: sessions.map((session) => session.session_id),
    scope_filter: "all",
    available_scope_counts: { main: Math.ceil(sessions.length / 2), subsession: Math.floor(sessions.length / 2), unknown: 1 },
    sessions,
    token_ledger: {
      total: covered(64000),
      input: covered(2000),
      output: covered(4000),
      cached_input: covered(200),
      reasoning_output: covered(1000),
      tool_call: covered(300),
      task: covered(6000),
      spawn_agent: covered(4000),
    },
    duration_ms: covered(16000000),
    factors: {
      start_context_size: covered(0),
      skills_count: covered(0),
      mcp_server_count: covered(0),
    },
    operations: {
      spawn_agent_calls: covered(8),
    },
    task_metrics: {
      task_count: covered(8),
    },
    task_facts: sessions.flatMap((session) => session.task_facts),
    used_skills: {
      skills: [
        {
          identifier: "openspec-apply-change",
          usage_count: Math.floor(count / 3),
          session_count: Math.floor(count / 3),
        },
      ],
      coverage: "partial",
      source: "derived",
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

beforeAll(() => {
  class ResizeObserverMock {
    observe() {}
    unobserve() {}
    disconnect() {}
  }

  vi.stubGlobal("ResizeObserver", ResizeObserverMock);
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get() {
      return 1200;
    },
  });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    get() {
      return 400;
    },
  });
  Object.defineProperty(HTMLElement.prototype, "getBoundingClientRect", {
    configurable: true,
    value() {
      return {
        width: 1200,
        height: 400,
        top: 0,
        left: 0,
        right: 1200,
        bottom: 400,
        x: 0,
        y: 0,
        toJSON() {
          return {};
        },
      };
    },
  });
});

afterEach(() => {
  cleanup();
});

describe("ProjectMetricsScreen", () => {
  it("renders summary chart controls, supports zoom and pan buttons, and keeps chart-point drill-down", async () => {
    const user = userEvent.setup();
    const onOpenSession = vi.fn();

    render(
      <ProjectMetricsScreen
        catalogBusy={false}
        currentSessionId={null}
        error={null}
        includeSpawnAgents={true}
        loading={false}
        metrics={makeResponse(16)}
        onIncludeSpawnAgentsChange={vi.fn()}
        onOpenSession={onOpenSession}
        onProjectChange={vi.fn()}
        onRangeChange={vi.fn()}
        onScopeFilterChange={vi.fn()}
        onRefresh={vi.fn()}
        projectOptions={[
          {
            projectKey: "project-alpha",
            backendProjectKeys: ["project:test"],
            label: "project-alpha",
            description: "main · https://example.com/repo.git · /repo/project-alpha · 16 sessions",
            state: "normal",
            sessionCount: 16,
            availableScopeCounts: { main: 8, subsession: 7, unknown: 1 },
          },
        ]}
        range={createInitialProjectMetricsRange()}
        selectedProjectKey="project-alpha"
        scopeFilter="all"
      />,
    );

    expect(screen.getByText("Summary chart")).toBeTruthy();
    expect(screen.getByTestId("project-metrics-toolbar")).toBeTruthy();
    expect(screen.getByTestId("project-metrics-inspector")).toBeTruthy();
    expect(screen.queryByTestId("project-metrics-analytics-controls")).toBeNull();
    expect(screen.getByTestId("project-metrics-overview-panel")).toBeTruthy();
    expect(screen.getByText("Last 14 days")).toBeTruthy();
    expect(screen.getByText("Partial data remains visible")).toBeTruthy();
    expect(screen.getByTestId("project-metrics-anomaly-panel")).toBeTruthy();
    expect(screen.getByText("Used skills")).toBeTruthy();
    expect(screen.getByText("openspec-apply-change")).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Chart settings" }));
    expect(screen.getByTestId("project-metrics-analytics-controls")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Trend" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("button", { name: "Moving average" }).getAttribute("aria-pressed")).toBe("false");
    expect(screen.getByRole("button", { name: "Moving median" }).getAttribute("aria-pressed")).toBe("false");
    expect(screen.getByRole("button", { name: "Raw values" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("button", { name: "Project median" }).getAttribute("aria-pressed")).toBe("false");
    expect(screen.getByTestId("project-metrics-mode-help").textContent).toContain("Theil-Sen");
    expect(screen.getAllByText(/Raw values:/i).length).toBeGreaterThan(0);

    await user.click(screen.getByRole("button", { name: "Moving median" }));
    expect(screen.getByRole("button", { name: "Moving median" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByTestId("project-metrics-mode-help").textContent).toContain("more resistant to spikes");

    await user.click(screen.getByRole("button", { name: "Raw values" }));
    expect(screen.getByRole("button", { name: "Raw values" }).getAttribute("aria-pressed")).toBe("false");
    expect(screen.queryByText(/Raw values:/i)).toBeNull();

    await user.click(screen.getByRole("button", { name: "Project median" }));
    expect(screen.getByRole("button", { name: "Project median" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getAllByText(/Project median:/i).length).toBeGreaterThan(0);

    expect(screen.queryByTestId("project-metrics-series-panel")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Series" }));
    expect(screen.getByTestId("project-metrics-series-panel")).toBeTruthy();

    const derivedToggle = screen.getByRole("button", { name: /Review \/ 1k tokens/i });
    expect(derivedToggle.getAttribute("aria-pressed")).toBe("false");
    await user.click(derivedToggle);
    expect(derivedToggle.getAttribute("aria-pressed")).toBe("true");

    expect(screen.getByText("Sessions 1-16 of 16")).toBeTruthy();
    expect(screen.getByText("Showing 16 of 16 sessions")).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Show 8 sessions" }));
    expect(screen.getByText("Sessions 9-16 of 16")).toBeTruthy();
    expect(screen.getByText("Showing 8 of 16 sessions")).toBeTruthy();

    await user.click(screen.getByRole("button", { name: /Earlier/i }));
    expect(screen.getByText("Sessions 5-12 of 16")).toBeTruthy();

    await user.click(screen.getByRole("button", { name: /Latest/i }));
    expect(screen.getByText("Sessions 9-16 of 16")).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Window size"), {
      target: { value: "6" },
    });
    expect(screen.getByText("Sessions 11-16 of 16")).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Window position"), {
      target: { value: "4" },
    });
    expect(screen.getByText("Sessions 5-10 of 16")).toBeTruthy();

    fireEvent.wheel(screen.getByTestId("project-metrics-chart-surface"), {
      deltaY: -120,
    });
    expect(screen.getByText("Sessions 4-9 of 16")).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Exclude outliers" }));
    expect(screen.getByRole("button", { name: "Exclude outliers" }).getAttribute("aria-pressed")).toBe("true");

    await user.click(screen.getByRole("button", { name: "Open session" }));
    expect(onOpenSession).toHaveBeenCalledWith("session-16");
  }, 10000);

  it("keeps the project stage and inspector on independent scroll containers", () => {
    render(
      <ProjectMetricsScreen
        catalogBusy={false}
        currentSessionId="session-16"
        error={null}
        includeSpawnAgents={true}
        loading={false}
        metrics={makeResponse(16)}
        onIncludeSpawnAgentsChange={vi.fn()}
        onOpenSession={vi.fn()}
        onProjectChange={vi.fn()}
        onRangeChange={vi.fn()}
        onScopeFilterChange={vi.fn()}
        onRefresh={vi.fn()}
        projectOptions={[
          {
            projectKey: "project-alpha",
            backendProjectKeys: ["project:test"],
            label: "project-alpha",
            description: "main · https://example.com/repo.git · /repo/project-alpha · 16 sessions",
            state: "normal",
            sessionCount: 16,
            availableScopeCounts: { main: 8, subsession: 7, unknown: 1 },
          },
        ]}
        range={createInitialProjectMetricsRange()}
        selectedProjectKey="project-alpha"
        scopeFilter="all"
      />,
    );

    expect(screen.getByTestId("project-metrics-shell").className).toContain("overflow-hidden");
    expect(screen.getByTestId("project-metrics-split-view").className).toContain("overflow-hidden");
    expect(screen.getByTestId("project-metrics-stage-scroll").className).toContain("h-full");
    expect(screen.getByTestId("project-metrics-inspector-scroll").className).toContain("h-full");
  });

  it("shows a focused fallback for a single-session chart window", () => {
    render(
      <ProjectMetricsScreen
        catalogBusy={false}
        currentSessionId="session-1"
        error={null}
        includeSpawnAgents={true}
        loading={false}
        metrics={makeResponse(1)}
        onIncludeSpawnAgentsChange={vi.fn()}
        onOpenSession={vi.fn()}
        onProjectChange={vi.fn()}
        onRangeChange={vi.fn()}
        onScopeFilterChange={vi.fn()}
        onRefresh={vi.fn()}
        projectOptions={[
          {
            projectKey: "project-alpha",
            backendProjectKeys: ["project:test"],
            label: "project-alpha",
            description: "main · https://example.com/repo.git · /repo/project-alpha · 1 sessions",
            state: "normal",
            sessionCount: 1,
            availableScopeCounts: { main: 1, subsession: 0, unknown: 0 },
          },
        ]}
        range={createInitialProjectMetricsRange()}
        selectedProjectKey="project-alpha"
        scopeFilter="all"
      />,
    );

    expect(screen.getByTestId("project-metrics-single-session-state")).toBeTruthy();
    expect(screen.getByText("Single-session window")).toBeTruthy();
    expect(screen.getByText(/В текущем окне только одна сессия/i)).toBeTruthy();
  });

  it("keeps chart-point click lightweight and updates selection without opening the session", () => {
    const onActivateSession = vi.fn();
    const onOpenSession = vi.fn();
    const response = makeResponse(4);
    const viewModel = buildProjectMetricsViewModel(response, true);
    const row = viewModel.chartRows.find((item) => item.metrics.tokens.value != null);

    expect(row).toBeTruthy();

    render(
      <svg>
        <SeriesDot
          currentSessionId={null}
          cx={24}
          cy={18}
          onActivateSession={onActivateSession}
          outlierMode="keep"
          payload={{
            index: row!.index,
            label: row!.label,
            modeValues: {
              trend: {},
              "moving-average": {},
              "moving-median": {},
            },
            outlierFlags: {},
            processedValues: { tokens: row!.metrics.tokens.value },
            row: row!,
            sessionId: row!.sessionId,
          }}
          seriesKey="tokens"
          stroke="#2563eb"
        />
      </svg>,
    );

    const chartPoint = document.querySelector(`circle[data-session-id="${row!.sessionId}"]`);
    expect(chartPoint).toBeTruthy();
    fireEvent.click(chartPoint!);

    expect(onActivateSession).toHaveBeenCalledWith(row!.sessionId);
    expect(onOpenSession).not.toHaveBeenCalled();
  });

  it("uses a scrollable container for long project selector lists", async () => {
    const user = userEvent.setup();

    render(
      <ProjectMetricsScreen
        catalogBusy={false}
        currentSessionId={null}
        error={null}
        includeSpawnAgents={true}
        loading={false}
        metrics={makeResponse(4)}
        onIncludeSpawnAgentsChange={vi.fn()}
        onOpenSession={vi.fn()}
        onProjectChange={vi.fn()}
        onRangeChange={vi.fn()}
        onScopeFilterChange={vi.fn()}
        onRefresh={vi.fn()}
        projectOptions={Array.from({ length: 30 }, (_, index) => ({
          projectKey: `project-${index + 1}`,
          backendProjectKeys: [`project:test:${index + 1}`],
          label: `project-${index + 1}`,
          description: `/repo/project-${index + 1}`,
          state: "normal" as const,
          sessionCount: index + 1,
          availableScopeCounts: { main: index + 1, subsession: 0, unknown: 0 },
        }))}
        range={createInitialProjectMetricsRange()}
        selectedProjectKey="project-1"
        scopeFilter="all"
      />,
    );

    await user.click(screen.getByRole("button", { name: "project-1" }));

    const scrollContainer = screen.getByTestId("project-selector-scroll");
    expect(scrollContainer.className).toContain("max-h-80");
    expect(scrollContainer.className).toContain("overflow-y-auto");
    expect(screen.getByRole("option", { name: /project-30/i })).toBeTruthy();
  });

  it("renders scope filter controls and unknown-scope hint for narrow filters", () => {
    render(
      <ProjectMetricsScreen
        catalogBusy={false}
        currentSessionId={null}
        error={null}
        includeSpawnAgents={true}
        loading={false}
        metrics={makeResponse(4)}
        onIncludeSpawnAgentsChange={vi.fn()}
        onOpenSession={vi.fn()}
        onProjectChange={vi.fn()}
        onRangeChange={vi.fn()}
        onScopeFilterChange={vi.fn()}
        onRefresh={vi.fn()}
        projectOptions={[
          {
            projectKey: "project-alpha",
            backendProjectKeys: ["project:test"],
            label: "project-alpha",
            description: "https://example.com/repo.git · /repo/project-alpha",
            state: "normal",
            sessionCount: 4,
            availableScopeCounts: { main: 2, subsession: 1, unknown: 1 },
          },
        ]}
        range={createInitialProjectMetricsRange()}
        selectedProjectKey="project-alpha"
        scopeFilter="main"
      />,
    );

    expect(screen.getAllByRole("button", { name: "All" }).length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: "Main" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Subsession" })).toBeTruthy();
    expect(screen.getByText(/Unknown session scope remains outside narrow filters/i)).toBeTruthy();
    expect(screen.getByText(/Partial data remains visible/i)).toBeTruthy();
  });

  it("builds chart modes, project median overlays, and preserves unknown gaps", () => {
    const response = makeResponse(8);
    response.sessions[7] = makeSession(8);
    response.sessions[7].duration.total_ms = covered(9_000_000);
    response.sessions[3].duration.total_ms = covered(null);

    const viewModel = buildProjectMetricsViewModel(response, true);
    const durationSeries = viewModel.chartSeries.find((item) => item.key === "duration");

    expect(durationSeries).toBeTruthy();

    const clamped = buildChartAnalysis({
      outlierMode: "clamp",
      rows: viewModel.chartRows,
      series: [durationSeries!],
    });
    const excluded = buildChartAnalysis({
      outlierMode: "exclude",
      rows: viewModel.chartRows,
      series: [durationSeries!],
    });

    expect(clamped.seriesAnalytics.duration?.outlierCount).toBe(1);
    expect(clamped.seriesAnalytics.duration?.projectMedian).not.toBeNull();
    expect(clamped.seriesAnalytics.duration?.smoothingWindowSize).toBe(3);
    expect(clamped.chartData.at(-1)?.processedValues.duration).not.toBe(
      viewModel.chartRows.at(-1)?.metrics.duration.value,
    );
    expect(clamped.chartData.at(3)?.modeValues.trend.duration).toBeNull();
    expect(clamped.chartData.at(3)?.modeValues["moving-average"].duration).toBeNull();
    expect(clamped.chartData.at(3)?.modeValues["moving-median"].duration).toBeNull();
    expect(clamped.chartData.at(-1)?.modeValues.trend.duration).toBeLessThan(6_000_000);
    expect(excluded.chartData.at(-1)?.processedValues.duration).toBeNull();
  });
});

describe("resolveSelectionWindow", () => {
  it("normalizes drag direction and ignores one-point selection", () => {
    expect(resolveSelectionWindow(9, 4, 16)).toEqual({
      startIndex: 4,
      endIndex: 9,
    });
    expect(resolveSelectionWindow(4, 4, 16)).toBeNull();
    expect(resolveSelectionWindow(null, 4, 16)).toBeNull();
  });
});

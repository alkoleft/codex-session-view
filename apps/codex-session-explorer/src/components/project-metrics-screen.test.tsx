// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps, ReactNode } from "react";
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

vi.mock("@/backend", async () => {
  const actual = await vi.importActual<typeof import("@/backend")>("@/backend");
  return {
    ...actual,
    extractErrorMessage: (error: unknown) => (error instanceof Error ? error.message : String(error)),
    loadProjectMetricsSessionDetailById: vi.fn(async (sessionId: string) => ({
      session_id: sessionId,
      session_ref: `${sessionId}.jsonl`,
      title: "Pinned request title",
      start_user_request: "Investigate the abnormal token spike.",
      start_user_request_source: "indexed_first_user_message",
      task_summary: "Compare the selected session against neighboring runs.",
      task_summary_source: "indexed_title",
      agent_role: "default",
      task_class: "analysis",
      task_class_confidence: "confident",
    })),
  };
});

vi.mock("recharts", async (importOriginal) => {
  const actual = await importOriginal<typeof import("recharts")>();
  return {
    ...actual,
    CartesianGrid: () => null,
    Bar: ({
      onClick,
    }: {
      onClick?: (payload: unknown) => void;
    }) => (
      <g
        data-testid="mock-primary-bar"
        onClick={() => onClick?.({ sessionId: "session-8" })}
      >
        <rect height="180" width="24" x="12" y="24" />
      </g>
    ),
    ComposedChart: ({
      children,
      ...props
    }: {
      children: ReactNode;
    } & React.SVGProps<SVGSVGElement>) => (
      <svg {...props}>
        {children}
      </svg>
    ),
    Line: () => null,
    ReferenceArea: () => null,
    ResponsiveContainer: ({ children }: { children: ReactNode }) => (
      <div style={{ height: 420, width: 1200 }}>{children}</div>
    ),
    Tooltip: () => null,
    XAxis: () => null,
    YAxis: () => null,
  };
});

import type {
  CoveredMetric,
  MetricCoverage,
  ProjectMetricsResponse,
  ProjectMetricsSessionDetail,
  SessionMetrics,
} from "@/backend";
import { loadProjectMetricsSessionDetailById } from "@/backend";
import { buildChartAnalysis } from "@/components/project-metrics-chart";
import { buildProjectMetricsViewModel, createInitialProjectMetricsRange } from "@/components/project-metrics";
import {
  ProjectMetricsScreen,
  ProjectMetricsShellControls,
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
    session_scope: index === 4 ? "unknown" : index % 2 === 0 ? "subsession" : "main",
    factors: {
      model: "gpt-5.4",
      reasoning_effort: "medium",
      cli_version: "1.0",
      sandbox_policy_kind: "workspace-write",
      approval_mode: "never",
      agent_role: index % 2 === 0 ? "worker" : "default",
      skills_count: covered(index),
      mcp_server_count: covered(0),
      mcp_call_count: covered(0),
      start_context_size: covered(0),
    },
    outcome: {
      outcome: index === 3 ? "failed" : "completed",
      coverage: "known",
      error_type: index === 3 ? "tool" : null,
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
      total_ms: covered(index === 6 ? 900000 : 120000 * index),
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
      tokens_per_successful_session: covered(100 + index),
      tokens_per_accepted_task: covered(100),
      review_findings_per_1k_tokens: covered(index / 10),
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
    task_facts: [],
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
      return 420;
    },
  });
  Object.defineProperty(HTMLElement.prototype, "getBoundingClientRect", {
    configurable: true,
    value() {
      return {
        width: 1200,
        height: 420,
        top: 0,
        left: 0,
        right: 1200,
        bottom: 420,
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
  vi.clearAllMocks();
  window.localStorage.clear();
});

describe("ProjectMetricsShellControls", () => {
  it("keeps project, period, scope, and spawn-agent controls in one shell surface", async () => {
    const user = userEvent.setup();
    const onProjectChange = vi.fn();
    const onScopeFilterChange = vi.fn();
    const onIncludeSpawnAgentsChange = vi.fn();
    const onRangeChange = vi.fn();

    render(
      <ProjectMetricsShellControls
        catalogBusy={true}
        includeSpawnAgents={true}
        onIncludeSpawnAgentsChange={onIncludeSpawnAgentsChange}
        onProjectChange={onProjectChange}
        onRangeChange={onRangeChange}
        onScopeFilterChange={onScopeFilterChange}
        projectOptions={[
          {
            projectKey: "project-alpha",
            backendProjectKeys: ["project:test"],
            label: "project-alpha",
            description: "/repo/project-alpha",
            state: "normal",
            sessionCount: 8,
            availableScopeCounts: { main: 4, subsession: 3, unknown: 1 },
          },
        ]}
        range={{ ...createInitialProjectMetricsRange(), preset: "custom", start: "2026-04-20T10:00", end: "2026-04-21T10:00" }}
        selectedProjectKey="project-alpha"
        scopeFilter="all"
      />,
    );

    expect(screen.getByTestId("project-metrics-shell-controls")).toBeTruthy();
    expect(screen.getByText(/Catalog scan updates project labels/i)).toBeTruthy();
    expect(screen.getByDisplayValue("project-alpha")).toBeTruthy();
    expect(screen.getByDisplayValue("Custom range")).toBeTruthy();
    expect(screen.getByDisplayValue("2026-04-20T10:00")).toBeTruthy();
    expect(screen.getByDisplayValue("2026-04-21T10:00")).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Main" }));
    expect(onScopeFilterChange).toHaveBeenCalledWith("main");

    await user.click(screen.getByText("Spawn agents"));
    expect(onIncludeSpawnAgentsChange).toHaveBeenCalledWith(false);
  });
});

describe("ProjectMetricsScreen", () => {
  it("renders chart-first workspace with pinned summary, tabbed side panel, and no removed blocks", async () => {
    const user = userEvent.setup();
    const onOpenSession = vi.fn();

    render(
      <ProjectMetricsScreen
        currentSessionId={null}
        error={null}
        includeSpawnAgents={true}
        loading={false}
        metrics={makeResponse(8)}
        onOpenSession={onOpenSession}
        selectedProjectKey="project-alpha"
      />,
    );

    expect(screen.getByTestId("project-metrics-split-view")).toBeTruthy();
    expect(screen.getByTestId("project-metrics-pinned-summary")).toBeTruthy();
    expect(screen.getByTestId("project-metrics-chart-overlay-controls")).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Window pulse" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: /Anomalies/i })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Series" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Chart" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Pinned session" })).toBeTruthy();
    expect(screen.queryByText("Used skills")).toBeNull();
    expect(screen.queryByText("Contributing sessions")).toBeNull();
    expect(screen.queryByText("Partial data remains visible")).toBeNull();

    await user.click(screen.getByRole("tab", { name: "Chart" }));
    expect(screen.getByTestId("project-metrics-chart-tab")).toBeTruthy();
    expect(screen.getByText("Global percent-delta index")).toBeTruthy();

    await user.click(screen.getByRole("tab", { name: /Anomalies/i }));
    expect(screen.getByTestId("project-metrics-anomalies-tab")).toBeTruthy();
    expect(screen.getByTestId("project-metrics-anomalies-tab").textContent).toMatch(/Metric|Baseline|Session|Data issue/);

    await user.click(screen.getByRole("tab", { name: "Pinned session" }));
    expect(screen.getByTestId("project-metrics-pinned-tab")).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Open session" }));
    expect(onOpenSession).toHaveBeenCalledWith("session-8");
  });

  it("keeps main area and side panel on independent scroll containers", () => {
    render(
      <ProjectMetricsScreen
        currentSessionId="session-8"
        error={null}
        includeSpawnAgents={true}
        loading={false}
        metrics={makeResponse(8)}
        onOpenSession={vi.fn()}
        selectedProjectKey="project-alpha"
      />,
    );

    expect(screen.getByTestId("project-metrics-shell").className).toContain("overflow-hidden");
    expect(screen.getByTestId("project-metrics-split-view").className).toContain("overflow-hidden");
    expect(screen.getAllByTestId("project-metrics-side-scroll")[0]?.className).toContain("h-full");
    expect(screen.getByTestId("project-metrics-main-scroll").className).toContain("h-full");
  });

  it("renders series and chart tabs as compact control surfaces", async () => {
    const user = userEvent.setup();

    render(
      <ProjectMetricsScreen
        currentSessionId={null}
        error={null}
        includeSpawnAgents={true}
        loading={false}
        metrics={makeResponse(8)}
        onOpenSession={vi.fn()}
        selectedProjectKey="project-alpha"
      />,
    );

    await user.click(screen.getByRole("tab", { name: "Series" }));
    const seriesTab = screen.getByTestId("project-metrics-series-tab");
    expect(within(seriesTab).getByText("operational")).toBeTruthy();
    expect(within(seriesTab).getAllByText("Raw values").length).toBeGreaterThan(0);
    expect(within(seriesTab).getAllByText("Make primary").length).toBeGreaterThan(0);

    await user.click(screen.getByRole("tab", { name: "Chart" }));
    expect(screen.getByRole("button", { name: "Trend" }).getAttribute("aria-pressed")).toBe("true");
    await user.click(screen.getByRole("button", { name: "Exclude outliers" }));
    expect(screen.getByRole("button", { name: "Exclude outliers" }).getAttribute("aria-pressed")).toBe("true");
  });

  it("reconciles workspace state across dataset refresh and preserves pinned session when still present", async () => {
    const user = userEvent.setup();
    const initialMetrics = makeResponse(8);
    const refreshedMetrics = makeResponse(10);
    const { rerender } = render(
      <ProjectMetricsScreen
        currentSessionId="session-6"
        error={null}
        includeSpawnAgents={true}
        loading={false}
        metrics={initialMetrics}
        onOpenSession={vi.fn()}
        selectedProjectKey="project-alpha"
      />,
    );

    await user.click(screen.getByRole("tab", { name: "Series" }));
    const tokensCard = screen.getByText("Total tokens").closest("div.rounded-xl") as HTMLElement | null;
    expect(tokensCard).toBeTruthy();
    await user.click(within(tokensCard!).getByRole("button", { name: "Raw values" }));

    await user.click(screen.getByRole("tab", { name: "Chart" }));
    await user.click(screen.getByRole("button", { name: "Exclude outliers" }));
    expect(screen.getByRole("button", { name: "Exclude outliers" }).getAttribute("aria-pressed")).toBe("true");

    rerender(
      <ProjectMetricsScreen
        currentSessionId="session-6"
        error={null}
        includeSpawnAgents={true}
        loading={false}
        metrics={refreshedMetrics}
        onOpenSession={vi.fn()}
        selectedProjectKey="project-alpha"
      />,
    );

    expect(screen.getByTestId("project-metrics-chart-tab")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Exclude outliers" }).getAttribute("aria-pressed")).toBe("true");

    await user.click(screen.getByRole("tab", { name: "Series" }));
    const refreshedTokensCard = screen.getByText("Total tokens").closest("div.rounded-xl") as HTMLElement | null;
    expect(refreshedTokensCard).toBeTruthy();
    expect(within(refreshedTokensCard!).getByRole("button", { name: "Raw values" }).getAttribute("aria-pressed")).toBe("true");

    await user.click(screen.getByRole("tab", { name: "Pinned session" }));
    expect(screen.getByTestId("project-metrics-pinned-tab").textContent).toContain("session-6");
  });

  it("persists chart and series settings in browser storage and restores them after remount", async () => {
    const user = userEvent.setup();
    const props = {
      currentSessionId: null,
      error: null,
      includeSpawnAgents: true,
      loading: false,
      metrics: makeResponse(8),
      onOpenSession: vi.fn(),
      selectedProjectKey: "project-alpha",
    } satisfies ComponentProps<typeof ProjectMetricsScreen>;
    const { unmount } = render(<ProjectMetricsScreen {...props} />);

    expect(screen.getByRole("button", { name: "Zoom out" }).getAttribute("disabled")).not.toBeNull();

    await user.click(screen.getByRole("button", { name: "Zoom in" }));
    expect(screen.getByRole("button", { name: "Zoom out" }).getAttribute("disabled")).toBeNull();

    await user.click(screen.getByRole("tab", { name: "Chart" }));
    await user.click(screen.getByRole("button", { name: "Moving median" }));
    await user.click(screen.getByRole("button", { name: "Exclude outliers" }));

    await user.click(screen.getByRole("tab", { name: "Series" }));
    const totalTokensCard = screen.getByText("Total tokens").closest("div.rounded-xl") as HTMLElement | null;
    expect(totalTokensCard).toBeTruthy();
    await user.click(within(totalTokensCard!).getByRole("button", { name: "Make primary" }));
    await user.click(within(totalTokensCard!).getByRole("button", { name: "Raw values" }));

    unmount();

    render(<ProjectMetricsScreen {...props} />);

    expect(screen.getByRole("button", { name: "Zoom out" }).getAttribute("disabled")).toBeNull();

    await user.click(screen.getByRole("tab", { name: "Chart" }));
    expect(screen.getByRole("button", { name: "Moving median" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("button", { name: "Exclude outliers" }).getAttribute("aria-pressed")).toBe("true");

    await user.click(screen.getByRole("tab", { name: "Series" }));
    const restoredTokensCard = screen.getByText("Total tokens").closest("div.rounded-xl") as HTMLElement | null;
    expect(restoredTokensCard).toBeTruthy();
    expect(within(restoredTokensCard!).getByRole("button", { name: "Raw values" }).getAttribute("aria-pressed")).toBe("true");
    expect(within(restoredTokensCard!).getByText("primary")).toBeTruthy();
  });

  it("resets a stale single-session zoom window when the dataset changes", async () => {
    const initialMetrics = makeResponse(1);
    const switchedMetrics = makeResponse(10);
    switchedMetrics.project_key = "project:other";
    switchedMetrics.contributing_session_ids = switchedMetrics.sessions.map((session, index) => {
      const sessionId = `other-session-${index + 1}`;
      session.session_id = sessionId;
      session.project.project_key = "project:other";
      return sessionId;
    });

    const { rerender } = render(
      <ProjectMetricsScreen
        currentSessionId={null}
        error={null}
        includeSpawnAgents={true}
        loading={false}
        metrics={initialMetrics}
        onOpenSession={vi.fn()}
        selectedProjectKey="project-alpha"
      />,
    );

    expect(screen.getByTestId("project-metrics-single-session-state")).toBeTruthy();

    rerender(
      <ProjectMetricsScreen
        currentSessionId={null}
        error={null}
        includeSpawnAgents={true}
        loading={false}
        metrics={switchedMetrics}
        onOpenSession={vi.fn()}
        selectedProjectKey="project-beta"
      />,
    );

    expect(screen.queryByTestId("project-metrics-single-session-state")).toBeNull();
    expect(screen.getByTestId("project-metrics-chart-surface")).toBeTruthy();
  });

  it("pins the clicked session from the primary bar layer", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <ProjectMetricsScreen
        currentSessionId={null}
        error={null}
        includeSpawnAgents={true}
        loading={false}
        metrics={makeResponse(8)}
        onOpenSession={vi.fn()}
        selectedProjectKey="project-alpha"
      />,
    );

    const primaryBar = container.querySelector("[data-testid='mock-primary-bar']");
    expect(primaryBar).toBeTruthy();

    await user.click(primaryBar as Element);

    expect(screen.getByTestId("project-metrics-pinned-tab")).toBeTruthy();
    expect(screen.getByTestId("project-metrics-pinned-tab").textContent).toContain("session-8");
  });

  it("opens pinned-session tab from a pin action and keeps hidden series in pinned detail", async () => {
    const user = userEvent.setup();

    render(
      <ProjectMetricsScreen
        currentSessionId={null}
        error={null}
        includeSpawnAgents={true}
        loading={false}
        metrics={makeResponse(8)}
        onOpenSession={vi.fn()}
        selectedProjectKey="project-alpha"
      />,
    );

    await user.click(screen.getByRole("tab", { name: /Anomalies/i }));
    await user.click(screen.getAllByRole("button", { name: "Pin session" })[0]!);

    expect(screen.getByTestId("project-metrics-pinned-tab")).toBeTruthy();

    await user.click(screen.getByRole("tab", { name: "Series" }));
    const tokensCard = screen.getByText("Total tokens").closest("div.rounded-xl") as HTMLElement | null;
    expect(tokensCard).toBeTruthy();
    const visibleButton = within(tokensCard!).getByRole("button", { name: "Visible" });
    await user.click(visibleButton);

    await user.click(screen.getByRole("tab", { name: "Pinned session" }));
    const pinnedTab = screen.getByTestId("project-metrics-pinned-tab");
    expect(within(pinnedTab).getAllByText("Tokens").length).toBeGreaterThan(0);
  });

  it("resolves delayed pinned-session detail requests without leaving the tab in loading state", async () => {
    const user = userEvent.setup();
    let resolveDetail!: (detail: ProjectMetricsSessionDetail) => void;
    vi.mocked(loadProjectMetricsSessionDetailById).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveDetail = resolve as (detail: ProjectMetricsSessionDetail) => void;
        }),
    );

    const { container } = render(
      <ProjectMetricsScreen
        currentSessionId={null}
        error={null}
        includeSpawnAgents={true}
        loading={false}
        metrics={makeResponse(8)}
        onOpenSession={vi.fn()}
        selectedProjectKey="project-alpha"
      />,
    );

    const primaryBar = container.querySelector("[data-testid='mock-primary-bar']");
    expect(primaryBar).toBeTruthy();

    await user.click(primaryBar as Element);
    expect(screen.getByText("Loading pinned detail...")).toBeTruthy();

    resolveDetail({
      session_id: "session-8",
      session_ref: "session-8.jsonl",
      title: "Pinned request title",
      start_user_request: "Investigate the abnormal token spike.",
      start_user_request_source: "indexed_first_user_message",
      task_summary: "Compare the selected session against neighboring runs.",
      task_summary_source: "indexed_title",
      agent_role: "default",
      task_class: "analysis",
      task_class_confidence: "confident",
    });

    await waitFor(() => {
      expect(screen.queryByText("Loading pinned detail...")).toBeNull();
    });
    expect(screen.getByText("Investigate the abnormal token spike.")).toBeTruthy();
  });

  it("shows a focused fallback for a single-session chart window", () => {
    render(
      <ProjectMetricsScreen
        currentSessionId="session-1"
        error={null}
        includeSpawnAgents={true}
        loading={false}
        metrics={makeResponse(1)}
        onOpenSession={vi.fn()}
        selectedProjectKey="project-alpha"
      />,
    );

    expect(screen.getByTestId("project-metrics-single-session-state")).toBeTruthy();
    expect(screen.getByText("Single-session window")).toBeTruthy();
  });

  it("keeps chart-point click lightweight and updates pin without opening the session", () => {
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
          payload={{
            index: row!.index,
            label: row!.label,
            modeValues: {
              trend: { tokens: 0.5 },
              "moving-average": {},
              "moving-median": {},
            },
            outlierFlags: {},
            processedValues: { tokens: row!.metrics.tokens.value },
            rawNormalizedValues: { tokens: 0.5 },
            row: row!,
            sessionId: row!.sessionId,
          }}
          payloadKey="tokens"
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

  it("keeps sparse series readable inside shared normalization", () => {
    const response = makeResponse(8);
    response.sessions[7] = makeSession(8);
    response.sessions[7].duration.total_ms = covered(9_000_000);
    response.sessions[0].operations.failed_operations = covered(0);
    response.sessions[1].operations.failed_operations = covered(0);
    response.sessions[2].operations.failed_operations = covered(1);
    response.sessions[3].operations.failed_operations = covered(0);

    const viewModel = buildProjectMetricsViewModel(response, true);
    const durationSeries = viewModel.chartSeries.find((item) => item.key === "duration");
    const failuresSeries = viewModel.chartSeries.find((item) => item.key === "failures");

    expect(durationSeries).toBeTruthy();
    expect(failuresSeries).toBeTruthy();

    const analysis = buildChartAnalysis({
      outlierMode: "clamp",
      rows: viewModel.chartRows,
      series: [durationSeries!, failuresSeries!],
    });

    expect(analysis.normalization.label).toBe("Global percent-delta index");
    expect(analysis.chartData[2]?.rawNormalizedValues.failures).toBeGreaterThan(0.12);
  });
});

describe("resolveSelectionWindow", () => {
  it("normalizes drag direction and ignores one-point selection", () => {
    expect(resolveSelectionWindow(9, 4, 16)).toEqual({
      endIndex: 9,
      startIndex: 4,
    });
    expect(resolveSelectionWindow(4, 4, 16)).toBeNull();
    expect(resolveSelectionWindow(null, 4, 16)).toBeNull();
  });
});

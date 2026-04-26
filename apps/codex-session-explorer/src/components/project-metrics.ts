import type {
  CoveredMetric,
  MetricCoverage,
  ProjectIdentity,
  ProjectMetricsResponse,
  SessionScope,
  SessionScopeCounts,
  SessionMetrics,
} from "@/backend";

export type ProjectMetricsRangePreset = "7d" | "14d" | "30d" | "90d" | "all" | "custom";

export type ProjectMetricsRangeSelection = {
  preset: ProjectMetricsRangePreset;
  start: string;
  end: string;
};

export type ProjectSelectorOption = {
  projectKey: string;
  backendProjectKeys: string[];
  label: string;
  description: string;
  state: ProjectIdentity["state"];
  sessionCount: number;
  availableScopeCounts: SessionScopeCounts;
};

export type ProjectSummaryCard = {
  label: string;
  value: string;
  tone?: "default" | "accent" | "danger";
};

export type ProjectMetricSeriesKey =
  | "duration"
  | "tokens"
  | "allTokens"
  | "tokenInput"
  | "tokenOutput"
  | "tokenCachedInput"
  | "tokenReasoningOutput"
  | "tokenTask"
  | "tokenSpawnAgent"
  | "failures"
  | "toolCalls"
  | "startContext"
  | "skillsCount"
  | "usedSkillsCount"
  | "mcpServerCount"
  | "taskCount"
  | "spawnAgentCalls"
  | "tokensPerSuccess"
  | "reviewFindingsPer1k";

export type ProjectMetricSeriesCategory = "operational" | "tokens" | "factors" | "derived";

export type ProjectMetricPoint = {
  sessionId: string;
  startedAt: string | null;
  label: string;
  value: number | null;
  formattedValue: string;
  coverage: MetricCoverage;
};

export type ProjectMetricSeries = {
  key: ProjectMetricSeriesKey;
  label: string;
  shortLabel: string;
  valueLabel: string;
  description: string;
  category: ProjectMetricSeriesCategory;
  defaultVisible: boolean;
  color: string;
  availablePoints: number;
  partialPoints: number;
  unknownPoints: number;
};

export type ProjectMetricsChartRow = {
  sessionId: string;
  startedAt: string | null;
  label: string;
  index: number;
  metrics: Record<ProjectMetricSeriesKey, ProjectMetricPoint>;
};

export type ProjectMetricsZoomWindow = {
  startIndex: number;
  endIndex: number;
};

export type ContributingSessionItem = {
  sessionId: string;
  startedAt: string | null;
  outcome: SessionMetrics["outcome"]["outcome"];
  sessionScope: SessionScope;
  coverage: MetricCoverage;
  duration: string;
  tokens: string;
  toolCalls: string;
  failures: string;
  projectState: SessionMetrics["project"]["state"];
};

export type UsedSkillSummaryItem = {
  identifier: string;
  usageCount: number;
  sessionCount: number;
  coverage: MetricCoverage;
};

export type ProjectMetricsViewModel = {
  summaryCards: ProjectSummaryCard[];
  chartRows: ProjectMetricsChartRow[];
  chartSeries: ProjectMetricSeries[];
  defaultVisibleSeriesKeys: ProjectMetricSeriesKey[];
  initialZoomWindow: ProjectMetricsZoomWindow;
  sessions: ContributingSessionItem[];
  usedSkills: UsedSkillSummaryItem[];
  usedSkillsCoverage: MetricCoverage;
  degraded: boolean;
  hasUnknownScope: boolean;
};

type ProjectMetricSeriesDefinition = {
  key: ProjectMetricSeriesKey;
  label: string;
  shortLabel: string;
  valueLabel: string;
  description: string;
  category: ProjectMetricSeriesCategory;
  defaultVisible: boolean;
  color: string;
  selectMetric: (session: SessionMetrics, includeSpawnAgents: boolean) => CoveredMetric<number>;
  formatValue: (metric: CoveredMetric<number>) => string;
};

const PROJECT_METRIC_SERIES_DEFINITIONS: readonly ProjectMetricSeriesDefinition[] = [
  {
    key: "duration",
    label: "Total duration",
    shortLabel: "Duration",
    valueLabel: "Duration",
    description: "Время работы по сессии.",
    category: "operational",
    defaultVisible: true,
    color: "#f97316",
    selectMetric: (session, includeSpawnAgents) =>
      includeSpawnAgents
        ? session.duration.total_ms
        : subtractCoveredMetric(session.duration.total_ms, session.duration.spawn_agent_ms),
    formatValue: formatDurationMetric,
  },
  {
    key: "tokens",
    label: "Tokens",
    shortLabel: "Tokens",
    valueLabel: "Tokens",
    description: "Токены без cached-input slice.",
    category: "operational",
    defaultVisible: true,
    color: "#2563eb",
    selectMetric: (session, includeSpawnAgents) =>
      subtractCoveredMetric(
        includeSpawnAgents
          ? session.token_ledger.total
          : subtractCoveredMetric(session.token_ledger.total, session.token_ledger.spawn_agent),
        session.token_ledger.cached_input,
      ),
    formatValue: formatCoveredNumber,
  },
  {
    key: "allTokens",
    label: "All tokens",
    shortLabel: "All tok",
    valueLabel: "All tokens",
    description: "Полный token ledger по сессии, включая cached-input slice.",
    category: "operational",
    defaultVisible: false,
    color: "#1d4ed8",
    selectMetric: (session, includeSpawnAgents) =>
      includeSpawnAgents
        ? session.token_ledger.total
        : subtractCoveredMetric(session.token_ledger.total, session.token_ledger.spawn_agent),
    formatValue: formatCoveredNumber,
  },
  {
    key: "tokenInput",
    label: "Input tokens",
    shortLabel: "Input",
    valueLabel: "Input tokens",
    description: "Input tokens по последнему накопительному snapshot.",
    category: "tokens",
    defaultVisible: false,
    color: "#1d4ed8",
    selectMetric: (session) => session.token_ledger.input,
    formatValue: formatCoveredNumber,
  },
  {
    key: "tokenOutput",
    label: "Output tokens",
    shortLabel: "Output",
    valueLabel: "Output tokens",
    description: "Output tokens по последнему накопительному snapshot.",
    category: "tokens",
    defaultVisible: false,
    color: "#0f766e",
    selectMetric: (session) => session.token_ledger.output,
    formatValue: formatCoveredNumber,
  },
  {
    key: "tokenCachedInput",
    label: "Cached tokens",
    shortLabel: "Cached",
    valueLabel: "Cached tokens",
    description: "Cached-token slice без подмены unknown нулями.",
    category: "tokens",
    defaultVisible: false,
    color: "#0891b2",
    selectMetric: (session) => session.token_ledger.cached_input,
    formatValue: formatCoveredNumber,
  },
  {
    key: "tokenReasoningOutput",
    label: "Reasoning output",
    shortLabel: "Reasoning",
    valueLabel: "Reasoning output tokens",
    description: "Reasoning-output slice отдельной series.",
    category: "tokens",
    defaultVisible: false,
    color: "#7c3aed",
    selectMetric: (session) => session.token_ledger.reasoning_output,
    formatValue: formatCoveredNumber,
  },
  {
    key: "tokenTask",
    label: "Task tokens",
    shortLabel: "Task tok",
    valueLabel: "Task tokens",
    description: "Task-scoped token contribution, если источник её даёт.",
    category: "tokens",
    defaultVisible: false,
    color: "#ea580c",
    selectMetric: (session) => session.token_ledger.task,
    formatValue: formatCoveredNumber,
  },
  {
    key: "tokenSpawnAgent",
    label: "Spawn-agent tokens",
    shortLabel: "Spawn tok",
    valueLabel: "Spawn-agent tokens",
    description: "Вклад дочерних spawn agents в token ledger.",
    category: "tokens",
    defaultVisible: false,
    color: "#be123c",
    selectMetric: (session) => session.token_ledger.spawn_agent,
    formatValue: formatCoveredNumber,
  },
  {
    key: "failures",
    label: "Errors / failed ops",
    shortLabel: "Failures",
    valueLabel: "Failures",
    description: "Ошибки и неуспешные операции без подмены unknown нулями.",
    category: "operational",
    defaultVisible: true,
    color: "#e11d48",
    selectMetric: (session) => preferFailureMetric(session),
    formatValue: formatCoveredNumber,
  },
  {
    key: "toolCalls",
    label: "Tool-call volume",
    shortLabel: "Tool calls",
    valueLabel: "Tool calls",
    description: "Количество tool-call операций по сессии.",
    category: "operational",
    defaultVisible: true,
    color: "#059669",
    selectMetric: (session) => session.operations.tool_calls,
    formatValue: formatCoveredNumber,
  },
  {
    key: "startContext",
    label: "Start context",
    shortLabel: "Context",
    valueLabel: "Start context",
    description: "Стартовый размер контекста по зафиксированной source precedence.",
    category: "factors",
    defaultVisible: false,
    color: "#9333ea",
    selectMetric: (session) => session.context.start_context_size,
    formatValue: formatCoveredNumber,
  },
  {
    key: "skillsCount",
    label: "Enabled skills",
    shortLabel: "Skills",
    valueLabel: "Enabled skills",
    description: "Количество skills из runtime context без synthetic zeros.",
    category: "factors",
    defaultVisible: false,
    color: "#475569",
    selectMetric: (session) => session.factors.skills_count,
    formatValue: formatCoveredNumber,
  },
  {
    key: "usedSkillsCount",
    label: "Used skills",
    shortLabel: "Used",
    valueLabel: "Used skills",
    description: "Количество явно использованных skills по unique usage markers.",
    category: "factors",
    defaultVisible: false,
    color: "#7c2d12",
    selectMetric: (session) => session.used_skills.count,
    formatValue: formatCoveredNumber,
  },
  {
    key: "mcpServerCount",
    label: "MCP servers",
    shortLabel: "MCP",
    valueLabel: "MCP servers",
    description: "Количество MCP servers из runtime context.",
    category: "factors",
    defaultVisible: false,
    color: "#0f766e",
    selectMetric: (session) => session.factors.mcp_server_count,
    formatValue: formatCoveredNumber,
  },
  {
    key: "taskCount",
    label: "Tasks",
    shortLabel: "Tasks",
    valueLabel: "Tasks",
    description: "Session-level task count для сравнения orchestration complexity.",
    category: "factors",
    defaultVisible: false,
    color: "#b45309",
    selectMetric: (session) => session.task_metrics.task_count,
    formatValue: formatCoveredNumber,
  },
  {
    key: "spawnAgentCalls",
    label: "Spawn calls",
    shortLabel: "Spawn",
    valueLabel: "Spawn-agent calls",
    description: "Количество spawn_agent calls по сессии.",
    category: "factors",
    defaultVisible: false,
    color: "#dc2626",
    selectMetric: (session) => session.operations.spawn_agent_calls,
    formatValue: formatCoveredNumber,
  },
  {
    key: "tokensPerSuccess",
    label: "Tokens / success",
    shortLabel: "Tokens/success",
    valueLabel: "Tokens / success",
    description: "Derived efficiency по успешным сессиям.",
    category: "derived",
    defaultVisible: false,
    color: "#0f766e",
    selectMetric: (session) => session.derived_efficiency.tokens_per_successful_session,
    formatValue: formatCoveredFloat,
  },
  {
    key: "reviewFindingsPer1k",
    label: "Review / 1k tokens",
    shortLabel: "Review/1k",
    valueLabel: "Review findings / 1k tokens",
    description: "Плотность review findings на 1000 токенов.",
    category: "derived",
    defaultVisible: false,
    color: "#7c3aed",
    selectMetric: (session) => session.derived_efficiency.review_findings_per_1k_tokens,
    formatValue: formatCoveredFloat,
  },
] as const;

export function createInitialProjectMetricsRange(): ProjectMetricsRangeSelection {
  return {
    preset: "14d",
    start: "",
    end: "",
  };
}

export function resolveProjectMetricsRange(
  range: ProjectMetricsRangeSelection,
  now = new Date(),
): { start_ts: string | null; end_ts: string | null } {
  if (range.preset === "all") {
    return {
      start_ts: null,
      end_ts: null,
    };
  }

  if (range.preset === "custom") {
    return {
      start_ts: normalizeDateTimeInput(range.start),
      end_ts: normalizeDateTimeInput(range.end),
    };
  }

  const end = new Date(now);
  const start = new Date(now);
  const days = range.preset === "7d"
    ? 7
    : range.preset === "14d"
      ? 14
      : range.preset === "90d"
        ? 90
        : 30;
  start.setUTCDate(start.getUTCDate() - days);

  return {
    start_ts: start.toISOString(),
    end_ts: end.toISOString(),
  };
}

export function aggregateProjectMetricsResponses(
  projectKey: string,
  responses: ProjectMetricsResponse[],
): ProjectMetricsResponse {
  const sessions = responses
    .flatMap((response) => response.sessions)
    .sort((left, right) => {
      const leftTs = left.started_at ?? left.ended_at ?? "";
      const rightTs = right.started_at ?? right.ended_at ?? "";
      return leftTs.localeCompare(rightTs) || left.session_id.localeCompare(right.session_id);
    });

  const tokenMetrics = sessions.map((session) => session.token_ledger.total);
  const totalTokens = sumCoveredMetrics(tokenMetrics);
  const totalDuration = sumCoveredMetrics(sessions.map((session) => session.duration.total_ms));
  const taskFacts = responses
    .flatMap((response) => response.task_facts)
    .sort((left, right) => {
      return (
        left.started_at.localeCompare(right.started_at)
        || left.session_id.localeCompare(right.session_id)
        || left.analytic_key.localeCompare(right.analytic_key)
      );
    });

  return {
    project_key: projectKey,
    session_count: sessions.length,
    contributing_session_ids: sessions.map((session) => session.session_id),
    scope_filter: responses[0]?.scope_filter ?? "all",
    available_scope_counts: responses.reduce<SessionScopeCounts>(
      (acc, response) => ({
        main: acc.main + response.available_scope_counts.main,
        subsession: acc.subsession + response.available_scope_counts.subsession,
        unknown: acc.unknown + response.available_scope_counts.unknown,
      }),
      { main: 0, subsession: 0, unknown: 0 },
    ),
    sessions,
    token_ledger: {
      total: totalTokens,
      input: sumCoveredMetrics(sessions.map((session) => session.token_ledger.input)),
      output: sumCoveredMetrics(sessions.map((session) => session.token_ledger.output)),
      cached_input: sumCoveredMetrics(sessions.map((session) => session.token_ledger.cached_input)),
      reasoning_output: sumCoveredMetrics(sessions.map((session) => session.token_ledger.reasoning_output)),
      tool_call: sumCoveredMetrics(sessions.map((session) => session.token_ledger.tool_call)),
      task: sumCoveredMetrics(sessions.map((session) => session.token_ledger.task)),
      spawn_agent: sumCoveredMetrics(sessions.map((session) => session.token_ledger.spawn_agent)),
    },
    duration_ms: totalDuration,
    factors: {
      start_context_size: sumCoveredMetrics(
        sessions.map((session) => session.context.start_context_size),
      ),
      skills_count: sumCoveredMetrics(sessions.map((session) => session.factors.skills_count)),
      mcp_server_count: sumCoveredMetrics(
        sessions.map((session) => session.factors.mcp_server_count),
      ),
    },
    operations: {
      spawn_agent_calls: sumCoveredMetrics(
        sessions.map((session) => session.operations.spawn_agent_calls),
      ),
    },
    task_metrics: {
      task_count: sumCoveredMetrics(sessions.map((session) => session.task_metrics.task_count)),
    },
    task_facts: taskFacts,
    used_skills: aggregateUsedSkills(sessions),
    baseline: {
      coverage: "unknown",
      token_usage_delta: unknownCoveredMetric(),
      duration_delta_ms: unknownCoveredMetric(),
      error_rate_delta: unknownCoveredMetric(),
      outcome_rate_delta: unknownCoveredMetric(),
    },
    derived_efficiency: {
      tokens_per_successful_session: unknownCoveredMetric(),
      tokens_per_accepted_task: unknownCoveredMetric(),
      review_findings_per_1k_tokens: unknownCoveredMetric(),
    },
  };
}

export function buildProjectMetricsViewModel(
  response: ProjectMetricsResponse,
  includeSpawnAgents: boolean,
): ProjectMetricsViewModel {
  const sessions = response.sessions;
  const degraded = sessions.some((session) => session.project.state === "degraded");
  const totalDuration = includeSpawnAgents
    ? response.duration_ms
    : subtractCoveredMetric(
        response.duration_ms,
        sumCoveredMetrics(sessions.map((session) => session.duration.spawn_agent_ms)),
      );
  const totalTokens = includeSpawnAgents
    ? response.token_ledger.total
    : subtractCoveredMetric(
        response.token_ledger.total,
        response.token_ledger.spawn_agent,
      );
  const nonCachedTokens = subtractCoveredMetric(totalTokens, response.token_ledger.cached_input);
  const cachedTokens = response.token_ledger.cached_input;
  const chartRows = sessions.map((session, index) =>
    buildChartRow(session, index, includeSpawnAgents),
  );
  const chartSeries = PROJECT_METRIC_SERIES_DEFINITIONS.map((series) =>
    buildChartSeriesMeta(series, chartRows),
  );
  const usedSkills = response.used_skills.skills.map((skill) => ({
    identifier: skill.identifier,
    usageCount: skill.usage_count,
    sessionCount: skill.session_count,
    coverage: response.used_skills.coverage,
  }));

  return {
    degraded,
    hasUnknownScope: response.available_scope_counts.unknown > 0,
    summaryCards: [
      {
        label: "Sessions",
        value: new Intl.NumberFormat("ru-RU").format(response.session_count),
        tone: "accent",
      },
      {
        label: "Total duration",
        value: formatDurationMetric(totalDuration),
      },
      {
        label: "Tokens",
        value: formatCoveredNumber(nonCachedTokens),
      },
      {
        label: "All tokens",
        value: formatCoveredNumber(totalTokens),
      },
      {
        label: "Cached tokens",
        value: formatCoveredNumber(cachedTokens),
      },
      {
        label: "Used skills",
        value: formatCoveredNumber(response.used_skills.count),
      },
      {
        label: "Baseline",
        value: formatCoverage(response.baseline.coverage),
      },
      {
        label: "Tokens / success",
        value: formatCoveredFloat(response.derived_efficiency.tokens_per_successful_session),
      },
      {
        label: "Review / 1k tokens",
        value: formatCoveredFloat(response.derived_efficiency.review_findings_per_1k_tokens),
      },
    ],
    chartRows,
    chartSeries,
    defaultVisibleSeriesKeys: chartSeries
      .filter((series) => series.defaultVisible)
      .map((series) => series.key),
    initialZoomWindow: buildInitialZoomWindow(chartRows.length),
    sessions: sessions.map((session) => ({
      sessionId: session.session_id,
      startedAt: session.started_at,
      outcome: session.outcome.outcome,
      sessionScope: session.session_scope,
      coverage: preferFailureMetric(session).coverage,
      duration: formatDurationMetric(
        includeSpawnAgents
          ? session.duration.total_ms
          : subtractCoveredMetric(session.duration.total_ms, session.duration.spawn_agent_ms),
      ),
      tokens: formatCoveredNumber(
        subtractCoveredMetric(
          includeSpawnAgents
            ? session.token_ledger.total
            : subtractCoveredMetric(session.token_ledger.total, session.token_ledger.spawn_agent),
          session.token_ledger.cached_input,
        ),
      ),
      toolCalls: formatCoveredNumber(session.operations.tool_calls),
      failures: formatCoveredNumber(preferFailureMetric(session)),
      projectState: session.project.state,
    })),
    usedSkills,
    usedSkillsCoverage: response.used_skills.coverage,
  };
}

export function getProjectMetricPoint(
  row: ProjectMetricsChartRow,
  seriesKey: ProjectMetricSeriesKey,
): ProjectMetricPoint {
  return row.metrics[seriesKey];
}

export function formatProjectMetricSeriesValue(
  seriesKey: ProjectMetricSeriesKey,
  value: number | null,
  coverage: MetricCoverage = value == null ? "unknown" : "known",
): string {
  const definition = PROJECT_METRIC_SERIES_DEFINITIONS.find((item) => item.key === seriesKey);
  if (!definition) {
    return formatCoveredNumber({
      value,
      coverage,
      source: "derived",
    });
  }

  return definition.formatValue({
    value,
    coverage,
    source: "derived",
  });
}

function buildChartRow(
  session: SessionMetrics,
  index: number,
  includeSpawnAgents: boolean,
): ProjectMetricsChartRow {
  const metrics = Object.fromEntries(
    PROJECT_METRIC_SERIES_DEFINITIONS.map((series) => {
      const metric = series.selectMetric(session, includeSpawnAgents);
      return [
        series.key,
        {
          sessionId: session.session_id,
          startedAt: session.started_at,
          label: formatDateTime(session.started_at),
          value: metric.value,
          formattedValue: series.formatValue(metric),
          coverage: metric.coverage,
        },
      ];
    }),
  ) as Record<ProjectMetricSeriesKey, ProjectMetricPoint>;

  return {
    sessionId: session.session_id,
    startedAt: session.started_at,
    label: formatDateTime(session.started_at),
    index,
    metrics,
  };
}

function buildChartSeriesMeta(
  definition: ProjectMetricSeriesDefinition,
  rows: ProjectMetricsChartRow[],
): ProjectMetricSeries {
  let availablePoints = 0;
  let partialPoints = 0;
  let unknownPoints = 0;

  for (const row of rows) {
    const point = row.metrics[definition.key];
    if (point.coverage === "partial") {
      partialPoints += 1;
    }
    if (point.coverage === "unknown") {
      unknownPoints += 1;
      continue;
    }
    if (point.value != null) {
      availablePoints += 1;
    }
  }

  return {
    key: definition.key,
    label: definition.label,
    shortLabel: definition.shortLabel,
    valueLabel: definition.valueLabel,
    description: definition.description,
    category: definition.category,
    defaultVisible: definition.defaultVisible,
    color: definition.color,
    availablePoints,
    partialPoints,
    unknownPoints,
  };
}

function buildInitialZoomWindow(totalPoints: number): ProjectMetricsZoomWindow {
  if (totalPoints <= 1) {
    return {
      startIndex: 0,
      endIndex: Math.max(0, totalPoints - 1),
    };
  }

  return {
    startIndex: 0,
    endIndex: totalPoints - 1,
  };
}

function preferFailureMetric(session: SessionMetrics): CoveredMetric<number> {
  if (session.operations.failed_operations.coverage !== "unknown") {
    return session.operations.failed_operations;
  }
  return session.error_count;
}

function sumCoveredMetrics(metrics: CoveredMetric<number>[]): CoveredMetric<number> {
  const values = metrics
    .map((metric) => metric.value)
    .filter((value): value is number => value != null);

  if (!values.length) {
    return {
      value: null,
      coverage: "unknown",
      source: "unavailable",
    };
  }

  const coverage = metrics.every((metric) => metric.coverage === "known") ? "known" : "partial";
  return {
    value: values.reduce((sum, value) => sum + value, 0),
    coverage,
    source: "derived",
  };
}

function aggregateUsedSkills(sessions: SessionMetrics[]): ProjectMetricsResponse["used_skills"] {
  const byIdentifier = new Map<string, { usage_count: number; session_count: number }>();
  let hasKnown = false;
  let allKnown = sessions.length > 0;

  for (const session of sessions) {
    if (session.used_skills.coverage === "unknown") {
      allKnown = false;
      continue;
    }
    hasKnown = true;
    if (session.used_skills.coverage !== "known") {
      allKnown = false;
    }
    for (const identifier of session.used_skills.identifiers) {
      const entry = byIdentifier.get(identifier) ?? { usage_count: 0, session_count: 0 };
      entry.usage_count += 1;
      entry.session_count += 1;
      byIdentifier.set(identifier, entry);
    }
  }

  if (!hasKnown) {
    return {
      skills: [],
      count: unknownCoveredMetric(),
      coverage: "unknown",
      source: "unavailable",
    };
  }

  return {
    skills: Array.from(byIdentifier.entries())
      .map(([identifier, counts]) => ({
        identifier,
        usage_count: counts.usage_count,
        session_count: counts.session_count,
      }))
      .sort((left, right) => right.usage_count - left.usage_count || left.identifier.localeCompare(right.identifier)),
    count: {
      value: byIdentifier.size,
      coverage: allKnown ? "known" : "partial",
      source: "derived",
    },
    coverage: allKnown ? "known" : "partial",
    source: "derived",
  };
}

function unknownCoveredMetric(): CoveredMetric<number> {
  return {
    value: null,
    coverage: "unknown",
    source: "unavailable",
  };
}

function subtractCoveredMetric(
  total: CoveredMetric<number>,
  excluded: CoveredMetric<number>,
): CoveredMetric<number> {
  if (total.value == null || excluded.value == null) {
    return total;
  }

  return {
    value: Math.max(0, total.value - excluded.value),
    coverage: total.coverage === "known" && excluded.coverage === "known" ? "known" : "partial",
    source: total.source,
  };
}

function normalizeDateTimeInput(value: string) {
  const trimmed = value.trim();
  if (!trimmed) {
    return null;
  }
  const date = new Date(trimmed);
  return Number.isNaN(date.getTime()) ? null : date.toISOString();
}

function formatCoveredNumber(metric: CoveredMetric<number>) {
  if (metric.value == null) {
    return formatCoverage(metric.coverage);
  }
  const formatted = new Intl.NumberFormat("ru-RU").format(metric.value);
  return metric.coverage === "known" ? formatted : `${formatted} (${metric.coverage})`;
}

function formatCoveredFloat(metric: CoveredMetric<number>) {
  if (metric.value == null) {
    return formatCoverage(metric.coverage);
  }
  const formatted = new Intl.NumberFormat("ru-RU", {
    maximumFractionDigits: 2,
  }).format(metric.value);
  return metric.coverage === "known" ? formatted : `${formatted} (${metric.coverage})`;
}

function formatDurationMetric(metric: CoveredMetric<number>) {
  if (metric.value == null) {
    return formatCoverage(metric.coverage);
  }
  const totalSeconds = Math.floor(metric.value / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const formatted = hours > 0
    ? `${hours}h ${String(minutes).padStart(2, "0")}m`
    : minutes > 0
      ? `${minutes}m ${String(seconds).padStart(2, "0")}s`
      : `${seconds}s`;
  return metric.coverage === "known" ? formatted : `${formatted} (${metric.coverage})`;
}

function formatDateTime(value: string | null) {
  if (!value) {
    return "n/a";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat("ru-RU", {
    day: "2-digit",
    month: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

function formatCoverage(coverage: MetricCoverage) {
  return coverage === "unknown" ? "unknown" : coverage;
}

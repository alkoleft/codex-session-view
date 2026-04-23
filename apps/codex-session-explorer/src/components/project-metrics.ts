import type {
  CoveredMetric,
  MetricCoverage,
  ProjectIdentity,
  ProjectMetricsResponse,
  SessionScope,
  SessionScopeCounts,
  SessionMetrics,
} from "@/backend";

export type ProjectMetricsRangePreset = "7d" | "30d" | "90d" | "all" | "custom";

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
  | "failures"
  | "toolCalls"
  | "tokensPerSuccess"
  | "reviewFindingsPer1k";

export type ProjectMetricSeriesCategory = "operational" | "derived";

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

export type ProjectMetricsViewModel = {
  summaryCards: ProjectSummaryCard[];
  chartRows: ProjectMetricsChartRow[];
  chartSeries: ProjectMetricSeries[];
  defaultVisibleSeriesKeys: ProjectMetricSeriesKey[];
  initialZoomWindow: ProjectMetricsZoomWindow;
  sessions: ContributingSessionItem[];
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
    label: "Total tokens",
    shortLabel: "Tokens",
    valueLabel: "Tokens",
    description: "Общий токен usage по сессии.",
    category: "operational",
    defaultVisible: true,
    color: "#2563eb",
    selectMetric: (session, includeSpawnAgents) =>
      includeSpawnAgents
        ? session.token_ledger.total
        : subtractCoveredMetric(session.token_ledger.total, session.token_ledger.spawn_agent),
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
    preset: "30d",
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
  const days = range.preset === "7d" ? 7 : range.preset === "90d" ? 90 : 30;
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
  const durationMetrics = sessions.map((session) => session.duration.total_ms);
  const totalTokens = sumCoveredMetrics(tokenMetrics);
  const totalDuration = sumCoveredMetrics(durationMetrics);

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
      input: unknownCoveredMetric(),
      output: unknownCoveredMetric(),
      cached_input: unknownCoveredMetric(),
      reasoning_output: unknownCoveredMetric(),
      tool_call: unknownCoveredMetric(),
      task: unknownCoveredMetric(),
      spawn_agent: unknownCoveredMetric(),
    },
    duration_ms: totalDuration,
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
  const chartRows = sessions.map((session, index) =>
    buildChartRow(session, index, includeSpawnAgents),
  );
  const chartSeries = PROJECT_METRIC_SERIES_DEFINITIONS.map((series) =>
    buildChartSeriesMeta(series, chartRows),
  );

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
        label: "Total tokens",
        value: formatCoveredNumber(totalTokens),
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
        includeSpawnAgents
          ? session.token_ledger.total
          : subtractCoveredMetric(session.token_ledger.total, session.token_ledger.spawn_agent),
      ),
      toolCalls: formatCoveredNumber(session.operations.tool_calls),
      failures: formatCoveredNumber(preferFailureMetric(session)),
      projectState: session.project.state,
    })),
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
    source: metrics[0]?.source ?? "derived",
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

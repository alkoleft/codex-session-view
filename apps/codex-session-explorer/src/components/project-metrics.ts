import type {
  CoveredMetric,
  IndexedSessionSummary,
  MetricCoverage,
  ProjectIdentity,
  ProjectMetricsResponse,
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
};

type DedupedProject = {
  projectKey: string;
  normalizedCwd: string | null;
  state: ProjectIdentity["state"];
  cwd: string | null;
  gitOriginUrl: string | null;
  gitBranch: string | null;
  newestUpdatedAt: string | null;
  sessionIds: Set<string>;
  backendProjectKeys: Set<string>;
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

export async function buildProjectSelectorOptions(
  sessions: IndexedSessionSummary[],
): Promise<ProjectSelectorOption[]> {
  const deduped = new Map<string, DedupedProject>();

  for (const session of sessions) {
    const project = await deriveProjectIdentity(session);
    const groupKey = project.normalized_cwd ?? `degraded:${session.session_id}`;
    const current = deduped.get(groupKey);
    if (current) {
      current.sessionIds.add(session.session_id);
      current.backendProjectKeys.add(project.project_key);
      if (isNewerTimestamp(session.updated_at, current.newestUpdatedAt)) {
        current.cwd = project.cwd;
        current.gitOriginUrl = project.git_origin_url;
        current.gitBranch = project.git_branch;
        current.newestUpdatedAt = session.updated_at;
      }
      continue;
    }

    deduped.set(groupKey, {
      projectKey: groupKey,
      normalizedCwd: project.normalized_cwd,
      state: project.state,
      cwd: project.cwd,
      gitOriginUrl: project.git_origin_url,
      gitBranch: project.git_branch,
      newestUpdatedAt: session.updated_at,
      sessionIds: new Set([session.session_id]),
      backendProjectKeys: new Set([project.project_key]),
    });
  }

  return Array.from(deduped.values())
    .map((project) => ({
      projectKey: project.projectKey,
      backendProjectKeys: Array.from(project.backendProjectKeys).sort(),
      label: formatProjectLabel(project),
      description: formatProjectDescription(project),
      state: project.state,
      sessionCount: project.sessionIds.size,
    }))
    .sort((left, right) => {
      if (left.state !== right.state) {
        return left.state === "normal" ? -1 : 1;
      }
      return left.label.localeCompare(right.label, "ru");
    });
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

export async function deriveProjectIdentity(
  summary: IndexedSessionSummary,
): Promise<ProjectIdentity> {
  const cwd = cleaned(summary.cwd);
  const normalizedCwd = normalizeProjectPath(cwd);
  const gitOriginUrl = cleaned(summary.git_origin_url);
  const gitBranch = cleaned(summary.git_branch);
  const gitSha = cleaned(summary.git_sha);
  const state = normalizedCwd ? "normal" : "degraded";

  let keyMaterial: string;
  if (normalizedCwd && gitOriginUrl && gitBranch) {
    keyMaterial = `${gitOriginUrl}|${gitBranch}|${normalizedCwd}`;
  } else if (normalizedCwd && gitOriginUrl) {
    keyMaterial = `${gitOriginUrl}|${normalizedCwd}`;
  } else if (normalizedCwd && gitBranch) {
    keyMaterial = `${normalizedCwd}|${gitBranch}`;
  } else if (normalizedCwd) {
    keyMaterial = normalizedCwd;
  } else {
    keyMaterial = summary.session_id.trim()
      ? `degraded-session:${summary.session_id.trim()}`
      : "degraded-unknown";
  }

  const prefix = state === "degraded" ? "degraded" : "project";
  const digest = await sha256Hex(keyMaterial);

  return {
    project_key: `${prefix}:${digest.slice(0, 8)}`,
    state,
    cwd,
    normalized_cwd: normalizedCwd,
    git_origin_url: gitOriginUrl,
    git_branch: gitBranch,
    git_sha: gitSha,
  };
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

function formatProjectLabel(project: DedupedProject) {
  if (project.state === "degraded") {
    return project.cwd ?? "Degraded project";
  }

  const normalized = project.cwd ?? "";
  const parts = normalized.split(/[\\/]/).filter(Boolean);
  const leaf = parts.at(-1);
  return leaf ?? project.gitBranch ?? "Project";
}

function formatProjectDescription(project: DedupedProject) {
  const parts = [
    project.state === "degraded" ? "degraded identity" : null,
    project.gitBranch,
    project.gitOriginUrl,
    project.cwd,
    `${project.sessionIds.size} sessions`,
  ].filter((value): value is string => Boolean(value));

  return parts.join(" · ");
}

function normalizeProjectPath(value: string | null) {
  if (!value) {
    return null;
  }

  const expanded = value.replaceAll("\\", "/").replace(/\/+/g, "/");
  const parts = expanded.split("/");
  const normalized: string[] = [];
  for (const part of parts) {
    if (!part || part === ".") {
      continue;
    }
    if (part === "..") {
      normalized.pop();
      continue;
    }
    normalized.push(part);
  }

  const prefix = expanded.startsWith("/") ? "/" : "";
  return `${prefix}${normalized.join("/")}` || prefix || null;
}

async function sha256Hex(value: string) {
  const encoded = new TextEncoder().encode(value);
  const digest = await globalThis.crypto.subtle.digest("SHA-256", encoded);
  return Array.from(new Uint8Array(digest))
    .map((item) => item.toString(16).padStart(2, "0"))
    .join("");
}

function isNewerTimestamp(left: string | null, right: string | null) {
  if (!left) {
    return false;
  }
  if (!right) {
    return true;
  }
  return new Date(left).getTime() > new Date(right).getTime();
}

function cleaned(value: string | null | undefined) {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
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

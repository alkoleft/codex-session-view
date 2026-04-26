import type {
  EventEntry,
  IndexedSessionSummary,
  LoadedSession,
  SessionMetrics,
  SessionPreview,
  TimelineItem,
} from "@/backend";

const EXCLUDED_TOOL_OPERATION_KINDS = new Set(["file.change"]);
const COLLAB_OPERATION_PREFIX = "collab.";
const MESSAGE_EVENT_PREFIX = "message.";
const ERROR_OPERATION_STATUSES = new Set(["error", "failed"]);

export type SessionMetricValue = {
  label: string;
  value: string;
  tone?: "default" | "danger" | "accent";
};

export type SessionMetricsViewModel = {
  items: SessionMetricValue[];
};

type OperationAggregate = {
  id: string;
  kind: string;
  scopeKey: string;
  toolName: string | null;
  terminalStatus: string | null;
};

type LoadedSessionAggregates = {
  toolCalls: number | null;
  successfulOperations: number | null;
  failedOperations: number | null;
  errors: number;
  messages: number;
  spawnAgentCalls: number | null;
  uniqueTools: number | null;
  successRate: number | null;
  errorRate: number | null;
};

export function buildSessionMetricsViewModel({
  includeSpawnAgents = true,
  selectedPreview,
  selectedIndexedSummary,
  selectedLoadedSession,
}: {
  includeSpawnAgents?: boolean;
  selectedPreview: SessionPreview | null;
  selectedIndexedSummary: IndexedSessionSummary | null;
  selectedLoadedSession: LoadedSession | null;
}): SessionMetricsViewModel {
  const backendMetrics = selectedLoadedSession?.metrics ?? null;
  if (backendMetrics) {
    return buildBackendMetricsViewModel(backendMetrics, includeSpawnAgents);
  }

  const duration = formatDurationBetween(
    selectedPreview?.first_ts ?? null,
    selectedPreview?.last_ts ?? null,
  );
  const events = selectedPreview
    ? formatMetricNumber(selectedPreview.event_count)
    : "n/a";
  const tokens = formatMetricNumber(selectedIndexedSummary?.tokens_used ?? null);
  const loadedAggregates = selectedLoadedSession
    ? buildLoadedSessionAggregates(selectedLoadedSession)
    : null;

  return {
    items: [
      { label: "Time worked", value: duration, tone: "accent" },
      {
        label: "Tool calls",
        value: formatMetricNumber(loadedAggregates?.toolCalls ?? null),
      },
      {
        label: "Errors",
        value: formatMetricNumber(loadedAggregates?.errors ?? null),
        tone:
          loadedAggregates?.errors != null && loadedAggregates.errors > 0
            ? "danger"
            : "default",
      },
      {
        label: "Failed ops",
        value: formatMetricNumber(loadedAggregates?.failedOperations ?? null),
        tone:
          loadedAggregates?.failedOperations != null && loadedAggregates.failedOperations > 0
            ? "danger"
            : "default",
      },
      { label: "Events", value: events },
      {
        label: "Unique tools",
        value: formatMetricNumber(loadedAggregates?.uniqueTools ?? null),
      },
      {
        label: "Threads",
        value: formatMetricNumber(selectedLoadedSession?.tree.thread_count ?? null),
      },
      {
        label: "Spawn agent",
        value: formatMetricNumber(loadedAggregates?.spawnAgentCalls ?? null),
      },
      {
        label: "Successes",
        value: formatMetricNumber(loadedAggregates?.successfulOperations ?? null),
      },
      {
        label: "Success rate",
        value: formatMetricPercent(loadedAggregates?.successRate ?? null),
      },
      {
        label: "Error rate",
        value: formatMetricPercent(loadedAggregates?.errorRate ?? null),
      },
      { label: "Messages", value: formatMetricNumber(loadedAggregates?.messages ?? null) },
      { label: "All tokens", value: tokens },
    ],
  };
}

function buildBackendMetricsViewModel(
  metrics: SessionMetrics,
  includeSpawnAgents: boolean,
): SessionMetricsViewModel {
  const totalTokens = includeSpawnAgents
    ? metrics.token_ledger.total
    : subtractCovered(metrics.token_ledger.total, metrics.token_ledger.spawn_agent);
  const displayedTokens = subtractCovered(totalTokens, metrics.token_ledger.cached_input);
  const totalDuration = includeSpawnAgents
    ? metrics.duration.total_ms
    : subtractCovered(metrics.duration.total_ms, metrics.duration.spawn_agent_ms);
  const reviewFindingsPer1k =
    metrics.derived_efficiency.review_findings_per_1k_tokens;

  return {
    items: [
      { label: "Time worked", value: formatCoveredDuration(totalDuration), tone: "accent" },
      { label: "Tool calls", value: formatCoveredNumber(metrics.operations.tool_calls) },
      {
        label: "Errors",
        value: formatCoveredNumber(metrics.error_count),
        tone: (metrics.error_count.value ?? 0) > 0 ? "danger" : "default",
      },
      {
        label: "Failed ops",
        value: formatCoveredNumber(metrics.operations.failed_operations),
        tone: (metrics.operations.failed_operations.value ?? 0) > 0 ? "danger" : "default",
      },
      { label: "Events", value: formatCoveredNumber(metrics.event_count) },
      { label: "Threads", value: formatCoveredNumber(metrics.thread_count) },
      { label: "Messages", value: formatCoveredNumber(metrics.message_count) },
      { label: "Tokens", value: formatCoveredNumber(displayedTokens) },
      { label: "All tokens", value: formatCoveredNumber(totalTokens) },
      { label: "Input tokens", value: formatCoveredNumber(metrics.token_ledger.input) },
      { label: "Output tokens", value: formatCoveredNumber(metrics.token_ledger.output) },
      { label: "Cached tokens", value: formatCoveredNumber(metrics.token_ledger.cached_input) },
      { label: "Reasoning tokens", value: formatCoveredNumber(metrics.token_ledger.reasoning_output) },
      { label: "Start context", value: formatCoveredNumber(metrics.context.start_context_size) },
      { label: "Spawn agent", value: formatCoveredNumber(metrics.operations.spawn_agent_calls) },
      { label: "Shell time", value: formatCoveredDuration(metrics.duration.shell_ms) },
      { label: "MCP calls", value: formatCoveredNumber(metrics.operations.mcp_calls) },
      { label: "Outcome", value: formatOutcome(metrics.outcome.outcome) },
      { label: "Model", value: metrics.factors.model ?? "unknown" },
      { label: "Reasoning", value: metrics.factors.reasoning_effort ?? "unknown" },
      { label: "CLI", value: metrics.factors.cli_version ?? "unknown" },
      { label: "Sandbox", value: metrics.factors.sandbox_policy_kind ?? "unknown" },
      { label: "Approval", value: metrics.factors.approval_mode ?? "unknown" },
      { label: "Agent role", value: metrics.factors.agent_role ?? "unknown" },
      { label: "Skills", value: formatCoveredNumber(metrics.factors.skills_count) },
      { label: "MCP servers", value: formatCoveredNumber(metrics.factors.mcp_server_count) },
      { label: "Tasks", value: formatCoveredNumber(metrics.task_metrics.task_count) },
      { label: "Turns", value: formatCoveredNumber(metrics.task_metrics.turn_count) },
      { label: "Context compression", value: formatCoveredNumber(metrics.context.context_compression) },
      { label: "Compactions", value: formatCoveredNumber(metrics.context.compaction_events) },
      { label: "Review cycles", value: formatCoveredNumber(metrics.business_review.review_cycles) },
      { label: "Review findings", value: formatCoveredNumber(metrics.business_review.review_findings) },
      { label: "Quality", value: coverageLabel(metrics.quality.feedback_score.coverage) },
      { label: "Baseline", value: coverageLabel(metrics.baseline.coverage) },
      { label: "Tokens/success", value: formatCoveredFloat(metrics.derived_efficiency.tokens_per_successful_session) },
      { label: "Review/1k tok", value: formatCoveredFloat(reviewFindingsPer1k) },
      ...metrics.tool_breakdown.map((item) => ({
        label: `Tool ${formatToolCategory(item.category)}`,
        value: `${formatMetricNumber(item.count)} / ${formatMetricNumber(item.failures)} failed`,
        tone: item.failures > 0 ? "danger" as const : "default" as const,
      })),
    ],
  };
}

export function formatDurationBetween(
  firstTs: string | null,
  lastTs: string | null,
): string {
  if (!firstTs || !lastTs) {
    return "n/a";
  }

  const startedAt = new Date(firstTs);
  const endedAt = new Date(lastTs);
  if (Number.isNaN(startedAt.getTime()) || Number.isNaN(endedAt.getTime())) {
    return "n/a";
  }

  const diffMs = endedAt.getTime() - startedAt.getTime();
  if (diffMs < 0) {
    return "n/a";
  }

  const totalSeconds = Math.floor(diffMs / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  if (hours > 0) {
    return `${hours}h ${String(minutes).padStart(2, "0")}m`;
  }
  if (minutes > 0) {
    return `${minutes}m ${String(seconds).padStart(2, "0")}s`;
  }
  return `${seconds}s`;
}

export function formatMetricNumber(value: number | null): string {
  if (value == null) {
    return "n/a";
  }

  return new Intl.NumberFormat("ru-RU").format(value);
}

export function formatMetricPercent(value: number | null): string {
  if (value == null) {
    return "n/a";
  }

  return `${new Intl.NumberFormat("ru-RU", {
    maximumFractionDigits: 1,
  }).format(value * 100)}%`;
}

function formatCoveredNumber(metric: { value: number | null; coverage: string }): string {
  if (metric.value == null) {
    return coverageLabel(metric.coverage);
  }
  const value = formatMetricNumber(metric.value);
  return metric.coverage === "known" ? value : `${value} (${metric.coverage})`;
}

function formatCoveredFloat(metric: { value: number | null; coverage: string }): string {
  if (metric.value == null) {
    return coverageLabel(metric.coverage);
  }
  const value = new Intl.NumberFormat("ru-RU", {
    maximumFractionDigits: 2,
  }).format(metric.value);
  return metric.coverage === "known" ? value : `${value} (${metric.coverage})`;
}

function formatCoveredDuration(metric: { value: number | null; coverage: string }): string {
  if (metric.value == null) {
    return coverageLabel(metric.coverage);
  }
  const seconds = Math.floor(metric.value / 1000);
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const rest = seconds % 60;
  const value = hours > 0
    ? `${hours}h ${String(minutes).padStart(2, "0")}m`
    : minutes > 0
      ? `${minutes}m ${String(rest).padStart(2, "0")}s`
      : `${rest}s`;
  return metric.coverage === "known" ? value : `${value} (${metric.coverage})`;
}

function subtractCovered(
  total: { value: number | null; coverage: string; source: string },
  excluded: { value: number | null; coverage: string; source: string },
) {
  if (total.value == null || excluded.value == null) {
    return total;
  }
  return {
    value: Math.max(0, total.value - excluded.value),
    coverage: total.coverage === "known" && excluded.coverage === "known" ? "known" : "partial",
    source: total.source,
  };
}

function coverageLabel(coverage: string) {
  return coverage === "unknown" ? "unknown" : coverage;
}

function formatOutcome(outcome: string) {
  return outcome.replaceAll("_", " ");
}

function formatToolCategory(category: string) {
  return category.replaceAll("_", " ");
}

function buildLoadedSessionAggregates(session: LoadedSession): LoadedSessionAggregates {
  const events = flattenSessionEvents(session);
  const messages = events.filter((event) => event.event_type.startsWith(MESSAGE_EVENT_PREFIX)).length;
  const toolOperations = collectOperations(events, {
    includeCollabOperations: true,
    excludeFileChangeOperations: true,
  });
  const allOperations = collectOperations(events, {
    includeCollabOperations: true,
    excludeFileChangeOperations: false,
  });
  const uniqueTools = new Set<string>();

  toolOperations.forEach((operation) => {
    const normalizedToolName = normalizeValue(operation.toolName);
    if (normalizedToolName) {
      uniqueTools.add(normalizedToolName);
    }
  });

  const hasToolOperationData = toolOperations.length > 0;
  const hasAnyOperationData = allOperations.length > 0;
  const failedOperations = hasAnyOperationData
    ? allOperations.filter((operation) =>
      operation.terminalStatus != null
      && ERROR_OPERATION_STATUSES.has(operation.terminalStatus),
    ).length
    : null;
  const directErrors = events.filter((event) => event.event_type === "error").length;

  return {
    toolCalls: hasToolOperationData ? toolOperations.length : null,
    successfulOperations: hasToolOperationData
      ? toolOperations.filter((operation) => operation.terminalStatus === "completed").length
      : null,
    failedOperations,
    errors: directErrors + (failedOperations ?? 0),
    spawnAgentCalls: hasAnyOperationData
      ? allOperations.filter((operation) => operation.kind === "collab.spawn_agent").length
      : null,
    uniqueTools: hasToolOperationData ? uniqueTools.size : null,
    successRate:
      hasToolOperationData
        ? toolOperations.filter((operation) => operation.terminalStatus === "completed").length
          / toolOperations.length
        : null,
    errorRate:
      hasAnyOperationData && failedOperations != null
        ? failedOperations / allOperations.length
        : null,
    messages,
  };
}

function flattenSessionEvents(session: LoadedSession): EventEntry[] {
  return [
    ...session.tree.roots.flatMap((thread) => flattenTimelineItems(thread.items)),
    ...session.tree.orphan_events,
  ];
}

function flattenTimelineItems(items: TimelineItem[]): EventEntry[] {
  return items.flatMap((item) => {
    if ("Event" in item) {
      return [item.Event.event, ...flattenTimelineItems(item.Event.children)];
    }

    return flattenTimelineItems(item.Thread.items);
  });
}

function collectOperations(
  events: EventEntry[],
  {
    includeCollabOperations,
    excludeFileChangeOperations,
  }: { includeCollabOperations: boolean; excludeFileChangeOperations: boolean },
): OperationAggregate[] {
  const operations = new Map<string, OperationAggregate>();

  events.forEach((event) => {
    const operationId = normalizeValue(event.operation_id);
    const operationKind = normalizeValue(event.operation_kind);
    if (
      !operationId
      || !operationKind
      || (excludeFileChangeOperations && EXCLUDED_TOOL_OPERATION_KINDS.has(operationKind))
      || (!includeCollabOperations && operationKind.startsWith(COLLAB_OPERATION_PREFIX))
    ) {
      return;
    }

    const key = buildOperationAggregateKey(event, operationKind, operationId);
    const current = operations.get(key);
    const toolName = normalizeValue(event.tool_name);
    const terminalStatus = normalizeValue(event.operation_status);
    const scopeKey =
      normalizeValue(event.operation_root_event_id)
      ?? normalizeValue(event.thread_id)
      ?? `event:${event.event_id}`;

    if (!current) {
      operations.set(key, {
        id: operationId,
        kind: operationKind,
        scopeKey,
        toolName,
        terminalStatus,
      });
      return;
    }

    if (!current.toolName && toolName) {
      current.toolName = toolName;
    }
    if (!current.terminalStatus && terminalStatus) {
      current.terminalStatus = terminalStatus;
    }
  });

  return [...operations.values()];
}

function buildOperationAggregateKey(
  event: EventEntry,
  operationKind: string,
  operationId: string,
) {
  const scopeKey =
    normalizeValue(event.operation_root_event_id)
    ?? normalizeValue(event.thread_id)
    ?? `event:${event.event_id}`;
  return `${operationKind}:${scopeKey}:${operationId}`;
}

function normalizeValue(value: string | null | undefined) {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

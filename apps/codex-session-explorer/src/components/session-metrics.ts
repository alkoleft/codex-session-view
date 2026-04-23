import type {
  EventEntry,
  IndexedSessionSummary,
  LoadedSession,
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
  selectedPreview,
  selectedIndexedSummary,
  selectedLoadedSession,
}: {
  selectedPreview: SessionPreview | null;
  selectedIndexedSummary: IndexedSessionSummary | null;
  selectedLoadedSession: LoadedSession | null;
}): SessionMetricsViewModel {
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
      { label: "Tokens", value: tokens },
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

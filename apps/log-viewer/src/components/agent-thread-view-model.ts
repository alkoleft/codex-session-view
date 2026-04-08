import type {
  EventEntry,
  EventNode,
  LoadedSession,
  ThreadNode,
  TimelineItem,
} from "@/backend";

const AGENT_ABORTED = "agent.aborted";
const AGENT_FAILED = "agent.failed";
const AGENT_REASONING = "agent.reasoning";
const COLLAB_CLOSE_AGENT = "collab.close_agent";
const COLLAB_SEND_INPUT = "collab.send_input";
const COLLAB_SPAWN_AGENT = "collab.spawn_agent";
const COLLAB_WAIT = "collab.wait";
const MESSAGE_PREFIX = "message.";
const TASK_COMPLETED = "task.completed";
const TASK_STARTED = "task.started";

export type AgentThreadStatus =
  | "failed"
  | "aborted"
  | "closed"
  | "completed"
  | "waiting"
  | "running"
  | "unknown";

export type ThreadPresentationMeta = {
  depth: number;
  displayLabel: string;
  isRoot: boolean;
  nickname: string | null;
  parentThreadId: string | null;
  role: string | null;
  threadId: string;
};

export type AgentCausalStep = {
  eventId: string;
  kind:
    | "spawn"
    | "activity"
    | "wait"
    | "send_input"
    | "completed"
    | "closed"
    | "failed"
    | "aborted";
  label: string;
  seq: number;
  summary: string | null;
  ts: string;
};

export type AgentThreadViewModel = {
  closeEventIds: string[];
  completedAt: string | null;
  depth: number;
  displayLabel: string;
  eventIds: string[];
  firstEventId: string | null;
  lastMeaningfulEventId: string | null;
  lastMeaningfulSeq: number | null;
  lastMeaningfulText: string | null;
  model: string | null;
  nickname: string | null;
  orderSeq: number;
  parentThreadId: string | null;
  promptPreview: string | null;
  reasoningEffort: string | null;
  requestedAgentType: string | null;
  role: string | null;
  sendInputEventIds: string[];
  spawnEventId: string | null;
  spawnPrompt: string | null;
  startedAt: string | null;
  status: AgentThreadStatus;
  steps: AgentCausalStep[];
  threadId: string;
  unboundEventIds: string[];
  waitEventIds: string[];
};

export type AgentGraphViewModel = {
  agents: AgentThreadViewModel[];
  byThreadId: Record<string, AgentThreadViewModel>;
  threadMetaById: Record<string, ThreadPresentationMeta>;
  unboundEventIds: string[];
};

type ThreadRecord = {
  depth: number;
  ownEvents: EventEntry[];
  parentThreadId: string | null;
  thread: ThreadNode;
};

export function buildAgentGraphViewModel(session: LoadedSession): AgentGraphViewModel {
  const threadRecords = collectThreadRecords(session.tree.roots);
  const threadMetaById = Object.fromEntries(
    threadRecords.map((record) => {
      const threadMeta: ThreadPresentationMeta = {
        depth: record.depth,
        displayLabel: formatAgentDisplayLabel(
          record.thread.nickname,
          record.thread.role,
          record.thread.thread_id,
          record.thread.is_root,
        ),
        isRoot: record.thread.is_root,
        nickname: normalizeText(record.thread.nickname),
        parentThreadId: record.parentThreadId,
        role: normalizeText(record.thread.role),
        threadId: record.thread.thread_id,
      };
      return [record.thread.thread_id, threadMeta];
    }),
  );
  const subagentRecords = threadRecords.filter((record) => !record.thread.is_root);
  const knownSubagentThreadIds = new Set(subagentRecords.map((record) => record.thread.thread_id));
  const sortedEvents = [
    ...threadRecords.flatMap((record) => record.ownEvents),
    ...session.tree.orphan_events,
  ].sort(compareEvents);
  const spawnEventsByThreadId = new Map<string, EventEntry[]>();
  const waitEventsByThreadId = new Map<string, EventEntry[]>();
  const sendInputEventsByThreadId = new Map<string, EventEntry[]>();
  const closeEventsByThreadId = new Map<string, EventEntry[]>();
  const unboundEventIds: string[] = [];

  sortedEvents.forEach((event) => {
    if (event.event_type === COLLAB_SPAWN_AGENT) {
      const receiverThreadId = normalizeText(event.spawn_agent?.receiver_thread_id);
      if (!receiverThreadId || !knownSubagentThreadIds.has(receiverThreadId)) {
        unboundEventIds.push(event.event_id);
        return;
      }

      pushEvent(spawnEventsByThreadId, receiverThreadId, event);
      return;
    }

    if (
      event.event_type !== COLLAB_WAIT
      && event.event_type !== COLLAB_SEND_INPUT
      && event.event_type !== COLLAB_CLOSE_AGENT
    ) {
      return;
    }

    const receiverThreadIds = uniqueThreadIds(event.receiver_thread_ids);
    if (receiverThreadIds.length === 0) {
      unboundEventIds.push(event.event_id);
      return;
    }

    let hasKnownReceiver = false;
    receiverThreadIds.forEach((threadId) => {
      if (!knownSubagentThreadIds.has(threadId)) {
        return;
      }

      hasKnownReceiver = true;
      if (event.event_type === COLLAB_WAIT) {
        pushEvent(waitEventsByThreadId, threadId, event);
        return;
      }
      if (event.event_type === COLLAB_SEND_INPUT) {
        pushEvent(sendInputEventsByThreadId, threadId, event);
        return;
      }
      pushEvent(closeEventsByThreadId, threadId, event);
    });

    if (!hasKnownReceiver) {
      unboundEventIds.push(event.event_id);
    }
  });

  const agents = subagentRecords
    .map((record) =>
      buildAgentThreadViewModel(
        record,
        threadMetaById,
        spawnEventsByThreadId.get(record.thread.thread_id) ?? [],
        waitEventsByThreadId.get(record.thread.thread_id) ?? [],
        sendInputEventsByThreadId.get(record.thread.thread_id) ?? [],
        closeEventsByThreadId.get(record.thread.thread_id) ?? [],
      ))
    .sort((left, right) => compareAgentModels(left, right));
  const byThreadId = Object.fromEntries(agents.map((agent) => [agent.threadId, agent]));

  return {
    agents,
    byThreadId,
    threadMetaById,
    unboundEventIds: uniqueThreadIds(unboundEventIds),
  };
}

export function buildThreadPresentationIndex(session: LoadedSession) {
  return buildAgentGraphViewModel(session).threadMetaById;
}

export function agentThreadIdsForEvent(event: EventEntry) {
  const ids: string[] = [];

  if (event.actor_type === "subagent") {
    const threadId = normalizeText(event.thread_id);
    if (threadId) {
      ids.push(threadId);
    }
  }

  if (event.event_type === COLLAB_SPAWN_AGENT) {
    const receiverThreadId = normalizeText(event.spawn_agent?.receiver_thread_id);
    if (receiverThreadId) {
      ids.push(receiverThreadId);
    }
  }

  event.receiver_thread_ids.forEach((threadId) => {
    const normalized = normalizeText(threadId);
    if (normalized) {
      ids.push(normalized);
    }
  });

  return uniqueThreadIds(ids);
}

export function agentSelectionThreadIdForEvent(event: EventEntry) {
  const ids = agentThreadIdsForEvent(event);
  if (event.actor_type === "subagent") {
    return ids[0] ?? null;
  }
  return ids.length === 1 ? ids[0] : null;
}

export function preferredAgentThreadId(graph: AgentGraphViewModel | null) {
  if (!graph || graph.agents.length === 0) {
    return null;
  }

  const activeAgent = graph.agents
    .filter((agent) => agent.status === "waiting" || agent.status === "running")
    .sort((left, right) => compareAgentModels(right, left))[0];
  if (activeAgent) {
    return activeAgent.threadId;
  }

  return [...graph.agents].sort((left, right) => compareAgentModels(right, left))[0]?.threadId ?? null;
}

export function formatAgentDisplayLabel(
  nickname: string | null | undefined,
  role: string | null | undefined,
  fallbackThreadId: string,
  isRoot = false,
) {
  const normalizedNickname = normalizeText(nickname);
  const normalizedRole = normalizeText(role);
  if (normalizedNickname && normalizedRole) {
    return `${normalizedNickname} · ${normalizedRole}`;
  }
  if (normalizedNickname) {
    return normalizedNickname;
  }
  if (normalizedRole) {
    return normalizedRole;
  }
  if (isRoot) {
    return "main";
  }
  return fallbackThreadId;
}

function buildAgentThreadViewModel(
  record: ThreadRecord,
  threadMetaById: Record<string, ThreadPresentationMeta>,
  spawnEvents: EventEntry[],
  waitEvents: EventEntry[],
  sendInputEvents: EventEntry[],
  closeEvents: EventEntry[],
): AgentThreadViewModel {
  const ownEvents = [...record.ownEvents].sort(compareEvents);
  const sortedSpawnEvents = [...spawnEvents].sort(compareEvents);
  const sortedWaitEvents = [...waitEvents].sort(compareEvents);
  const sortedSendInputEvents = [...sendInputEvents].sort(compareEvents);
  const sortedCloseEvents = [...closeEvents].sort(compareEvents);
  const primarySpawnEvent = sortedSpawnEvents[0] ?? null;
  const duplicateSpawnEventIds = sortedSpawnEvents.slice(1).map((event) => event.event_id);
  const firstRelevantEvent = ownEvents.find(isAgentActivityEvent) ?? null;
  const lastMeaningfulEvent = resolveLastMeaningfulEvent(ownEvents, sortedSendInputEvents);
  const failedEvent = findLastEventByType(ownEvents, AGENT_FAILED);
  const abortedEvent = findLastEventByType(ownEvents, AGENT_ABORTED);
  const completedEvent = findLastEventByType(ownEvents, TASK_COMPLETED);
  const closedEvent = sortedCloseEvents.at(-1) ?? null;
  const normalizedThreadStatus = normalizeText(record.thread.status)?.toLowerCase();

  let status: AgentThreadStatus = "unknown";
  if (failedEvent) {
    status = "failed";
  } else if (abortedEvent) {
    status = "aborted";
  } else if (closedEvent) {
    status = "closed";
  } else if (completedEvent || normalizedThreadStatus === "completed") {
    status = "completed";
  } else if (sortedWaitEvents.length > 0) {
    status = "waiting";
  } else if (primarySpawnEvent || ownEvents.length > 0 || normalizedThreadStatus) {
    status = "running";
  }

  const eventIds = uniqueThreadIds([
    ...ownEvents.map((event) => event.event_id),
    ...sortedSpawnEvents.map((event) => event.event_id),
    ...sortedWaitEvents.map((event) => event.event_id),
    ...sortedSendInputEvents.map((event) => event.event_id),
    ...sortedCloseEvents.map((event) => event.event_id),
  ]);
  const steps = buildAgentSteps(
    primarySpawnEvent,
    firstRelevantEvent,
    sortedWaitEvents,
    sortedSendInputEvents,
    completedEvent,
    closedEvent,
    failedEvent,
    abortedEvent,
  );
  const threadMeta = threadMetaById[record.thread.thread_id];
  const firstOwnEvent = ownEvents[0] ?? null;
  const lastTerminalEvent =
    failedEvent
    ?? abortedEvent
    ?? closedEvent
    ?? completedEvent
    ?? null;

  return {
    closeEventIds: sortedCloseEvents.map((event) => event.event_id),
    completedAt: lastTerminalEvent?.ts ?? null,
    depth: Math.max(0, threadMeta.depth - 1),
    displayLabel: threadMeta.displayLabel,
    eventIds,
    firstEventId: firstOwnEvent?.event_id ?? null,
    lastMeaningfulEventId: lastMeaningfulEvent?.event_id ?? null,
    lastMeaningfulSeq: lastMeaningfulEvent?.seq ?? null,
    lastMeaningfulText: summarizeAgentEvent(lastMeaningfulEvent),
    model: normalizeText(primarySpawnEvent?.spawn_agent?.model),
    nickname: threadMeta.nickname,
    orderSeq: primarySpawnEvent?.seq ?? firstOwnEvent?.seq ?? Number.MAX_SAFE_INTEGER,
    parentThreadId: threadMeta.parentThreadId,
    promptPreview: previewText(primarySpawnEvent?.spawn_agent?.prompt ?? null, 120),
    reasoningEffort: normalizeText(primarySpawnEvent?.spawn_agent?.reasoning_effort),
    requestedAgentType: normalizeText(primarySpawnEvent?.spawn_agent?.requested_agent_type),
    role: threadMeta.role,
    sendInputEventIds: sortedSendInputEvents.map((event) => event.event_id),
    spawnEventId: primarySpawnEvent?.event_id ?? null,
    spawnPrompt: normalizeText(primarySpawnEvent?.spawn_agent?.prompt),
    startedAt: primarySpawnEvent?.ts ?? firstOwnEvent?.ts ?? null,
    status,
    steps,
    threadId: record.thread.thread_id,
    unboundEventIds: duplicateSpawnEventIds,
    waitEventIds: sortedWaitEvents.map((event) => event.event_id),
  };
}

function buildAgentSteps(
  spawnEvent: EventEntry | null,
  firstRelevantEvent: EventEntry | null,
  waitEvents: EventEntry[],
  sendInputEvents: EventEntry[],
  completedEvent: EventEntry | null,
  closedEvent: EventEntry | null,
  failedEvent: EventEntry | null,
  abortedEvent: EventEntry | null,
) {
  const steps: AgentCausalStep[] = [];

  if (spawnEvent) {
    steps.push({
      eventId: spawnEvent.event_id,
      kind: "spawn",
      label: "spawn",
      seq: spawnEvent.seq,
      summary: previewText(spawnEvent.spawn_agent?.prompt ?? null, 160),
      ts: spawnEvent.ts,
    });
  }

  if (firstRelevantEvent && firstRelevantEvent.event_id !== spawnEvent?.event_id) {
    steps.push({
      eventId: firstRelevantEvent.event_id,
      kind: "activity",
      label: activityLabel(firstRelevantEvent),
      seq: firstRelevantEvent.seq,
      summary: summarizeAgentEvent(firstRelevantEvent),
      ts: firstRelevantEvent.ts,
    });
  }

  [...waitEvents, ...sendInputEvents]
    .sort(compareEvents)
    .forEach((event) => {
      steps.push({
        eventId: event.event_id,
        kind: event.event_type === COLLAB_WAIT ? "wait" : "send_input",
        label: event.event_type === COLLAB_WAIT ? "wait" : "send input",
        seq: event.seq,
        summary: summarizeAgentEvent(event),
        ts: event.ts,
      });
    });

  if (completedEvent) {
    steps.push({
      eventId: completedEvent.event_id,
      kind: "completed",
      label: "completed",
      seq: completedEvent.seq,
      summary: summarizeAgentEvent(completedEvent),
      ts: completedEvent.ts,
    });
  }

  if (closedEvent) {
    steps.push({
      eventId: closedEvent.event_id,
      kind: "closed",
      label: "closed",
      seq: closedEvent.seq,
      summary: summarizeAgentEvent(closedEvent),
      ts: closedEvent.ts,
    });
  }

  if (failedEvent) {
    steps.push({
      eventId: failedEvent.event_id,
      kind: "failed",
      label: "failed",
      seq: failedEvent.seq,
      summary: summarizeAgentEvent(failedEvent),
      ts: failedEvent.ts,
    });
  }

  if (abortedEvent) {
    steps.push({
      eventId: abortedEvent.event_id,
      kind: "aborted",
      label: "aborted",
      seq: abortedEvent.seq,
      summary: summarizeAgentEvent(abortedEvent),
      ts: abortedEvent.ts,
    });
  }

  return uniqueSteps(steps).sort((left, right) => compareStepEntries(left, right));
}

function resolveLastMeaningfulEvent(ownEvents: EventEntry[], sendInputEvents: EventEntry[]) {
  return [...ownEvents, ...sendInputEvents]
    .filter((event) => isMeaningfulEvent(event) || event.event_type === COLLAB_SEND_INPUT)
    .sort(compareEvents)
    .at(-1) ?? null;
}

function summarizeAgentEvent(event: EventEntry | null) {
  if (!event) {
    return null;
  }

  const lastAgentMessage = normalizeText(event.last_agent_message);
  if (lastAgentMessage) {
    return previewText(lastAgentMessage, 180);
  }

  const prompt = normalizeText(event.spawn_agent?.prompt);
  if (prompt) {
    return previewText(prompt, 180);
  }

  const summary = normalizeText(event.summary);
  if (summary && summary !== event.event_type) {
    return previewText(summary, 180);
  }

  const aggregatedOutput = normalizeText(event.aggregated_output);
  if (aggregatedOutput) {
    return previewText(aggregatedOutput, 180);
  }

  return null;
}

function activityLabel(event: EventEntry) {
  if (event.event_type === TASK_STARTED) {
    return "task started";
  }
  if (event.event_type === TASK_COMPLETED) {
    return "task completed";
  }
  if (event.event_type === AGENT_REASONING) {
    return "reasoning";
  }
  if (event.event_type.startsWith(MESSAGE_PREFIX)) {
    return "message";
  }
  if (event.event_type === AGENT_FAILED) {
    return "failed";
  }
  if (event.event_type === AGENT_ABORTED) {
    return "aborted";
  }
  return event.event_type;
}

function isAgentActivityEvent(event: EventEntry) {
  return (
    event.event_type === TASK_STARTED
    || event.event_type === TASK_COMPLETED
    || event.event_type === AGENT_FAILED
    || event.event_type === AGENT_ABORTED
    || event.event_type === AGENT_REASONING
    || event.event_type.startsWith(MESSAGE_PREFIX)
  );
}

function isMeaningfulEvent(event: EventEntry) {
  return (
    event.event_type === AGENT_FAILED
    || event.event_type === AGENT_ABORTED
    || event.event_type === TASK_COMPLETED
    || event.event_type.startsWith(MESSAGE_PREFIX)
  );
}

function compareAgentModels(left: AgentThreadViewModel, right: AgentThreadViewModel) {
  return (
    left.orderSeq - right.orderSeq
    || (left.lastMeaningfulSeq ?? Number.MAX_SAFE_INTEGER)
      - (right.lastMeaningfulSeq ?? Number.MAX_SAFE_INTEGER)
    || left.threadId.localeCompare(right.threadId)
  );
}

function compareEvents(left: EventEntry, right: EventEntry) {
  return (
    left.seq - right.seq
    || left.ts.localeCompare(right.ts)
    || left.event_id.localeCompare(right.event_id)
  );
}

function compareStepEntries(left: AgentCausalStep, right: AgentCausalStep) {
  return left.seq - right.seq || left.eventId.localeCompare(right.eventId);
}

function collectThreadRecords(
  threads: ThreadNode[],
  parentThreadId: string | null = null,
  depth = 0,
  out: ThreadRecord[] = [],
) {
  threads.forEach((thread) => {
    out.push({
      depth,
      ownEvents: collectThreadOwnEvents(thread.items),
      parentThreadId,
      thread,
    });
    collectChildThreadRecords(thread.items, thread.thread_id, depth + 1, out);
  });

  return out;
}

function collectChildThreadRecords(
  items: TimelineItem[],
  parentThreadId: string,
  depth: number,
  out: ThreadRecord[],
) {
  items.forEach((item) => {
    if ("Thread" in item) {
      collectThreadRecords([item.Thread], parentThreadId, depth, out);
      return;
    }

    collectChildThreadRecordsFromEventNode(item.Event, parentThreadId, depth, out);
  });
}

function collectChildThreadRecordsFromEventNode(
  node: EventNode,
  parentThreadId: string,
  depth: number,
  out: ThreadRecord[],
) {
  node.children.forEach((child) => {
    if ("Thread" in child) {
      collectThreadRecords([child.Thread], parentThreadId, depth, out);
      return;
    }

    collectChildThreadRecordsFromEventNode(child.Event, parentThreadId, depth, out);
  });
}

function collectThreadOwnEvents(items: TimelineItem[], out: EventEntry[] = []) {
  items.forEach((item) => {
    if (!("Event" in item)) {
      return;
    }

    collectEventNode(item.Event, out);
  });
  return out;
}

function collectEventNode(node: EventNode, out: EventEntry[]) {
  out.push(node.event);
  node.children.forEach((child) => {
    if ("Event" in child) {
      collectEventNode(child.Event, out);
    }
  });
}

function findLastEventByType(events: EventEntry[], eventType: string) {
  return [...events].reverse().find((event) => event.event_type === eventType) ?? null;
}

function uniqueThreadIds(values: Array<string | null | undefined>) {
  return values.reduce<string[]>((acc, value) => {
    const normalized = normalizeText(value);
    if (!normalized || acc.includes(normalized)) {
      return acc;
    }
    acc.push(normalized);
    return acc;
  }, []);
}

function uniqueSteps(steps: AgentCausalStep[]) {
  return steps.filter(
    (step, index) =>
      steps.findIndex((candidate) => candidate.eventId === step.eventId) === index,
  );
}

function pushEvent(map: Map<string, EventEntry[]>, key: string, event: EventEntry) {
  const current = map.get(key) ?? [];
  current.push(event);
  map.set(key, current);
}

function normalizeText(value: string | null | undefined) {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

function previewText(value: string | null, limit: number) {
  const normalized = normalizeText(value);
  if (!normalized) {
    return null;
  }
  if (normalized.length <= limit && !normalized.includes("\n")) {
    return normalized;
  }
  return `${normalized.replace(/\s+/g, " ").slice(0, limit).trimEnd()}...`;
}

import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { Check, CircleAlert, CircleX } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import {
  agentSelectionThreadIdForEvent,
  buildThreadPresentationIndex,
  formatAgentDisplayLabel,
  type ThreadPresentationMeta,
} from "@/components/agent-thread-view-model";
import {
  sortEntriesByRenderSeq,
  sortTimelineItemsForRender,
  timelineItemRenderSeq,
  timelineItemsRenderSeq,
} from "@/components/session-event-list-order";
import {
  mergedPlanUpdateRenderData,
  planUpdateRenderData,
  type PlanUpdateRenderData,
} from "@/components/session-event-list-plan";
import {
  patchApplyStatus,
  type PatchApplyStatusData,
} from "@/components/session-event-list-patch";
import { preferredTerminalChildIndexFromSnapshot } from "@/components/session-event-list-selection";
import type {
  EventEntry,
  EventNode,
  LoadedSession,
  PatchApplyChangeEntry,
  ShellParsedCommandEntry,
  TimelineItem,
  UserInputOptionEntry,
  UserInputQuestionEntry,
  UserInputRequestEntry,
} from "@/backend";

const SURFACE_CARD_CLASS = "rounded-none border-0 bg-transparent shadow-none ring-0";
const SHELL_CALL = "shell.call";
const SHELL_RESULT = "shell.result";
const COLLAB_SPAWN_AGENT = "collab.spawn_agent";
const COLLAB_SEND_INPUT = "collab.send_input";
const COLLAB_WAIT = "collab.wait";
const COLLAB_CLOSE_AGENT = "collab.close_agent";
const COLLAB_RESUME_AGENT = "collab.resume_agent";
const INFO_TOKENS = "info.tokens";
const USER_INPUT_REQUEST = "user.input.request";
const RUNTIME_CONTEXT = "runtime.context";
const PATCH_APPLY = "patch.apply";
const PATCH_APPLY_DUPLICATE = "patch.apply.duplicate";
const TODO_UPDATE = "todo.update";
const TASK_STARTED = "task.started";
const TASK_COMPLETED = "task.completed";
const AGENT_META = "agent.meta";
const RESPONSE_ITEM_FUNCTION_CALL_OUTPUT = "response_item.function_call_output";
const TEXT_COLLAPSE_CHAR_LIMIT = 240;
const TEXT_COLLAPSE_LINE_LIMIT = 4;
const COMPACT_CARD_CONTENT_CLASS = "min-w-0 flex flex-col gap-1.5 p-3";
const COMPACT_CARD_CONTENT_SPACED_CLASS = "min-w-0 flex flex-col gap-2 p-3";
const COMPACT_EVENT_HEADER_CLASS =
  "flex flex-wrap items-center gap-x-2 gap-y-1 text-[11px] text-muted-foreground";

type CollabOperationStateEntry = {
  threadId: string | null;
  label: string | null;
  text: string;
};

type PatchApplyDiffSection = {
  key: string;
  label: string;
  diffText: string;
};

type PatchApplyRenderData = {
  phase: string | null;
  status: string | null;
  output: string | null;
  changes: PatchApplyChangeEntry[];
  diffSections: PatchApplyDiffSection[];
  fallbackDiffText: string | null;
};

type TaskTimelinePalette = {
  accent: string;
  surface: string;
  line: string;
  lineOpen: string;
};

type TimelineRenderItem =
  | { kind: "item"; item: TimelineItem; renderSeq: number; index: number }
  | {
      kind: "task-lifecycle";
      items: TimelineItem[];
      isClosed: boolean;
      mode: string | null;
      renderSeq: number;
      index: number;
    };

type SingleCard = {
  kind: "single";
  event: EventEntry;
  children: TimelineItem[];
};

type ShellMergedCard = {
  kind: "shell";
  call: EventEntry;
  result: EventEntry;
  children: TimelineItem[];
};

type SpawnMergedCard = {
  kind: "spawn";
  call: EventEntry;
  result: EventEntry;
  children: TimelineItem[];
};

type UserInputMergedCard = {
  kind: "user-input";
  call: EventEntry;
  result: EventEntry;
  children: TimelineItem[];
};

type CollabMergedCard = {
  kind: "collab";
  call: EventEntry;
  result: EventEntry;
  children: TimelineItem[];
};

type PatchApplyMergedCard = {
  kind: "patch-apply";
  call: EventEntry;
  result: EventEntry;
  children: TimelineItem[];
};

type PlanUpdateMergedCard = {
  kind: "plan-update";
  call: EventEntry;
  result: EventEntry;
  children: TimelineItem[];
};

type MergedCard =
  | SingleCard
  | ShellMergedCard
  | SpawnMergedCard
  | UserInputMergedCard
  | CollabMergedCard
  | PatchApplyMergedCard
  | PlanUpdateMergedCard;

const TaskTimelinePaletteContext = createContext<TaskTimelinePalette | null>(null);
const TimelineThreadMetaContext = createContext<Record<string, ThreadPresentationMeta>>({});

export function SessionEventList({
  focusEventId = null,
  focusRevision = 0,
  onAgentSelect,
  selectedAgentThreadId = null,
  session,
}: {
  focusEventId?: string | null;
  focusRevision?: number;
  onAgentSelect?: (threadId: string) => void;
  selectedAgentThreadId?: string | null;
  session: LoadedSession;
}) {
  const rootItems = session.tree.roots.flatMap((thread) => thread.items);
  const orphanEvents = [...session.tree.orphan_events].sort((left, right) => {
    const leftRenderSeq = left.operation_terminal_seq ?? left.seq;
    const rightRenderSeq = right.operation_terminal_seq ?? right.seq;
    if (leftRenderSeq !== rightRenderSeq) {
      return rightRenderSeq - leftRenderSeq;
    }

    return left.seq - right.seq;
  });
  const threadMetaById = buildThreadPresentationIndex(session);
  const eventElementMapRef = useRef(new Map<string, HTMLDivElement>());

  useEffect(() => {
    if (!focusEventId) {
      return;
    }

    const element = eventElementMapRef.current.get(focusEventId);
    element?.scrollIntoView({
      behavior: "smooth",
      block: "center",
    });
  }, [focusEventId, focusRevision]);

  const registerEventIds = (eventIds: string[], element: HTMLDivElement | null) => {
    const map = eventElementMapRef.current;
    eventIds.forEach((eventId) => {
      if (!eventId) {
        return;
      }

      if (element) {
        map.set(eventId, element);
        return;
      }

      map.delete(eventId);
    });
  };

  return (
    <TimelineThreadMetaContext.Provider value={threadMetaById}>
      <div className="flex flex-col">
        {rootItems.length > 0 ? (
          <TimelineItemsView
            items={rootItems}
            onAgentSelect={onAgentSelect}
            path="root"
            registerEventIds={registerEventIds}
            selectedAgentThreadId={selectedAgentThreadId}
          />
        ) : null}

        {orphanEvents.length > 0 ? (
          <div className="flex flex-col">
            {orphanEvents.map((event, index) => (
              <TimelineListItem key={`orphan-${event.event_id}-${index}`} withDivider={index > 0}>
                <TimelineEventCard
                  eventIds={[event.event_id]}
                  onAgentSelect={onAgentSelect}
                  registerEventIds={registerEventIds}
                  selectedAgentThreadId={selectedAgentThreadId}
                  threadId={agentSelectionThreadIdForEvent(event)}
                >
                  <EventCard event={event} />
                </TimelineEventCard>
              </TimelineListItem>
            ))}
          </div>
        ) : null}
      </div>
    </TimelineThreadMetaContext.Provider>
  );
}

function TimelineItemsView({
  items,
  onAgentSelect,
  path,
  registerEventIds,
  selectedAgentThreadId,
  groupTaskLifecycles = true,
}: {
  items: TimelineItem[];
  onAgentSelect?: (threadId: string) => void;
  path: string;
  registerEventIds: (eventIds: string[], element: HTMLDivElement | null) => void;
  selectedAgentThreadId: string | null;
  groupTaskLifecycles?: boolean;
}) {
  const renderedItems = groupTaskLifecycles ? buildTimelineRenderItems(items) : sortTimelineItemsForRender(items).map((item, index) => ({
    kind: "item" as const,
    item,
    renderSeq: timelineItemRenderSeq(item),
    index,
  }));

  return (
    <div className="flex flex-col">
      {renderedItems.map((entry, index) => {
        if (entry.kind === "task-lifecycle") {
          return (
            <TimelineListItem key={`${path}-task-${index}`} withDivider={index > 0}>
              <TaskLifecycleSegmentView
                isClosed={entry.isClosed}
                items={entry.items}
                mode={entry.mode}
                onAgentSelect={onAgentSelect}
                path={`${path}-task-${index}`}
                registerEventIds={registerEventIds}
                selectedAgentThreadId={selectedAgentThreadId}
              />
            </TimelineListItem>
          );
        }

        return (
          <TimelineListItem key={timelineItemKey(entry.item, path, index)} withDivider={index > 0}>
            <TimelineItemView
              item={entry.item}
              onAgentSelect={onAgentSelect}
              path={`${path}-item-${index}`}
              registerEventIds={registerEventIds}
              selectedAgentThreadId={selectedAgentThreadId}
            />
          </TimelineListItem>
        );
      })}
    </div>
  );
}

function TimelineItemView({
  item,
  onAgentSelect,
  path,
  registerEventIds,
  selectedAgentThreadId,
}: {
  item: TimelineItem;
  onAgentSelect?: (threadId: string) => void;
  path: string;
  registerEventIds: (eventIds: string[], element: HTMLDivElement | null) => void;
  selectedAgentThreadId: string | null;
}) {
  if ("Event" in item) {
    return (
      <EventNodeView
        node={item.Event}
        onAgentSelect={onAgentSelect}
        path={`${path}-event`}
        registerEventIds={registerEventIds}
        selectedAgentThreadId={selectedAgentThreadId}
      />
    );
  }

  return (
    <TimelineItemsView
      items={item.Thread.items}
      onAgentSelect={onAgentSelect}
      path={`${path}-thread-${item.Thread.thread_id}`}
      registerEventIds={registerEventIds}
      selectedAgentThreadId={selectedAgentThreadId}
    />
  );
}

function TaskLifecycleSegmentView({
  items,
  isClosed,
  mode,
  onAgentSelect,
  path,
  registerEventIds,
  selectedAgentThreadId,
}: {
  items: TimelineItem[];
  isClosed: boolean;
  mode: string | null;
  onAgentSelect?: (threadId: string) => void;
  path: string;
  registerEventIds: (eventIds: string[], element: HTMLDivElement | null) => void;
  selectedAgentThreadId: string | null;
}) {
  const palette = taskModePalette(mode);

  return (
    <TaskTimelinePaletteContext.Provider value={palette}>
      <div
        className="relative overflow-hidden py-0.5 pl-4"
        style={{ backgroundColor: palette.surface }}
      >
        <div
          className="absolute bottom-0.5 left-[7px] top-0.5 w-px"
          style={{ backgroundColor: isClosed ? palette.line : palette.lineOpen }}
        />
        <TimelineItemsView
          groupTaskLifecycles={false}
          items={items}
          onAgentSelect={onAgentSelect}
          path={path}
          registerEventIds={registerEventIds}
          selectedAgentThreadId={selectedAgentThreadId}
        />
      </div>
    </TaskTimelinePaletteContext.Provider>
  );
}

function TimelineListItem({
  children,
  withDivider,
}: {
  children: ReactNode;
  withDivider: boolean;
}) {
  return (
    <div className={cn(withDivider ? "border-t border-border/60 pt-1.5" : "")}>
      {children}
    </div>
  );
}

function TimelineEventCard({
  children,
  eventIds,
  onAgentSelect,
  registerEventIds,
  selectedAgentThreadId,
  threadId,
}: {
  children: ReactNode;
  eventIds: string[];
  onAgentSelect?: (threadId: string) => void;
  registerEventIds: (eventIds: string[], element: HTMLDivElement | null) => void;
  selectedAgentThreadId: string | null;
  threadId: string | null;
}) {
  const isSelected = Boolean(threadId && selectedAgentThreadId === threadId);

  return (
    <div
      className={cn(
        "rounded-xl transition-colors",
        threadId ? "cursor-pointer" : "",
        isSelected ? "bg-muted/20 ring-1 ring-[color:var(--accent-strong)]/40" : "",
      )}
      onClick={() => {
        if (threadId) {
          onAgentSelect?.(threadId);
        }
      }}
      ref={(element) => {
        registerEventIds(eventIds, element);
      }}
    >
      {children}
    </div>
  );
}

function EventNodeView({
  node,
  onAgentSelect,
  path,
  registerEventIds,
  selectedAgentThreadId,
}: {
  node: EventNode;
  onAgentSelect?: (threadId: string) => void;
  path: string;
  registerEventIds: (eventIds: string[], element: HTMLDivElement | null) => void;
  selectedAgentThreadId: string | null;
}) {
  const card = buildMergedCard(node);
  const hiddenSingle = card.kind === "single" && shouldHideSingleEvent(card.event);
  const cardEventIds = mergedCardEventIds(card);
  const cardThreadId = mergedCardThreadId(card);

  return (
    <>
      {card.kind === "single" ? (
        !hiddenSingle ? (
          <TimelineEventCard
            eventIds={cardEventIds}
            onAgentSelect={onAgentSelect}
            registerEventIds={registerEventIds}
            selectedAgentThreadId={selectedAgentThreadId}
            threadId={cardThreadId}
          >
            <EventCard event={card.event} />
          </TimelineEventCard>
        ) : null
      ) : (
        <TimelineEventCard
          eventIds={cardEventIds}
          onAgentSelect={onAgentSelect}
          registerEventIds={registerEventIds}
          selectedAgentThreadId={selectedAgentThreadId}
          threadId={cardThreadId}
        >
          <MergedEventCard card={card} />
        </TimelineEventCard>
      )}
      {card.children.length > 0 ? (
        <TimelineItemsView
          items={card.children}
          onAgentSelect={onAgentSelect}
          path={`${path}-children`}
          registerEventIds={registerEventIds}
          selectedAgentThreadId={selectedAgentThreadId}
        />
      ) : null}
    </>
  );
}

function buildTimelineRenderItems(items: TimelineItem[]): TimelineRenderItem[] {
  const renderedItems: TimelineRenderItem[] = [];
  let index = 0;

  while (index < items.length) {
    const taskSegment = taskLifecycleSegmentEnd(items, index);
    if (taskSegment) {
      const [endIndex, isClosed] = taskSegment;
      const segmentItems = items.slice(index, endIndex + 1);
      const mode = timelineItemEvent(segmentItems[0])?.collaboration_mode_kind ?? null;
      renderedItems.push({
        kind: "task-lifecycle",
        items: segmentItems,
        isClosed,
        mode,
        renderSeq: timelineItemsRenderSeq(segmentItems),
        index,
      });
      index = endIndex + 1;
      continue;
    }

    renderedItems.push({
      kind: "item",
      item: items[index],
      renderSeq: timelineItemRenderSeq(items[index]),
      index,
    });
    index += 1;
  }

  return sortEntriesByRenderSeq(renderedItems);
}

function taskLifecycleSegmentEnd(items: TimelineItem[], startIndex: number) {
  const startEvent = timelineItemEvent(items[startIndex]);
  if (!startEvent || !isTaskStartedEvent(startEvent)) {
    return null;
  }

  const startTurnId = startEvent.turn_id ?? null;
  let fallbackEnd = items.length - 1;

  for (let index = startIndex + 1; index < items.length; index += 1) {
    const event = timelineItemEvent(items[index]);
    if (!event) {
      continue;
    }

    if (isTaskCompletedEvent(event)) {
      const sameTurn =
        startTurnId && event.turn_id ? startTurnId === event.turn_id : true;
      if (sameTurn) {
        return [index, true] as const;
      }
    }

    if (isTaskStartedEvent(event)) {
      fallbackEnd = Math.max(startIndex, index - 1);
      break;
    }
  }

  return [fallbackEnd, false] as const;
}

function timelineItemEvent(item: TimelineItem | undefined) {
  if (!item || !("Event" in item)) {
    return null;
  }

  return item.Event.event;
}

function timelineItemKey(item: TimelineItem, path: string, index: number) {
  if ("Event" in item) {
    return `${path}-event-${item.Event.event.event_id}-${index}`;
  }

  return `${path}-thread-${item.Thread.thread_id}-${index}`;
}

function isTaskStartedEvent(event: EventEntry) {
  return (
    event.event_type === TASK_STARTED
    || (event.event_type === AGENT_META && event.meta_type === "task_started")
  );
}

function isTaskCompletedEvent(event: EventEntry) {
  return (
    event.event_type === TASK_COMPLETED
    || (event.event_type === AGENT_META && event.meta_type === "task_complete")
  );
}

function taskModePalette(mode: string | null | undefined): TaskTimelinePalette {
  const normalized = mode?.trim().toLowerCase() ?? "";
  switch (normalized) {
    case "default":
      return {
        accent: "#2563eb",
        surface: "rgba(37, 99, 235, 0.08)",
        line: "rgba(37, 99, 235, 0.95)",
        lineOpen: "rgba(37, 99, 235, 0.35)",
      };
    case "plan":
    case "planning":
      return {
        accent: "#d97706",
        surface: "rgba(217, 119, 6, 0.10)",
        line: "rgba(217, 119, 6, 0.92)",
        lineOpen: "rgba(217, 119, 6, 0.34)",
      };
    case "review":
    case "reviewer":
      return {
        accent: "#be123c",
        surface: "rgba(190, 18, 60, 0.10)",
        line: "rgba(190, 18, 60, 0.92)",
        lineOpen: "rgba(190, 18, 60, 0.34)",
      };
    case "implementation":
    case "worker":
      return {
        accent: "#0f766e",
        surface: "rgba(15, 118, 110, 0.10)",
        line: "rgba(15, 118, 110, 0.92)",
        lineOpen: "rgba(15, 118, 110, 0.34)",
      };
    case "approval":
      return {
        accent: "#7c3aed",
        surface: "rgba(124, 58, 237, 0.10)",
        line: "rgba(124, 58, 237, 0.92)",
        lineOpen: "rgba(124, 58, 237, 0.34)",
      };
    default:
      return taskModeFallbackPalette(normalized);
  }
}

function taskModeFallbackPalette(mode: string) {
  const palette: TaskTimelinePalette[] = [
    {
      accent: "#2563eb",
      surface: "rgba(37, 99, 235, 0.08)",
      line: "rgba(37, 99, 235, 0.95)",
      lineOpen: "rgba(37, 99, 235, 0.35)",
    },
    {
      accent: "#7c3aed",
      surface: "rgba(124, 58, 237, 0.10)",
      line: "rgba(124, 58, 237, 0.92)",
      lineOpen: "rgba(124, 58, 237, 0.34)",
    },
    {
      accent: "#0891b2",
      surface: "rgba(8, 145, 178, 0.10)",
      line: "rgba(8, 145, 178, 0.92)",
      lineOpen: "rgba(8, 145, 178, 0.34)",
    },
    {
      accent: "#d97706",
      surface: "rgba(217, 119, 6, 0.10)",
      line: "rgba(217, 119, 6, 0.92)",
      lineOpen: "rgba(217, 119, 6, 0.34)",
    },
    {
      accent: "#16a34a",
      surface: "rgba(22, 163, 74, 0.10)",
      line: "rgba(22, 163, 74, 0.92)",
      lineOpen: "rgba(22, 163, 74, 0.34)",
    },
    {
      accent: "#be123c",
      surface: "rgba(190, 18, 60, 0.10)",
      line: "rgba(190, 18, 60, 0.92)",
      lineOpen: "rgba(190, 18, 60, 0.34)",
    },
  ];
  const hash = Array.from(mode).reduce(
    (value, character) => (value * 16777619 + character.charCodeAt(0)) >>> 0,
    0,
  );
  return palette[hash % palette.length] ?? palette[0];
}

function EventCard({ event }: { event: EventEntry }) {
  const inheritedTaskPalette = useContext(TaskTimelinePaletteContext);
  const threadMetaById = useContext(TimelineThreadMetaContext);

  if (isStandaloneShellResult(event)) {
    return <SingleShellEventCard event={event} />;
  }

  const infoTokens = infoTokensRenderData(event);
  if (infoTokens) {
    return <InfoTokensEventCard data={infoTokens} event={event} />;
  }

  const patchApply = patchApplyRenderData(event);
  if (patchApply) {
    return <SinglePatchApplyEventCard data={patchApply} event={event} />;
  }

  const planUpdate = planUpdateRenderData(event);
  if (planUpdate) {
    return <PlanUpdateEventCard data={planUpdate} event={event} />;
  }

  if (
    event.event_type === USER_INPUT_REQUEST
    && event.user_input_request
    && userInputRequestHasRenderableContent(event.user_input_request)
  ) {
    return <UserInputRequestEventCard event={event} request={event.user_input_request} />;
  }

  const summary = eventSummaryText(event);
  const detail = singleDetailText(event);
  const subagentLabel = eventSubagentLabel(event, threadMetaById);
  const runtimeContext = runtimeContextRenderData(event);
  const metaItems = [
    ...taskEventMetaItems(event),
    ...genericEventMetaItems(event, threadMetaById),
  ];
  const taskModeBadge = taskEventModeBadge(event);
  const taskMessage = taskCompletedMessage(event);
  const taskMarker = taskLifecycleMarker(event, inheritedTaskPalette);
  const eventLabel = eventHeaderLabel(event, threadMetaById);
  const terminalBadge = eventTerminalBadge(event);

  return (
    <div className="relative">
      {taskMarker ? <TaskLifecycleMarker marker={taskMarker} /> : null}
      <Card
        className={cn(
          SURFACE_CARD_CLASS,
          subagentLabel ? "border-l-4 border-l-[color:var(--accent-strong)]" : "",
        )}
        size="sm"
      >
        <CardContent className={COMPACT_CARD_CONTENT_CLASS}>
          <div className={COMPACT_EVENT_HEADER_CLASS}>
            <span className="font-mono">#{event.seq}</span>
            <time>{formatTime(event.ts)}</time>
            <span className="font-mono">{eventLabel}</span>
            {taskModeBadge ? <TaskModeBadge badge={taskModeBadge} /> : null}
            {terminalBadge ? <TerminalEventBadge badge={terminalBadge} /> : null}
            {subagentLabel ? <Badge variant="outline">{subagentLabel}</Badge> : null}
          </div>
          {metaItems.length > 0 ? <EventMetaRow items={metaItems} /> : null}
          {runtimeContext ? <RuntimeContextBlock data={runtimeContext} /> : null}
          {summary ? <CardText text={summary} tone="default" /> : null}
          {taskMessage ? <TaskCompletedMessageBlock text={taskMessage} /> : null}
          {!taskMessage && detail ? <CardText text={detail} tone="muted" /> : null}
        </CardContent>
      </Card>
    </div>
  );
}

function SinglePatchApplyEventCard({
  event,
  data,
}: {
  event: EventEntry;
  data: PatchApplyRenderData;
}) {
  const threadMetaById = useContext(TimelineThreadMetaContext);
  const subagentLabel = eventSubagentLabel(event, threadMetaById);
  const status = patchApplyStatus(data.status, data.phase);

  return (
    <Card
      className={cn(
        SURFACE_CARD_CLASS,
        subagentLabel ? "border-l-4 border-l-[color:var(--accent-strong)]" : "",
      )}
      size="sm"
    >
      <CardContent className={COMPACT_CARD_CONTENT_CLASS}>
        <PatchApplyEventHeader
          eventLabel={event.event_type}
          seqLabel={`#${event.seq}`}
          status={status}
          subagentLabel={subagentLabel}
          timestampLabel={formatTime(event.ts)}
        />
        <PatchApplyBlock data={data} />
      </CardContent>
    </Card>
  );
}

function TaskCompletedMessageBlock({ text }: { text: string }) {
  return (
    <div className="flex flex-col gap-0.5">
      <div className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
        Last Agent Message
      </div>
      <CardText text={text} tone="default" />
    </div>
  );
}

function TaskModeBadge({
  badge,
}: {
  badge: { label: string; palette: TaskTimelinePalette };
}) {
  return (
    <span
      className="inline-flex items-center rounded-sm px-1.5 py-0.5 text-[11px] font-semibold uppercase tracking-[0.08em]"
      style={{
        color: badge.palette.accent,
        backgroundColor: badge.palette.surface,
      }}
    >
      {badge.label}
    </span>
  );
}

function TerminalEventBadge({
  badge,
}: {
  badge: { className: string; label: string };
}) {
  return (
    <span className={badge.className}>
      {badge.label}
    </span>
  );
}

function TaskLifecycleMarker({
  marker,
}: {
  marker: { accent: string; completed: boolean };
}) {
  return (
    <span
      aria-hidden="true"
      className="absolute -left-4 top-4 z-10 size-2.5 rounded-full border-2"
      style={{
        borderColor: marker.accent,
        backgroundColor: marker.completed ? "var(--background)" : marker.accent,
      }}
    />
  );
}

function MergedEventCard({
  card,
}: {
  card: Exclude<MergedCard, { kind: "single" }>;
}) {
  const threadMetaById = useContext(TimelineThreadMetaContext);

  if (card.kind === "shell") {
    return <MergedShellEventCard card={card} />;
  }

  if (card.kind === "patch-apply") {
    return <MergedPatchApplyEventCard card={card} />;
  }

  if (card.kind === "plan-update") {
    const data = mergedPlanUpdateRenderData(card.call, card.result);
    if (data) {
      return <MergedPlanUpdateEventCard card={card} data={data} />;
    }
  }

  if (card.kind === "user-input") {
    const request = mergedUserInputRequestEntry(card.call, card.result);
    if (request && userInputRequestHasRenderableContent(request)) {
      return <MergedUserInputRequestEventCard card={card} request={request} />;
    }
  }

  const summary = mergedSummaryText(card.call, card.result);
  const detail = mergedDetailText(card);
  const subagentLabel =
    eventSubagentLabel(card.call, threadMetaById)
    ?? eventSubagentLabel(card.result, threadMetaById);
  const eventLabel = mergedEventHeaderLabel(card.call, card.result, threadMetaById);
  const metaItems = mergedGenericEventMetaItems(card.call, card.result, threadMetaById);
  const timeLabel =
    card.call.ts === card.result.ts
      ? formatTime(card.call.ts)
      : `${formatTime(card.call.ts)} -> ${formatTime(card.result.ts)}`;

  return (
    <Card
      className={cn(
        SURFACE_CARD_CLASS,
        subagentLabel ? "border-l-4 border-l-[color:var(--accent-strong)]" : "",
      )}
      size="sm"
    >
      <CardContent className={COMPACT_CARD_CONTENT_CLASS}>
        <div className={COMPACT_EVENT_HEADER_CLASS}>
          <span className="font-mono">
            #{card.call.seq}, #{card.result.seq}
          </span>
          <time>{timeLabel}</time>
          <span className="font-mono">{eventLabel}</span>
          {subagentLabel ? <Badge variant="outline">{subagentLabel}</Badge> : null}
        </div>
        {metaItems.length > 0 ? <EventMetaRow items={metaItems} /> : null}
        {summary ? <CardText text={summary} tone="default" /> : null}
        {detail ? <CardText text={detail} tone="muted" /> : null}
      </CardContent>
    </Card>
  );
}

function MergedPatchApplyEventCard({
  card,
}: {
  card: PatchApplyMergedCard;
}) {
  const threadMetaById = useContext(TimelineThreadMetaContext);
  const subagentLabel =
    eventSubagentLabel(card.call, threadMetaById)
    ?? eventSubagentLabel(card.result, threadMetaById);
  const patchApply = mergedPatchApplyRenderData(card.call, card.result);
  const status = patchApplyStatus(patchApply.status, patchApply.phase);

  return (
    <Card
      className={cn(
        SURFACE_CARD_CLASS,
        subagentLabel ? "border-l-4 border-l-[color:var(--accent-strong)]" : "",
      )}
      size="sm"
    >
      <CardContent className={COMPACT_CARD_CONTENT_CLASS}>
        <PatchApplyEventHeader
          eventLabel={card.call.event_type}
          seqLabel={`#${card.call.seq}`}
          status={status}
          subagentLabel={subagentLabel}
          timestampLabel={formatTime(card.call.ts)}
        />
        <PatchApplyBlock data={patchApply} />
      </CardContent>
    </Card>
  );
}

function SingleShellEventCard({ event }: { event: EventEntry }) {
  const threadMetaById = useContext(TimelineThreadMetaContext);
  const subagentLabel = eventSubagentLabel(event, threadMetaById);
  const metaItems = singleShellMetaItems(event);
  const outputSizeLabel = shellOutputSizeLabel(event.aggregated_output);
  const durationLabel = formatShellDurationNs(event.shell_duration_ns);
  const detailData = shellDetailData(event, event);

  return (
    <Card
      className={cn(
        SURFACE_CARD_CLASS,
        subagentLabel ? "border-l-4 border-l-[color:var(--accent-strong)]" : "",
      )}
      size="sm"
    >
      <CardContent className={COMPACT_CARD_CONTENT_SPACED_CLASS}>
        <ShellEventHeader
          durationLabel={durationLabel}
          eventLabel={event.event_type}
          exitCode={event.shell_exit_code}
          outputSizeLabel={outputSizeLabel}
          seqLabel={`#${event.seq.toString().padStart(4, "0")}`}
          subagentLabel={subagentLabel}
          timestampLabel={formatTime(event.ts)}
        />
        {metaItems.length > 0 ? <EventMetaRow items={metaItems} /> : null}
        <ShellBlock
          command={event.shell_command}
          detailData={detailData}
          output={event.aggregated_output}
        />
      </CardContent>
    </Card>
  );
}

function MergedShellEventCard({
  card,
}: {
  card: ShellMergedCard;
}) {
  const threadMetaById = useContext(TimelineThreadMetaContext);
  const subagentLabel =
    eventSubagentLabel(card.call, threadMetaById)
    ?? eventSubagentLabel(card.result, threadMetaById);
  const metaItems = mergedShellMetaItems(card.call, card.result);
  const outputSizeLabel = shellOutputSizeLabel(card.result.aggregated_output);
  const durationLabel = formatShellDurationNs(
    card.result.shell_duration_ns ?? card.call.shell_duration_ns,
  );
  const detailData = shellDetailData(card.call, card.result);
  const timestampLabel =
    card.call.ts === card.result.ts
      ? formatTime(card.call.ts)
      : `${formatTime(card.call.ts)} -> ${formatTime(card.result.ts)}`;

  return (
    <Card
      className={cn(
        SURFACE_CARD_CLASS,
        subagentLabel ? "border-l-4 border-l-[color:var(--accent-strong)]" : "",
      )}
      size="sm"
    >
      <CardContent className={COMPACT_CARD_CONTENT_SPACED_CLASS}>
        <ShellEventHeader
          durationLabel={durationLabel}
          eventLabel={`${card.call.event_type}, ${card.result.event_type}`}
          exitCode={card.result.shell_exit_code}
          outputSizeLabel={outputSizeLabel}
          seqLabel={`#${card.call.seq.toString().padStart(4, "0")}, #${card.result.seq
            .toString()
            .padStart(4, "0")}`}
          subagentLabel={subagentLabel}
          timestampLabel={timestampLabel}
        />
        {metaItems.length > 0 ? <EventMetaRow items={metaItems} /> : null}
        <ShellBlock
          command={card.call.shell_command ?? card.result.shell_command}
          detailData={detailData}
          output={card.result.aggregated_output}
        />
      </CardContent>
    </Card>
  );
}

function ShellEventHeader({
  durationLabel,
  seqLabel,
  timestampLabel,
  eventLabel,
  subagentLabel,
  exitCode,
  outputSizeLabel,
}: {
  durationLabel: string | null;
  seqLabel: string;
  timestampLabel: string;
  eventLabel: string;
  subagentLabel: string | null;
  exitCode: number | null;
  outputSizeLabel: string | null;
}) {
  const status = shellStatus(exitCode);
  const statusText = shellStatusDetailText(exitCode);

  return (
    <div className="grid min-w-0 w-full grid-cols-[minmax(0,1fr)_auto] items-start gap-2.5 text-[11px] text-muted-foreground">
      <div className="flex min-w-0 flex-wrap items-center gap-2">
        <span className="font-mono">{seqLabel}</span>
        <time>{timestampLabel}</time>
        <span className="font-mono">{eventLabel}</span>
        {subagentLabel ? <Badge variant="outline">{subagentLabel}</Badge> : null}
      </div>
      <div className="flex shrink-0 items-center justify-self-end gap-2">
        {statusText ? (
          <span
            className={cn(
              "whitespace-nowrap font-medium",
              status.variant === "failure"
                ? "text-rose-700 dark:text-rose-300"
                : "text-amber-700 dark:text-amber-300",
            )}
          >
            {statusText}
          </span>
        ) : null}
        {durationLabel ? <span className="whitespace-nowrap">{durationLabel}</span> : null}
        {outputSizeLabel ? <span className="whitespace-nowrap">{outputSizeLabel}</span> : null}
        <span
          className={cn(
            "inline-flex items-center",
            status.variant === "success"
              ? "text-emerald-700 dark:text-emerald-300"
              : status.variant === "failure"
                ? "text-rose-700 dark:text-rose-300"
                : "text-amber-700 dark:text-amber-300",
          )}
          aria-label={status.ariaLabel}
          title={status.ariaLabel}
        >
          <status.Icon aria-hidden="true" className="size-3.5 shrink-0" />
          <span className="sr-only">{status.ariaLabel}</span>
        </span>
      </div>
    </div>
  );
}

function EventMetaRow({
  items,
}: {
  items: Array<{ label: string; value: string }>;
}) {
  return (
    <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[11px] text-muted-foreground">
      {items.map((item) => (
        <span className="inline-flex items-center gap-1" key={`${item.label}-${item.value}`}>
          <span>{item.label}</span>
          <span className="ui-selectable font-semibold text-foreground">{item.value}</span>
        </span>
      ))}
    </div>
  );
}

function PatchApplyEventHeader({
  seqLabel,
  timestampLabel,
  eventLabel,
  subagentLabel,
  status,
}: {
  seqLabel: string;
  timestampLabel: string;
  eventLabel: string;
  subagentLabel: string | null;
  status: PatchApplyStatusData;
}) {
  const Icon =
    status.variant === "success"
      ? Check
      : status.variant === "failure"
        ? CircleX
        : CircleAlert;

  return (
    <div className="grid min-w-0 w-full grid-cols-[minmax(0,1fr)_auto] items-start gap-2.5 text-[11px] text-muted-foreground">
      <div className="flex min-w-0 flex-wrap items-center gap-2">
        <span className="font-mono">{seqLabel}</span>
        <time>{timestampLabel}</time>
        <span className="font-mono">{eventLabel}</span>
        {subagentLabel ? <Badge variant="outline">{subagentLabel}</Badge> : null}
      </div>
      <div className="flex shrink-0 items-center justify-self-end gap-2">
        {status.detailText ? (
          <span
            className={cn(
              "whitespace-nowrap font-medium",
              status.variant === "success"
                ? "text-emerald-700 dark:text-emerald-300"
                : status.variant === "failure"
                  ? "text-rose-700 dark:text-rose-300"
                  : "text-amber-700 dark:text-amber-300",
            )}
          >
            {status.detailText}
          </span>
        ) : null}
        <span
          className={cn(
            "inline-flex items-center",
            status.variant === "success"
              ? "text-emerald-700 dark:text-emerald-300"
              : status.variant === "failure"
                ? "text-rose-700 dark:text-rose-300"
                : "text-amber-700 dark:text-amber-300",
          )}
          aria-label={status.ariaLabel}
          title={status.ariaLabel}
        >
          <Icon aria-hidden="true" className="size-3.5 shrink-0" />
          <span className="sr-only">{status.ariaLabel}</span>
        </span>
      </div>
    </div>
  );
}

function InfoTokensEventCard({
  event,
  data,
}: {
  event: EventEntry;
  data: {
    pairs: Array<{ label: string; value: string }>;
    fallbackText: string | null;
  };
}) {
  const threadMetaById = useContext(TimelineThreadMetaContext);
  const subagentLabel = eventSubagentLabel(event, threadMetaById);

  return (
    <Card
      className={cn(
        SURFACE_CARD_CLASS,
        subagentLabel ? "border-l-4 border-l-[color:var(--accent-strong)]" : "",
      )}
      size="sm"
    >
      <CardContent className="min-w-0 p-3">
        <div className="grid min-w-0 w-full grid-cols-[auto_minmax(0,1fr)] items-center gap-3">
          <div className="flex shrink-0 items-center gap-2 whitespace-nowrap text-[11px] text-muted-foreground">
            <span className="font-mono">#{event.seq}</span>
            <time>{formatTime(event.ts)}</time>
            {subagentLabel ? <Badge variant="outline">{subagentLabel}</Badge> : null}
          </div>
          <InfoTokensBlock fallbackText={data.fallbackText} pairs={data.pairs} />
        </div>
      </CardContent>
    </Card>
  );
}

function InfoTokensBlock({
  pairs,
  fallbackText,
}: {
  pairs: Array<{ label: string; value: string }>;
  fallbackText: string | null;
}) {
  if (pairs.length > 0) {
    return (
      <div className="min-w-0 overflow-x-auto">
        <div className="flex min-w-max flex-nowrap items-center justify-end gap-3 text-right">
          {pairs.map((pair) => (
            <div
              className="inline-flex shrink-0 items-baseline gap-1.5 whitespace-nowrap"
              key={`${pair.label}-${pair.value}`}
            >
              <span className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
                {pair.label}
              </span>
              <span className="ui-selectable font-mono text-xs text-foreground">
                {pair.value}
              </span>
            </div>
          ))}
        </div>
      </div>
    );
  }

  if (fallbackText) {
    return (
      <div className="min-w-0 overflow-x-auto">
        <div className="ui-selectable min-w-max whitespace-nowrap text-right text-xs text-muted-foreground">
          {fallbackText}
        </div>
      </div>
    );
  }

  return (
    <div />
  );
}

function PlanUpdateEventCard({
  event,
  data,
}: {
  event: EventEntry;
  data: PlanUpdateRenderData;
}) {
  const threadMetaById = useContext(TimelineThreadMetaContext);
  const subagentLabel = eventSubagentLabel(event, threadMetaById);
  const metaItems = singlePlanUpdateMetaItems(event, data);

  return (
    <Card
      className={cn(
        SURFACE_CARD_CLASS,
        subagentLabel ? "border-l-4 border-l-[color:var(--accent-strong)]" : "",
      )}
      size="sm"
    >
      <CardContent className={COMPACT_CARD_CONTENT_SPACED_CLASS}>
        <div className={COMPACT_EVENT_HEADER_CLASS}>
          <span className="font-mono">#{event.seq}</span>
          <time>{formatTime(event.ts)}</time>
          <span className="font-mono">{event.event_type}</span>
          {subagentLabel ? <Badge variant="outline">{subagentLabel}</Badge> : null}
        </div>
        {metaItems.length > 0 ? <EventMetaRow items={metaItems} /> : null}
        <PlanUpdateBlock data={data} />
      </CardContent>
    </Card>
  );
}

function MergedPlanUpdateEventCard({
  card,
  data,
}: {
  card: PlanUpdateMergedCard;
  data: PlanUpdateRenderData;
}) {
  const threadMetaById = useContext(TimelineThreadMetaContext);
  const subagentLabel =
    eventSubagentLabel(card.call, threadMetaById)
    ?? eventSubagentLabel(card.result, threadMetaById);
  const metaItems = mergedPlanUpdateMetaItems(card.call, card.result, data);
  const eventLabel =
    card.call.event_type === card.result.event_type
      ? card.call.event_type
      : `${card.call.event_type}/${card.result.event_type}`;
  const timeLabel =
    card.call.ts === card.result.ts
      ? formatTime(card.call.ts)
      : `${formatTime(card.call.ts)} -> ${formatTime(card.result.ts)}`;

  return (
    <Card
      className={cn(
        SURFACE_CARD_CLASS,
        subagentLabel ? "border-l-4 border-l-[color:var(--accent-strong)]" : "",
      )}
      size="sm"
    >
      <CardContent className={COMPACT_CARD_CONTENT_SPACED_CLASS}>
        <div className={COMPACT_EVENT_HEADER_CLASS}>
          <span className="font-mono">
            #{card.call.seq}, #{card.result.seq}
          </span>
          <time>{timeLabel}</time>
          <span className="font-mono">{eventLabel}</span>
          {subagentLabel ? <Badge variant="outline">{subagentLabel}</Badge> : null}
        </div>
        {metaItems.length > 0 ? <EventMetaRow items={metaItems} /> : null}
        <PlanUpdateBlock data={data} />
      </CardContent>
    </Card>
  );
}

function PlanUpdateBlock({ data }: { data: PlanUpdateRenderData }) {
  return (
    <div className="flex flex-col gap-3">
      {data.explanation ? (
        <div className="flex flex-col gap-1">
          <div className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
            explanation
          </div>
          <CardText text={data.explanation} tone="default" />
        </div>
      ) : null}
      {data.steps.length > 0 ? (
        <div className="flex flex-col gap-2">
          <div className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
            steps
          </div>
          <div className="flex flex-col gap-2">
            {data.steps.map((step, index) => (
              <div
                className="grid gap-2 rounded-xl border border-border/60 bg-muted/10 px-3 py-2 sm:grid-cols-[auto_minmax(0,1fr)_auto] sm:items-start sm:gap-3"
                key={`${index + 1}-${step.step}-${step.status ?? ""}`}
              >
                <div className="font-mono text-[11px] text-muted-foreground">
                  {(index + 1).toString().padStart(2, "0")}
                </div>
                <div className="ui-selectable whitespace-pre-wrap break-words text-sm text-foreground">
                  {step.step}
                </div>
                {step.status ? <PlanStepStatusBadge status={step.status} /> : null}
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}

function PlanStepStatusBadge({ status }: { status: string }) {
  return (
    <span className={planStepStatusClassName(status)}>
      {status.replaceAll("_", " ")}
    </span>
  );
}

function planStepStatusClassName(status: string) {
  switch (status.trim()) {
    case "completed":
      return "inline-flex items-center rounded-full border border-emerald-500/35 bg-emerald-500/12 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.08em] text-emerald-700 dark:text-emerald-300";
    case "in_progress":
      return "inline-flex items-center rounded-full border border-sky-500/35 bg-sky-500/12 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.08em] text-sky-700 dark:text-sky-300";
    case "pending":
      return "inline-flex items-center rounded-full border border-amber-500/35 bg-amber-500/12 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.08em] text-amber-700 dark:text-amber-300";
    default:
      return "inline-flex items-center rounded-full border border-border/60 bg-background/60 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground";
  }
}

function UserInputRequestEventCard({
  event,
  request,
}: {
  event: EventEntry;
  request: UserInputRequestEntry;
}) {
  const threadMetaById = useContext(TimelineThreadMetaContext);
  const subagentLabel = eventSubagentLabel(event, threadMetaById);
  const metaItems = userInputRequestMetaItems(request);

  return (
    <Card
      className={cn(
        SURFACE_CARD_CLASS,
        subagentLabel ? "border-l-4 border-l-[color:var(--accent-strong)]" : "",
      )}
      size="sm"
    >
      <CardContent className={COMPACT_CARD_CONTENT_SPACED_CLASS}>
        <div className={COMPACT_EVENT_HEADER_CLASS}>
          <span className="font-mono">#{event.seq}</span>
          <time>{formatTime(event.ts)}</time>
          <span className="font-mono">{event.event_type}</span>
          {subagentLabel ? <Badge variant="outline">{subagentLabel}</Badge> : null}
        </div>
        {metaItems.length > 0 ? <EventMetaRow items={metaItems} /> : null}
        <UserInputRequestBlock request={request} />
      </CardContent>
    </Card>
  );
}

function MergedUserInputRequestEventCard({
  card,
  request,
}: {
  card: UserInputMergedCard;
  request: UserInputRequestEntry;
}) {
  const threadMetaById = useContext(TimelineThreadMetaContext);
  const subagentLabel =
    eventSubagentLabel(card.call, threadMetaById)
    ?? eventSubagentLabel(card.result, threadMetaById);
  const metaItems = userInputRequestMetaItems(request);
  const eventLabel =
    card.call.event_type === card.result.event_type
      ? card.call.event_type
      : `${card.call.event_type}/${card.result.event_type}`;
  const timeLabel =
    card.call.ts === card.result.ts
      ? formatTime(card.call.ts)
      : `${formatTime(card.call.ts)} -> ${formatTime(card.result.ts)}`;

  return (
    <Card
      className={cn(
        SURFACE_CARD_CLASS,
        subagentLabel ? "border-l-4 border-l-[color:var(--accent-strong)]" : "",
      )}
      size="sm"
    >
      <CardContent className={COMPACT_CARD_CONTENT_SPACED_CLASS}>
        <div className={COMPACT_EVENT_HEADER_CLASS}>
          <span className="font-mono">
            #{card.call.seq}, #{card.result.seq}
          </span>
          <time>{timeLabel}</time>
          <span className="font-mono">{eventLabel}</span>
          {subagentLabel ? <Badge variant="outline">{subagentLabel}</Badge> : null}
        </div>
        {metaItems.length > 0 ? <EventMetaRow items={metaItems} /> : null}
        <UserInputRequestBlock request={request} />
      </CardContent>
    </Card>
  );
}

function UserInputRequestBlock({ request }: { request: UserInputRequestEntry }) {
  const extraAnswers = request.extra_answers.filter(
    (answer) => answer.id.trim() && userInputAnswerValues(answer.answers).length > 0,
  );

  return (
    <div className="flex flex-col gap-3">
      {request.questions.length > 0 ? (
        <div className="flex flex-col gap-2">
          <div className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
            questions
          </div>
          <div className="flex flex-col gap-3">
            {request.questions.map((question, index) => (
              <UserInputQuestionBlock
                key={question.id ?? `${question.header ?? "question"}-${index}`}
                question={question}
              />
            ))}
          </div>
        </div>
      ) : null}
      {extraAnswers.length > 0 ? (
        <div className="flex flex-col gap-2">
          <div className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
            answers
          </div>
          <div className="grid gap-2">
            {extraAnswers.map((answer) => (
              <div
                className="grid gap-1 rounded-xl border border-border/60 bg-muted/10 px-3 py-2 sm:grid-cols-[minmax(0,180px)_minmax(0,1fr)] sm:items-start sm:gap-3"
                key={`${answer.id}-${answer.answers.join("|")}`}
              >
                <code className="ui-selectable whitespace-pre-wrap break-words text-[11px] text-muted-foreground">
                  {answer.id}
                </code>
                <UserInputAnswerChips answers={answer.answers} selected={false} />
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}

function UserInputQuestionBlock({ question }: { question: UserInputQuestionEntry }) {
  const unmatchedAnswers = userInputAnswerValues(question.answers).filter(
    (answer) =>
      !question.options.some((option) => option.label.trim() === answer.trim()),
  );

  return (
    <div className="flex flex-col gap-3 rounded-2xl border border-border/60 bg-muted/10 p-3">
      <div className="flex flex-wrap items-start gap-3">
        <div className="min-w-0 flex-1">
          {question.question?.trim() ? <CardText text={question.question.trim()} tone="default" /> : null}
        </div>
        <div className="ml-auto flex flex-wrap items-center justify-end gap-1.5">
          {question.header?.trim() ? (
            <span className="inline-flex items-center rounded-full border border-border/60 bg-background/60 px-2 py-0.5 text-[10px] uppercase tracking-[0.08em] text-muted-foreground">
              {question.header.trim()}
            </span>
          ) : null}
          {question.id?.trim() ? (
            <span className="inline-flex items-center rounded-full border border-border/60 bg-background/60 px-2 py-0.5 text-[10px] text-muted-foreground">
              <code className="ui-selectable">{question.id.trim()}</code>
            </span>
          ) : null}
        </div>
      </div>
      {question.options.length > 0 ? (
        <div className="flex flex-col gap-2">
          {question.options.map((option, index) => {
            const isSelected = question.answers.some(
              (answer) => answer.trim() === option.label.trim(),
            );
            return (
              <UserInputOptionBlock
                isSelected={isSelected}
                key={`${option.label}-${index}`}
                option={option}
              />
            );
          })}
        </div>
      ) : null}
      {unmatchedAnswers.length > 0 ? (
        <div className="flex flex-col gap-1">
          <div className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
            answers
          </div>
          <UserInputAnswerChips answers={unmatchedAnswers} selected />
        </div>
      ) : null}
    </div>
  );
}

function UserInputOptionBlock({
  option,
  isSelected,
}: {
  option: UserInputOptionEntry;
  isSelected: boolean;
}) {
  return (
    <div
      className={cn(
        "flex flex-col gap-2 rounded-xl border px-3 py-2",
        isSelected
          ? "border-emerald-500/40 bg-emerald-500/10"
          : "border-border/60 bg-background/40",
      )}
    >
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="ui-selectable whitespace-pre-wrap break-words text-sm font-medium text-foreground">
          {option.label}
        </div>
        {isSelected ? (
          <span className="inline-flex items-center rounded-full border border-emerald-500/35 bg-emerald-500/12 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.08em] text-emerald-700 dark:text-emerald-300">
            selected
          </span>
        ) : null}
      </div>
      {option.description?.trim() ? <CardText text={option.description.trim()} tone="muted" /> : null}
    </div>
  );
}

function UserInputAnswerChips({
  answers,
  selected,
}: {
  answers: string[];
  selected: boolean;
}) {
  const values = userInputAnswerValues(answers);
  if (values.length === 0) {
    return null;
  }

  return (
    <div className="flex flex-wrap gap-1.5">
      {values.map((answer, index) => (
        <span
          className={cn(
            "ui-selectable inline-flex items-center rounded-full border px-2 py-0.5 text-xs",
            selected
              ? "border-emerald-500/35 bg-emerald-500/12 text-emerald-700 dark:text-emerald-300"
              : "border-border/60 bg-background/60 text-muted-foreground",
          )}
          key={`${index}-${answer}`}
        >
          {answer}
        </span>
      ))}
    </div>
  );
}

function ShellBlock({
  command,
  detailData,
  output,
}: {
  command: string | null;
  detailData: {
    parsedHeaders: Array<{ kind: string; typeLabel: string; present: string | null }>;
    items: Array<{ label: string; value: string }>;
  };
  output: string | null;
}) {
  const [expanded, setExpanded] = useState(false);
  const hasOutput = Boolean(output?.trim());
  const hasDetails = detailData.items.length > 0;
  const hasExpandable = hasOutput || hasDetails;
  const hasParsedHeaders = detailData.parsedHeaders.length > 0;
  const showCommandInline = !hasExpandable || !hasParsedHeaders;
  const showCommandInExpandedBlock = Boolean(command?.trim()) && hasExpandable && hasParsedHeaders;
  const toggleLabel = shellExpandToggleLabel(expanded, hasDetails, hasOutput);

  return (
    <div className="min-w-0 w-full flex flex-col gap-2">
      <div className="grid min-w-0 w-full grid-cols-[minmax(0,1fr)_auto] items-start gap-3">
        <div className="min-w-0 flex flex-1 flex-col gap-1">
          {detailData.parsedHeaders.length > 0 ? (
            <div className="flex min-w-0 flex-col gap-1">
              {detailData.parsedHeaders.map((header, index) => (
                <code
                  className="ui-selectable block max-w-full whitespace-pre-wrap break-words text-sm font-medium text-foreground"
                  key={`${header.kind}-${header.typeLabel}-${header.present ?? ""}-${index}`}
                >
                  <span className={shellParsedCommandPalette(header.kind)}>
                    {header.typeLabel}
                  </span>
                  {header.present ? ` ${header.present}` : ""}
                </code>
              ))}
            </div>
          ) : null}
          {showCommandInline && command ? (
            <code className="ui-selectable block max-w-full whitespace-pre-wrap break-words text-sm font-medium text-foreground">
              {`$ ${command}`}
            </code>
          ) : null}
          {showCommandInline && !command ? (
            <span className="text-sm text-muted-foreground">command unavailable</span>
          ) : null}
        </div>
        {toggleLabel ? (
          <button
            className="inline-flex shrink-0 items-center justify-self-end text-xs font-medium text-[color:var(--accent-strong)] transition-opacity hover:opacity-80"
            onClick={() => setExpanded((current) => !current)}
            type="button"
          >
            {toggleLabel}
          </button>
        ) : null}
      </div>
      {expanded && hasExpandable ? (
        <div className="grid gap-2 border-l border-border/60 pl-3">
          {showCommandInExpandedBlock ? (
            <code className="ui-selectable block whitespace-pre-wrap break-words text-sm font-medium text-foreground">
              {`$ ${command}`}
            </code>
          ) : null}
          {detailData.items.map((item) => (
            <div
              className="grid gap-1 sm:grid-cols-[minmax(0,132px)_minmax(0,1fr)] sm:items-start sm:gap-3"
              key={`${item.label}-${item.value}`}
            >
              <div className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
                {item.label}
              </div>
              <div className="ui-selectable whitespace-pre-wrap break-words text-xs text-foreground">
                {item.value}
              </div>
            </div>
          ))}
          {hasOutput ? (
            <code className="ui-selectable block whitespace-pre-wrap break-words text-xs text-foreground">
              {output}
            </code>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function shellExpandToggleLabel(
  expanded: boolean,
  hasDetails: boolean,
  hasOutput: boolean,
) {
  if (!hasDetails && !hasOutput) {
    return null;
  }

  if (hasDetails && hasOutput) {
    return expanded ? "Скрыть детали и вывод" : "Показать детали и вывод";
  }
  if (hasDetails) {
    return expanded ? "Скрыть детали" : "Показать детали";
  }
  return expanded ? "Скрыть вывод" : "Показать вывод";
}

function shellParsedCommandPalette(kind: string) {
  switch (kind.toLowerCase()) {
    case "read":
      return "text-sky-700 dark:text-sky-300";
    case "search":
      return "text-amber-700 dark:text-amber-300";
    case "list_files":
      return "text-emerald-700 dark:text-emerald-300";
    case "write":
      return "text-rose-700 dark:text-rose-300";
    default:
      return "text-cyan-700 dark:text-cyan-300";
  }
}

function CardText({
  text,
  tone,
}: {
  text: string;
  tone: "default" | "muted";
}) {
  const [expanded, setExpanded] = useState(false);
  const collapsible = shouldCollapseText(text);
  const displayedText = collapsible && !expanded ? truncatePreviewText(text) : text;

  return (
    <div className="flex flex-col items-start gap-1">
      <p
        className={cn(
          "ui-selectable whitespace-pre-wrap break-words",
          tone === "default" ? "text-sm leading-5" : "text-xs leading-4 text-muted-foreground",
        )}
      >
        {displayedText}
      </p>
      {collapsible ? (
        <button
          className="inline-flex items-center text-[11px] font-semibold uppercase tracking-[0.08em] text-[color:var(--accent-strong)] underline-offset-4 transition-opacity hover:underline hover:opacity-80"
          onClick={() => setExpanded((current) => !current)}
          type="button"
        >
          {expanded ? "Скрыть" : "Показать полностью"}
        </button>
      ) : null}
    </div>
  );
}

function PatchApplyBlock({ data }: { data: PatchApplyRenderData }) {
  return (
    <div className="flex flex-col gap-2">
      {data.changes.length > 0 ? (
        <div className="grid gap-1.5">
          {data.changes.map((change) => (
            <div className="flex flex-wrap items-start gap-x-2 gap-y-1 text-xs" key={`${change.path}-${change.change_type ?? ""}`}>
              {change.change_type ? (
                <span className="shrink-0 text-muted-foreground">
                  {patchChangeTypeLabel(change.change_type)}
                </span>
              ) : null}
              <code className="ui-selectable whitespace-pre-wrap break-words text-foreground">
                {patchChangeDisplayPath(change)}
              </code>
            </div>
          ))}
        </div>
      ) : null}
      {data.diffSections.length > 0 || data.fallbackDiffText ? (
        <PatchDiffBlock fallbackText={data.fallbackDiffText} sections={data.diffSections} />
      ) : null}
      {data.output && data.changes.length === 0 ? (
        <div className="flex flex-col gap-1">
          <div className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
            output
          </div>
          <CardText text={data.output} tone="muted" />
        </div>
      ) : null}
    </div>
  );
}

function PatchDiffBlock({
  sections,
  fallbackText,
}: {
  sections: PatchApplyDiffSection[];
  fallbackText: string | null;
}) {
  const [visible, setVisible] = useState(false);
  const hasDiff = sections.length > 0 || Boolean(fallbackText?.trim());

  if (!hasDiff) {
    return null;
  }

  return (
    <div className="flex flex-col items-start gap-1">
      <button
        className="inline-flex items-center text-[11px] font-semibold uppercase tracking-[0.08em] text-[color:var(--accent-strong)] underline-offset-4 transition-opacity hover:underline hover:opacity-80"
        onClick={() => setVisible((current) => !current)}
        type="button"
      >
        {visible ? "Скрыть diff" : "Показать diff"}
      </button>
      {visible ? (
        <div className="grid w-full gap-3">
          {sections.length > 0 ? (
            sections.map((section) => (
              <div className="grid gap-1" key={section.key}>
                <div className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
                  {section.label}
                </div>
                <PatchDiffLines text={section.diffText} />
              </div>
            ))
          ) : fallbackText ? (
            <PatchDiffLines text={fallbackText} />
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function PatchDiffLines({ text }: { text: string }) {
  const lines = text.split("\n");

  return (
    <div className="w-full overflow-hidden">
      <div className="grid gap-px">
        {lines.map((line, index) => (
          <code
            className={cn(
              "ui-selectable block whitespace-pre-wrap break-words px-2 py-0.5 text-xs leading-5",
              patchDiffLineClassName(line),
            )}
            key={`${index}-${line}`}
          >
            {line || " "}
          </code>
        ))}
      </div>
    </div>
  );
}

function RuntimeContextBlock({
  data,
}: {
  data: {
    summaryItems: Array<{ label: string; value: string }>;
    detailItems: Array<{ label: string; value: string; isLongText?: boolean }>;
  };
}) {
  return (
    <details className="group/runtime">
      <summary className="flex cursor-pointer list-none items-start justify-between gap-3 text-xs [&::-webkit-details-marker]:hidden">
        <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-[11px] text-muted-foreground">
          {data.summaryItems.length > 0 ? (
            data.summaryItems.map((item) => (
              <span className="inline-flex items-center gap-1" key={`${item.label}-${item.value}`}>
                <span>{item.label}</span>
                <span className="ui-selectable font-semibold text-foreground">{item.value}</span>
              </span>
            ))
          ) : (
            <span className="text-xs text-muted-foreground">runtime context</span>
          )}
        </div>
        <span className="shrink-0 text-[11px] font-semibold uppercase tracking-[0.08em] text-[color:var(--accent-strong)]">
          <span className="group-open/runtime:hidden">details</span>
          <span className="hidden group-open/runtime:inline">hide</span>
        </span>
      </summary>
      {data.detailItems.length > 0 ? (
        <div className="mt-2 grid gap-2 border-l border-border/60 pl-3">
          {data.detailItems.map((item) => (
            <div
              className="grid gap-1 sm:grid-cols-[minmax(0,180px)_minmax(0,1fr)] sm:items-start sm:gap-3"
              key={`${item.label}-${item.value}`}
            >
              <div className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
                {item.label}
              </div>
              {item.isLongText ? (
                <CardText text={item.value} tone="muted" />
              ) : (
                <div className="ui-selectable whitespace-pre-wrap break-words text-xs text-foreground">
                  {item.value}
                </div>
              )}
            </div>
          ))}
        </div>
      ) : null}
    </details>
  );
}

function buildMergedCard(node: EventNode): MergedCard {
  const shellResultIndex = pairedCommandShellResultChildIndex(node);
  if (shellResultIndex != null) {
    const result = node.children[shellResultIndex];
    if ("Event" in result) {
      return {
        kind: "shell",
        call: node.event,
        result: result.Event.event,
        children: mergedShellOperationChildren(node, shellResultIndex),
      };
    }
  }

  const patchApplyResultIndex = pairedPatchApplyResultChildIndex(node);
  if (patchApplyResultIndex != null) {
    const result = node.children[patchApplyResultIndex];
    if ("Event" in result) {
      return {
        kind: "patch-apply",
        call: node.event,
        result: result.Event.event,
        children: mergedPatchApplyOperationChildren(node, patchApplyResultIndex),
      };
    }
  }

  const planUpdateResultIndex = pairedPlanUpdateResultChildIndex(node);
  if (planUpdateResultIndex != null) {
    const result = node.children[planUpdateResultIndex];
    if ("Event" in result) {
      return {
        kind: "plan-update",
        call: node.event,
        result: result.Event.event,
        children: mergedPlanUpdateOperationChildren(node, planUpdateResultIndex),
      };
    }
  }

  const spawnResultIndex = pairedSpawnAgentResultChildIndex(node);
  if (spawnResultIndex != null) {
    const result = node.children[spawnResultIndex];
    if ("Event" in result) {
      return {
        kind: "spawn",
        call: node.event,
        result: result.Event.event,
        children: mergedSpawnAgentOperationChildren(node, spawnResultIndex),
      };
    }
  }

  const userInputResultIndex = pairedUserInputRequestResultChildIndex(node);
  if (userInputResultIndex != null) {
    const result = node.children[userInputResultIndex];
    if ("Event" in result) {
      return {
        kind: "user-input",
        call: node.event,
        result: result.Event.event,
        children: mergedUserInputRequestOperationChildren(node, userInputResultIndex),
      };
    }
  }

  const collabResultIndex = pairedCollabOperationResultChildIndex(node);
  if (collabResultIndex != null) {
    const result = node.children[collabResultIndex];
    if ("Event" in result) {
      return {
        kind: "collab",
        call: node.event,
        result: result.Event.event,
        children: mergedCollabOperationChildren(node, collabResultIndex),
      };
    }
  }

  return {
    kind: "single",
    event: node.event,
    children: node.children,
  };
}

function mergedCardEventIds(card: MergedCard) {
  if (card.kind === "single") {
    return [card.event.event_id];
  }

  return uniqueEventIds([card.call.event_id, card.result.event_id]);
}

function mergedCardThreadId(card: MergedCard) {
  if (card.kind === "single") {
    return agentSelectionThreadIdForEvent(card.event);
  }

  const callThreadId = agentSelectionThreadIdForEvent(card.call);
  const resultThreadId = agentSelectionThreadIdForEvent(card.result);
  if (callThreadId && resultThreadId && callThreadId !== resultThreadId) {
    return null;
  }
  return callThreadId ?? resultThreadId ?? null;
}

function eventSubagentLabel(
  event: EventEntry,
  threadMetaById: Record<string, ThreadPresentationMeta>,
) {
  if (event.actor_type !== "subagent") {
    return null;
  }

  return threadDisplayLabel(event.thread_id, threadMetaById)
    ?? normalizeDisplayText(event.subagent_nickname)
    ?? normalizeDisplayText(event.thread_id)
    ?? null;
}

function eventHeaderLabel(
  event: EventEntry,
  threadMetaById: Record<string, ThreadPresentationMeta>,
) {
  if (event.event_type !== COLLAB_SPAWN_AGENT) {
    return event.event_type;
  }

  const sourceLabel = threadDisplayLabel(event.thread_id, threadMetaById) ?? "main";
  const targetLabel = collabTargetLabels(event, threadMetaById)[0] ?? "unknown";
  return `spawn ${sourceLabel} -> ${targetLabel}`;
}

function mergedEventHeaderLabel(
  call: EventEntry,
  result: EventEntry,
  threadMetaById: Record<string, ThreadPresentationMeta>,
) {
  const callLabel = eventHeaderLabel(call, threadMetaById);
  const resultLabel = eventHeaderLabel(result, threadMetaById);
  return callLabel === resultLabel ? callLabel : `${callLabel}/${resultLabel}`;
}

function genericEventMetaItems(
  event: EventEntry,
  threadMetaById: Record<string, ThreadPresentationMeta>,
) {
  const items: Array<{ label: string; value: string }> = [];
  const targetLabels = collabTargetLabels(event, threadMetaById);

  if (event.event_type === COLLAB_SPAWN_AGENT) {
    if (targetLabels[0]) {
      items.push({ label: "agent", value: targetLabels[0] });
    }
    const requestedType = normalizeDisplayText(event.spawn_agent?.requested_agent_type);
    if (requestedType) {
      items.push({ label: "agent type", value: requestedType });
    }
    const model = normalizeDisplayText(event.spawn_agent?.model);
    if (model) {
      items.push({ label: "model", value: model });
    }
    const effort = normalizeDisplayText(event.spawn_agent?.reasoning_effort);
    if (effort) {
      items.push({ label: "effort", value: effort });
    }
    return items;
  }

  if (isPairableCollabOperationEvent(event)) {
    items.push({
      label: targetLabels.length > 1 ? "agents" : "agent",
      value: targetLabels.length > 0 ? targetLabels.join(", ") : "unknown/unbound",
    });
  }

  return items;
}

function mergedGenericEventMetaItems(
  call: EventEntry,
  result: EventEntry,
  threadMetaById: Record<string, ThreadPresentationMeta>,
) {
  const items = [...genericEventMetaItems(call, threadMetaById), ...genericEventMetaItems(result, threadMetaById)];
  return items.filter(
    (item, index) =>
      items.findIndex(
        (candidate) => candidate.label === item.label && candidate.value === item.value,
      ) === index,
  );
}

function collabTargetLabels(
  event: EventEntry,
  threadMetaById: Record<string, ThreadPresentationMeta>,
) {
  const labels = collabTargetThreadIds(event).map((threadId) => threadDisplayLabel(threadId, threadMetaById) ?? threadId);
  return labels.filter(
    (label, index) => labels.findIndex((candidate) => candidate === label) === index,
  );
}

function collabTargetThreadIds(event: EventEntry) {
  if (event.event_type === COLLAB_SPAWN_AGENT) {
    return uniqueEventIds([normalizeDisplayText(event.spawn_agent?.receiver_thread_id)]);
  }

  return uniqueEventIds(event.receiver_thread_ids.map((threadId) => normalizeDisplayText(threadId)));
}

function threadDisplayLabel(
  threadId: string | null | undefined,
  threadMetaById: Record<string, ThreadPresentationMeta>,
) {
  const normalizedThreadId = normalizeDisplayText(threadId);
  if (!normalizedThreadId) {
    return null;
  }

  const threadMeta = threadMetaById[normalizedThreadId];
  if (!threadMeta) {
    return normalizedThreadId;
  }

  return formatAgentDisplayLabel(
    threadMeta.nickname,
    threadMeta.role,
    threadMeta.threadId,
    threadMeta.isRoot,
  );
}

function eventTerminalBadge(event: EventEntry) {
  if (event.actor_type !== "subagent") {
    return null;
  }

  switch (event.event_type) {
    case TASK_COMPLETED:
      return {
        className:
          "inline-flex items-center rounded-full border border-emerald-500/35 bg-emerald-500/12 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.08em] text-emerald-700 dark:text-emerald-300",
        label: "completed",
      };
    case "agent.failed":
      return {
        className:
          "inline-flex items-center rounded-full border border-rose-500/35 bg-rose-500/12 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.08em] text-rose-700 dark:text-rose-300",
        label: "failed",
      };
    case "agent.aborted":
      return {
        className:
          "inline-flex items-center rounded-full border border-amber-500/35 bg-amber-500/12 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.08em] text-amber-700 dark:text-amber-300",
        label: "aborted",
      };
    default:
      return null;
  }
}

function uniqueEventIds(values: Array<string | null | undefined>) {
  return values.filter(
    (value, index): value is string =>
      Boolean(value)
      && values.findIndex((candidate) => candidate === value) === index,
  );
}

function normalizeDisplayText(value: string | null | undefined) {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

function formatTime(value: string | null) {
  if (!value) {
    return "n/a";
  }

  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }

  return new Intl.DateTimeFormat("ru-RU", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(date);
}

function shouldSkipEventSummary(event: EventEntry) {
  if (event.event_type === RUNTIME_CONTEXT || event.event_type === SHELL_RESULT) {
    return true;
  }

  if (isTaskStartedEvent(event)) {
    return true;
  }

  return false;
}

function normalizeSummary(summary: string, eventType: string) {
  const trimmed = summary.trim();
  if (
    !trimmed
    || trimmed === eventType
    || eventType === RUNTIME_CONTEXT
    || eventType === SHELL_RESULT
  ) {
    return null;
  }

  return trimmed;
}

function eventSummaryText(event: EventEntry) {
  if (shouldSkipEventSummary(event)) {
    return null;
  }

  return normalizeSummary(event.summary, event.event_type);
}

function mergedSummaryText(call: EventEntry, result: EventEntry) {
  return normalizeSummary(result.summary, result.event_type)
    ?? normalizeSummary(call.summary, call.event_type)
    ?? null;
}

function singleDetailText(event: EventEntry) {
  if (event.event_type === RUNTIME_CONTEXT) {
    return null;
  }

  if (isTaskCompletedEvent(event) && taskCompletedMessage(event)) {
    return null;
  }

  if (event.event_type === SHELL_CALL && isCommandShellEvent(event)) {
    return event.shell_command ? `$ ${event.shell_command}` : null;
  }

  if (event.event_type === COLLAB_SPAWN_AGENT) {
    const prompt = event.spawn_agent?.prompt?.trim();
    return prompt ? prompt : null;
  }

  if (event.event_type === USER_INPUT_REQUEST) {
    return userInputRequestCountsText(event.user_input_request);
  }

  if (isPairableCollabOperationEvent(event)) {
    return collabOperationStates(event)[0]?.text ?? null;
  }

  return null;
}

function taskEventMetaItems(event: EventEntry) {
  const items: Array<{ label: string; value: string }> = [];

  if (isTaskStartedEvent(event)) {
    const mode = event.collaboration_mode_kind?.trim();
    if (mode) {
      items.push({ label: "mode", value: mode });
    }
    if (event.turn_id?.trim()) {
      items.push({ label: "turn", value: event.turn_id.trim() });
    }
    if (event.model_context_window?.trim()) {
      items.push({ label: "context window", value: event.model_context_window.trim() });
    }
    return items;
  }

  if (isTaskCompletedEvent(event) && event.turn_id?.trim()) {
    items.push({ label: "turn", value: event.turn_id.trim() });
  }

  return items;
}

function taskCompletedMessage(event: EventEntry) {
  if (!isTaskCompletedEvent(event)) {
    return null;
  }

  const message = event.last_agent_message?.trim();
  return message ? message : null;
}

function taskEventModeBadge(event: EventEntry) {
  if (!isTaskStartedEvent(event)) {
    return null;
  }

  const label = event.collaboration_mode_kind?.trim();
  if (!label) {
    return null;
  }

  return {
    label,
    palette: taskModePalette(label),
  };
}

function taskLifecycleMarker(
  event: EventEntry,
  inheritedPalette: TaskTimelinePalette | null,
) {
  if (!isTaskStartedEvent(event) && !isTaskCompletedEvent(event)) {
    return null;
  }

  const palette = inheritedPalette ?? taskModePalette(event.collaboration_mode_kind);
  return {
    accent: palette.accent,
    completed: isTaskCompletedEvent(event),
  };
}

function mergedDetailText(card: Exclude<MergedCard, { kind: "single" }>) {
  if (card.kind === "spawn") {
    const prompt = card.call.spawn_agent?.prompt ?? card.result.spawn_agent?.prompt;
    return prompt?.trim() ? prompt.trim() : null;
  }

  if (card.kind === "user-input") {
    return userInputRequestCountsText(
      mergedUserInputRequestEntry(card.call, card.result),
    );
  }

  const firstState = collabOperationStates(card.result)[0]?.text;
  return firstState ? firstState : null;
}

function singlePlanUpdateMetaItems(event: EventEntry, data: PlanUpdateRenderData) {
  const items: Array<{ label: string; value: string }> = [];
  const phase = event.phase?.trim();
  if (phase) {
    items.push({ label: "phase", value: phase });
  }
  if (data.steps.length > 0) {
    items.push({ label: "steps", value: String(data.steps.length) });
  }
  return items;
}

function mergedPlanUpdateMetaItems(
  call: EventEntry,
  result: EventEntry,
  data: PlanUpdateRenderData,
) {
  const items: Array<{ label: string; value: string }> = [];
  const phase = mergedPlanUpdatePhase(call, result);
  if (phase) {
    items.push({ label: "phase", value: phase });
  }
  if (data.steps.length > 0) {
    items.push({ label: "steps", value: String(data.steps.length) });
  }
  return items;
}

function mergedPlanUpdatePhase(call: EventEntry, result: EventEntry) {
  const callPhase = call.phase?.trim();
  const resultPhase = result.phase?.trim();
  if (callPhase && resultPhase) {
    return callPhase === resultPhase ? callPhase : `${callPhase} -> ${resultPhase}`;
  }

  return callPhase ?? resultPhase ?? null;
}

function userInputRequestCountsText(request: UserInputRequestEntry | null | undefined) {
  if (!request) {
    return null;
  }

  return `questions: ${request.questions.length} · answers: ${totalUserInputRequestAnswerCount(request)}`;
}

function pairedCommandShellResultChildIndex(node: EventNode) {
  if (node.event.event_type !== SHELL_CALL || !isCommandShellEvent(node.event)) {
    return null;
  }

  const snapshotIndex = preferredTerminalChildIndexFromSnapshot(node);
  if (snapshotIndex != null) {
    return snapshotIndex;
  }

  return preferredChildIndex(
    node.children,
    (item) =>
      "Event" in item
      && item.Event.event.event_type === SHELL_RESULT
      && isCommandShellEvent(item.Event.event)
      && shellOperationIdsMatch(node.event, item.Event.event),
    (item) => shellResultPreference(item.Event.event),
  );
}

function pairedPatchApplyResultChildIndex(node: EventNode) {
  if (node.event.event_type !== PATCH_APPLY || node.event.phase !== "started") {
    return null;
  }

  const snapshotIndex = preferredTerminalChildIndexFromSnapshot(node);
  if (snapshotIndex != null) {
    return snapshotIndex;
  }

  return preferredChildIndex(
    node.children,
    (item) =>
      "Event" in item
      && item.Event.event.event_type === PATCH_APPLY
      && item.Event.event.phase === "completed"
      && shellOperationIdsMatch(node.event, item.Event.event),
    (item) => patchApplyResultPreference(item.Event.event),
  );
}

function mergedPatchApplyOperationChildren(node: EventNode, resultIndex: number) {
  const preferredResult = eventChildAt(node.children, resultIndex)?.event;
  if (!preferredResult) {
    return node.children;
  }

  const children: TimelineItem[] = [];
  node.children.forEach((item, index) => {
    if (index === resultIndex) {
      if ("Event" in item) {
        children.push(...item.Event.children);
      }
      return;
    }

    if (isRedundantPatchApplyDuplicate(item, node.event)) {
      return;
    }

    children.push(item);
  });

  return children;
}

function patchApplyResultPreference(event: EventEntry) {
  return [
    boolScore(event.patch_apply_changes.length > 0),
    boolScore(Boolean(event.patch_apply_status)),
    boolScore(Boolean(event.aggregated_output)),
    event.seq,
  ];
}

function pairedPlanUpdateResultChildIndex(node: EventNode) {
  if (!isPlanTodoEventEntry(node.event) || node.event.phase !== "started") {
    return null;
  }

  const snapshotIndex = preferredTerminalChildIndexFromSnapshot(node);
  if (snapshotIndex != null) {
    return snapshotIndex;
  }

  return preferredChildIndex(
    node.children,
    (item) =>
      "Event" in item
      && isPlanTodoEventEntry(item.Event.event)
      && item.Event.event.phase === "completed"
      && shellOperationIdsMatch(node.event, item.Event.event),
    (item) => planUpdateResultPreference(item.Event.event),
  );
}

function mergedPlanUpdateOperationChildren(node: EventNode, resultIndex: number) {
  const preferredResult = eventChildAt(node.children, resultIndex)?.event;
  if (!preferredResult) {
    return node.children;
  }

  const children: TimelineItem[] = [];
  node.children.forEach((item, index) => {
    if (index === resultIndex) {
      if ("Event" in item) {
        children.push(...item.Event.children);
      }
      return;
    }

    if (isRedundantResponseItemPlanUpdateResult(item, node.event, preferredResult)) {
      return;
    }

    children.push(item);
  });

  return children;
}

function planUpdateResultPreference(event: EventEntry) {
  return [
    event.plan_steps.length,
    boolScore(Boolean(event.plan_explanation?.trim())),
    boolScore(event.raw_type === "response_item"),
    boolScore(event.output_value != null),
    event.seq,
  ];
}

function mergedShellOperationChildren(node: EventNode, shellResultIndex: number) {
  const preferredResult = eventChildAt(node.children, shellResultIndex)?.event;
  if (!preferredResult) {
    return node.children;
  }

  const children: TimelineItem[] = [];
  node.children.forEach((item, index) => {
    if (index === shellResultIndex) {
      if ("Event" in item) {
        children.push(...item.Event.children);
      }
      return;
    }

    if (isRedundantResponseItemShellResult(item, node.event, preferredResult)) {
      if ("Event" in item) {
        children.push(...item.Event.children);
      }
      return;
    }

    children.push(item);
  });
  return children;
}

function shellResultPreference(event: EventEntry) {
  return [
    boolScore(event.duplicate_of === RESPONSE_ITEM_FUNCTION_CALL_OUTPUT),
    boolScore(Boolean(event.shell_command)),
    boolScore(Boolean(event.aggregated_output)),
    boolScore(event.shell_exit_code != null),
    event.seq,
  ];
}

function pairedSpawnAgentResultChildIndex(node: EventNode) {
  if (node.event.event_type !== COLLAB_SPAWN_AGENT || node.event.phase !== "started") {
    return null;
  }

  const snapshotIndex = preferredTerminalChildIndexFromSnapshot(node);
  if (snapshotIndex != null) {
    return snapshotIndex;
  }

  return preferredChildIndex(
    node.children,
    (item) =>
      "Event" in item
      && item.Event.event.event_type === COLLAB_SPAWN_AGENT
      && item.Event.event.phase === "completed"
      && shellOperationIdsMatch(node.event, item.Event.event),
    (item) => spawnAgentResultPreference(item.Event.event),
  );
}

function mergedSpawnAgentOperationChildren(node: EventNode, spawnResultIndex: number) {
  const preferredResult = eventChildAt(node.children, spawnResultIndex)?.event;
  if (!preferredResult) {
    return node.children;
  }

  const children: TimelineItem[] = [];
  node.children.forEach((item, index) => {
    if (index === spawnResultIndex) {
      if ("Event" in item) {
        children.push(...item.Event.children);
      }
      return;
    }

    if (isRedundantResponseItemSpawnResult(item, node.event, preferredResult)) {
      return;
    }

    children.push(item);
  });
  return children;
}

function spawnAgentResultPreference(event: EventEntry) {
  const spawn = event.spawn_agent;
  return [
    boolScore(event.duplicate_of === RESPONSE_ITEM_FUNCTION_CALL_OUTPUT),
    boolScore(Boolean(spawn?.receiver_thread_id)),
    boolScore(Boolean(spawn?.receiver_nickname)),
    boolScore(Boolean(spawn?.receiver_role)),
    boolScore(Boolean(spawn?.model)),
    boolScore(Boolean(spawn?.reasoning_effort)),
    boolScore(Boolean(spawn?.receiver_status)),
    event.seq,
  ];
}

function pairedUserInputRequestResultChildIndex(node: EventNode) {
  if (node.event.event_type !== USER_INPUT_REQUEST || node.event.phase !== "started") {
    return null;
  }

  const snapshotIndex = preferredTerminalChildIndexFromSnapshot(node);
  if (snapshotIndex != null) {
    return snapshotIndex;
  }

  return preferredChildIndex(
    node.children,
    (item) =>
      "Event" in item
      && item.Event.event.event_type === USER_INPUT_REQUEST
      && item.Event.event.phase === "completed"
      && shellOperationIdsMatch(node.event, item.Event.event),
    (item) => userInputRequestResultPreference(item.Event.event),
  );
}

function mergedUserInputRequestOperationChildren(node: EventNode, resultIndex: number) {
  const preferredResult = eventChildAt(node.children, resultIndex)?.event;
  if (!preferredResult) {
    return node.children;
  }

  const children: TimelineItem[] = [];
  node.children.forEach((item, index) => {
    if (index === resultIndex) {
      if ("Event" in item) {
        children.push(...item.Event.children);
      }
      return;
    }

    if (isRedundantResponseItemUserInputRequestResult(item, node.event, preferredResult)) {
      return;
    }

    children.push(item);
  });
  return children;
}

function userInputRequestResultPreference(event: EventEntry) {
  return [
    boolScore(event.duplicate_of === RESPONSE_ITEM_FUNCTION_CALL_OUTPUT),
    totalUserInputRequestAnswerCount(event.user_input_request),
    event.user_input_request?.questions.length ?? 0,
    event.seq,
  ];
}

function pairedCollabOperationResultChildIndex(node: EventNode) {
  if (!isPairableCollabOperationEvent(node.event) || node.event.phase !== "started") {
    return null;
  }

  const snapshotIndex = preferredTerminalChildIndexFromSnapshot(node);
  if (snapshotIndex != null) {
    return snapshotIndex;
  }

  return preferredChildIndex(
    node.children,
    (item) =>
      "Event" in item
      && item.Event.event.event_type === node.event.event_type
      && item.Event.event.phase === "completed"
      && shellOperationIdsMatch(node.event, item.Event.event),
    (item) => collabOperationResultPreference(item.Event.event),
  );
}

function mergedCollabOperationChildren(node: EventNode, resultIndex: number) {
  const preferredResult = eventChildAt(node.children, resultIndex)?.event;
  if (!preferredResult) {
    return node.children;
  }

  const children: TimelineItem[] = [];
  node.children.forEach((item, index) => {
    if (index === resultIndex) {
      if ("Event" in item) {
        children.push(...item.Event.children);
      }
      return;
    }

    if (isRedundantResponseItemCollabResult(item, node.event, preferredResult)) {
      return;
    }

    children.push(item);
  });
  return children;
}

function collabOperationResultPreference(event: EventEntry) {
  return [
    boolScore(event.duplicate_of === RESPONSE_ITEM_FUNCTION_CALL_OUTPUT),
    collabOperationStates(event).length,
    event.seq,
  ];
}

function isPairableCollabOperationEvent(event: EventEntry) {
  return [
    COLLAB_SEND_INPUT,
    COLLAB_WAIT,
    COLLAB_CLOSE_AGENT,
    COLLAB_RESUME_AGENT,
  ].includes(event.event_type);
}

function isRedundantResponseItemSpawnResult(
  item: TimelineItem,
  call: EventEntry,
  preferredResult: EventEntry,
) {
  if (!("Event" in item)) {
    return false;
  }

  const event = item.Event.event;
  return (
    event.event_id !== preferredResult.event_id
    && event.event_type === COLLAB_SPAWN_AGENT
    && event.phase === "completed"
    && event.raw_type === "response_item"
    && event.duplicate_of == null
    && shellOperationIdsMatch(call, event)
    && preferredResult.raw_type !== "response_item"
  );
}

function isRedundantPatchApplyDuplicate(item: TimelineItem, call: EventEntry) {
  if (!("Event" in item)) {
    return false;
  }

  const event = item.Event.event;
  return (
    event.event_type === PATCH_APPLY_DUPLICATE
    && shellOperationIdsMatch(call, event)
  );
}

function isRedundantResponseItemShellResult(
  item: TimelineItem,
  call: EventEntry,
  preferredResult: EventEntry,
) {
  if (!("Event" in item)) {
    return false;
  }

  const event = item.Event.event;
  return (
    event.event_id !== preferredResult.event_id
    && event.event_type === SHELL_RESULT
    && event.raw_type === "response_item"
    && event.duplicate_of == null
    && isCommandShellEvent(event)
    && shellOperationIdsMatch(call, event)
    && preferredResult.raw_type !== "response_item"
  );
}

function isRedundantResponseItemUserInputRequestResult(
  item: TimelineItem,
  call: EventEntry,
  preferredResult: EventEntry,
) {
  if (!("Event" in item)) {
    return false;
  }

  const event = item.Event.event;
  return (
    event.event_id !== preferredResult.event_id
    && event.event_type === USER_INPUT_REQUEST
    && event.phase === "completed"
    && event.raw_type === "response_item"
    && event.duplicate_of == null
    && shellOperationIdsMatch(call, event)
    && preferredResult.raw_type !== "response_item"
  );
}

function isRedundantResponseItemPlanUpdateResult(
  item: TimelineItem,
  call: EventEntry,
  preferredResult: EventEntry,
) {
  if (!("Event" in item)) {
    return false;
  }

  const event = item.Event.event;
  return (
    event.event_id !== preferredResult.event_id
    && isPlanTodoEventEntry(event)
    && event.phase === "completed"
    && event.raw_type === "response_item"
    && event.duplicate_of == null
    && shellOperationIdsMatch(call, event)
    && preferredResult.raw_type !== "response_item"
  );
}

function isPlanTodoEventEntry(event: EventEntry) {
  return (
    event.event_type === TODO_UPDATE
    && (
      event.tool_name === "update_plan"
      || Boolean(event.plan_explanation?.trim())
      || event.plan_steps.length > 0
    )
  );
}

function isRedundantResponseItemCollabResult(
  item: TimelineItem,
  call: EventEntry,
  preferredResult: EventEntry,
) {
  if (!("Event" in item)) {
    return false;
  }

  const event = item.Event.event;
  return (
    event.event_id !== preferredResult.event_id
    && isPairableCollabOperationEvent(event)
    && event.phase === "completed"
    && event.raw_type === "response_item"
    && event.duplicate_of == null
    && shellOperationIdsMatch(call, event)
    && preferredResult.raw_type !== "response_item"
  );
}

function isCommandShellEvent(event: EventEntry) {
  return (
    (event.event_type === SHELL_CALL || event.event_type === SHELL_RESULT)
    && ["command_execution", "exec_command"].includes(event.tool_name ?? "")
  );
}

function shellOperationIdsMatch(call: EventEntry, result: EventEntry) {
  if (call.operation_id && result.operation_id) {
    return call.operation_id === result.operation_id;
  }

  return true;
}

function totalUserInputRequestAnswerCount(request: UserInputRequestEntry | null | undefined) {
  if (!request) {
    return 0;
  }

  return (
    request.questions.reduce((total, question) => total + question.answers.length, 0)
    + request.extra_answers.reduce((total, answer) => total + answer.answers.length, 0)
  );
}

function mergedUserInputRequestEntry(call: EventEntry, result: EventEntry) {
  const callRequest = call.user_input_request;
  const resultRequest = result.user_input_request;
  const baseRequest = callRequest ?? resultRequest;
  if (!baseRequest) {
    return null;
  }

  const answersById = new Map<string, string[]>();
  if (callRequest) {
    collectUserInputAnswers(callRequest, answersById);
  }
  if (resultRequest) {
    collectUserInputAnswers(resultRequest, answersById);
  }

  const matchedAnswerIds = new Set<string>();
  const sourceQuestions =
    baseRequest.questions.length > 0
      ? baseRequest.questions
      : resultRequest?.questions ?? [];
  const questions = sourceQuestions.map((question) => {
    const id = question.id?.trim() || null;
    const answers = id ? answersById.get(id) ?? question.answers : question.answers;
    if (id && answersById.has(id)) {
      matchedAnswerIds.add(id);
    }

    return {
      header: question.header,
      id: question.id,
      question: question.question,
      options: question.options.map((option) => ({
        label: option.label,
        description: option.description,
      })),
      answers: [...answers],
    };
  });

  const extra_answers = Array.from(answersById.entries())
    .filter(([id, answers]) => !matchedAnswerIds.has(id) && answers.length > 0)
    .sort(([leftId], [rightId]) => leftId.localeCompare(rightId))
    .map(([id, answers]) => ({
      id,
      answers: [...answers],
    }));

  return {
    questions,
    extra_answers,
  };
}

function collectUserInputAnswers(
  request: UserInputRequestEntry,
  answersById: Map<string, string[]>,
) {
  request.questions.forEach((question) => {
    const id = question.id?.trim();
    if (!id) {
      return;
    }

    mergeUserInputAnswerValues(answersById, id, question.answers);
  });

  request.extra_answers.forEach((answer) => {
    const id = answer.id.trim();
    if (!id) {
      return;
    }

    mergeUserInputAnswerValues(answersById, id, answer.answers);
  });
}

function mergeUserInputAnswerValues(
  answersById: Map<string, string[]>,
  id: string,
  source: string[],
) {
  const target = answersById.get(id) ?? [];
  source.forEach((answer) => {
    if (!answer.trim() || target.includes(answer)) {
      return;
    }

    target.push(answer);
  });
  answersById.set(id, target);
}

function userInputRequestMetaItems(request: UserInputRequestEntry) {
  return [
    { label: "questions", value: String(request.questions.length) },
    { label: "answers", value: String(totalUserInputRequestAnswerCount(request)) },
  ];
}

function userInputRequestHasRenderableContent(request: UserInputRequestEntry) {
  return request.questions.length > 0 || request.extra_answers.length > 0;
}

function userInputAnswerValues(answers: string[]) {
  return answers.filter((answer) => answer.trim());
}

function collabOperationStates(event: EventEntry) {
  const output = event.output_value;
  if (output == null) {
    return [] as CollabOperationStateEntry[];
  }

  const fallbackThreadId = event.receiver_thread_ids[0] ?? null;
  const states: CollabOperationStateEntry[] = [];
  const outputObject = asObject(output);

  if (outputObject) {
    const statuses = asObject(outputObject.statuses);
    if (statuses) {
      Object.entries(statuses).forEach(([threadId, value]) => {
        collectCollabOperationStateEntries(states, threadId, null, value);
      });
    }

    const agentStatuses = asArray(outputObject.agent_statuses);
    if (agentStatuses) {
      agentStatuses.forEach((entry) => {
        const entryObject = asObject(entry);
        if (!entryObject) {
          return;
        }

        const threadId = asString(entryObject.thread_id);
        if ("status" in entryObject) {
          collectCollabOperationStateEntries(states, threadId, null, entryObject.status);
        }
        if ("message" in entryObject) {
          collectCollabOperationStateEntries(states, threadId, "message", entryObject.message);
        }
      });
    }

    if ("status" in outputObject) {
      collectCollabOperationStateEntries(states, fallbackThreadId, null, outputObject.status);
    }
    if ("previous_status" in outputObject) {
      collectCollabOperationStateEntries(
        states,
        fallbackThreadId,
        "previous status",
        outputObject.previous_status,
      );
    }

    const agentStates = asObject(outputObject.agents_states);
    if (agentStates) {
      Object.entries(agentStates).forEach(([threadId, value]) => {
        collectCollabOperationStateEntries(states, threadId, null, value);
      });
    }
  }

  if (states.length === 0) {
    collectCollabOperationStateEntries(states, fallbackThreadId, null, output);
  }

  return states.filter(
    (state, index) =>
      states.findIndex(
        (candidate) =>
          candidate.threadId === state.threadId
          && candidate.label === state.label
          && candidate.text === state.text,
      ) === index,
  );
}

function collectCollabOperationStateEntries(
  out: CollabOperationStateEntry[],
  threadId: string | null,
  label: string | null,
  value: unknown,
) {
  if (value == null) {
    return;
  }

  if (typeof value === "string") {
    const text = value.trim();
    if (!text) {
      return;
    }

    out.push({ threadId, label, text });
    return;
  }

  if (typeof value === "boolean" || typeof value === "number") {
    out.push({ threadId, label, text: String(value) });
    return;
  }

  if (Array.isArray(value)) {
    value.forEach((nested) => {
      collectCollabOperationStateEntries(out, threadId, label, nested);
    });
    return;
  }

  const object = asObject(value);
  if (!object) {
    return;
  }

  Object.entries(object).forEach(([key, nested]) => {
    if (
      [
        "agent_nickname",
        "agent_role",
        "model",
        "reasoning_effort",
        "thread_id",
        "session_path",
      ].includes(key)
    ) {
      return;
    }

    collectCollabOperationStateEntries(out, threadId, key, nested);
  });
}

function preferredChildIndex(
  items: TimelineItem[],
  predicate: (item: TimelineItem) => boolean,
  score: (item: Extract<TimelineItem, { Event: EventNode }>) => number[],
) {
  let bestIndex: number | null = null;
  let bestScore: number[] | null = null;

  items.forEach((item, index) => {
    if (!predicate(item) || !("Event" in item)) {
      return;
    }

    const currentScore = score(item);
    if (!bestScore || compareScores(currentScore, bestScore) > 0) {
      bestIndex = index;
      bestScore = currentScore;
    }
  });

  return bestIndex;
}

function compareScores(left: number[], right: number[]) {
  const maxLength = Math.max(left.length, right.length);
  for (let index = 0; index < maxLength; index += 1) {
    const a = left[index] ?? 0;
    const b = right[index] ?? 0;
    if (a === b) {
      continue;
    }
    return a > b ? 1 : -1;
  }

  return 0;
}

function eventChildAt(items: TimelineItem[], index: number) {
  const item = items[index];
  return item && "Event" in item ? item.Event : null;
}

function boolScore(value: boolean) {
  return value ? 1 : 0;
}

function isStandaloneShellResult(event: EventEntry) {
  return event.event_type === SHELL_RESULT && isCommandShellEvent(event);
}

function shouldHideSingleEvent(event: EventEntry) {
  return event.event_type === PATCH_APPLY_DUPLICATE;
}

function patchApplyRenderData(event: EventEntry) {
  if (![PATCH_APPLY, PATCH_APPLY_DUPLICATE].includes(event.event_type)) {
    return null;
  }

  const patchText = event.patch_apply_input?.trim() ? event.patch_apply_input.trim() : null;
  const output = event.aggregated_output?.trim() ? event.aggregated_output.trim() : null;
  const phase = event.phase?.trim() ? event.phase.trim() : null;
  const status = event.patch_apply_status?.trim() ? event.patch_apply_status.trim() : null;
  const changes = event.patch_apply_changes.length > 0
    ? event.patch_apply_changes
    : patchApplyTargetsFromInput(patchText);
  const diffSections = patchApplyDiffSections(event.patch_apply_changes);
  const fallbackDiffText = diffSections.length === 0 ? patchText : null;

  if (!phase && !status && changes.length === 0 && !fallbackDiffText && !output) {
    return null;
  }

  return {
    phase,
    status,
    output,
    changes,
    diffSections,
    fallbackDiffText,
  };
}

function mergedPatchApplyRenderData(call: EventEntry, result: EventEntry) {
  const patchText = call.patch_apply_input?.trim() ? call.patch_apply_input.trim() : null;
  const output = result.aggregated_output?.trim() ? result.aggregated_output.trim() : null;
  const phase = call.phase?.trim() ? call.phase.trim() : null;
  const status = result.patch_apply_status?.trim()
    ? result.patch_apply_status.trim()
    : result.phase?.trim()
      ? result.phase.trim()
      : null;
  const changes = result.patch_apply_changes.length > 0
    ? result.patch_apply_changes
    : patchApplyTargetsFromInput(patchText);
  const diffSections = patchApplyDiffSections(result.patch_apply_changes);
  const fallbackDiffText = diffSections.length === 0 ? patchText : null;

  return {
    phase,
    status,
    output,
    changes,
    diffSections,
    fallbackDiffText,
  };
}

function patchApplyTargetsFromInput(input: string | null) {
  if (!input) {
    return [] as PatchApplyChangeEntry[];
  }

  const entries: PatchApplyChangeEntry[] = [];
  const seen = new Set<string>();

  input.split("\n").forEach((line) => {
    for (const [prefix, changeType] of PATCH_APPLY_LINE_PREFIXES) {
      if (!line.startsWith(prefix)) {
        continue;
      }

      const path = line.slice(prefix.length).trim();
      if (!path) {
        return;
      }

      const key = `${changeType}:${path}`;
      if (seen.has(key)) {
        return;
      }

      seen.add(key);
      entries.push({
        path,
        change_type: changeType,
        unified_diff: null,
        move_path: null,
      });
      return;
    }
  });

  return entries;
}

function patchChangeTypeLabel(changeType: string) {
  const normalized = changeType.trim().toLowerCase();
  if (normalized === "update" || normalized === "updated") {
    return "update";
  }
  if (normalized === "add" || normalized === "added" || normalized === "create" || normalized === "created") {
    return "add";
  }
  if (normalized === "delete" || normalized === "deleted" || normalized === "remove" || normalized === "removed") {
    return "delete";
  }
  return normalized;
}

function patchChangeDisplayPath(change: PatchApplyChangeEntry) {
  return change.move_path ? `${change.path} -> ${change.move_path}` : change.path;
}

function patchApplyDiffSections(changes: PatchApplyChangeEntry[]) {
  return changes.flatMap((change) => {
    const diffText = change.unified_diff?.trim();
    if (!diffText) {
      return [];
    }

    return [{
      key: `${change.path}-${change.move_path ?? ""}-${change.change_type ?? ""}`,
      label: patchChangeSectionLabel(change),
      diffText,
    }];
  });
}

function patchChangeSectionLabel(change: PatchApplyChangeEntry) {
  const typeLabel = change.change_type ? patchChangeTypeLabel(change.change_type) : "change";
  return `${typeLabel} ${patchChangeDisplayPath(change)}`;
}

function patchDiffLineClassName(line: string) {
  if (line.startsWith("@@")) {
    return "bg-sky-500/10 text-sky-200";
  }

  if (line.startsWith("+") && !line.startsWith("+++")) {
    return "bg-emerald-500/12 text-emerald-200";
  }

  if (line.startsWith("-") && !line.startsWith("---")) {
    return "bg-rose-500/12 text-rose-200";
  }

  if (
    line.startsWith("*** Begin Patch")
    || line.startsWith("*** End Patch")
    || line.startsWith("*** End of File")
  ) {
    return "text-muted-foreground";
  }

  if (
    line.startsWith("*** Update File:")
    || line.startsWith("*** Add File:")
    || line.startsWith("*** Delete File:")
    || line.startsWith("*** Move to:")
  ) {
    return "bg-amber-500/10 text-amber-200";
  }

  return "text-muted-foreground";
}

function mergedShellMetaItems(call: EventEntry, result: EventEntry) {
  const items: Array<{ label: string; value: string }> = [];

  if (call.parse_status !== "parsed") {
    items.push({ label: "call parse", value: call.parse_status });
    items.push({ label: "call raw", value: call.raw_type });
  }

  if (result.parse_status !== "parsed") {
    items.push({ label: "result parse", value: result.parse_status });
    items.push({ label: "result raw", value: result.raw_type });
  }

  return items;
}

function singleShellMetaItems(event: EventEntry) {
  const items: Array<{ label: string; value: string }> = [];

  if (event.parse_status !== "parsed") {
    items.push({ label: "parse", value: event.parse_status });
    items.push({ label: "raw", value: event.raw_type });
  }

  return items;
}

function shellStatus(exitCode: number | null) {
  if (exitCode === 0) {
    return {
      variant: "success" as const,
      Icon: Check,
      label: null,
      ariaLabel: "Успешное завершение",
    };
  }

  if (exitCode != null) {
    return {
      variant: "failure" as const,
      Icon: CircleX,
      label: `код ${exitCode}`,
      ariaLabel: `Завершение с кодом ${exitCode}`,
    };
  }

  return {
    variant: "unknown" as const,
    Icon: CircleAlert,
    label: "без кода",
    ariaLabel: "Код завершения неизвестен",
  };
}

function shellDetailData(call: EventEntry, result: EventEntry) {
  const cwd = normalizedShellText(result.shell_cwd) ?? normalizedShellText(call.shell_cwd);
  const workdir =
    normalizedShellText(call.shell_workdir) ?? normalizedShellText(result.shell_workdir);
  const yieldTime = formatShellYieldTimeMs(
    call.shell_yield_time_ms ?? result.shell_yield_time_ms,
  );
  const maxOutputTokens = formatShellMaxOutputTokens(
    call.shell_max_output_tokens ?? result.shell_max_output_tokens,
  );
  const originalTokenCount = formatShellOriginalTokenCount(
    result.shell_original_token_count ?? call.shell_original_token_count,
  );
  const shellBinary =
    normalizedShellText(call.shell_binary) ?? normalizedShellText(result.shell_binary);
  const login = (call.shell_login ?? result.shell_login) === true;
  const tty = (call.shell_tty ?? result.shell_tty) === true;
  const items: Array<{ label: string; value: string }> = [];

  if (cwd) {
    items.push({ label: "cwd", value: cwd });
  }
  if (workdir && workdir !== cwd) {
    items.push({ label: "workdir", value: workdir });
  }
  if (yieldTime) {
    items.push({ label: "yield", value: yieldTime });
  }
  if (maxOutputTokens) {
    items.push({ label: "max output", value: maxOutputTokens });
  }
  if (originalTokenCount) {
    items.push({ label: "original tokens", value: originalTokenCount });
  }
  if (shellBinary && !["bash", "/bin/bash", "sh", "/bin/sh"].includes(shellBinary)) {
    items.push({ label: "shell", value: shellBinary });
  }
  if (login) {
    items.push({ label: "login", value: "yes" });
  }
  if (tty) {
    items.push({ label: "tty", value: "yes" });
  }

  return {
    parsedHeaders: shellParsedCommandHeaders(call, result),
    items,
  };
}

function shellStatusDetailText(exitCode: number | null) {
  if (exitCode != null) {
    return exitCode === 0 ? null : `код ${exitCode}`;
  }

  return "без кода";
}

function shellOutputSizeBytes(output: string | null) {
  if (!output?.trim()) {
    return null;
  }

  return new TextEncoder().encode(output).length;
}

function formatBodySizeBytes(bytes: number) {
  return `${new Intl.NumberFormat("ru-RU").format(bytes)} B`;
}

function shellOutputSizeLabel(output: string | null) {
  const bytes = shellOutputSizeBytes(output);
  return bytes == null ? null : formatBodySizeBytes(bytes);
}

function formatCompactNumber(value: number) {
  return new Intl.NumberFormat("ru-RU").format(value);
}

function normalizedShellText(value: string | null | undefined) {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

function formatShellYieldTimeMs(value: number | null | undefined) {
  if (value == null || !Number.isFinite(value) || value < 0) {
    return null;
  }

  return `${formatCompactNumber(Math.trunc(value))} ms`;
}

function formatShellMaxOutputTokens(value: number | null | undefined) {
  if (value == null || !Number.isFinite(value) || value < 0) {
    return null;
  }

  return `${formatCompactNumber(Math.trunc(value))} tok`;
}

function formatShellOriginalTokenCount(value: number | null | undefined) {
  if (value == null || !Number.isFinite(value) || value < 0) {
    return null;
  }

  return `${formatCompactNumber(Math.trunc(value))} tok`;
}

function formatShellDurationNs(value: number | null | undefined) {
  if (value == null || !Number.isFinite(value) || value < 0) {
    return null;
  }

  if (value >= 1_000_000_000) {
    return formatShellDurationUnit(value / 1_000_000_000, "s");
  }
  if (value >= 1_000_000) {
    return formatShellDurationUnit(value / 1_000_000, "ms");
  }
  if (value >= 1_000) {
    return formatShellDurationUnit(value / 1_000, "us");
  }
  return `${formatCompactNumber(Math.trunc(value))} ns`;
}

function formatShellDurationUnit(value: number, unit: string) {
  const maximumFractionDigits = value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${new Intl.NumberFormat("ru-RU", { maximumFractionDigits }).format(value)} ${unit}`;
}

function normalizedShellParsedKind(value: string | null | undefined) {
  const kind = normalizedShellText(value);
  if (!kind) {
    return null;
  }
  return kind.toLowerCase() === "unknown" ? null : kind;
}

function shellParsedCommandHeaders(call: EventEntry, result: EventEntry) {
  const sourceEntries =
    result.shell_parsed_commands.length > 0
      ? result.shell_parsed_commands
      : call.shell_parsed_commands;
  const seen = new Set<string>();
  const headers: Array<{ kind: string; typeLabel: string; present: string | null }> = [];

  sourceEntries.forEach((entry) => {
    const kind = normalizedShellParsedKind(entry.kind);
    if (!kind) {
      return;
    }

    const present = shellParsedCommandPresent(entry, kind);
    const key = `${kind}\u0000${present ?? ""}`;
    if (seen.has(key)) {
      return;
    }

    seen.add(key);
    headers.push({ kind, typeLabel: kind, present });
  });

  return headers;
}

function shellParsedCommandPresent(entry: ShellParsedCommandEntry, kind: string) {
  switch (kind.toLowerCase()) {
    case "read":
      return normalizedShellText(entry.path) ?? normalizedShellText(entry.name);
    case "search":
      return normalizedShellText(entry.query) ?? normalizedShellText(entry.command);
    case "list_files":
      return normalizedShellText(entry.path);
    default:
      return null;
  }
}

function infoTokensRenderData(event: EventEntry) {
  if (event.event_type !== INFO_TOKENS) {
    return null;
  }

  const pairs =
    event.summary_pairs.length > 0
      ? event.summary_pairs.map(([label, value]) => ({ label, value }))
      : [
        { label: "input", value: event.input_tokens },
        { label: "cached input", value: event.cached_input_tokens },
        { label: "output", value: event.output_tokens },
        { label: "reasoning output", value: event.reasoning_output_tokens },
        { label: "total", value: event.total_tokens },
      ]
          .filter(
            (pair): pair is { label: string; value: number } => pair.value != null,
          )
          .map((pair) => ({
            label: pair.label,
            value: formatCompactNumber(pair.value),
          }));

  const fallbackText = normalizeInfoTokensSummary(event.summary);
  if (pairs.length === 0 && !fallbackText) {
    return null;
  }

  return {
    pairs,
    fallbackText: pairs.length > 0 ? null : fallbackText,
  };
}

function normalizeInfoTokensSummary(summary: string) {
  const trimmed = summary.trim();
  if (!trimmed || trimmed === INFO_TOKENS) {
    return null;
  }

  const withoutPrefix = trimmed.replace(/^tokens\b[:\s-]*/i, "").trim();
  return withoutPrefix || null;
}

function runtimeContextRenderData(event: EventEntry) {
  if (event.event_type !== RUNTIME_CONTEXT || event.runtime_context_pairs.length === 0) {
    return null;
  }

  const summaryItems = [
    runtimeContextSummaryItem(event, "model"),
    runtimeContextSummaryItem(event, "effort"),
    runtimeContextSummaryItem(event, "approval_policy"),
    runtimeContextSandboxTypeItem(event),
  ].filter((item): item is { label: string; value: string } => item != null);

  const detailItems = runtimeContextDetailPairs(event).map(([label, value]) => ({
    label,
    value,
    isLongText: isRuntimeContextLongTextLabel(label),
  }));

  return {
    summaryItems,
    detailItems,
  };
}

function runtimeContextSummaryItem(event: EventEntry, label: string) {
  const value = runtimeContextPairValue(event, label)?.trim();
  if (!value) {
    return null;
  }

  return { label, value };
}

function runtimeContextSandboxTypeItem(event: EventEntry) {
  const rawValue = runtimeContextPairValue(event, "sandbox_policy");
  if (!rawValue) {
    return null;
  }

  const parsed = safeParseJson(rawValue);
  const parsedObject = asObject(parsed);
  const typeValue =
    parsedObject && typeof parsedObject.type === "string"
      ? parsedObject.type
      : rawValue;
  const value = typeValue.trim();
  if (!value) {
    return null;
  }

  return { label: "sandbox_policy.type", value };
}

function runtimeContextDetailPairs(event: EventEntry) {
  const baseIncluded = new Set([
    "cwd",
    "current_date",
    "timezone",
    "sandbox_policy",
    "collaboration_mode",
  ]);

  return event.runtime_context_pairs.filter(
    ([label, value]) =>
      (baseIncluded.has(label) || isRuntimeContextInstructionsLabel(label)) && value.trim(),
  );
}

function runtimeContextPairValue(event: EventEntry, label: string) {
  const pair = event.runtime_context_pairs.find(([key]) => key === label);
  return pair ? pair[1] : null;
}

function isRuntimeContextInstructionsLabel(label: string) {
  return label.endsWith("_instructions");
}

function isRuntimeContextLongTextLabel(label: string) {
  return label === "collaboration_mode" || isRuntimeContextInstructionsLabel(label);
}

function shouldCollapseText(text: string) {
  return (
    text.length > TEXT_COLLAPSE_CHAR_LIMIT
    || text.split("\n").length > TEXT_COLLAPSE_LINE_LIMIT
  );
}

function truncatePreviewText(text: string) {
  if (!shouldCollapseText(text)) {
    return text;
  }

  const truncated = text.slice(0, TEXT_COLLAPSE_CHAR_LIMIT).trimEnd();
  return `${truncated}...`;
}

function asObject(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }

  return value as Record<string, unknown>;
}

function asArray(value: unknown): unknown[] | null {
  return Array.isArray(value) ? value : null;
}

function asString(value: unknown) {
  return typeof value === "string" ? value : null;
}

function safeParseJson(value: string) {
  try {
    return JSON.parse(value) as unknown;
  } catch {
    return null;
  }
}

const PATCH_APPLY_LINE_PREFIXES = [
  ["*** Update File: ", "update"],
  ["*** Add File: ", "add"],
  ["*** Delete File: ", "delete"],
] as const;

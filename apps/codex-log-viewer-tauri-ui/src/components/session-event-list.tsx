import { useState, type ReactNode } from "react";
import { Check, CircleAlert, CircleX } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import type {
  EventEntry,
  EventNode,
  LoadedSession,
  PatchApplyChangeEntry,
  TimelineItem,
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
const USER_INPUT_REQUEST = "user.input.request";
const RUNTIME_CONTEXT = "runtime.context";
const PATCH_APPLY = "patch.apply";
const PATCH_APPLY_DUPLICATE = "patch.apply.duplicate";
const RESPONSE_ITEM_FUNCTION_CALL_OUTPUT = "response_item.function_call_output";
const TEXT_COLLAPSE_CHAR_LIMIT = 240;
const TEXT_COLLAPSE_LINE_LIMIT = 4;

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

type MergedCard =
  | SingleCard
  | ShellMergedCard
  | SpawnMergedCard
  | UserInputMergedCard
  | CollabMergedCard
  | PatchApplyMergedCard;

export function SessionEventList({ session }: { session: LoadedSession }) {
  const rootItems = session.tree.roots.flatMap((thread) => thread.items);

  return (
    <div className="flex flex-col">
      {rootItems.length > 0 ? <TimelineItemsView items={rootItems} path="root" /> : null}

      {session.tree.orphan_events.length > 0 ? (
        <div className="flex flex-col">
          {session.tree.orphan_events.map((event, index) => (
            <TimelineListItem key={`orphan-${event.event_id}-${index}`} withDivider={index > 0}>
              <EventCard event={event} />
            </TimelineListItem>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function TimelineItemsView({
  items,
  path,
}: {
  items: TimelineItem[];
  path: string;
}) {
  return (
    <div className="flex flex-col">
      {items.map((item, index) => {
        if ("Event" in item) {
          return (
            <TimelineListItem key={`${path}-event-${index}`} withDivider={index > 0}>
              <EventNodeView node={item.Event} path={`${path}-event-${index}`} />
            </TimelineListItem>
          );
        }

        return (
          <TimelineListItem key={`${path}-thread-${item.Thread.thread_id}-${index}`} withDivider={index > 0}>
            <TimelineItemsView
              items={item.Thread.items}
              path={`${path}-thread-${item.Thread.thread_id}-${index}`}
            />
          </TimelineListItem>
        );
      })}
    </div>
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
    <div className={cn(withDivider ? "border-t border-border/60 pt-2" : "")}>
      {children}
    </div>
  );
}

function EventNodeView({
  node,
  path,
}: {
  node: EventNode;
  path: string;
}) {
  const card = buildMergedCard(node);
  const hiddenSingle = card.kind === "single" && shouldHideSingleEvent(card.event);

  return (
    <>
      {card.kind === "single" ? (
        !hiddenSingle ? (
        <EventCard event={card.event} />
        ) : null
      ) : (
        <MergedEventCard card={card} />
      )}
      {card.children.length > 0 ? (
        <TimelineItemsView items={card.children} path={`${path}-children`} />
      ) : null}
    </>
  );
}

function EventCard({ event }: { event: EventEntry }) {
  if (isStandaloneShellResult(event)) {
    return <SingleShellEventCard event={event} />;
  }

  const summary = eventSummaryText(event);
  const detail = singleDetailText(event);
  const subagentLabel = eventSubagentLabel(event);
  const runtimeContext = runtimeContextRenderData(event);
  const patchApply = patchApplyRenderData(event);

  return (
    <Card
      className={cn(
        SURFACE_CARD_CLASS,
        subagentLabel ? "border-l-4 border-l-[color:var(--accent-strong)]" : "",
      )}
      size="sm"
    >
      <CardContent className="flex flex-col gap-2 p-4">
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <span className="font-mono">#{event.seq}</span>
          <time>{formatTime(event.ts)}</time>
          <span className="font-mono">{event.event_type}</span>
          {subagentLabel ? <Badge variant="outline">{subagentLabel}</Badge> : null}
        </div>
        {runtimeContext ? <RuntimeContextBlock data={runtimeContext} /> : null}
        {patchApply ? <PatchApplyBlock data={patchApply} /> : null}
        {!patchApply && summary ? <CardText text={summary} tone="default" /> : null}
        {!patchApply && detail ? <CardText text={detail} tone="muted" /> : null}
      </CardContent>
    </Card>
  );
}

function MergedEventCard({
  card,
}: {
  card: Exclude<MergedCard, { kind: "single" }>;
}) {
  if (card.kind === "shell") {
    return <MergedShellEventCard card={card} />;
  }

  if (card.kind === "patch-apply") {
    return <MergedPatchApplyEventCard card={card} />;
  }

  const summary = mergedSummaryText(card.call, card.result);
  const detail = mergedDetailText(card);
  const subagentLabel = eventSubagentLabel(card.call) ?? eventSubagentLabel(card.result);
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
      <CardContent className="flex flex-col gap-2 p-4">
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <span className="font-mono">
            #{card.call.seq}, #{card.result.seq}
          </span>
          <time>{timeLabel}</time>
          <span className="font-mono">{eventLabel}</span>
          {subagentLabel ? <Badge variant="outline">{subagentLabel}</Badge> : null}
        </div>
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
  const subagentLabel = eventSubagentLabel(card.call) ?? eventSubagentLabel(card.result);
  const patchApply = mergedPatchApplyRenderData(card.call, card.result);

  return (
    <Card
      className={cn(
        SURFACE_CARD_CLASS,
        subagentLabel ? "border-l-4 border-l-[color:var(--accent-strong)]" : "",
      )}
      size="sm"
    >
      <CardContent className="flex flex-col gap-2 p-4">
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <span className="font-mono">#{card.call.seq}</span>
          <time>{formatTime(card.call.ts)}</time>
          <span className="font-mono">{card.call.event_type}</span>
          {subagentLabel ? <Badge variant="outline">{subagentLabel}</Badge> : null}
        </div>
        <PatchApplyBlock data={patchApply} />
      </CardContent>
    </Card>
  );
}

function SingleShellEventCard({ event }: { event: EventEntry }) {
  const subagentLabel = eventSubagentLabel(event);
  const metaItems = singleShellMetaItems(event);
  const outputSizeLabel = shellOutputSizeLabel(event.aggregated_output);

  return (
    <Card
      className={cn(
        SURFACE_CARD_CLASS,
        subagentLabel ? "border-l-4 border-l-[color:var(--accent-strong)]" : "",
      )}
      size="sm"
    >
      <CardContent className="min-w-0 flex flex-col gap-3 p-4">
        <ShellEventHeader
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
  const subagentLabel = eventSubagentLabel(card.call) ?? eventSubagentLabel(card.result);
  const metaItems = mergedShellMetaItems(card.call, card.result);
  const outputSizeLabel = shellOutputSizeLabel(card.result.aggregated_output);
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
      <CardContent className="min-w-0 flex flex-col gap-3 p-4">
        <ShellEventHeader
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
          output={card.result.aggregated_output}
        />
      </CardContent>
    </Card>
  );
}

function ShellEventHeader({
  seqLabel,
  timestampLabel,
  eventLabel,
  subagentLabel,
  exitCode,
  outputSizeLabel,
}: {
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
    <div className="grid min-w-0 w-full grid-cols-[minmax(0,1fr)_auto] items-start gap-3 text-xs text-muted-foreground">
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
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
      {items.map((item) => (
        <span className="inline-flex items-center gap-1" key={`${item.label}-${item.value}`}>
          <span>{item.label}</span>
          <span className="ui-selectable font-semibold text-foreground">{item.value}</span>
        </span>
      ))}
    </div>
  );
}

function ShellBlock({
  command,
  output,
}: {
  command: string | null;
  output: string | null;
}) {
  const [outputVisible, setOutputVisible] = useState(false);
  const hasOutput = Boolean(output?.trim());

  return (
    <div className="min-w-0 w-full flex flex-col gap-2">
      <div className="grid min-w-0 w-full grid-cols-[minmax(0,1fr)_auto] items-start gap-3">
        <div className="min-w-0 flex-1">
          {command ? (
            <code className="ui-selectable block max-w-full whitespace-pre-wrap break-words text-sm font-medium text-foreground">
              {`$ ${command}`}
            </code>
          ) : (
            <span className="text-sm text-muted-foreground">command unavailable</span>
          )}
        </div>
        {hasOutput ? (
          <button
            className="inline-flex shrink-0 items-center justify-self-end text-xs font-medium text-[color:var(--accent-strong)] transition-opacity hover:opacity-80"
            onClick={() => setOutputVisible((current) => !current)}
            type="button"
          >
            {outputVisible ? "Скрыть вывод" : "Показать вывод"}
          </button>
        ) : null}
      </div>
      {hasOutput && outputVisible ? (
        <div className="border-l border-border/60 pl-3">
          <code className="ui-selectable block whitespace-pre-wrap break-words text-xs text-foreground">
            {output}
          </code>
        </div>
      ) : null}
    </div>
  );
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
          tone === "default" ? "text-sm leading-6" : "text-xs leading-5 text-muted-foreground",
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
  const summaryItems = [
    data.changes.length === 0 && data.phase ? { label: "phase", value: data.phase } : null,
    data.status && data.status !== "completed" ? { label: "status", value: data.status } : null,
  ].filter((item): item is { label: string; value: string } => item != null);

  return (
    <div className="flex flex-col gap-2">
      {summaryItems.length > 0 ? <EventMetaRow items={summaryItems} /> : null}
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
        <div className="flex flex-col gap-1">
          <div className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
            patch
          </div>
          <PatchDiffBlock fallbackText={data.fallbackDiffText} sections={data.diffSections} />
        </div>
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
        <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-muted-foreground">
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

function eventSubagentLabel(event: EventEntry) {
  if (event.actor_type !== "subagent") {
    return null;
  }

  const label = event.subagent_nickname?.trim();
  return label ? label : null;
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
  return call.user_input_request ?? result.user_input_request ?? null;
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

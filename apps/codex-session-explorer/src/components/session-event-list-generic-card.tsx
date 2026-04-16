import {
  CardText,
  COMPACT_CARD_CONTENT_CLASS,
  EventMetaLine,
  EventMetaRow,
  EventSurfaceCard,
  EventTypeLabel,
  RuntimeContextBlock,
  TaskCompletedMessageBlock,
  TaskLifecycleMarker,
  TaskModeBadge,
  TerminalEventBadge,
} from "@/components/session-event-list-common";
import type { EventCardTone } from "@/components/session-event-list-tone";

export function GenericSingleEventCardView({
  detail,
  eventLabel,
  metaItems,
  runtimeContext,
  seqLabel,
  subagentLabel,
  summary,
  taskMarker,
  taskMessage,
  taskModeBadge,
  terminalBadge,
  tone,
  timestampLabel,
}: {
  detail: string | null;
  eventLabel: string;
  metaItems: Array<{ label: string; value: string }>;
  runtimeContext: {
    summaryItems: Array<{ label: string; value: string }>;
    detailItems: Array<{ label: string; value: string; isLongText?: boolean }>;
  } | null;
  seqLabel: string;
  subagentLabel: string | null;
  summary: string | null;
  taskMarker: { accent: string; completed: boolean } | null;
  taskMessage: string | null;
  taskModeBadge: { label: string; palette: { accent: string; surface: string } } | null;
  terminalBadge: { className: string; label: string } | null;
  tone: EventCardTone;
  timestampLabel: string;
}) {
  const isUtility =
    !summary
    && !detail
    && !runtimeContext
    && metaItems.length === 0
    && !taskMessage
    && !taskModeBadge
    && !terminalBadge;
  const isMessageCard =
    tone === "message"
    && !runtimeContext
    && !taskMessage
    && !taskModeBadge
    && !terminalBadge
    && (Boolean(summary) || Boolean(detail));
  const messageRole = isMessageCard ? messageRoleLabel(eventLabel) : null;

  return (
    <div className="relative">
      {taskMarker ? <TaskLifecycleMarker marker={taskMarker} /> : null}
      <EventSurfaceCard
        contentClassName={
          isUtility
            ? "min-w-0 flex flex-col gap-1 px-3 py-2"
            : isMessageCard
              ? "min-w-0 flex flex-col gap-1.5 px-3 py-2.5"
              : COMPACT_CARD_CONTENT_CLASS
        }
        subagentLabel={subagentLabel}
      >
        {isUtility ? (
          <div className="flex flex-wrap items-start gap-x-2 gap-y-1">
            <EventMetaLine
              seqLabel={seqLabel}
              subagentLabel={subagentLabel}
              timestampLabel={timestampLabel}
            />
            <EventTypeLabel compact label={eventLabel} tone={tone} />
            {taskModeBadge ? <TaskModeBadge badge={taskModeBadge} /> : null}
            {terminalBadge ? <TerminalEventBadge badge={terminalBadge} /> : null}
          </div>
        ) : isMessageCard ? (
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <EventMetaLine
              seqLabel={seqLabel}
              subagentLabel={subagentLabel}
              timestampLabel={timestampLabel}
            />
            {messageRole ? <MessageRoleChip label={messageRole} /> : null}
          </div>
        ) : (
          <div className="flex flex-col gap-1">
            <EventMetaLine
              seqLabel={seqLabel}
              subagentLabel={subagentLabel}
              timestampLabel={timestampLabel}
            />
            <div className="flex flex-wrap items-start gap-x-2 gap-y-1">
              <EventTypeLabel label={eventLabel} tone={tone} />
              {taskModeBadge ? <TaskModeBadge badge={taskModeBadge} /> : null}
              {terminalBadge ? <TerminalEventBadge badge={terminalBadge} /> : null}
            </div>
          </div>
        )}
        {metaItems.length > 0 ? <EventMetaRow items={metaItems} /> : null}
        {runtimeContext ? <RuntimeContextBlock data={runtimeContext} /> : null}
        {summary ? (
          <div className={isMessageCard ? "max-w-[74ch]" : "max-w-[78ch]"}>
            <CardText text={summary} tone="default" />
          </div>
        ) : null}
        {taskMessage ? (
          <div className="max-w-[78ch]">
            <TaskCompletedMessageBlock text={taskMessage} />
          </div>
        ) : null}
        {!taskMessage && detail ? (
          <div className={isMessageCard ? "max-w-[74ch]" : "max-w-[78ch]"}>
            <CardText text={detail} tone="muted" />
          </div>
        ) : null}
      </EventSurfaceCard>
    </div>
  );
}

export function GenericMergedEventCardView({
  detail,
  eventLabel,
  metaItems,
  seqLabel,
  subagentLabel,
  summary,
  tone,
  timestampLabel,
}: {
  detail: string | null;
  eventLabel: string;
  metaItems: Array<{ label: string; value: string }>;
  seqLabel: string;
  subagentLabel: string | null;
  summary: string | null;
  tone: EventCardTone;
  timestampLabel: string;
}) {
  const isUtility = !summary && !detail && metaItems.length === 0;
  const isMessageCard = tone === "message" && (Boolean(summary) || Boolean(detail));
  const messageRole = isMessageCard ? messageRoleLabel(eventLabel) : null;

  return (
    <EventSurfaceCard
      contentClassName={
        isUtility
          ? "min-w-0 flex flex-col gap-1 px-3 py-2"
          : isMessageCard
            ? "min-w-0 flex flex-col gap-1.5 px-3 py-2.5"
            : COMPACT_CARD_CONTENT_CLASS
      }
      subagentLabel={subagentLabel}
    >
      {isUtility ? (
        <div className="flex flex-wrap items-start gap-x-2 gap-y-1">
          <EventMetaLine
            seqLabel={seqLabel}
            subagentLabel={subagentLabel}
            timestampLabel={timestampLabel}
          />
          <EventTypeLabel compact label={eventLabel} tone={tone} />
        </div>
      ) : isMessageCard ? (
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <EventMetaLine
            seqLabel={seqLabel}
            subagentLabel={subagentLabel}
            timestampLabel={timestampLabel}
          />
          {messageRole ? <MessageRoleChip label={messageRole} /> : null}
        </div>
      ) : (
        <div className="flex flex-col gap-1">
          <EventMetaLine
            seqLabel={seqLabel}
            subagentLabel={subagentLabel}
            timestampLabel={timestampLabel}
          />
          <EventTypeLabel label={eventLabel} tone={tone} />
        </div>
      )}
      {metaItems.length > 0 ? <EventMetaRow items={metaItems} /> : null}
      {summary ? (
        <div className={isMessageCard ? "max-w-[74ch]" : "max-w-[78ch]"}>
          <CardText text={summary} tone="default" />
        </div>
      ) : null}
      {detail ? (
        <div className={isMessageCard ? "max-w-[74ch]" : "max-w-[78ch]"}>
          <CardText text={detail} tone="muted" />
        </div>
      ) : null}
    </EventSurfaceCard>
  );
}

function messageRoleLabel(eventLabel: string) {
  const role = eventLabel.split(".").at(-1)?.trim();
  return role ? role.replaceAll("_", " ") : "message";
}

function MessageRoleChip({ label }: { label: string }) {
  return (
    <span className="inline-flex items-center rounded-full border border-sky-500/20 bg-sky-500/8 px-2 py-0.5 text-[10px] font-medium tracking-[0.04em] text-sky-700 dark:text-sky-300">
      {label}
    </span>
  );
}

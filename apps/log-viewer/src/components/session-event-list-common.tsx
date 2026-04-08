import { useState, type ReactNode } from "react";
import { Badge } from "@/components/ui/badge";

import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import type { EventCardTone } from "@/components/session-event-list-tone";

export const SURFACE_CARD_CLASS = "rounded-none border-0 bg-transparent shadow-none ring-0";
export const COMPACT_CARD_CONTENT_CLASS = "min-w-0 flex flex-col gap-1.5 p-3";
export const COMPACT_CARD_CONTENT_SPACED_CLASS = "min-w-0 flex flex-col gap-2 p-3";
export const COMPACT_EVENT_HEADER_CLASS =
  "flex flex-wrap items-start gap-x-2 gap-y-1 text-[11px] text-muted-foreground";

const TEXT_COLLAPSE_CHAR_LIMIT = 240;
const TEXT_COLLAPSE_LINE_LIMIT = 4;

export function EventSurfaceCard({
  children,
  contentClassName,
  subagentLabel,
}: {
  children: ReactNode;
  contentClassName: string;
  subagentLabel: string | null;
}) {
  return (
    <Card
      className={cn(
        SURFACE_CARD_CLASS,
        subagentLabel ? "border-l-4 border-l-[color:var(--accent-strong)]" : "",
      )}
      size="sm"
    >
      <CardContent className={contentClassName}>{children}</CardContent>
    </Card>
  );
}

export function EventMetaRow({
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

export function EventMetaLine({
  seqLabel,
  subagentLabel,
  timestampLabel,
}: {
  seqLabel: string;
  subagentLabel: string | null;
  timestampLabel: string;
}) {
  return (
    <div className="flex flex-wrap items-start gap-x-2 gap-y-1 text-[10px] text-muted-foreground/85">
      <span className="font-mono">{seqLabel}</span>
      <time>{timestampLabel}</time>
      {subagentLabel ? (
        <Badge className="h-5 px-1.5 text-[10px]" variant="outline">
          {subagentLabel}
        </Badge>
      ) : null}
    </div>
  );
}

export function EventTypeLabel({
  compact = false,
  label,
  tone,
}: {
  compact?: boolean;
  label: string;
  tone: EventCardTone;
}) {
  return (
    <span
      className={cn(
        "inline-flex min-w-0 items-center gap-1.5",
        compact ? "text-[11px] font-medium leading-4" : "text-[12px] font-semibold leading-4",
        eventToneTextClassName(tone),
      )}
    >
      <span
        aria-hidden="true"
        className={cn("mt-0.5 size-1.5 shrink-0 rounded-full", eventToneDotClassName(tone))}
      />
      <span className="min-w-0 break-words">
        {label}
      </span>
    </span>
  );
}

export function CardText({
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

export function TaskCompletedMessageBlock({ text }: { text: string }) {
  return (
    <div className="flex flex-col gap-0.5">
      <div className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
        Last Agent Message
      </div>
      <CardText text={text} tone="default" />
    </div>
  );
}

export function TaskModeBadge({
  badge,
}: {
  badge: { label: string; palette: { accent: string; surface: string } };
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

export function TerminalEventBadge({
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

export function TaskLifecycleMarker({
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

export function RuntimeContextBlock({
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

function eventToneTextClassName(tone: EventCardTone) {
  switch (tone) {
    case "message":
      return "text-sky-700 dark:text-sky-300";
    case "shell":
      return "text-amber-700 dark:text-amber-300";
    case "plan":
      return "text-cyan-700 dark:text-cyan-300";
    case "patch":
      return "text-rose-700 dark:text-rose-300";
    case "tokens":
      return "text-emerald-700 dark:text-emerald-300";
    case "collab":
      return "text-violet-700 dark:text-violet-300";
    case "meta":
      return "text-slate-700 dark:text-slate-300";
    case "task":
      return "text-blue-700 dark:text-blue-300";
    case "error":
      return "text-rose-700 dark:text-rose-300";
    default:
      return "text-foreground";
  }
}

function eventToneDotClassName(tone: EventCardTone) {
  switch (tone) {
    case "message":
      return "bg-sky-500";
    case "shell":
      return "bg-amber-500";
    case "plan":
      return "bg-cyan-500";
    case "patch":
      return "bg-rose-500";
    case "tokens":
      return "bg-emerald-500";
    case "collab":
      return "bg-violet-500";
    case "meta":
      return "bg-slate-500";
    case "task":
      return "bg-blue-500";
    case "error":
      return "bg-rose-500";
    default:
      return "bg-muted-foreground";
  }
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

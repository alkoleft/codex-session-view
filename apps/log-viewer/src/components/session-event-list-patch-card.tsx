import { useState } from "react";
import { Check, CircleAlert, CircleX } from "lucide-react";

import {
  COMPACT_CARD_CONTENT_CLASS,
  CardText,
  EventMetaLine,
  EventSurfaceCard,
  EventTypeLabel,
} from "@/components/session-event-list-common";
import type { PatchApplyStatusData } from "@/components/session-event-list-patch";
import type { EventCardTone } from "@/components/session-event-list-tone";
import { cn } from "@/lib/utils";

type PatchApplyChangeEntry = {
  path: string;
  change_type: string | null;
  unified_diff: string | null;
  move_path: string | null;
};

export function PatchApplyEventCardView({
  data,
  eventLabel,
  seqLabel,
  status,
  subagentLabel,
  tone,
  timestampLabel,
}: {
  data: {
    phase: string | null;
    status: string | null;
    output: string | null;
    changes: PatchApplyChangeEntry[];
    diffSections: Array<{ key: string; label: string; diffText: string }>;
    fallbackDiffText: string | null;
  };
  eventLabel: string;
  seqLabel: string;
  status: PatchApplyStatusData;
  subagentLabel: string | null;
  tone: EventCardTone;
  timestampLabel: string;
}) {
  return (
    <EventSurfaceCard
      contentClassName={COMPACT_CARD_CONTENT_CLASS}
      subagentLabel={subagentLabel}
    >
      <PatchApplyEventHeader
        eventLabel={eventLabel}
        seqLabel={seqLabel}
        status={status}
        subagentLabel={subagentLabel}
        tone={tone}
        timestampLabel={timestampLabel}
      />
      <PatchApplyBlock data={data} />
    </EventSurfaceCard>
  );
}

function PatchApplyEventHeader({
  seqLabel,
  timestampLabel,
  eventLabel,
  subagentLabel,
  status,
  tone,
}: {
  seqLabel: string;
  timestampLabel: string;
  eventLabel: string;
  subagentLabel: string | null;
  status: PatchApplyStatusData;
  tone: EventCardTone;
}) {
  const Icon =
    status.variant === "success"
      ? Check
      : status.variant === "failure"
        ? CircleX
        : CircleAlert;

  return (
    <div className="grid min-w-0 w-full grid-cols-[minmax(0,1fr)_auto] items-start gap-2.5 text-[11px] text-muted-foreground">
      <div className="min-w-0 flex flex-col gap-1">
        <EventMetaLine
          seqLabel={seqLabel}
          subagentLabel={subagentLabel}
          timestampLabel={timestampLabel}
        />
        <EventTypeLabel label={eventLabel} tone={tone} />
      </div>
      <div className="flex shrink-0 items-start justify-self-end gap-2 pt-0.5">
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

function PatchApplyBlock({
  data,
}: {
  data: {
    phase: string | null;
    status: string | null;
    output: string | null;
    changes: PatchApplyChangeEntry[];
    diffSections: Array<{ key: string; label: string; diffText: string }>;
    fallbackDiffText: string | null;
  };
}) {
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
  sections: Array<{ key: string; label: string; diffText: string }>;
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

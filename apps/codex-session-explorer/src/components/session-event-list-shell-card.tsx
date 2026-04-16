import { useState } from "react";
import { Check, CircleAlert, CircleX } from "lucide-react";

import {
  COMPACT_CARD_CONTENT_SPACED_CLASS,
  EventMetaLine,
  EventMetaRow,
  EventSurfaceCard,
  EventTypeLabel,
} from "@/components/session-event-list-common";
import type { EventCardTone } from "@/components/session-event-list-tone";
import { cn } from "@/lib/utils";

type ShellParsedHeader = {
  kind: string;
  typeLabel: string;
  present: string | null;
};

export function ShellEventCardView({
  command,
  detailData,
  durationLabel,
  eventLabel,
  exitCode,
  metaItems,
  output,
  outputSizeLabel,
  seqLabel,
  subagentLabel,
  tone,
  timestampLabel,
}: {
  command: string | null;
  detailData: {
    parsedHeaders: ShellParsedHeader[];
    items: Array<{ label: string; value: string }>;
  };
  durationLabel: string | null;
  eventLabel: string;
  exitCode: number | null;
  metaItems: Array<{ label: string; value: string }>;
  output: string | null;
  outputSizeLabel: string | null;
  seqLabel: string;
  subagentLabel: string | null;
  tone: EventCardTone;
  timestampLabel: string;
}) {
  return (
    <EventSurfaceCard
      contentClassName={COMPACT_CARD_CONTENT_SPACED_CLASS}
      subagentLabel={subagentLabel}
    >
      <ShellEventHeader
        durationLabel={durationLabel}
        eventLabel={eventLabel}
        exitCode={exitCode}
        outputSizeLabel={outputSizeLabel}
        seqLabel={seqLabel}
        subagentLabel={subagentLabel}
        tone={tone}
        timestampLabel={timestampLabel}
      />
      {metaItems.length > 0 ? <EventMetaRow items={metaItems} /> : null}
      <ShellBlock
        command={command}
        detailData={detailData}
        output={output}
      />
    </EventSurfaceCard>
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
  tone,
}: {
  durationLabel: string | null;
  seqLabel: string;
  timestampLabel: string;
  eventLabel: string;
  subagentLabel: string | null;
  exitCode: number | null;
  outputSizeLabel: string | null;
  tone: EventCardTone;
}) {
  const status = shellStatus(exitCode);
  const statusText = shellStatusDetailText(exitCode);

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

function ShellBlock({
  command,
  detailData,
  output,
}: {
  command: string | null;
  detailData: {
    parsedHeaders: ShellParsedHeader[];
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
      <div className="min-w-0 flex flex-col gap-1.5">
        <div className="min-w-0 flex flex-1 flex-col gap-1">
          <div className="max-w-[96ch]">
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
        </div>
        {toggleLabel ? (
          <button
            className="inline-flex items-center self-start text-[11px] font-medium text-[color:var(--accent-strong)] transition-opacity hover:opacity-80"
            onClick={() => setExpanded((current) => !current)}
            type="button"
          >
            {toggleLabel}
          </button>
        ) : null}
      </div>
      {expanded && hasExpandable ? (
        <div className="grid max-w-[96ch] gap-2 border-l border-border/40 pl-3">
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

function shellStatus(exitCode: number | null) {
  if (exitCode === 0) {
    return {
      variant: "success" as const,
      Icon: Check,
      ariaLabel: "Успешное завершение",
    };
  }

  if (exitCode != null) {
    return {
      variant: "failure" as const,
      Icon: CircleX,
      ariaLabel: `Завершение с кодом ${exitCode}`,
    };
  }

  return {
    variant: "unknown" as const,
    Icon: CircleAlert,
    ariaLabel: "Код завершения неизвестен",
  };
}

function shellStatusDetailText(exitCode: number | null) {
  if (exitCode != null) {
    return exitCode === 0 ? null : `код ${exitCode}`;
  }

  return "без кода";
}

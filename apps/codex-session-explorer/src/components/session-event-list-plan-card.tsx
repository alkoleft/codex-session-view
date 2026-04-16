import {
  COMPACT_CARD_CONTENT_SPACED_CLASS,
  CardText,
  EventMetaLine,
  EventSurfaceCard,
  EventTypeLabel,
} from "@/components/session-event-list-common";
import type { PlanUpdateRenderData } from "@/components/session-event-list-plan";
import type { EventCardTone } from "@/components/session-event-list-tone";

export function PlanUpdateEventCardView({
  data,
  eventLabel,
  phaseLabel,
  seqLabel,
  subagentLabel,
  tone,
  timestampLabel,
}: {
  data: PlanUpdateRenderData;
  eventLabel: string;
  phaseLabel: string | null;
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
      <div className="flex flex-col gap-1">
        <EventMetaLine
          seqLabel={seqLabel}
          subagentLabel={subagentLabel}
          timestampLabel={timestampLabel}
        />
        <EventTypeLabel label={eventLabel} tone={tone} />
      </div>
      <PlanUpdateBlock data={data} phaseLabel={phaseLabel} />
    </EventSurfaceCard>
  );
}

function PlanUpdateBlock({
  data,
  phaseLabel,
}: {
  data: PlanUpdateRenderData;
  phaseLabel: string | null;
}) {
  return (
    <div className="flex max-w-[88ch] flex-col gap-2.5">
      {data.explanation ? (
        <div className="grid gap-2 sm:grid-cols-[auto_minmax(0,1fr)_auto] sm:items-start sm:gap-2.5">
          <div aria-hidden="true" className="font-mono text-[10px] opacity-0">
            00
          </div>
          <div className="sm:pl-2.5">
            <CardText text={data.explanation} tone="default" />
          </div>
        </div>
      ) : null}
      {data.steps.length > 0 ? (
        <div className="flex flex-col gap-1.5">
          <div className="flex flex-col gap-1.5">
            {data.steps.map((step, index) => (
              <div
                className="grid gap-2 rounded-lg border border-border/35 bg-background/20 px-2.5 py-1.5 sm:grid-cols-[auto_minmax(0,1fr)_auto] sm:items-start sm:gap-2.5"
                key={`${index + 1}-${step.step}-${step.status ?? ""}`}
              >
                <div className="font-mono text-[10px] text-muted-foreground/80">
                  {(index + 1).toString().padStart(2, "0")}
                </div>
                <div className="ui-selectable whitespace-pre-wrap break-words text-[13px] leading-5 text-foreground">
                  {step.step}
                </div>
                {step.status ? <PlanStepStatusBadge status={step.status} /> : null}
              </div>
            ))}
          </div>
        </div>
      ) : null}
      {phaseLabel ? (
        <div className="grid gap-2 sm:grid-cols-[auto_minmax(0,1fr)_auto] sm:items-start sm:gap-2.5">
          <div aria-hidden="true" className="font-mono text-[10px] opacity-0">
            00
          </div>
          <div className="text-[10px] text-muted-foreground sm:pl-2.5">
            phase {phaseLabel}
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
      return "inline-flex items-center rounded-full border border-emerald-500/20 bg-emerald-500/8 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-[0.06em] text-emerald-700 dark:text-emerald-300";
    case "in_progress":
      return "inline-flex items-center rounded-full border border-sky-500/20 bg-sky-500/8 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-[0.06em] text-sky-700 dark:text-sky-300";
    case "pending":
      return "inline-flex items-center rounded-full border border-amber-500/20 bg-amber-500/8 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-[0.06em] text-amber-700 dark:text-amber-300";
    default:
      return "inline-flex items-center rounded-full border border-border/35 bg-background/35 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-[0.06em] text-muted-foreground";
  }
}

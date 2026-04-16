import { Badge } from "@/components/ui/badge";

import { EventSurfaceCard } from "@/components/session-event-list-common";

export function InfoTokensEventCardView({
  fallbackText,
  pairs,
  seqLabel,
  subagentLabel,
  timestampLabel,
}: {
  fallbackText: string | null;
  pairs: Array<{ label: string; value: string }>;
  seqLabel: string;
  subagentLabel: string | null;
  timestampLabel: string;
}) {
  return (
    <EventSurfaceCard contentClassName="min-w-0 px-3 py-2" subagentLabel={subagentLabel}>
      <div className="grid min-w-0 w-full grid-cols-[auto_minmax(0,1fr)] items-center gap-2">
        <div className="flex shrink-0 items-center gap-1.5 whitespace-nowrap text-[11px] text-muted-foreground">
          <span className="font-mono">{seqLabel}</span>
          <time>{timestampLabel}</time>
          {subagentLabel ? <Badge variant="outline">{subagentLabel}</Badge> : null}
        </div>
        <InfoTokensBlock fallbackText={fallbackText} pairs={pairs} />
      </div>
    </EventSurfaceCard>
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

  return <div />;
}

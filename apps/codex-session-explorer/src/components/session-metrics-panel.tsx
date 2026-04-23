import { Activity, AlertTriangle, Bot, Gauge, Wrench } from "lucide-react";

import { buildSessionMetricsViewModel } from "@/components/session-metrics";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { IndexedSessionSummary, LoadedSession, SessionPreview } from "@/backend";
import { cn } from "@/lib/utils";

const SURFACE_CARD_CLASS = "border-border bg-background shadow-none";

export function SessionMetricsPanel({
  selectedIndexedSummary,
  selectedLoadedSession,
  selectedPreview,
}: {
  selectedIndexedSummary: IndexedSessionSummary | null;
  selectedLoadedSession: LoadedSession | null;
  selectedPreview: SessionPreview | null;
}) {
  const metrics = buildSessionMetricsViewModel({
    selectedPreview,
    selectedIndexedSummary,
    selectedLoadedSession,
  });

  return (
    <Card className={SURFACE_CARD_CLASS} size="sm">
      <CardHeader className="gap-2">
        <div className="flex items-center gap-2">
          <Gauge className="size-4 text-[color:var(--accent-strong)]" />
          <CardTitle className="text-sm">Session metrics</CardTitle>
        </div>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
          {metrics.items.map((metric) => (
            <section
              className="rounded-2xl border border-border/60 bg-background/40 px-3 py-3"
              key={metric.label}
            >
              <div className="flex items-center gap-2 text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
                <MetricIcon label={metric.label} />
                <span>{metric.label}</span>
              </div>
              <div
                className={cn(
                  "mt-2 text-xl font-semibold tracking-[-0.03em] text-foreground",
                  metric.tone === "accent" && "text-[color:var(--accent-strong)]",
                  metric.tone === "danger" && "text-rose-300",
                )}
              >
                {metric.value}
              </div>
            </section>
          ))}
        </div>
        {!selectedLoadedSession ? (
          <p className="mt-3 text-xs leading-5 text-muted-foreground">
            Метрики по операциям, потокам и сообщениям появятся после загрузки полного дерева
            событий.
          </p>
        ) : null}
      </CardContent>
    </Card>
  );
}

function MetricIcon({ label }: { label: string }) {
  switch (label) {
    case "Time worked":
      return <Activity className="size-3.5" />;
    case "Tool calls":
    case "Unique tools":
      return <Wrench className="size-3.5" />;
    case "Errors":
    case "Failed ops":
      return <AlertTriangle className="size-3.5" />;
    case "Threads":
    case "Spawn agent":
    case "Successes":
      return <Bot className="size-3.5" />;
    default:
      return <Gauge className="size-3.5" />;
  }
}

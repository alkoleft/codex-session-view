import { useState } from "react";
import { Activity, AlertTriangle, Bot, Gauge, Wrench } from "lucide-react";

import { buildSessionMetricsViewModel } from "@/components/session-metrics";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type {
  IndexedSessionSummary,
  LoadedSession,
  ProjectMetricsResponse,
  SessionPreview,
} from "@/backend";
import { cn } from "@/lib/utils";

const SURFACE_CARD_CLASS = "border-border bg-background shadow-none";

export function SessionMetricsPanel({
  projectMetrics,
  selectedIndexedSummary,
  selectedLoadedSession,
  selectedPreview,
}: {
  projectMetrics: ProjectMetricsResponse | null;
  selectedIndexedSummary: IndexedSessionSummary | null;
  selectedLoadedSession: LoadedSession | null;
  selectedPreview: SessionPreview | null;
}) {
  const [includeSpawnAgents, setIncludeSpawnAgents] = useState(true);
  const metrics = buildSessionMetricsViewModel({
    includeSpawnAgents,
    selectedPreview,
    selectedIndexedSummary,
    selectedLoadedSession,
  });
  const hasBackendMetrics = Boolean(selectedLoadedSession?.metrics);

  return (
    <Card className={SURFACE_CARD_CLASS} size="sm">
      <CardHeader className="gap-2">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <Gauge className="size-4 text-[color:var(--accent-strong)]" />
            <CardTitle className="text-sm">Session metrics</CardTitle>
          </div>
          {hasBackendMetrics ? (
            <label className="flex items-center gap-2 text-xs text-muted-foreground">
              <input
                checked={includeSpawnAgents}
                className="size-3.5 accent-[color:var(--accent-strong)]"
                onChange={(event) => setIncludeSpawnAgents(event.target.checked)}
                type="checkbox"
              />
              <span>Spawn agents</span>
            </label>
          ) : null}
        </div>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
          {metrics.items.map((metric) => (
            <section
              className="rounded-lg border border-border/60 bg-background/40 px-3 py-3"
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
        {projectMetrics ? (
          <section className="mt-3 rounded-lg border border-border/60 bg-background/40 px-3 py-3">
            <div className="flex items-center justify-between gap-3 text-xs">
              <span className="font-semibold text-foreground">Project sessions</span>
              <span className="text-muted-foreground">
                {projectMetrics.session_count} · {formatProjectTokens(projectMetrics)}
              </span>
            </div>
            <div className="mt-2 flex flex-wrap gap-1.5">
              {projectMetrics.sessions.slice(0, 6).map((session) => (
                <span
                  className="rounded-md border border-border/60 px-2 py-1 font-mono text-[11px] text-muted-foreground"
                  key={session.session_id}
                  title={session.started_at ?? session.session_id}
                >
                  {session.session_id.slice(0, 8)}
                </span>
              ))}
            </div>
          </section>
        ) : null}
      </CardContent>
    </Card>
  );
}

function formatProjectTokens(projectMetrics: ProjectMetricsResponse) {
  const value = projectMetrics.token_ledger.total.value;
  if (value == null) {
    return projectMetrics.token_ledger.total.coverage;
  }
  return new Intl.NumberFormat("ru-RU").format(value);
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

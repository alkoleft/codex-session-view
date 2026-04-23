import { useEffect, useId, useRef, useState } from "react";
import {
  AlertTriangle,
  BarChart3,
  Check,
  ChevronDown,
  FolderGit2,
  Gauge,
  RefreshCcw,
} from "lucide-react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import {
  buildProjectMetricsViewModel,
  createInitialProjectMetricsRange,
  type ProjectMetricChart,
  type ProjectMetricsRangePreset,
  type ProjectMetricsRangeSelection,
  type ProjectSelectorOption,
} from "@/components/project-metrics";
import type { ProjectMetricsResponse } from "@/backend";

const SURFACE_CARD_CLASS = "border-border bg-background shadow-none";
const FILTER_CONTROL_CLASS =
  "flex h-10 w-full items-center justify-between rounded-xl border border-border/70 bg-background px-3 text-sm text-foreground transition hover:border-border hover:bg-muted/30";
const FILTER_META_CLASS = "text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground";

export function ProjectMetricsScreen({
  catalogBusy,
  currentSessionId,
  error,
  includeSpawnAgents,
  loading,
  metrics,
  onIncludeSpawnAgentsChange,
  onOpenSession,
  onProjectChange,
  onRangeChange,
  onRefresh,
  projectOptions,
  range,
  selectedProjectKey,
}: {
  catalogBusy: boolean;
  currentSessionId: string | null;
  error: string | null;
  includeSpawnAgents: boolean;
  loading: boolean;
  metrics: ProjectMetricsResponse | null;
  onIncludeSpawnAgentsChange: (value: boolean) => void;
  onOpenSession: (sessionId: string) => void;
  onProjectChange: (projectKey: string) => void;
  onRangeChange: (range: ProjectMetricsRangeSelection) => void;
  onRefresh: () => void;
  projectOptions: ProjectSelectorOption[];
  range: ProjectMetricsRangeSelection;
  selectedProjectKey: string | null;
}) {
  const viewModel = metrics ? buildProjectMetricsViewModel(metrics, includeSpawnAgents) : null;
  const selectedProject = projectOptions.find((option) => option.projectKey === selectedProjectKey) ?? null;
  const [projectMenuOpen, setProjectMenuOpen] = useState(false);
  const [windowMenuOpen, setWindowMenuOpen] = useState(false);
  const projectMenuId = useId();
  const windowMenuId = useId();
  const projectMenuRef = useRef<HTMLDivElement | null>(null);
  const windowMenuRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    function handlePointerDown(event: PointerEvent) {
      const target = event.target;
      if (!(target instanceof Node)) {
        return;
      }

      if (projectMenuRef.current && !projectMenuRef.current.contains(target)) {
        setProjectMenuOpen(false);
      }
      if (windowMenuRef.current && !windowMenuRef.current.contains(target)) {
        setWindowMenuOpen(false);
      }
    }

    function handleEscape(event: KeyboardEvent) {
      if (event.key !== "Escape") {
        return;
      }
      setProjectMenuOpen(false);
      setWindowMenuOpen(false);
    }

    window.addEventListener("pointerdown", handlePointerDown);
    window.addEventListener("keydown", handleEscape);
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown);
      window.removeEventListener("keydown", handleEscape);
    };
  }, []);

  const selectedWindowLabel = describeRangePreset(range.preset);

  return (
    <div className="grid min-h-0 flex-1 gap-5 xl:grid-cols-[minmax(0,1.7fr)_minmax(320px,0.9fr)]">
      <Card className={cn(SURFACE_CARD_CLASS, "min-h-0")}>
        <CardHeader className="gap-3 border-b border-border/50">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="space-y-1">
              <div className="flex items-center gap-2">
                <BarChart3 className="size-4 text-[color:var(--accent-strong)]" />
                <CardTitle>Project metrics</CardTitle>
              </div>
              <p className="text-sm text-muted-foreground">
                Отдельный экран хронологии по проекту поверх `query_project_metrics`.
              </p>
            </div>
            <Button onClick={onRefresh} size="sm" type="button" variant="outline">
              <RefreshCcw className={cn("size-3.5", loading && "animate-spin")} data-icon="inline-start" />
              Refresh
            </Button>
          </div>

          <div className="grid gap-3 md:grid-cols-[minmax(0,1.6fr)_minmax(180px,0.7fr)_minmax(220px,0.8fr)]">
            <div className="flex flex-col gap-1.5" ref={projectMenuRef}>
              <span className={FILTER_META_CLASS}>Project</span>
              <button
                aria-controls={projectMenuId}
                aria-expanded={projectMenuOpen}
                className={FILTER_CONTROL_CLASS}
                onClick={() => {
                  setProjectMenuOpen((current) => !current);
                  setWindowMenuOpen(false);
                }}
                type="button"
              >
                <span className="min-w-0 truncate text-left font-medium">
                  {selectedProject?.label ?? "Projects unavailable"}
                </span>
                <ChevronDown className={cn("size-4 shrink-0 text-muted-foreground transition", projectMenuOpen && "rotate-180")} />
              </button>
              {projectMenuOpen ? (
                <div
                  className="absolute z-20 mt-[4.75rem] w-[min(34rem,calc(100vw-3rem))] overflow-hidden rounded-xl border border-border/80 bg-card shadow-2xl shadow-black/25"
                  id={projectMenuId}
                  role="listbox"
                >
                  <ScrollArea className="max-h-80">
                    <div className="flex flex-col p-1.5">
                      {projectOptions.length === 0 ? (
                        <div className="px-3 py-3 text-sm text-muted-foreground">
                          Projects unavailable
                        </div>
                      ) : (
                        projectOptions.map((option) => (
                          <button
                            className={cn(
                              "grid grid-cols-[auto_1fr] items-start gap-3 rounded-lg px-3 py-2.5 text-left transition hover:bg-muted/50",
                              option.projectKey === selectedProjectKey && "bg-muted text-foreground",
                            )}
                            key={option.projectKey}
                            onClick={() => {
                              onProjectChange(option.projectKey);
                              setProjectMenuOpen(false);
                            }}
                            role="option"
                            type="button"
                          >
                            <span className="pt-0.5 text-[color:var(--accent-strong)]">
                              {option.projectKey === selectedProjectKey ? <Check className="size-4" /> : <span className="block size-4" />}
                            </span>
                            <span className="min-w-0">
                              <span className="block truncate text-sm font-medium text-foreground">
                                {option.label}
                              </span>
                              <span className="mt-0.5 block break-all text-xs leading-5 text-muted-foreground">
                                {option.description}
                              </span>
                            </span>
                          </button>
                        ))
                      )}
                    </div>
                  </ScrollArea>
                </div>
              ) : null}
            </div>

            <div className="flex flex-col gap-1.5" ref={windowMenuRef}>
              <span className={FILTER_META_CLASS}>Window</span>
              <button
                aria-controls={windowMenuId}
                aria-expanded={windowMenuOpen}
                className={FILTER_CONTROL_CLASS}
                onClick={() => {
                  setWindowMenuOpen((current) => !current);
                  setProjectMenuOpen(false);
                }}
                type="button"
              >
                <span className="font-medium">{selectedWindowLabel}</span>
                <ChevronDown className={cn("size-4 shrink-0 text-muted-foreground transition", windowMenuOpen && "rotate-180")} />
              </button>
              {windowMenuOpen ? (
                <div
                  className="absolute z-20 mt-[4.75rem] w-48 overflow-hidden rounded-xl border border-border/80 bg-card shadow-2xl shadow-black/25"
                  id={windowMenuId}
                  role="listbox"
                >
                  <div className="flex flex-col p-1.5">
                    {(["7d", "30d", "90d", "all", "custom"] as ProjectMetricsRangePreset[]).map((preset) => (
                      <button
                        className={cn(
                          "flex items-center justify-between rounded-lg px-3 py-2 text-left text-sm transition hover:bg-muted/50",
                          range.preset === preset && "bg-muted text-foreground",
                        )}
                        key={preset}
                        onClick={() => {
                          onRangeChange({
                            ...range,
                            preset,
                          });
                          setWindowMenuOpen(false);
                        }}
                        role="option"
                        type="button"
                      >
                        <span>{describeRangePreset(preset)}</span>
                        {range.preset === preset ? <Check className="size-4 text-[color:var(--accent-strong)]" /> : null}
                      </button>
                    ))}
                  </div>
                </div>
              ) : null}
            </div>

            <label className="flex min-h-10 items-center gap-3 rounded-xl border border-border/70 bg-background px-3 text-sm text-foreground">
              <input
                checked={includeSpawnAgents}
                className="size-4 rounded border-border bg-background accent-[color:var(--accent-strong)]"
                onChange={(event) => onIncludeSpawnAgentsChange(event.target.checked)}
                type="checkbox"
              />
              <span className="font-medium">Include spawn agents</span>
            </label>
          </div>

          {range.preset === "custom" ? (
            <div className="grid gap-3 md:grid-cols-2">
              <label className="flex flex-col gap-1.5">
                <span className={FILTER_META_CLASS}>Start</span>
                <input
                  className="h-10 rounded-xl border border-border/70 bg-background px-3 text-sm text-foreground transition hover:border-border focus:border-border"
                  onChange={(event) => onRangeChange({ ...range, start: event.target.value })}
                  type="datetime-local"
                  value={range.start}
                />
              </label>
              <label className="flex flex-col gap-1.5">
                <span className={FILTER_META_CLASS}>End</span>
                <input
                  className="h-10 rounded-xl border border-border/70 bg-background px-3 text-sm text-foreground transition hover:border-border focus:border-border"
                  onChange={(event) => onRangeChange({ ...range, end: event.target.value })}
                  type="datetime-local"
                  value={range.end}
                />
              </label>
            </div>
          ) : null}

          {selectedProject ? (
            <div className="flex flex-wrap items-center gap-2 rounded-xl border border-border/60 bg-muted/20 px-3 py-2 text-sm text-foreground">
              <FolderGit2 className="size-4 text-muted-foreground" />
              <span className="min-w-0 flex-1 break-all leading-6 text-muted-foreground">
                {selectedProject.description}
              </span>
              <CoveragePill coverage={selectedProject.state === "degraded" ? "unknown" : "known"} />
              {catalogBusy ? (
                <span className="text-xs font-medium text-muted-foreground">
                  Scanning full session catalog…
                </span>
              ) : null}
            </div>
          ) : null}
        </CardHeader>

        <CardContent className="min-h-0 flex-1 pt-4">
          {error ? (
            <Alert className="mb-4" variant="destructive">
              <AlertTriangle className="size-4" />
              <AlertTitle>Project metrics query failed</AlertTitle>
              <AlertDescription className="ui-selectable">{error}</AlertDescription>
            </Alert>
          ) : null}

          {loading ? (
            <Alert>
              <RefreshCcw className="size-4 animate-spin" />
              <AlertTitle>Project metrics loading</AlertTitle>
              <AlertDescription>Запрос активен, предыдущие ответы не перетрут новый выбор.</AlertDescription>
            </Alert>
          ) : null}

          {!selectedProjectKey && !loading ? (
            <Alert>
              <Gauge className="size-4" />
              <AlertTitle>Project not selected</AlertTitle>
              <AlertDescription>Выберите проект из session catalog, чтобы загрузить хронологию.</AlertDescription>
            </Alert>
          ) : null}

          {selectedProjectKey && !loading && !error && metrics && metrics.sessions.length === 0 ? (
            <Alert>
              <Gauge className="size-4" />
              <AlertTitle>No sessions for selected range</AlertTitle>
              <AlertDescription>Для выбранного окна данных нет. Измените range или выберите другой проект.</AlertDescription>
            </Alert>
          ) : null}

          {viewModel && metrics && metrics.sessions.length > 0 ? (
            <div className="grid min-h-0 gap-4 xl:grid-cols-[minmax(0,1.5fr)_minmax(290px,0.85fr)]">
              <div className="flex min-h-0 flex-col gap-4">
                {viewModel.degraded ? (
                  <Alert>
                    <AlertTriangle className="size-4" />
                    <AlertTitle>Degraded project identity</AlertTitle>
                    <AlertDescription>
                      Часть project identity неполная. Метрики показаны честно, но bucket нельзя считать полным project catalog.
                    </AlertDescription>
                  </Alert>
                ) : null}

                <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
                  {viewModel.summaryCards.map((card) => (
                    <Card className={SURFACE_CARD_CLASS} key={card.label} size="sm">
                      <CardContent className="py-1">
                        <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
                          {card.label}
                        </div>
                        <div
                          className={cn(
                            "mt-2 text-xl font-semibold tracking-[-0.03em]",
                            card.tone === "accent" && "text-[color:var(--accent-strong)]",
                            card.tone === "danger" && "text-rose-300",
                          )}
                        >
                          {card.value}
                        </div>
                      </CardContent>
                    </Card>
                  ))}
                </div>

                <div className="flex flex-wrap gap-2 text-xs text-muted-foreground">
                  <CoveragePill coverage="known" />
                  <CoveragePill coverage="partial" />
                  <CoveragePill coverage="unknown" />
                </div>

                <div className="grid gap-4">
                  {viewModel.charts.map((chart) => (
                    <MetricChart
                      chart={chart}
                      currentSessionId={currentSessionId}
                      key={chart.key}
                      onOpenSession={onOpenSession}
                    />
                  ))}
                </div>
              </div>

              <Card className={cn(SURFACE_CARD_CLASS, "min-h-0")} size="sm">
                <CardHeader className="gap-2">
                  <CardTitle className="text-sm">Contributing sessions</CardTitle>
                </CardHeader>
                <CardContent className="min-h-0 pt-0">
                  <ScrollArea className="h-[min(58vh,48rem)]">
                    <div className="flex flex-col gap-2 pr-3">
                      {viewModel.sessions.map((session) => (
                        <button
                          className={cn(
                            "rounded-lg border border-border/70 bg-background/70 px-3 py-3 text-left transition hover:border-border hover:bg-muted/40",
                            session.sessionId === currentSessionId && "border-foreground/20 bg-muted/50",
                          )}
                          key={session.sessionId}
                          onClick={() => onOpenSession(session.sessionId)}
                          type="button"
                        >
                          <div className="flex items-center justify-between gap-2">
                            <span className="font-mono text-xs text-foreground">{session.sessionId.slice(0, 8)}</span>
                            <CoveragePill coverage={session.coverage} />
                          </div>
                          <div className="mt-2 text-xs text-muted-foreground">
                            {session.startedAt ?? "n/a"} · {session.outcome}
                          </div>
                          <dl className="mt-3 grid grid-cols-2 gap-2 text-xs">
                            <MetaItem label="Duration" value={session.duration} />
                            <MetaItem label="Tokens" value={session.tokens} />
                            <MetaItem label="Tool calls" value={session.toolCalls} />
                            <MetaItem label="Failures" value={session.failures} />
                          </dl>
                        </button>
                      ))}
                    </div>
                  </ScrollArea>
                </CardContent>
              </Card>
            </div>
          ) : null}
        </CardContent>
      </Card>
    </div>
  );
}

function describeRangePreset(preset: ProjectMetricsRangePreset) {
  switch (preset) {
    case "7d":
      return "Last 7 days";
    case "30d":
      return "Last 30 days";
    case "90d":
      return "Last 90 days";
    case "all":
      return "All time";
    case "custom":
      return "Custom range";
    default:
      return preset;
  }
}

function MetricChart({
  chart,
  currentSessionId,
  onOpenSession,
}: {
  chart: ProjectMetricChart;
  currentSessionId: string | null;
  onOpenSession: (sessionId: string) => void;
}) {
  const width = 640;
  const height = 180;
  const paddingX = 20;
  const paddingTop = 18;
  const paddingBottom = 32;
  const usableHeight = height - paddingTop - paddingBottom;
  const knownValues = chart.points
    .map((point) => point.value)
    .filter((value): value is number => value != null);
  const maxValue = knownValues.length > 0 ? Math.max(...knownValues, 1) : 1;
  const stepX = chart.points.length > 1 ? (width - paddingX * 2) / (chart.points.length - 1) : 0;

  let path = "";
  chart.points.forEach((point, index) => {
    if (point.value == null) {
      return;
    }
    const x = paddingX + stepX * index;
    const y = paddingTop + (1 - point.value / maxValue) * usableHeight;
    path += path ? ` L ${x} ${y}` : `M ${x} ${y}`;
  });

  return (
    <Card className={SURFACE_CARD_CLASS} size="sm">
      <CardHeader className="gap-2">
        <CardTitle className="text-sm">{chart.label}</CardTitle>
      </CardHeader>
      <CardContent className="pt-0">
        <svg className="w-full overflow-visible" role="img" viewBox={`0 0 ${width} ${height}`}>
          <title>{chart.label}</title>
          <line
            stroke="currentColor"
            strokeDasharray="4 4"
            strokeOpacity="0.12"
            x1={paddingX}
            x2={width - paddingX}
            y1={height - paddingBottom}
            y2={height - paddingBottom}
          />
          {path ? (
            <path
              d={path}
              fill="none"
              stroke="currentColor"
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeOpacity="0.7"
              strokeWidth="2"
            />
          ) : null}
          {chart.points.map((point, index) => {
            const x = paddingX + stepX * index;
            const y = point.value == null
              ? height - paddingBottom + 8
              : paddingTop + (1 - point.value / maxValue) * usableHeight;
            const selected = point.sessionId === currentSessionId;
            return (
              <g key={`${chart.key}-${point.sessionId}`}>
                {point.value == null ? (
                  <rect
                    fill="currentColor"
                    fillOpacity={selected ? 0.9 : 0.45}
                    height="8"
                    rx="1.5"
                    transform={`rotate(45 ${x} ${y})`}
                    width="8"
                    x={x - 4}
                    y={y - 4}
                  >
                    <title>{`${point.label}: ${point.formattedValue}`}</title>
                  </rect>
                ) : (
                  <circle
                    cx={x}
                    cy={y}
                    fill={point.coverage === "partial" ? "var(--canvas)" : "currentColor"}
                    fillOpacity={selected ? 1 : 0.9}
                    r={selected ? 5 : 4}
                    stroke="currentColor"
                    strokeWidth={point.coverage === "partial" ? 2 : 0}
                  >
                    <title>{`${point.label}: ${point.formattedValue}`}</title>
                  </circle>
                )}
                <circle
                  className="cursor-pointer"
                  cx={x}
                  cy={point.value == null ? height - paddingBottom + 8 : y}
                  fill="transparent"
                  onClick={() => onOpenSession(point.sessionId)}
                  r="10"
                />
              </g>
            );
          })}
        </svg>
        <div className="mt-2 flex items-center justify-between gap-2 text-xs text-muted-foreground">
          <span>{chart.valueLabel}</span>
          <span>{chart.points.length} sessions</span>
        </div>
      </CardContent>
    </Card>
  );
}

function CoveragePill({ coverage }: { coverage: "known" | "partial" | "unknown" }) {
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-full border px-2 py-0.5 text-[11px] uppercase tracking-[0.14em]",
        coverage === "known" && "border-emerald-200/80 text-emerald-700",
        coverage === "partial" && "border-amber-200/80 text-amber-700",
        coverage === "unknown" && "border-rose-200/80 text-rose-700",
      )}
    >
      {coverage}
    </span>
  );
}

function MetaItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md border border-border/60 bg-background/50 px-2 py-2">
      <div className="text-[10px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
        {label}
      </div>
      <div className="mt-1 text-foreground">{value}</div>
    </div>
  );
}

export { createInitialProjectMetricsRange };

import { useEffect, useId, useRef, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import {
  AlertTriangle,
  BarChart3,
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  FolderGit2,
  Gauge,
  RefreshCcw,
  Search,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import {
  Brush,
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import type {
  DotProps,
  TooltipContentProps,
} from "recharts";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import {
  buildProjectMetricsViewModel,
  createInitialProjectMetricsRange,
  getProjectMetricPoint,
  type ProjectMetricSeries,
  type ProjectMetricSeriesKey,
  type ProjectMetricsChartRow,
  type ProjectMetricsRangePreset,
  type ProjectMetricsRangeSelection,
  type ProjectMetricsViewModel,
  type ProjectSelectorOption,
} from "@/components/project-metrics";
import type { ProjectMetricsResponse } from "@/backend";

const SURFACE_CARD_CLASS = "border-border bg-background shadow-none";
const FILTER_CONTROL_CLASS =
  "flex h-10 w-full items-center justify-between rounded-xl border border-border/70 bg-background px-3 text-sm text-foreground transition hover:border-border hover:bg-muted/30";
const FILTER_META_CLASS = "text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground";
const ZOOM_BUTTON_CLASS =
  "rounded-full border border-border/70 px-3 py-1.5 text-xs font-medium text-foreground transition hover:border-border hover:bg-muted/40 disabled:cursor-not-allowed disabled:opacity-50";
const ZOOM_BUTTON_ACTIVE_CLASS = "border-foreground/20 bg-muted/50";

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
  const metricsResetKey = metrics
    ? `${metrics.project_key}:${metrics.sessions.length}:${includeSpawnAgents ? "with-spawn" : "without-spawn"}`
    : "empty";
  const selectedProject = projectOptions.find((option) => option.projectKey === selectedProjectKey) ?? null;
  const [projectMenuOpen, setProjectMenuOpen] = useState(false);
  const [windowMenuOpen, setWindowMenuOpen] = useState(false);
  const [visibleSeriesKeys, setVisibleSeriesKeys] = useState<ProjectMetricSeriesKey[]>([]);
  const [zoomWindow, setZoomWindow] = useState<{ startIndex: number; endIndex: number }>({
    startIndex: 0,
    endIndex: 0,
  });
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
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

  useEffect(() => {
    if (!viewModel) {
      setVisibleSeriesKeys([]);
      setZoomWindow({ startIndex: 0, endIndex: 0 });
      setActiveSessionId(null);
      return;
    }

    setVisibleSeriesKeys(viewModel.defaultVisibleSeriesKeys);
    setZoomWindow(viewModel.initialZoomWindow);
    setActiveSessionId(currentSessionId ?? viewModel.chartRows.at(-1)?.sessionId ?? null);
  }, [currentSessionId, metricsResetKey]);

  useEffect(() => {
    if (!viewModel || !currentSessionId) {
      return;
    }
    const exists = viewModel.chartRows.some((row) => row.sessionId === currentSessionId);
    if (exists) {
      setActiveSessionId(currentSessionId);
    }
  }, [currentSessionId, metricsResetKey]);

  const selectedWindowLabel = describeRangePreset(range.preset);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <Card className={cn(SURFACE_CARD_CLASS, "min-h-0 w-full flex-1")}>
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
            <ProjectMetricsContent
              activeSessionId={activeSessionId}
              currentSessionId={currentSessionId}
              onActiveSessionIdChange={setActiveSessionId}
              onOpenSession={onOpenSession}
              setVisibleSeriesKeys={setVisibleSeriesKeys}
              setZoomWindow={setZoomWindow}
              viewModel={viewModel}
              visibleSeriesKeys={visibleSeriesKeys}
              zoomWindow={zoomWindow}
            />
          ) : null}
        </CardContent>
      </Card>
    </div>
  );
}

function ProjectMetricsContent({
  activeSessionId,
  currentSessionId,
  onActiveSessionIdChange,
  onOpenSession,
  setVisibleSeriesKeys,
  setZoomWindow,
  viewModel,
  visibleSeriesKeys,
  zoomWindow,
}: {
  activeSessionId: string | null;
  currentSessionId: string | null;
  onActiveSessionIdChange: (value: string | null) => void;
  onOpenSession: (sessionId: string) => void;
  setVisibleSeriesKeys: Dispatch<SetStateAction<ProjectMetricSeriesKey[]>>;
  setZoomWindow: Dispatch<SetStateAction<{ startIndex: number; endIndex: number }>>;
  viewModel: ProjectMetricsViewModel;
  visibleSeriesKeys: ProjectMetricSeriesKey[];
  zoomWindow: { startIndex: number; endIndex: number };
}) {
  const clampedWindow = clampZoomWindow(zoomWindow, viewModel.chartRows.length);
  const visibleRows = viewModel.chartRows.slice(clampedWindow.startIndex, clampedWindow.endIndex + 1);
  const visibleSeries = viewModel.chartSeries.filter((series) => visibleSeriesKeys.includes(series.key));
  const activeRow = findActiveRow({
    activeSessionId,
    currentSessionId,
    rows: visibleRows,
  });
  const selectedSessionItem = activeRow
    ? viewModel.sessions.find((session) => session.sessionId === activeRow.sessionId) ?? null
    : null;
  const windowSize = getWindowSize(clampedWindow);
  const focusIndex = activeRow?.index ?? getWindowCenterIndex(clampedWindow);
  const quickWindowSizes = [8, 16, 32].filter((size) => size < viewModel.chartRows.length);
  const windowSummary = `Sessions ${clampedWindow.startIndex + 1}-${clampedWindow.endIndex + 1} of ${viewModel.chartRows.length}`;
  const windowLabelSummary = visibleRows.length > 0
    ? `${visibleRows[0]?.label} → ${visibleRows.at(-1)?.label}`
    : "n/a";
  const hasAvailableVisibleData = visibleRows.some((row) =>
    visibleSeries.some((series) => {
      const point = getProjectMetricPoint(row, series.key);
      return point.value != null || point.coverage !== "unknown";
    }),
  );

  return (
    <div className="grid min-h-0 gap-4 xl:grid-cols-[minmax(0,1.65fr)_minmax(300px,0.85fr)]">
      <ScrollArea className="h-[min(70vh,56rem)] min-h-0">
        <div className="flex min-h-0 flex-col gap-4 pr-3">
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

          <Card className={SURFACE_CARD_CLASS} size="sm">
            <CardHeader className="gap-3">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="space-y-1">
                  <CardTitle className="text-sm">Summary chart</CardTitle>
                  <p className="text-sm text-muted-foreground">
                    Готовый chart stack на `Recharts`: hover details, Brush zoom, curated multi-series overlays.
                  </p>
                </div>
                <div className="flex flex-wrap gap-2 text-xs text-muted-foreground">
                  <CoveragePill coverage="known" />
                  <CoveragePill coverage="partial" />
                  <CoveragePill coverage="unknown" />
                </div>
              </div>

              <div className="grid gap-2 md:grid-cols-2">
                {viewModel.chartSeries.map((series) => {
                  const selected = visibleSeriesKeys.includes(series.key);
                  const disabled = series.availablePoints === 0;
                  return (
                    <button
                      aria-pressed={selected}
                      className={cn(
                        "rounded-xl border px-3 py-3 text-left transition",
                        selected
                          ? "border-foreground/20 bg-muted/50"
                          : "border-border/70 bg-background hover:border-border hover:bg-muted/30",
                        disabled && "opacity-70",
                      )}
                      key={series.key}
                      onClick={() => {
                        setVisibleSeriesKeys((current) => toggleSeries(current, series.key));
                      }}
                      type="button"
                    >
                      <div className="flex items-start justify-between gap-3">
                        <span className="space-y-1">
                          <span className="flex items-center gap-2 text-sm font-medium text-foreground">
                            <span
                              aria-hidden="true"
                              className="size-2.5 rounded-full"
                              style={{ backgroundColor: series.color }}
                            />
                            {series.label}
                          </span>
                          <span className="block text-xs leading-5 text-muted-foreground">
                            {series.description}
                          </span>
                        </span>
                        <span className="rounded-full border border-border/70 px-2 py-0.5 text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
                          {series.category}
                        </span>
                      </div>
                      <div className="mt-3 flex flex-wrap gap-2 text-xs text-muted-foreground">
                        <span>{series.availablePoints}/{viewModel.chartRows.length} ready</span>
                        {series.partialPoints > 0 ? <span>{series.partialPoints} partial</span> : null}
                        {series.unknownPoints > 0 ? <span>{series.unknownPoints} unknown</span> : null}
                      </div>
                    </button>
                  );
                })}
              </div>

              <div className="flex flex-wrap items-center gap-2">
                <span className="inline-flex items-center gap-2 text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">
                  <ZoomIn className="size-3.5" />
                  Zoom
                </span>
                <button
                  className={cn(ZOOM_BUTTON_CLASS, getWindowSize(clampedWindow) === viewModel.chartRows.length && ZOOM_BUTTON_ACTIVE_CLASS)}
                  onClick={() => {
                    setZoomWindow(clampZoomWindow({ startIndex: 0, endIndex: viewModel.chartRows.length - 1 }, viewModel.chartRows.length));
                  }}
                  type="button"
                >
                  All
                </button>
                <button
                  className={ZOOM_BUTTON_CLASS}
                  disabled={windowSize <= 2}
                  onClick={() => {
                    setZoomWindow(zoomWindowIn(clampedWindow, viewModel.chartRows.length, focusIndex));
                  }}
                  type="button"
                >
                  <span className="inline-flex items-center gap-1.5">
                    <ZoomIn className="size-3.5" />
                    Zoom in
                  </span>
                </button>
                <button
                  className={ZOOM_BUTTON_CLASS}
                  disabled={windowSize >= viewModel.chartRows.length}
                  onClick={() => {
                    setZoomWindow(zoomWindowOut(clampedWindow, viewModel.chartRows.length, focusIndex));
                  }}
                  type="button"
                >
                  <span className="inline-flex items-center gap-1.5">
                    <ZoomOut className="size-3.5" />
                    Zoom out
                  </span>
                </button>
                <button
                  className={ZOOM_BUTTON_CLASS}
                  disabled={clampedWindow.startIndex === 0}
                  onClick={() => {
                    setZoomWindow(shiftZoomWindow(clampedWindow, viewModel.chartRows.length, -Math.max(1, Math.floor(windowSize / 2))));
                  }}
                  type="button"
                >
                  <span className="inline-flex items-center gap-1.5">
                    <ChevronLeft className="size-3.5" />
                    Earlier
                  </span>
                </button>
                <button
                  className={ZOOM_BUTTON_CLASS}
                  disabled={clampedWindow.endIndex >= viewModel.chartRows.length - 1}
                  onClick={() => {
                    setZoomWindow(shiftZoomWindow(clampedWindow, viewModel.chartRows.length, Math.max(1, Math.floor(windowSize / 2))));
                  }}
                  type="button"
                >
                  <span className="inline-flex items-center gap-1.5">
                    Later
                    <ChevronRight className="size-3.5" />
                  </span>
                </button>
                {windowSize < viewModel.chartRows.length ? (
                  <button
                    className={ZOOM_BUTTON_CLASS}
                    onClick={() => {
                      setZoomWindow(focusRecentWindow(viewModel.chartRows.length, windowSize));
                    }}
                    type="button"
                  >
                    Latest
                  </button>
                ) : null}
                {quickWindowSizes.map((size) => (
                  <button
                    aria-label={`Show ${size} sessions`}
                    className={cn(ZOOM_BUTTON_CLASS, windowSize === size && ZOOM_BUTTON_ACTIVE_CLASS)}
                    key={size}
                    onClick={() => {
                      setZoomWindow(focusWindowAroundIndex(viewModel.chartRows.length, size, focusIndex));
                    }}
                    type="button"
                  >
                    {size}
                  </button>
                ))}
              </div>

              <div className="flex flex-wrap items-center gap-2 rounded-xl border border-border/70 bg-muted/15 px-3 py-2 text-xs text-muted-foreground">
                <span className="font-medium text-foreground">{windowSummary}</span>
                <span>·</span>
                <span>{windowLabelSummary}</span>
                <span className="ml-auto">Showing {visibleRows.length} of {viewModel.chartRows.length} sessions</span>
              </div>

              <div className="rounded-xl border border-border/70 bg-muted/15 px-3 py-2 text-xs text-muted-foreground">
                Hollow points = `partial`, gaps in the line = `unknown`. Use buttons for coarse zoom/pan and `Brush` for fine adjustment.
              </div>
            </CardHeader>
            <CardContent className="space-y-4 pt-0">
              {visibleSeries.length === 0 ? (
                <Alert>
                  <Search className="size-4" />
                  <AlertTitle>No series selected</AlertTitle>
                  <AlertDescription>Включите хотя бы одну curated series, чтобы построить summary chart.</AlertDescription>
                </Alert>
              ) : !hasAvailableVisibleData ? (
                <Alert>
                  <AlertTriangle className="size-4" />
                  <AlertTitle>Visible window has no chartable values</AlertTitle>
                  <AlertDescription>
                    В текущем zoom-window у выбранных series только `unknown` значения. Расширьте окно или включите другие метрики.
                  </AlertDescription>
                </Alert>
              ) : (
                <>
                  <div className="h-[24rem] w-full">
                    <ResponsiveContainer width="100%" height="100%">
                      <LineChart
                        data={viewModel.chartRows}
                        margin={{ top: 12, right: 24, bottom: 20, left: 8 }}
                        onMouseMove={(state) => {
                          const row = extractChartRow(
                            (state as { activePayload?: unknown }).activePayload,
                          );
                          if (row) {
                            onActiveSessionIdChange(row.sessionId);
                          }
                        }}
                      >
                        <CartesianGrid stroke="currentColor" strokeDasharray="4 6" strokeOpacity={0.12} />
                        <XAxis
                          dataKey="label"
                          minTickGap={24}
                          stroke="currentColor"
                          strokeOpacity={0.45}
                          tick={{ fill: "currentColor", fontSize: 12 }}
                        />
                        <YAxis
                          hide
                          yAxisId="preview"
                          domain={computeSeriesDomain(viewModel.chartRows, visibleSeriesKeys)}
                        />
                        {visibleSeries.map((series) => (
                          <YAxis
                            domain={computeSeriesDomain(viewModel.chartRows, [series.key])}
                            hide
                            key={series.key}
                            yAxisId={series.key}
                          />
                        ))}
                        <Tooltip
                          content={(props) => (
                            <SummaryChartTooltip
                              {...props}
                              series={visibleSeries}
                            />
                          )}
                        />
                        {visibleSeries.map((series) => (
                          <Line
                            activeDot={{ r: 6, strokeWidth: 2 }}
                            connectNulls={false}
                            dataKey={(row: ProjectMetricsChartRow) => getProjectMetricPoint(row, series.key).value}
                            dot={
                              <SeriesDot
                                currentSessionId={currentSessionId}
                                onActivateSession={onActiveSessionIdChange}
                                onOpenSession={onOpenSession}
                                seriesKey={series.key}
                              />
                            }
                            isAnimationActive={false}
                            key={series.key}
                            name={series.label}
                            stroke={series.color}
                            strokeWidth={2.2}
                            type="monotone"
                            yAxisId={series.key}
                          />
                        ))}
                        <Brush
                          dataKey="label"
                          endIndex={clampedWindow.endIndex}
                          fill="color-mix(in srgb, var(--background) 88%, transparent)"
                          height={26}
                          onChange={(nextWindow) => {
                            if (
                              typeof nextWindow?.startIndex !== "number"
                              || typeof nextWindow?.endIndex !== "number"
                            ) {
                              return;
                            }
                            setZoomWindow(clampZoomWindow(nextWindow, viewModel.chartRows.length));
                          }}
                          startIndex={clampedWindow.startIndex}
                          stroke="color-mix(in srgb, currentColor 35%, transparent)"
                          travellerWidth={10}
                        />
                      </LineChart>
                    </ResponsiveContainer>
                  </div>

                  {activeRow ? (
                    <ActivePointPanel
                      activeRow={activeRow}
                      onOpenSession={onOpenSession}
                      selectedSessionItem={selectedSessionItem}
                      series={visibleSeries}
                    />
                  ) : (
                    <Alert>
                      <Search className="size-4" />
                      <AlertTitle>No active point</AlertTitle>
                      <AlertDescription>Наведите курсор на chart point или кликните по session row справа.</AlertDescription>
                    </Alert>
                  )}
                </>
              )}
            </CardContent>
          </Card>
        </div>
      </ScrollArea>

      <Card className={cn(SURFACE_CARD_CLASS, "min-h-0")} size="sm">
        <CardHeader className="gap-2">
          <CardTitle className="text-sm">Contributing sessions</CardTitle>
        </CardHeader>
        <CardContent className="min-h-0 pt-0">
          <ScrollArea className="h-[min(70vh,56rem)]">
            <div className="flex flex-col gap-2 pr-3">
              {viewModel.sessions.map((session) => (
                <button
                  className={cn(
                    "rounded-lg border border-border/70 bg-background/70 px-3 py-3 text-left transition hover:border-border hover:bg-muted/40",
                    session.sessionId === currentSessionId && "border-foreground/20 bg-muted/50",
                    session.sessionId === activeRow?.sessionId && "ring-1 ring-foreground/15",
                  )}
                  key={session.sessionId}
                  onClick={() => {
                    onActiveSessionIdChange(session.sessionId);
                    onOpenSession(session.sessionId);
                  }}
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
  );
}

function SummaryChartTooltip({
  active,
  payload,
  series,
}: TooltipContentProps<any, any> & {
  series: ProjectMetricSeries[];
}) {
  const row = extractChartRow(payload);
  if (!active || !row) {
    return null;
  }

  return (
    <div className="w-72 rounded-xl border border-border/80 bg-card/95 p-3 shadow-2xl shadow-black/20 backdrop-blur">
      <div className="flex items-center justify-between gap-2">
        <div>
          <div className="text-sm font-medium text-foreground">{row.label}</div>
          <div className="font-mono text-[11px] text-muted-foreground">{row.sessionId}</div>
        </div>
        <CoveragePill coverage={dominantCoverage(row, series)} />
      </div>
      <div className="mt-3 space-y-2">
        {series.map((item) => {
          const point = getProjectMetricPoint(row, item.key);
          return (
            <div className="flex items-center justify-between gap-3 text-xs" key={item.key}>
              <span className="flex items-center gap-2 text-muted-foreground">
                <span className="size-2 rounded-full" style={{ backgroundColor: item.color }} />
                {item.shortLabel}
              </span>
              <span className="text-right text-foreground">
                {point.formattedValue} <span className="text-muted-foreground">· {point.coverage}</span>
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function ActivePointPanel({
  activeRow,
  onOpenSession,
  selectedSessionItem,
  series,
}: {
  activeRow: ProjectMetricsChartRow;
  onOpenSession: (sessionId: string) => void;
  selectedSessionItem: ProjectMetricsViewModel["sessions"][number] | null;
  series: ProjectMetricSeries[];
}) {
  return (
    <div className="grid gap-4 rounded-2xl border border-border/70 bg-background/70 p-4 lg:grid-cols-[minmax(0,1.3fr)_minmax(220px,0.7fr)]">
      <div className="space-y-3">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="space-y-1">
            <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
              Active point
            </div>
            <div className="text-base font-semibold text-foreground">{activeRow.label}</div>
            <div className="font-mono text-xs text-muted-foreground">{activeRow.sessionId}</div>
          </div>
          <Button
            onClick={() => onOpenSession(activeRow.sessionId)}
            size="sm"
            type="button"
            variant="outline"
          >
            Open session
          </Button>
        </div>

        <div className="grid gap-2 sm:grid-cols-2">
          {series.map((item) => {
            const point = getProjectMetricPoint(activeRow, item.key);
            return (
              <div className="rounded-xl border border-border/70 bg-muted/20 p-3" key={item.key}>
                <div className="flex items-center justify-between gap-2">
                  <span className="flex items-center gap-2 text-sm font-medium text-foreground">
                    <span className="size-2.5 rounded-full" style={{ backgroundColor: item.color }} />
                    {item.shortLabel}
                  </span>
                  <CoveragePill coverage={point.coverage} />
                </div>
                <div className="mt-2 text-lg font-semibold tracking-[-0.03em] text-foreground">
                  {point.formattedValue}
                </div>
                <div className="mt-1 text-xs text-muted-foreground">{item.valueLabel}</div>
              </div>
            );
          })}
        </div>
      </div>

      <div className="rounded-xl border border-border/70 bg-muted/10 p-3">
        <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
          Session context
        </div>
        {selectedSessionItem ? (
          <dl className="mt-3 grid grid-cols-2 gap-2 text-xs">
            <MetaItem label="Outcome" value={selectedSessionItem.outcome} />
            <MetaItem label="Duration" value={selectedSessionItem.duration} />
            <MetaItem label="Tokens" value={selectedSessionItem.tokens} />
            <MetaItem label="Tool calls" value={selectedSessionItem.toolCalls} />
            <MetaItem label="Failures" value={selectedSessionItem.failures} />
            <MetaItem label="Project state" value={selectedSessionItem.projectState} />
          </dl>
        ) : (
          <p className="mt-3 text-sm text-muted-foreground">Session summary unavailable.</p>
        )}
      </div>
    </div>
  );
}

function SeriesDot({
  currentSessionId,
  onActivateSession,
  onOpenSession,
  seriesKey,
  ...props
}: DotProps & {
  currentSessionId: string | null;
  onActivateSession: (value: string) => void;
  onOpenSession: (sessionId: string) => void;
  payload?: ProjectMetricsChartRow;
  seriesKey: ProjectMetricSeriesKey;
}) {
  const row = props.payload as ProjectMetricsChartRow | undefined;
  if (!row || typeof props.cx !== "number" || typeof props.cy !== "number") {
    return null;
  }

  const point = getProjectMetricPoint(row, seriesKey);
  if (point.value == null) {
    return null;
  }

  const selected = row.sessionId === currentSessionId;
  const partial = point.coverage === "partial";

  return (
    <circle
      className="cursor-pointer"
      cx={props.cx}
      cy={props.cy}
      data-session-id={row.sessionId}
      fill={partial ? "var(--background)" : props.stroke ?? "currentColor"}
      fillOpacity={selected ? 1 : 0.92}
      onClick={() => {
        onActivateSession(row.sessionId);
        onOpenSession(row.sessionId);
      }}
      onMouseEnter={() => onActivateSession(row.sessionId)}
      r={selected ? 5.5 : 4}
      stroke={props.stroke ?? "currentColor"}
      strokeWidth={partial ? 2.4 : 0}
    />
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

function toggleSeries(
  current: ProjectMetricSeriesKey[],
  key: ProjectMetricSeriesKey,
): ProjectMetricSeriesKey[] {
  if (current.includes(key)) {
    return current.filter((item) => item !== key);
  }
  return [...current, key];
}

function clampZoomWindow(
  window: { startIndex: number; endIndex: number },
  totalRows: number,
) {
  if (totalRows <= 0) {
    return { startIndex: 0, endIndex: 0 };
  }
  const upperBound = totalRows - 1;
  const startIndex = Math.max(0, Math.min(window.startIndex, upperBound));
  const endIndex = Math.max(startIndex, Math.min(window.endIndex, upperBound));
  return { startIndex, endIndex };
}

function getWindowSize(window: { startIndex: number; endIndex: number }) {
  return Math.max(0, window.endIndex - window.startIndex + 1);
}

function getWindowCenterIndex(window: { startIndex: number; endIndex: number }) {
  return window.startIndex + Math.floor((getWindowSize(window) - 1) / 2);
}

function focusWindowAroundIndex(totalRows: number, size: number, focusIndex: number) {
  if (totalRows <= 0) {
    return { startIndex: 0, endIndex: 0 };
  }

  const boundedSize = Math.max(1, Math.min(size, totalRows));
  const boundedFocus = Math.max(0, Math.min(focusIndex, totalRows - 1));
  const half = Math.floor((boundedSize - 1) / 2);
  let startIndex = boundedFocus - half;
  let endIndex = startIndex + boundedSize - 1;

  if (startIndex < 0) {
    startIndex = 0;
    endIndex = boundedSize - 1;
  }
  if (endIndex >= totalRows) {
    endIndex = totalRows - 1;
    startIndex = Math.max(0, endIndex - boundedSize + 1);
  }

  return { startIndex, endIndex };
}

function shiftZoomWindow(
  window: { startIndex: number; endIndex: number },
  totalRows: number,
  delta: number,
) {
  if (totalRows <= 0) {
    return { startIndex: 0, endIndex: 0 };
  }

  const size = Math.max(1, Math.min(getWindowSize(window), totalRows));
  const maxStart = Math.max(0, totalRows - size);
  const startIndex = Math.max(0, Math.min(window.startIndex + delta, maxStart));
  return {
    startIndex,
    endIndex: Math.min(totalRows - 1, startIndex + size - 1),
  };
}

function zoomWindowIn(
  window: { startIndex: number; endIndex: number },
  totalRows: number,
  focusIndex: number,
) {
  const currentSize = getWindowSize(window);
  if (currentSize <= 2) {
    return clampZoomWindow(window, totalRows);
  }

  const nextSize = Math.max(2, Math.floor(currentSize / 2));
  return focusWindowAroundIndex(totalRows, nextSize, focusIndex);
}

function zoomWindowOut(
  window: { startIndex: number; endIndex: number },
  totalRows: number,
  focusIndex: number,
) {
  const currentSize = getWindowSize(window);
  if (currentSize >= totalRows) {
    return clampZoomWindow(window, totalRows);
  }

  const nextSize = Math.min(totalRows, Math.max(currentSize + 1, currentSize * 2));
  if (nextSize >= totalRows) {
    return { startIndex: 0, endIndex: totalRows - 1 };
  }
  return focusWindowAroundIndex(totalRows, nextSize, focusIndex);
}

function focusRecentWindow(totalRows: number, size: number) {
  if (totalRows <= 0) {
    return { startIndex: 0, endIndex: 0 };
  }
  const boundedSize = Math.max(1, Math.min(size, totalRows));
  const endIndex = totalRows - 1;
  return {
    startIndex: Math.max(0, endIndex - boundedSize + 1),
    endIndex,
  };
}

function computeSeriesDomain(
  rows: ProjectMetricsChartRow[],
  seriesKeys: ProjectMetricSeriesKey[],
): [number, number] {
  const values = rows.flatMap((row) =>
    seriesKeys
      .map((seriesKey) => getProjectMetricPoint(row, seriesKey).value)
      .filter((value): value is number => value != null),
  );

  if (values.length === 0) {
    return [0, 1];
  }

  const min = Math.min(...values);
  const max = Math.max(...values);
  if (min === max) {
    return [Math.max(0, min - 1), max + 1];
  }
  const padding = Math.max((max - min) * 0.08, 1);
  return [Math.max(0, min - padding), max + padding];
}

function extractChartRow(
  payload: unknown,
): ProjectMetricsChartRow | null {
  if (!Array.isArray(payload) || payload.length === 0) {
    return null;
  }
  const item = payload[0] as { payload?: ProjectMetricsChartRow } | undefined;
  return item?.payload ?? null;
}

function dominantCoverage(
  row: ProjectMetricsChartRow,
  series: ProjectMetricSeries[],
) {
  const coverages = series.map((item) => getProjectMetricPoint(row, item.key).coverage);
  if (coverages.includes("partial")) {
    return "partial";
  }
  if (coverages.every((coverage) => coverage === "unknown")) {
    return "unknown";
  }
  return "known";
}

function findActiveRow({
  activeSessionId,
  currentSessionId,
  rows,
}: {
  activeSessionId: string | null;
  currentSessionId: string | null;
  rows: ProjectMetricsChartRow[];
}) {
  if (rows.length === 0) {
    return null;
  }
  return rows.find((row) => row.sessionId === activeSessionId)
    ?? rows.find((row) => row.sessionId === currentSessionId)
    ?? rows.at(-1)
    ?? null;
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

export { clampZoomWindow, createInitialProjectMetricsRange, focusRecentWindow };

import { Fragment, useEffect, useId, useRef, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import {
  AlertTriangle,
  BarChart3,
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Gauge,
  RefreshCcw,
  Search,
  SlidersHorizontal,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import {
  Brush,
  CartesianGrid,
  Line,
  LineChart,
  ReferenceArea,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import type { DotProps, TooltipContentProps } from "recharts";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import {
  buildProjectMetricsViewModel,
  createInitialProjectMetricsRange,
  formatProjectMetricSeriesValue,
  getProjectMetricPoint,
  type ProjectMetricSeries,
  type ProjectMetricSeriesCategory,
  type ProjectMetricSeriesKey,
  type ProjectMetricsChartRow,
  type ProjectMetricsRangePreset,
  type ProjectMetricsRangeSelection,
  type ProjectMetricsViewModel,
  type ProjectSelectorOption,
} from "@/components/project-metrics";
import type { ProjectMetricsResponse, SessionScopeFilter } from "@/backend";

const SURFACE_CARD_CLASS = "border-border bg-background shadow-none";
const TOOLBAR_TRIGGER_CLASS =
  "flex h-10 w-full items-center justify-between gap-3 rounded-xl border border-border/70 bg-background px-3 text-sm text-foreground transition hover:border-border hover:bg-muted/30";
const TOOLBAR_META_CLASS = "text-[10px] font-semibold uppercase tracking-[0.16em] text-muted-foreground";
const ZOOM_BUTTON_CLASS =
  "rounded-full border border-border/70 px-3 py-1.5 text-xs font-medium text-foreground transition hover:border-border hover:bg-muted/40 disabled:cursor-not-allowed disabled:opacity-50";
const ZOOM_BUTTON_ACTIVE_CLASS = "border-foreground/20 bg-muted/50";

type ChartOutlierMode = "keep" | "clamp" | "exclude";

type ChartDisplayRow = {
  index: number;
  label: string;
  outlierFlags: Partial<Record<ProjectMetricSeriesKey, boolean>>;
  processedValues: Partial<Record<ProjectMetricSeriesKey, number | null>>;
  row: ProjectMetricsChartRow;
  sessionId: string;
  trendValues: Partial<Record<ProjectMetricSeriesKey, number | null>>;
};

type ChartSeriesAnalytics = {
  availableCount: number;
  lowerFence: number | null;
  median: number | null;
  outlierCount: number;
  processedCount: number;
  trendWindowSize: number;
  upperFence: number | null;
};

type ChartAnalysis = {
  chartData: ChartDisplayRow[];
  seriesAnalytics: Partial<Record<ProjectMetricSeriesKey, ChartSeriesAnalytics>>;
};

type ChartSelection = {
  endIndex: number | null;
  startIndex: number | null;
};

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
  onScopeFilterChange,
  onRefresh,
  projectOptions,
  range,
  selectedProjectKey,
  scopeFilter,
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
  onScopeFilterChange: (value: SessionScopeFilter) => void;
  onRefresh: () => void;
  projectOptions: ProjectSelectorOption[];
  range: ProjectMetricsRangeSelection;
  selectedProjectKey: string | null;
  scopeFilter: SessionScopeFilter;
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
    <div className="flex min-w-0 flex-col">
      <Card className={cn(SURFACE_CARD_CLASS, "w-full")}>
        <CardHeader className="gap-3 border-b border-border/50 pb-3">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="space-y-0.5">
              <div className="flex flex-wrap items-center gap-2">
                <BarChart3 className="size-4 text-[color:var(--accent-strong)]" />
                <CardTitle>Project metrics</CardTitle>
                {selectedProject ? (
                  <Badge className="h-6 px-2 text-[11px]" variant="outline">
                    {selectedProject.sessionCount} sessions
                  </Badge>
                ) : null}
              </div>
            </div>

            <div className="flex flex-wrap items-center gap-2">
              {selectedProject?.state === "degraded" ? (
                <Badge className="h-6 px-2 text-[11px]" variant="destructive">
                  degraded
                </Badge>
              ) : null}
              {catalogBusy ? (
                <Badge className="h-6 px-2 text-[11px]" variant="outline">
                  catalog scan
                </Badge>
              ) : null}
              <Button onClick={onRefresh} size="sm" type="button" variant="outline">
                <RefreshCcw className={cn("size-3.5", loading && "animate-spin")} data-icon="inline-start" />
                Refresh
              </Button>
            </div>
          </div>

          <div
            className="grid gap-2 xl:grid-cols-[minmax(0,1.75fr)_minmax(0,0.72fr)_minmax(220px,1fr)_auto]"
            data-testid="project-metrics-toolbar"
          >
            <div className="relative" ref={projectMenuRef}>
              <button
                aria-label={selectedProject?.label ?? "Projects unavailable"}
                aria-controls={projectMenuId}
                aria-expanded={projectMenuOpen}
                className={TOOLBAR_TRIGGER_CLASS}
                onClick={() => {
                  setProjectMenuOpen((current) => !current);
                  setWindowMenuOpen(false);
                }}
                type="button"
              >
                <span className="min-w-0 text-left">
                  <span className={TOOLBAR_META_CLASS}>Project</span>
                  <span className="mt-0.5 block truncate font-medium">
                    {selectedProject?.label ?? "Projects unavailable"}
                  </span>
                </span>
                <ChevronDown className={cn("size-4 shrink-0 text-muted-foreground transition", projectMenuOpen && "rotate-180")} />
              </button>
              {projectMenuOpen ? (
                <div
                  className="absolute left-0 top-full z-20 mt-1 w-[min(34rem,calc(100vw-3rem))] overflow-hidden rounded-xl border border-border/80 bg-card shadow-2xl shadow-black/25"
                  id={projectMenuId}
                  role="listbox"
                >
                  <div className="max-h-80 overflow-y-auto overscroll-contain p-1.5" data-testid="project-selector-scroll">
                    <div className="flex flex-col">
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
                              <span className="mt-1 block text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
                                {formatScopeCountsSummary(option.availableScopeCounts)}
                              </span>
                              <span className="mt-0.5 block break-all text-xs leading-5 text-muted-foreground">
                                {option.description}
                              </span>
                            </span>
                          </button>
                        ))
                      )}
                    </div>
                  </div>
                </div>
              ) : null}
            </div>

            <div className="relative" ref={windowMenuRef}>
              <button
                aria-controls={windowMenuId}
                aria-expanded={windowMenuOpen}
                className={TOOLBAR_TRIGGER_CLASS}
                onClick={() => {
                  setWindowMenuOpen((current) => !current);
                  setProjectMenuOpen(false);
                }}
                type="button"
              >
                <span className="min-w-0 text-left">
                  <span className={TOOLBAR_META_CLASS}>Window</span>
                  <span className="mt-0.5 block truncate font-medium">{selectedWindowLabel}</span>
                </span>
                <ChevronDown className={cn("size-4 shrink-0 text-muted-foreground transition", windowMenuOpen && "rotate-180")} />
              </button>
              {windowMenuOpen ? (
                <div
                  className="absolute left-0 top-full z-20 mt-1 w-52 overflow-hidden rounded-xl border border-border/80 bg-card shadow-2xl shadow-black/25"
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

            <div className="flex flex-wrap items-center gap-1 rounded-xl border border-border/70 bg-background p-1">
              <span className="px-2 text-[10px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
                Scope
              </span>
              {(["all", "main", "subsession"] as SessionScopeFilter[]).map((value) => (
                <button
                  className={cn(
                    "rounded-lg px-3 py-2 text-sm font-medium transition",
                    scopeFilter === value
                      ? "bg-muted text-foreground"
                      : "text-muted-foreground hover:bg-muted/40 hover:text-foreground",
                  )}
                  key={value}
                  onClick={() => onScopeFilterChange(value)}
                  type="button"
                >
                  {describeScopeFilter(value)}
                </button>
              ))}
            </div>

            <label className="flex min-h-10 items-center gap-2 rounded-xl border border-border/70 bg-background px-3 text-sm text-foreground">
              <input
                checked={includeSpawnAgents}
                className="size-4 rounded border-border bg-background accent-[color:var(--accent-strong)]"
                onChange={(event) => onIncludeSpawnAgentsChange(event.target.checked)}
                type="checkbox"
              />
              <span className="min-w-0 font-medium">Spawn agents</span>
            </label>
          </div>

          {range.preset === "custom" ? (
            <div className="grid gap-2 md:grid-cols-2">
              <label className="flex flex-col gap-1">
                <span className={TOOLBAR_META_CLASS}>Start</span>
                <input
                  className="h-10 rounded-xl border border-border/70 bg-background px-3 text-sm text-foreground transition hover:border-border focus:border-border"
                  onChange={(event) => onRangeChange({ ...range, start: event.target.value })}
                  type="datetime-local"
                  value={range.start}
                />
              </label>
              <label className="flex flex-col gap-1">
                <span className={TOOLBAR_META_CLASS}>End</span>
                <input
                  className="h-10 rounded-xl border border-border/70 bg-background px-3 text-sm text-foreground transition hover:border-border focus:border-border"
                  onChange={(event) => onRangeChange({ ...range, end: event.target.value })}
                  type="datetime-local"
                  value={range.end}
                />
              </label>
            </div>
          ) : null}

        </CardHeader>

        <CardContent className="min-h-0 flex-1 pt-4">
          <div className="flex min-h-0 flex-col gap-3">
            {error ? (
              <Alert variant="destructive">
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
                <AlertTitle>No sessions for selected filter</AlertTitle>
                <AlertDescription>
                  Для выбранных project/range/scope данных нет. Измените окно, scope или выберите другой проект.
                </AlertDescription>
              </Alert>
            ) : null}

            {selectedProjectKey && !loading && !error && metrics && scopeFilter !== "all" && metrics.available_scope_counts.unknown > 0 ? (
              <Alert>
                <AlertTriangle className="size-4" />
                <AlertTitle>Unknown session scope remains outside narrow filters</AlertTitle>
                <AlertDescription>
                  В выбранном окне есть {metrics.available_scope_counts.unknown} сесс.
                  с `unknown` scope. Они остаются видимыми только в режиме `all`.
                </AlertDescription>
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
          </div>
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
  const [secondaryPanel, setSecondaryPanel] = useState<"overview" | "series" | null>(null);
  const [outlierMode, setOutlierMode] = useState<ChartOutlierMode>("clamp");
  const [showMedian, setShowMedian] = useState(true);
  const [showTrend, setShowTrend] = useState(true);
  const [selection, setSelection] = useState<ChartSelection>({
    endIndex: null,
    startIndex: null,
  });
  const clampedWindow = clampZoomWindow(zoomWindow, viewModel.chartRows.length);
  const visibleSeries = viewModel.chartSeries.filter((series) => visibleSeriesKeys.includes(series.key));
  const chartAnalysis = buildChartAnalysis({
    outlierMode,
    rows: viewModel.chartRows,
    series: visibleSeries,
    window: clampedWindow,
  });
  const visibleRows = viewModel.chartRows.slice(clampedWindow.startIndex, clampedWindow.endIndex + 1);
  const selectedRow = findActiveRow({
    activeSessionId,
    currentSessionId,
    rows: viewModel.chartRows,
  });
  const selectedChartRow = selectedRow
    ? chartAnalysis.chartData.find((item) => item.sessionId === selectedRow.sessionId) ?? null
    : null;
  const selectedSessionItem = selectedRow
    ? viewModel.sessions.find((session) => session.sessionId === selectedRow.sessionId) ?? null
    : null;
  const selectedRowVisible = selectedRow
    ? visibleRows.some((row) => row.sessionId === selectedRow.sessionId)
    : false;
  const windowSize = getWindowSize(clampedWindow);
  const focusIndex = selectedRow?.index ?? getWindowCenterIndex(clampedWindow);
  const quickWindowSizes = [8, 16, 32].filter((size) => size < viewModel.chartRows.length);
  const windowSummary = `Sessions ${clampedWindow.startIndex + 1}-${clampedWindow.endIndex + 1} of ${viewModel.chartRows.length}`;
  const windowLabelSummary = visibleRows.length > 0
    ? `${visibleRows[0]?.label} → ${visibleRows.at(-1)?.label}`
    : "n/a";
  const analyticsOverlayActive = showMedian || showTrend;
  const rawLineOpacity = analyticsOverlayActive ? 0.34 : 0.95;
  const rawLineWidth = analyticsOverlayActive ? 1.4 : 2.2;
  const showRawDots = !analyticsOverlayActive && visibleRows.length <= 18;
  const hasAvailableVisibleData = hasVisibleChartData({
    analytics: chartAnalysis.seriesAnalytics,
    chartData: chartAnalysis.chartData,
    series: visibleSeries,
    showMedian,
    showTrend,
    window: clampedWindow,
  });
  const canSlideWindow = viewModel.chartRows.length > 1;
  const maxWindowSize = Math.max(1, viewModel.chartRows.length);
  const maxWindowStart = Math.max(0, viewModel.chartRows.length - windowSize);
  const selectionPreview = resolveSelectionWindow(
    selection.startIndex,
    selection.endIndex,
    viewModel.chartRows.length,
  );

  function clearSelection() {
    setSelection({
      endIndex: null,
      startIndex: null,
    });
  }

  useEffect(() => {
    setSelection({
      endIndex: null,
      startIndex: null,
    });
  }, [viewModel.chartRows.length, visibleSeriesKeys.length]);

  function focusSession(sessionId: string) {
    const row = viewModel.chartRows.find((item) => item.sessionId === sessionId);
    onActiveSessionIdChange(sessionId);
    if (!row) {
      return;
    }
    if (row.index < clampedWindow.startIndex || row.index > clampedWindow.endIndex) {
      setZoomWindow(focusWindowAroundIndex(viewModel.chartRows.length, Math.max(4, windowSize), row.index));
    }
  }

  function readActiveLabelIndex(state: unknown) {
    const activeLabel = (state as { activeLabel?: unknown } | null)?.activeLabel;
    if (typeof activeLabel !== "number" || !Number.isFinite(activeLabel)) {
      return null;
    }
    return Math.max(0, Math.min(viewModel.chartRows.length - 1, Math.round(activeLabel)));
  }

  function handleChartMouseDown(state: unknown) {
    const index = readActiveLabelIndex(state);
    if (index == null) {
      clearSelection();
      return;
    }
    setSelection({
      endIndex: index,
      startIndex: index,
    });
  }

  function handleChartMouseMove(state: unknown) {
    const row = extractChartRow((state as { activePayload?: unknown } | null)?.activePayload);
    if (row) {
      onActiveSessionIdChange(row.sessionId);
    }

    if (selection.startIndex == null) {
      return;
    }

    const index = readActiveLabelIndex(state);
    if (index == null) {
      return;
    }

    setSelection((current) => (
      current.startIndex == null || current.endIndex === index
        ? current
        : {
            ...current,
            endIndex: index,
          }
    ));
  }

  function handleChartMouseUp() {
    if (!selectionPreview) {
      clearSelection();
      return;
    }
    setZoomWindow(selectionPreview);
    clearSelection();
  }

  return (
    <div className="grid min-h-0 gap-4 xl:grid-cols-[minmax(0,1.85fr)_minmax(320px,0.78fr)]">
      <div className="flex min-h-0 flex-col gap-4" data-testid="project-metrics-stage">
        <Card className={SURFACE_CARD_CLASS} size="sm">
          <CardHeader className="gap-3 border-b border-border/50 pb-3">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div className="space-y-1">
                <CardTitle className="text-sm">Summary chart</CardTitle>
                <p className="text-xs text-muted-foreground">
                  Главный рабочий слой: сначала тренд, затем детали выбранной точки.
                </p>
              </div>

              <div className="flex flex-wrap items-center gap-2">
                <div className="flex flex-wrap gap-1">
                  <Button
                    aria-pressed={secondaryPanel === "overview"}
                    onClick={() => setSecondaryPanel((current) => current === "overview" ? null : "overview")}
                    size="xs"
                    type="button"
                    variant={secondaryPanel === "overview" ? "secondary" : "outline"}
                  >
                    Overview
                  </Button>
                  <Button
                    aria-pressed={secondaryPanel === "series"}
                    onClick={() => setSecondaryPanel((current) => current === "series" ? null : "series")}
                    size="xs"
                    type="button"
                    variant={secondaryPanel === "series" ? "secondary" : "outline"}
                  >
                    <SlidersHorizontal className="size-3" data-icon="inline-start" />
                    Series
                  </Button>
                </div>
                <div className="flex flex-wrap gap-1">
                  <CoveragePill coverage="known" />
                  <CoveragePill coverage="partial" />
                  <CoveragePill coverage="unknown" />
                </div>
              </div>
            </div>

            {secondaryPanel === "overview" ? (
              <OverviewPanel viewModel={viewModel} />
            ) : null}

            {secondaryPanel === "series" ? (
              <SeriesPanel
                chartRowsCount={viewModel.chartRows.length}
                series={viewModel.chartSeries}
                setVisibleSeriesKeys={setVisibleSeriesKeys}
                visibleSeriesKeys={visibleSeriesKeys}
              />
            ) : null}

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

            <div
              className="grid gap-3 rounded-2xl border border-border/70 bg-background/70 p-3 xl:grid-cols-[minmax(0,1.25fr)_minmax(0,1.25fr)_auto]"
              data-testid="project-metrics-analytics-controls"
            >
              <label className="flex flex-col gap-2">
                <div className="flex items-center justify-between gap-2 text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
                  <span>Window size</span>
                  <span className="text-foreground">{windowSize} sessions</span>
                </div>
                <input
                  aria-label="Window size"
                  className="accent-[color:var(--accent-strong)]"
                  disabled={!canSlideWindow}
                  max={maxWindowSize}
                  min={Math.min(2, maxWindowSize)}
                  onChange={(event) => {
                    const nextSize = Number(event.target.value);
                    if (!Number.isFinite(nextSize)) {
                      return;
                    }
                    setZoomWindow(
                      focusWindowAroundIndex(
                        viewModel.chartRows.length,
                        nextSize,
                        focusIndex,
                      ),
                    );
                  }}
                  step={1}
                  type="range"
                  value={windowSize}
                />
              </label>

              <label className="flex flex-col gap-2">
                <div className="flex items-center justify-between gap-2 text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
                  <span>Window position</span>
                  <span className="text-foreground">
                    {maxWindowStart > 0 ? clampedWindow.startIndex + 1 : 1}
                  </span>
                </div>
                <input
                  aria-label="Window position"
                  className="accent-[color:var(--accent-strong)]"
                  disabled={!canSlideWindow || maxWindowStart === 0}
                  max={maxWindowStart}
                  min={0}
                  onChange={(event) => {
                    const startIndex = Number(event.target.value);
                    if (!Number.isFinite(startIndex)) {
                      return;
                    }
                    setZoomWindow({
                      startIndex,
                      endIndex: Math.min(viewModel.chartRows.length - 1, startIndex + windowSize - 1),
                    });
                  }}
                  step={1}
                  type="range"
                  value={Math.min(clampedWindow.startIndex, maxWindowStart)}
                />
              </label>

              <div className="flex flex-col gap-3">
                <div className="flex flex-wrap gap-2">
                  <button
                    aria-pressed={showTrend}
                    className={cn(ZOOM_BUTTON_CLASS, showTrend && ZOOM_BUTTON_ACTIVE_CLASS)}
                    onClick={() => setShowTrend((current) => !current)}
                    type="button"
                  >
                    Trend
                  </button>
                  <button
                    aria-pressed={showMedian}
                    className={cn(ZOOM_BUTTON_CLASS, showMedian && ZOOM_BUTTON_ACTIVE_CLASS)}
                    onClick={() => setShowMedian((current) => !current)}
                    type="button"
                  >
                    Median
                  </button>
                </div>
                <div className="flex flex-wrap gap-2">
                  {(["keep", "clamp", "exclude"] as ChartOutlierMode[]).map((value) => (
                    <button
                      aria-pressed={outlierMode === value}
                      className={cn(
                        ZOOM_BUTTON_CLASS,
                        outlierMode === value && ZOOM_BUTTON_ACTIVE_CLASS,
                      )}
                      key={value}
                      onClick={() => setOutlierMode(value)}
                      type="button"
                    >
                      {describeOutlierMode(value)}
                    </button>
                  ))}
                </div>
              </div>
            </div>

            <div className="flex flex-wrap items-center gap-2 rounded-xl border border-border/70 bg-muted/15 px-3 py-2 text-xs text-muted-foreground">
              <span className="font-medium text-foreground">{windowSummary}</span>
              <span>·</span>
              <span>{windowLabelSummary}</span>
              <span>·</span>
              <span>{describeOutlierMode(outlierMode)}</span>
              <span className="ml-auto">Showing {visibleRows.length} of {viewModel.chartRows.length} sessions</span>
            </div>
          </CardHeader>

          <CardContent className="space-y-3 pt-3">
            {viewModel.degraded ? (
              <Alert>
                <AlertTriangle className="size-4" />
                <AlertTitle>Degraded project identity</AlertTitle>
                <AlertDescription>
                  Часть project identity неполная. Метрики показаны честно, но bucket нельзя считать полным project catalog.
                </AlertDescription>
              </Alert>
            ) : null}

            {viewModel.hasUnknownScope ? (
              <Alert>
                <AlertTriangle className="size-4" />
                <AlertTitle>Unknown scope detected</AlertTitle>
                <AlertDescription>
                  Часть project sessions не удалось надёжно классифицировать как `main` или `subsession`.
                </AlertDescription>
              </Alert>
            ) : null}

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
                <div className="rounded-xl border border-border/70 bg-muted/10 px-3 py-2 text-xs text-muted-foreground">
                  Solid line = displayed values, dashed line = trend, dotted horizontal = median.
                  Trend and median are drawn on top with contrast halo. Drag across the chart to zoom into an interval.
                </div>
                {selectionPreview ? (
                  <div className="rounded-xl border border-[color:var(--accent-strong)]/35 bg-[color:var(--accent-strong)]/10 px-3 py-2 text-xs text-foreground">
                    Selecting sessions {selectionPreview.startIndex + 1}-{selectionPreview.endIndex + 1}
                    {" · "}
                    {chartAnalysis.chartData[selectionPreview.startIndex]?.label ?? "n/a"}
                    {" → "}
                    {chartAnalysis.chartData[selectionPreview.endIndex]?.label ?? "n/a"}
                  </div>
                ) : null}

                <div className="h-[28rem] w-full lg:h-[32rem]">
                  <ResponsiveContainer height="100%" width="100%">
                    <LineChart
                      className="cursor-crosshair"
                      data={chartAnalysis.chartData}
                      margin={{ top: 12, right: 24, bottom: 20, left: 8 }}
                      onMouseDown={handleChartMouseDown}
                      onMouseMove={handleChartMouseMove}
                      onMouseUp={handleChartMouseUp}
                    >
                      <CartesianGrid stroke="currentColor" strokeDasharray="4 6" strokeOpacity={0.12} />
                      {selectionPreview ? (
                        <ReferenceArea
                          fill="color-mix(in srgb, var(--accent-strong) 16%, transparent)"
                          fillOpacity={0.8}
                          ifOverflow="visible"
                          stroke="color-mix(in srgb, var(--accent-strong) 50%, transparent)"
                          strokeOpacity={0.9}
                          x1={selectionPreview.startIndex}
                          x2={selectionPreview.endIndex}
                          yAxisId="preview"
                        />
                      ) : null}
                      <XAxis
                        allowDataOverflow
                        dataKey="index"
                        domain={[clampedWindow.startIndex, clampedWindow.endIndex]}
                        minTickGap={24}
                        stroke="currentColor"
                        strokeOpacity={0.45}
                        tickFormatter={(value) => chartAnalysis.chartData[Number(value)]?.label ?? ""}
                        tick={{ fill: "currentColor", fontSize: 12 }}
                        type="number"
                      />
                      <YAxis
                        allowDataOverflow
                        domain={computeSeriesDomain({
                          analytics: chartAnalysis.seriesAnalytics,
                          chartData: chartAnalysis.chartData,
                          seriesKeys: visibleSeriesKeys,
                          showMedian,
                          showTrend,
                        })}
                        hide
                        yAxisId="preview"
                      />
                      {showMedian
                        ? visibleSeries.map((series) => {
                            const stats = chartAnalysis.seriesAnalytics[series.key];
                            if (!stats || stats.median == null) {
                              return null;
                            }
                            return (
                              <Fragment key={`${series.key}:median:overlay`}>
                                <ReferenceLine
                                  ifOverflow="visible"
                                  stroke="var(--background)"
                                  strokeOpacity={0.96}
                                  strokeWidth={6}
                                  y={stats.median}
                                  yAxisId={series.key}
                                />
                                <ReferenceLine
                                  ifOverflow="visible"
                                  label={{
                                    fill: series.color,
                                    fontSize: 11,
                                    fontWeight: 700,
                                    position: "insideTopRight",
                                    value: `${series.shortLabel} median`,
                                  }}
                                  stroke={series.color}
                                  strokeDasharray="2 6"
                                  strokeOpacity={0.95}
                                  strokeWidth={2.2}
                                  y={stats.median}
                                  yAxisId={series.key}
                                />
                              </Fragment>
                            );
                          })
                        : null}
                      {visibleSeries.map((series) => (
                        <YAxis
                          allowDataOverflow
                          domain={computeSeriesDomain({
                            analytics: chartAnalysis.seriesAnalytics,
                            chartData: chartAnalysis.chartData,
                            seriesKeys: [series.key],
                            showMedian,
                            showTrend,
                          })}
                          hide
                          key={series.key}
                          yAxisId={series.key}
                        />
                      ))}
                      <Tooltip
                        content={(props) => (
                          <SummaryChartTooltip
                            {...props}
                            analytics={chartAnalysis.seriesAnalytics}
                            outlierMode={outlierMode}
                            series={visibleSeries}
                            showMedian={showMedian}
                            showTrend={showTrend}
                          />
                        )}
                      />
                      {showTrend
                        ? visibleSeries.map((series) => (
                            <Fragment key={`${series.key}:trend:overlay`}>
                              <Line
                                connectNulls={false}
                                dataKey={(row: ChartDisplayRow) => row.trendValues[series.key] ?? null}
                                dot={false}
                                isAnimationActive={false}
                                name={`${series.label} trend mask`}
                                stroke="var(--background)"
                                strokeOpacity={0.96}
                                strokeWidth={7}
                                type="monotone"
                                yAxisId={series.key}
                              />
                              <Line
                                connectNulls={false}
                                dataKey={(row: ChartDisplayRow) => row.trendValues[series.key] ?? null}
                                dot={false}
                                isAnimationActive={false}
                                name={`${series.label} trend`}
                                stroke={series.color}
                                strokeDasharray="10 6"
                                strokeOpacity={1}
                                strokeWidth={3.2}
                                type="monotone"
                                yAxisId={series.key}
                              />
                            </Fragment>
                          ))
                        : null}
                      {visibleSeries.map((series) => (
                        <Line
                          activeDot={{ r: 6, strokeWidth: 2 }}
                          connectNulls={false}
                          dataKey={(row: ChartDisplayRow) => row.processedValues[series.key] ?? null}
                          dot={showRawDots ? (
                            <SeriesDot
                              currentSessionId={currentSessionId}
                              onActivateSession={onActiveSessionIdChange}
                              outlierMode={outlierMode}
                              seriesKey={series.key}
                            />
                          ) : false}
                          isAnimationActive={false}
                          key={series.key}
                          name={series.label}
                          stroke={series.color}
                          strokeOpacity={rawLineOpacity}
                          strokeWidth={rawLineWidth}
                          type="monotone"
                          yAxisId={series.key}
                        />
                      ))}
                      <Brush
                        dataKey="index"
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
                        tickFormatter={(value) => chartAnalysis.chartData[Number(value)]?.label ?? ""}
                        travellerWidth={10}
                      />
                    </LineChart>
                  </ResponsiveContainer>
                </div>

                {selectedRow && !selectedRowVisible ? (
                  <Alert>
                    <Search className="size-4" />
                    <AlertTitle>Selected session is outside the current window</AlertTitle>
                    <AlertDescription className="flex flex-wrap items-center gap-2">
                      <span>Inspector stays synced, but the selected point is not visible on the current chart window.</span>
                      <Button
                        onClick={() => {
                          setZoomWindow(focusWindowAroundIndex(viewModel.chartRows.length, Math.max(4, windowSize), selectedRow.index));
                        }}
                        size="xs"
                        type="button"
                        variant="outline"
                      >
                        Bring into view
                      </Button>
                    </AlertDescription>
                  </Alert>
                ) : null}
              </>
            )}
          </CardContent>
        </Card>
      </div>

      <ProjectMetricsInspector
        currentSessionId={currentSessionId}
        onFocusSession={focusSession}
        onOpenSession={onOpenSession}
        outlierMode={outlierMode}
        selectedChartRow={selectedChartRow}
        selectedRow={selectedRow}
        selectedSessionItem={selectedSessionItem}
        series={visibleSeries}
        seriesAnalytics={chartAnalysis.seriesAnalytics}
        sessions={viewModel.sessions}
        showMedian={showMedian}
        showTrend={showTrend}
      />
    </div>
  );
}

function OverviewPanel({ viewModel }: { viewModel: ProjectMetricsViewModel }) {
  return (
    <div
      className="rounded-2xl border border-border/70 bg-muted/10 p-3"
      data-testid="project-metrics-overview-panel"
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
          Overview
        </div>
        <div className="flex flex-wrap gap-1 text-xs text-muted-foreground">
          {viewModel.degraded ? <Badge variant="outline">degraded project</Badge> : null}
          {viewModel.hasUnknownScope ? <Badge variant="outline">unknown scope present</Badge> : null}
        </div>
      </div>

      <div className="mt-3 grid gap-2 sm:grid-cols-2 xl:grid-cols-4">
        {viewModel.summaryCards.map((card) => (
          <div className="rounded-xl border border-border/70 bg-background/80 px-3 py-2.5" key={card.label}>
            <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
              {card.label}
            </div>
            <div
              className={cn(
                "mt-1.5 text-base font-semibold tracking-[-0.03em] text-foreground",
                card.tone === "accent" && "text-[color:var(--accent-strong)]",
                card.tone === "danger" && "text-rose-300",
              )}
            >
              {card.value}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function SeriesPanel({
  chartRowsCount,
  series,
  setVisibleSeriesKeys,
  visibleSeriesKeys,
}: {
  chartRowsCount: number;
  series: ProjectMetricSeries[];
  setVisibleSeriesKeys: Dispatch<SetStateAction<ProjectMetricSeriesKey[]>>;
  visibleSeriesKeys: ProjectMetricSeriesKey[];
}) {
  return (
    <div
      className="rounded-2xl border border-border/70 bg-muted/10 p-3"
      data-testid="project-metrics-series-panel"
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
          Series catalog
        </div>
        <div className="text-xs text-muted-foreground">
          Compact control surface instead of the full toggle grid.
        </div>
      </div>

      <div className="mt-3 grid gap-3 lg:grid-cols-2">
        {(["operational", "derived"] as ProjectMetricSeriesCategory[]).map((category) => {
          const categorySeries = series.filter((item) => item.category === category);
          if (categorySeries.length === 0) {
            return null;
          }
          return (
            <div className="space-y-2" key={category}>
              <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
                {category}
              </div>
              <div className="flex flex-wrap gap-2">
                {categorySeries.map((item) => {
                  const selected = visibleSeriesKeys.includes(item.key);
                  const disabled = item.availablePoints === 0;
                  return (
                    <button
                      aria-pressed={selected}
                      className={cn(
                        "flex min-w-[12rem] flex-1 items-center justify-between gap-3 rounded-xl border px-3 py-2 text-left text-sm transition",
                        selected
                          ? "border-foreground/20 bg-muted/60"
                          : "border-border/70 bg-background hover:border-border hover:bg-muted/30",
                        disabled && "opacity-70",
                      )}
                      key={item.key}
                      onClick={() => {
                        setVisibleSeriesKeys((current) => toggleSeries(current, item.key));
                      }}
                      title={item.description}
                      type="button"
                    >
                      <span className="min-w-0">
                        <span className="flex items-center gap-2 font-medium text-foreground">
                          <span
                            aria-hidden="true"
                            className="size-2.5 rounded-full"
                            style={{ backgroundColor: item.color }}
                          />
                          <span className="truncate">{item.label}</span>
                        </span>
                        <span className="mt-1 block text-xs text-muted-foreground">
                          {item.availablePoints}/{chartRowsCount} ready
                          {item.partialPoints > 0 ? ` · ${item.partialPoints} partial` : ""}
                          {item.unknownPoints > 0 ? ` · ${item.unknownPoints} unknown` : ""}
                        </span>
                      </span>
                      <span className="shrink-0 rounded-full border border-border/70 px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
                        {selected ? "on" : "off"}
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function ProjectMetricsInspector({
  currentSessionId,
  onFocusSession,
  onOpenSession,
  outlierMode,
  selectedChartRow,
  selectedRow,
  selectedSessionItem,
  series,
  seriesAnalytics,
  sessions,
  showMedian,
  showTrend,
}: {
  currentSessionId: string | null;
  onFocusSession: (sessionId: string) => void;
  onOpenSession: (sessionId: string) => void;
  outlierMode: ChartOutlierMode;
  selectedChartRow: ChartDisplayRow | null;
  selectedRow: ProjectMetricsChartRow | null;
  selectedSessionItem: ProjectMetricsViewModel["sessions"][number] | null;
  series: ProjectMetricSeries[];
  seriesAnalytics: Partial<Record<ProjectMetricSeriesKey, ChartSeriesAnalytics>>;
  sessions: ProjectMetricsViewModel["sessions"];
  showMedian: boolean;
  showTrend: boolean;
}) {
  return (
    <Card
      className={cn(SURFACE_CARD_CLASS, "min-h-0")}
      data-testid="project-metrics-inspector"
      size="sm"
    >
      <CardHeader className="gap-3 border-b border-border/50 pb-3">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="space-y-1">
            <CardTitle className="text-sm">Inspector</CardTitle>
            <p className="text-xs text-muted-foreground">
              Active point details and contributing sessions stay in one synchronized flow.
            </p>
          </div>
          {selectedRow ? (
            <Button
              onClick={() => onOpenSession(selectedRow.sessionId)}
              size="xs"
              type="button"
              variant="outline"
            >
              Open session
            </Button>
          ) : null}
        </div>
      </CardHeader>

      <CardContent className="flex min-h-0 flex-1 flex-col gap-3 pt-3">
        {selectedRow ? (
          <>
            <div className="rounded-2xl border border-border/70 bg-muted/10 p-3">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="space-y-1">
                  <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
                    Selected point
                  </div>
                  <div className="text-base font-semibold text-foreground">{selectedRow.label}</div>
                  <div className="font-mono text-xs text-muted-foreground">{selectedRow.sessionId}</div>
                </div>
                {series.length > 0 ? <CoveragePill coverage={dominantCoverage(selectedRow, series)} /> : null}
              </div>

              {selectedSessionItem ? (
                <dl className="mt-3 grid grid-cols-2 gap-2 text-xs">
                  <MetaItem label="Outcome" value={selectedSessionItem.outcome} />
                  <MetaItem label="Duration" value={selectedSessionItem.duration} />
                  <MetaItem label="Tokens" value={selectedSessionItem.tokens} />
                  <MetaItem label="Tool calls" value={selectedSessionItem.toolCalls} />
                  <MetaItem label="Failures" value={selectedSessionItem.failures} />
                  <MetaItem label="Project state" value={selectedSessionItem.projectState} />
                  <MetaItem label="Scope" value={describeScopeBadge(selectedSessionItem.sessionScope)} />
                </dl>
              ) : null}
            </div>

            {series.length > 0 ? (
              <div className="rounded-2xl border border-border/70 bg-background/80 p-3">
                <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
                  Metric snapshot
                </div>
                <div className="mt-3 grid gap-2 sm:grid-cols-2">
                  {series.map((item) => {
                    const point = getProjectMetricPoint(selectedRow, item.key);
                    const chartValue = selectedChartRow?.processedValues[item.key] ?? null;
                    const trendValue = selectedChartRow?.trendValues[item.key] ?? null;
                    const stats = seriesAnalytics[item.key];
                    const chartValueChanged = !areMetricValuesEqual(point.value, chartValue);
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
                        <div className="mt-1 space-y-1 text-xs text-muted-foreground">
                          <div>{item.valueLabel}</div>
                          {chartValueChanged ? (
                            <div>
                              Chart: {formatProjectMetricSeriesValue(item.key, chartValue)}
                            </div>
                          ) : null}
                          {showTrend && trendValue != null ? (
                            <div>
                              Trend: {formatProjectMetricSeriesValue(item.key, trendValue)}
                            </div>
                          ) : null}
                          {showMedian && stats?.median != null ? (
                            <div>
                              Median: {formatProjectMetricSeriesValue(item.key, stats.median)}
                            </div>
                          ) : null}
                          {selectedChartRow?.outlierFlags[item.key] ? (
                            <div>
                              Outlier: {describeOutlierMode(outlierMode)}
                            </div>
                          ) : null}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            ) : (
              <Alert>
                <Search className="size-4" />
                <AlertTitle>No active series in the chart</AlertTitle>
                <AlertDescription>Enable at least one series to populate the metric snapshot.</AlertDescription>
              </Alert>
            )}
          </>
        ) : (
          <Alert>
            <Search className="size-4" />
            <AlertTitle>No active point</AlertTitle>
            <AlertDescription>Наведите курсор на chart point или выберите сессию ниже.</AlertDescription>
          </Alert>
        )}

        <div className="min-h-0 rounded-2xl border border-border/70 bg-background/80 p-3">
          <div className="flex items-center justify-between gap-3">
            <div>
              <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
                Contributing sessions
              </div>
              <div className="mt-1 text-xs text-muted-foreground">
                Hover/click on the chart and inspector selection remain aligned.
              </div>
            </div>
            <Badge className="h-5 px-2 text-[10px]" variant="outline">
              {sessions.length} total
            </Badge>
          </div>

          <ScrollArea className="mt-3 h-[min(48vh,30rem)]">
            <div className="flex flex-col gap-2 pr-3">
              {sessions.map((session) => {
                const selected = session.sessionId === selectedRow?.sessionId;
                const opened = session.sessionId === currentSessionId;
                return (
                  <div
                    className={cn(
                      "rounded-xl border border-border/70 bg-background/70 p-3",
                      selected && "border-foreground/20 bg-muted/50",
                      opened && "ring-1 ring-foreground/15",
                    )}
                    key={session.sessionId}
                  >
                    <div className="flex items-start gap-2">
                      <button
                        className="min-w-0 flex-1 text-left"
                        onClick={() => onFocusSession(session.sessionId)}
                        type="button"
                      >
                        <div className="flex items-center justify-between gap-2">
                          <span className="font-mono text-xs text-foreground">{session.sessionId.slice(0, 8)}</span>
                          <CoveragePill coverage={session.coverage} />
                        </div>
                        <div className="mt-2 text-xs text-muted-foreground">
                          {session.startedAt ?? "n/a"} · {session.outcome} · {describeScopeBadge(session.sessionScope)}
                        </div>
                        <dl className="mt-3 grid grid-cols-2 gap-2 text-xs">
                          <MetaItem label="Duration" value={session.duration} />
                          <MetaItem label="Tokens" value={session.tokens} />
                          <MetaItem label="Tool calls" value={session.toolCalls} />
                          <MetaItem label="Failures" value={session.failures} />
                        </dl>
                      </button>
                      <Button
                        onClick={() => {
                          onFocusSession(session.sessionId);
                          onOpenSession(session.sessionId);
                        }}
                        size="xs"
                        type="button"
                        variant="outline"
                      >
                        Open
                      </Button>
                    </div>
                  </div>
                );
              })}
            </div>
          </ScrollArea>
        </div>
      </CardContent>
    </Card>
  );
}

function SummaryChartTooltip({
  active,
  analytics,
  outlierMode,
  payload,
  series,
  showMedian,
  showTrend,
}: TooltipContentProps<any, any> & {
  analytics: Partial<Record<ProjectMetricSeriesKey, ChartSeriesAnalytics>>;
  outlierMode: ChartOutlierMode;
  series: ProjectMetricSeries[];
  showMedian: boolean;
  showTrend: boolean;
}) {
  const chartRow = extractChartRow(payload);
  if (!active || !chartRow) {
    return null;
  }

  const row = chartRow.row;

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
          const chartValue = chartRow.processedValues[item.key] ?? null;
          const trendValue = chartRow.trendValues[item.key] ?? null;
          const stats = analytics[item.key];
          const chartValueChanged = !areMetricValuesEqual(point.value, chartValue);
          return (
            <div className="space-y-1 text-xs" key={item.key}>
              <div className="flex items-center justify-between gap-3">
                <span className="flex items-center gap-2 text-muted-foreground">
                  <span className="size-2 rounded-full" style={{ backgroundColor: item.color }} />
                  {item.shortLabel}
                </span>
                <span className="text-right text-foreground">
                  {point.formattedValue} <span className="text-muted-foreground">· {point.coverage}</span>
                </span>
              </div>
              {(chartValueChanged || trendValue != null || stats?.median != null || chartRow.outlierFlags[item.key]) ? (
                <div className="space-y-0.5 pl-4 text-[11px] text-muted-foreground">
                  {chartValueChanged ? (
                    <div>chart: {formatProjectMetricSeriesValue(item.key, chartValue)}</div>
                  ) : null}
                  {showTrend && trendValue != null ? (
                    <div>trend: {formatProjectMetricSeriesValue(item.key, trendValue)}</div>
                  ) : null}
                  {showMedian && stats?.median != null ? (
                    <div>median: {formatProjectMetricSeriesValue(item.key, stats.median)}</div>
                  ) : null}
                  {chartRow.outlierFlags[item.key] ? (
                    <div>outlier: {describeOutlierMode(outlierMode)}</div>
                  ) : null}
                </div>
              ) : null}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function SeriesDot({
  currentSessionId,
  onActivateSession,
  outlierMode,
  seriesKey,
  ...props
}: DotProps & {
  currentSessionId: string | null;
  onActivateSession: (value: string) => void;
  outlierMode: ChartOutlierMode;
  payload?: ChartDisplayRow;
  seriesKey: ProjectMetricSeriesKey;
}) {
  const row = props.payload as ChartDisplayRow | undefined;
  if (!row || typeof props.cx !== "number" || typeof props.cy !== "number") {
    return null;
  }

  const point = getProjectMetricPoint(row.row, seriesKey);
  const displayValue = row.processedValues[seriesKey] ?? null;
  if (displayValue == null) {
    return null;
  }

  const selected = row.sessionId === currentSessionId;
  const partial = point.coverage === "partial";
  const outlier = row.outlierFlags[seriesKey];

  return (
    <circle
      className="cursor-pointer"
      cx={props.cx}
      cy={props.cy}
      data-session-id={row.sessionId}
      fill={partial || (outlier && outlierMode !== "keep") ? "var(--background)" : props.stroke ?? "currentColor"}
      fillOpacity={selected ? 1 : 0.92}
      onClick={() => {
        onActivateSession(row.sessionId);
      }}
      onMouseEnter={() => onActivateSession(row.sessionId)}
      r={selected ? 5.5 : 4}
      stroke={props.stroke ?? "currentColor"}
      strokeWidth={partial || (outlier && outlierMode !== "keep") ? 2.4 : 0}
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

function describeScopeFilter(value: SessionScopeFilter) {
  switch (value) {
    case "main":
      return "Main";
    case "subsession":
      return "Subsession";
    case "all":
    default:
      return "All";
  }
}

function describeScopeBadge(value: "main" | "subsession" | "unknown") {
  switch (value) {
    case "main":
      return "main";
    case "subsession":
      return "subsession";
    case "unknown":
    default:
      return "unknown";
  }
}

function describeOutlierMode(value: ChartOutlierMode) {
  switch (value) {
    case "clamp":
      return "Clamp outliers";
    case "exclude":
      return "Exclude outliers";
    case "keep":
    default:
      return "Keep outliers";
  }
}

function formatScopeCountsSummary(counts: { main: number; subsession: number; unknown: number }) {
  return [`main ${counts.main}`, `sub ${counts.subsession}`, `unknown ${counts.unknown}`].join(" · ");
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

function resolveSelectionWindow(
  startIndex: number | null,
  endIndex: number | null,
  totalRows: number,
) {
  if (startIndex == null || endIndex == null || totalRows <= 0) {
    return null;
  }

  const nextStartIndex = Math.max(0, Math.min(startIndex, endIndex));
  const nextEndIndex = Math.min(totalRows - 1, Math.max(startIndex, endIndex));
  if (nextStartIndex === nextEndIndex) {
    return null;
  }

  return {
    startIndex: nextStartIndex,
    endIndex: nextEndIndex,
  };
}

function computeSeriesDomain(
  {
    analytics,
    chartData,
    seriesKeys,
    showMedian,
    showTrend,
  }: {
    analytics: Partial<Record<ProjectMetricSeriesKey, ChartSeriesAnalytics>>;
    chartData: ChartDisplayRow[];
    seriesKeys: ProjectMetricSeriesKey[];
    showMedian: boolean;
    showTrend: boolean;
  },
): [number, number] {
  const values = chartData.flatMap((row) => {
    const displayValues = seriesKeys
      .map((seriesKey) => row.processedValues[seriesKey] ?? null)
      .filter((value): value is number => value != null);
    const trendValues = showTrend
      ? seriesKeys
          .map((seriesKey) => row.trendValues[seriesKey] ?? null)
          .filter((value): value is number => value != null)
      : [];
    return [...displayValues, ...trendValues];
  });

  if (showMedian) {
    values.push(
      ...seriesKeys
        .map((seriesKey) => analytics[seriesKey]?.median ?? null)
        .filter((value): value is number => value != null),
    );
  }

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
): ChartDisplayRow | null {
  if (!Array.isArray(payload) || payload.length === 0) {
    return null;
  }
  const item = payload[0] as { payload?: ChartDisplayRow } | undefined;
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

function buildChartAnalysis({
  outlierMode,
  rows,
  series,
  window,
}: {
  outlierMode: ChartOutlierMode;
  rows: ProjectMetricsChartRow[];
  series: ProjectMetricSeries[];
  window: { startIndex: number; endIndex: number };
}): ChartAnalysis {
  const chartData: ChartDisplayRow[] = rows.map((row) => ({
    index: row.index,
    label: row.label,
    outlierFlags: {},
    processedValues: {},
    row,
    sessionId: row.sessionId,
    trendValues: {},
  }));
  const seriesAnalytics: Partial<Record<ProjectMetricSeriesKey, ChartSeriesAnalytics>> = {};

  for (const seriesItem of series) {
    const rawValues = rows
      .slice(window.startIndex, window.endIndex + 1)
      .map((row) => getProjectMetricPoint(row, seriesItem.key).value);
    const numericValues = rawValues.filter((value): value is number => value != null);
    const fences = computeOutlierFences(numericValues);
    const processedValues = rawValues.map((value) =>
      applyOutlierMode(value, fences, outlierMode),
    );
    const trendWindowSize = pickTrendWindowSize(processedValues);
    const trendValues = computeRollingAverage(processedValues, trendWindowSize);
    const median = computeQuantile(
      processedValues.filter((value): value is number => value != null),
      0.5,
    );
    const outlierCount = rawValues.reduce<number>((count, value) => (
      isOutlierValue(value, fences) ? count + 1 : count
    ), 0);

    seriesAnalytics[seriesItem.key] = {
      availableCount: numericValues.length,
      lowerFence: fences.lowerFence,
      median,
      outlierCount,
      processedCount: processedValues.filter((value): value is number => value != null).length,
      trendWindowSize,
      upperFence: fences.upperFence,
    };

    for (let localIndex = 0; localIndex < rawValues.length; localIndex += 1) {
      const absoluteIndex = window.startIndex + localIndex;
      const dataRow = chartData[absoluteIndex];
      dataRow.processedValues[seriesItem.key] = processedValues[localIndex] ?? null;
      dataRow.trendValues[seriesItem.key] = trendValues[localIndex] ?? null;
      dataRow.outlierFlags[seriesItem.key] = isOutlierValue(rawValues[localIndex], fences);
    }
  }

  return {
    chartData,
    seriesAnalytics,
  };
}

function hasVisibleChartData({
  analytics,
  chartData,
  series,
  showMedian,
  showTrend,
  window,
}: {
  analytics: Partial<Record<ProjectMetricSeriesKey, ChartSeriesAnalytics>>;
  chartData: ChartDisplayRow[];
  series: ProjectMetricSeries[];
  showMedian: boolean;
  showTrend: boolean;
  window: { startIndex: number; endIndex: number };
}) {
  const visibleData = chartData.slice(window.startIndex, window.endIndex + 1);
  return series.some((seriesItem) => {
    const hasDisplayedValue = visibleData.some((row) => row.processedValues[seriesItem.key] != null);
    const hasTrendValue = showTrend && visibleData.some((row) => row.trendValues[seriesItem.key] != null);
    const hasMedianValue = showMedian && analytics[seriesItem.key]?.median != null;
    return hasDisplayedValue || hasTrendValue || hasMedianValue;
  });
}

function computeOutlierFences(values: number[]) {
  if (values.length < 4) {
    return {
      lowerFence: null,
      upperFence: null,
    };
  }

  const q1 = computeQuantile(values, 0.25);
  const q3 = computeQuantile(values, 0.75);
  if (q1 == null || q3 == null) {
    return {
      lowerFence: null,
      upperFence: null,
    };
  }

  const iqr = q3 - q1;
  if (iqr === 0) {
    return {
      lowerFence: null,
      upperFence: null,
    };
  }

  return {
    lowerFence: q1 - iqr * 1.5,
    upperFence: q3 + iqr * 1.5,
  };
}

function isOutlierValue(
  value: number | null,
  fences: { lowerFence: number | null; upperFence: number | null },
) {
  if (value == null || fences.lowerFence == null || fences.upperFence == null) {
    return false;
  }
  return value < fences.lowerFence || value > fences.upperFence;
}

function applyOutlierMode(
  value: number | null,
  fences: { lowerFence: number | null; upperFence: number | null },
  mode: ChartOutlierMode,
) {
  if (value == null || !isOutlierValue(value, fences)) {
    return value;
  }

  if (mode === "exclude") {
    return null;
  }
  if (mode === "clamp" && fences.lowerFence != null && fences.upperFence != null) {
    return Math.min(fences.upperFence, Math.max(fences.lowerFence, value));
  }
  return value;
}

function computeQuantile(values: number[], quantile: number) {
  if (values.length === 0) {
    return null;
  }
  const sorted = [...values].sort((left, right) => left - right);
  const position = (sorted.length - 1) * quantile;
  const baseIndex = Math.floor(position);
  const rest = position - baseIndex;
  const baseValue = sorted[baseIndex];
  const nextValue = sorted[baseIndex + 1];
  if (nextValue == null) {
    return baseValue;
  }
  return baseValue + (nextValue - baseValue) * rest;
}

function pickTrendWindowSize(values: Array<number | null>) {
  const availableCount = values.filter((value): value is number => value != null).length;
  if (availableCount <= 1) {
    return 1;
  }

  if (availableCount <= 4) {
    return Math.min(availableCount, 3);
  }

  let size = Math.max(3, Math.min(9, Math.round(availableCount / 4)));
  if (size % 2 === 0) {
    size += 1;
  }
  return Math.min(size, availableCount);
}

function computeRollingAverage(values: Array<number | null>, windowSize: number) {
  if (values.length === 0) {
    return [];
  }
  if (windowSize <= 1) {
    return [...values];
  }

  const halfWindow = Math.floor(windowSize / 2);
  return values.map((_, index) => {
    const start = Math.max(0, index - halfWindow);
    const end = Math.min(values.length - 1, index + halfWindow);
    const slice = values
      .slice(start, end + 1)
      .filter((value): value is number => value != null);
    if (slice.length === 0) {
      return null;
    }
    return slice.reduce((sum, value) => sum + value, 0) / slice.length;
  });
}

function areMetricValuesEqual(left: number | null, right: number | null) {
  if (left == null || right == null) {
    return left == null && right == null;
  }
  return Math.abs(left - right) < 0.0001;
}

export {
  buildChartAnalysis,
  clampZoomWindow,
  createInitialProjectMetricsRange,
  focusRecentWindow,
  resolveSelectionWindow,
  SeriesDot,
};

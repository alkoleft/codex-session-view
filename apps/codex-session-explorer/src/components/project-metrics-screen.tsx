import { useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode, WheelEvent as ReactWheelEvent } from "react";
import {
  AlertTriangle,
  ChevronLeft,
  ChevronRight,
  LoaderCircle,
  Pin,
  Search,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import {
  Bar,
  CartesianGrid,
  ComposedChart,
  Line,
  Rectangle,
  ReferenceArea,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import type { BarShapeProps, DotProps, TooltipContentProps } from "recharts";

import {
  extractErrorMessage,
  loadProjectMetricsSessionDetailById,
  type MetricCoverage,
  type ProjectMetricsResponse,
  type ProjectMetricsSessionDetail,
  type SessionMetrics,
  type SessionScopeFilter,
} from "@/backend";
import {
  buildProjectMetricsViewModel,
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
import {
  buildChartAnalysis,
  getChartModeValue,
  PROJECT_METRICS_CHART_MODE_META,
  PROJECT_METRICS_CHART_MODES,
  PROJECT_METRICS_NORMALIZATION_META,
  type ChartDisplayRow,
  type ChartOutlierMode,
  type ProjectMetricsChartMode,
} from "@/components/project-metrics-chart";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { cn } from "@/lib/utils";

type ChartSelection = {
  endIndex: number | null;
  startIndex: number | null;
};

type ProjectMetricsSideTab = "window-pulse" | "anomalies" | "series" | "chart" | "pinned-session";

type SeriesRuntimeConfig = {
  emphasized: boolean;
  overrideMode: ProjectMetricsChartMode | null;
  showRawValues: boolean;
  visible: boolean;
};

type ProjectMetricsWorkspaceState = {
  activeTab: ProjectMetricsSideTab;
  defaultMode: ProjectMetricsChartMode;
  outlierMode: ChartOutlierMode;
  primarySeriesKey: ProjectMetricSeriesKey | null;
  seriesConfig: Partial<Record<ProjectMetricSeriesKey, SeriesRuntimeConfig>>;
  zoomWindow: { endIndex: number; startIndex: number };
};

type PersistedProjectMetricsWorkspaceState = Pick<
  ProjectMetricsWorkspaceState,
  "defaultMode" | "outlierMode" | "primarySeriesKey" | "seriesConfig" | "zoomWindow"
>;

type LazyDetailState =
  | { status: "idle" | "loading" }
  | { status: "ready"; detail: ProjectMetricsSessionDetail }
  | { error: string; status: "error" };

type AnomalyClass = "Metric" | "Baseline" | "Session" | "Data issue";

type AnomalyItem = {
  actionLabel: string;
  actionType: "focus-range" | "pin-session";
  className: AnomalyClass;
  description: string;
  id: string;
  range?: { endIndex: number; startIndex: number };
  sessionId?: string;
  severity: number;
  startedAt: string | null;
  title: string;
};

type AnomalyGroup = {
  className: AnomalyClass;
  items: AnomalyItem[];
  severity: number;
};

const SURFACE_CARD_CLASS = "border-border bg-background shadow-none";
const PRIMARY_BAR_FILL_OPACITY = 0.28;
const PROJECT_METRICS_WORKSPACE_STORAGE_PREFIX = "codex-session-explorer.project-metrics.workspace.v1";

export function ProjectMetricsShellControls({
  catalogBusy,
  includeSpawnAgents,
  onIncludeSpawnAgentsChange,
  onProjectChange,
  onRangeChange,
  onScopeFilterChange,
  projectOptions,
  range,
  selectedProjectKey,
  scopeFilter,
}: {
  catalogBusy: boolean;
  includeSpawnAgents: boolean;
  onIncludeSpawnAgentsChange: (value: boolean) => void;
  onProjectChange: (projectKey: string) => void;
  onRangeChange: (range: ProjectMetricsRangeSelection) => void;
  onScopeFilterChange: (value: SessionScopeFilter) => void;
  projectOptions: ProjectSelectorOption[];
  range: ProjectMetricsRangeSelection;
  selectedProjectKey: string | null;
  scopeFilter: SessionScopeFilter;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-2" data-testid="project-metrics-shell-controls">
      <div className="grid gap-2 xl:grid-cols-[minmax(0,1.6fr)_minmax(180px,0.7fr)_minmax(220px,0.9fr)_auto]">
        <label className="flex min-w-0 flex-col gap-1">
          <span className="text-[10px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
            Project
          </span>
          <select
            className="h-10 rounded-xl border border-border/70 bg-background px-3 text-sm text-foreground"
            disabled={projectOptions.length === 0}
            onChange={(event) => onProjectChange(event.target.value)}
            value={selectedProjectKey ?? ""}
          >
            {projectOptions.length === 0 ? (
              <option value="">Projects unavailable</option>
            ) : null}
            {projectOptions.map((option) => (
              <option key={option.projectKey} value={option.projectKey}>
                {option.label}
              </option>
            ))}
          </select>
        </label>

        <label className="flex flex-col gap-1">
          <span className="text-[10px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
            Period
          </span>
          <select
            className="h-10 rounded-xl border border-border/70 bg-background px-3 text-sm text-foreground"
            onChange={(event) =>
              onRangeChange({
                ...range,
                preset: event.target.value as ProjectMetricsRangePreset,
              })}
            value={range.preset}
          >
            {(["7d", "14d", "30d", "90d", "all", "custom"] as ProjectMetricsRangePreset[]).map((preset) => (
              <option key={preset} value={preset}>
                {describeRangePreset(preset)}
              </option>
            ))}
          </select>
        </label>

        <div className="flex min-h-10 flex-wrap items-center gap-1 rounded-xl border border-border/70 bg-background p-1">
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
          <span className="font-medium">Spawn agents</span>
        </label>
      </div>

      {range.preset === "custom" ? (
        <div className="grid gap-2 md:grid-cols-2">
          <label className="flex flex-col gap-1">
            <span className="text-[10px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
              Start
            </span>
            <input
              className="h-10 rounded-xl border border-border/70 bg-background px-3 text-sm text-foreground"
              onChange={(event) => onRangeChange({ ...range, start: event.target.value })}
              type="datetime-local"
              value={range.start}
            />
          </label>
          <label className="flex flex-col gap-1">
            <span className="text-[10px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
              End
            </span>
            <input
              className="h-10 rounded-xl border border-border/70 bg-background px-3 text-sm text-foreground"
              onChange={(event) => onRangeChange({ ...range, end: event.target.value })}
              type="datetime-local"
              value={range.end}
            />
          </label>
        </div>
      ) : null}

      {catalogBusy ? (
        <div className="text-xs text-muted-foreground">
          Catalog scan updates project labels in the background.
        </div>
      ) : null}
    </div>
  );
}

export function ProjectMetricsScreen({
  currentSessionId,
  error,
  includeSpawnAgents,
  loading,
  metrics,
  onOpenSession,
  selectedProjectKey,
}: {
  currentSessionId: string | null;
  error: string | null;
  includeSpawnAgents: boolean;
  loading: boolean;
  metrics: ProjectMetricsResponse | null;
  onOpenSession: (sessionId: string) => void;
  selectedProjectKey: string | null;
}) {
  const queryModel = useMemo(
    () => (metrics ? buildProjectMetricsViewModel(metrics, includeSpawnAgents) : null),
    [includeSpawnAgents, metrics],
  );
  const workspaceDataKey = useMemo(
    () => (metrics ? buildWorkspaceDataKey(metrics, includeSpawnAgents) : null),
    [includeSpawnAgents, metrics],
  );
  const workspaceStorageKey = useMemo(
    () => (metrics ? buildWorkspaceStorageKey(metrics, includeSpawnAgents) : null),
    [includeSpawnAgents, metrics],
  );
  const [workspaceState, setWorkspaceState] = useState<ProjectMetricsWorkspaceState>(() =>
    createEmptyWorkspaceState(),
  );
  const [hoveredSessionId, setHoveredSessionId] = useState<string | null>(null);
  const [hoveredSeriesKey, setHoveredSeriesKey] = useState<ProjectMetricSeriesKey | null>(null);
  const [pinnedSessionId, setPinnedSessionId] = useState<string | null>(null);
  const [selection, setSelection] = useState<ChartSelection>({
    endIndex: null,
    startIndex: null,
  });
  const [detailCache, setDetailCache] = useState<Record<string, LazyDetailState>>({});
  const suppressDirectActivationRef = useRef(false);
  const workspaceDataKeyRef = useRef<string | null>(null);
  const workspaceStorageKeyRef = useRef<string | null>(null);
  const pendingWorkspaceStorageKeyRef = useRef<string | null>(null);
  const hydratedWorkspaceStorageKeyRef = useRef<string | null>(null);

  useEffect(() => {
    if (!queryModel) {
      setWorkspaceState(createEmptyWorkspaceState());
      setHoveredSessionId(null);
      setHoveredSeriesKey(null);
      setPinnedSessionId(null);
      setSelection({
        endIndex: null,
        startIndex: null,
      });
      workspaceDataKeyRef.current = null;
      workspaceStorageKeyRef.current = null;
      pendingWorkspaceStorageKeyRef.current = null;
      hydratedWorkspaceStorageKeyRef.current = null;
      return;
    }

    const restorePersistedState =
      workspaceStorageKey != null && workspaceStorageKeyRef.current !== workspaceStorageKey
        ? loadPersistedWorkspaceState(workspaceStorageKey, queryModel, currentSessionId)
        : null;
    const resetZoomWindow = workspaceDataKeyRef.current !== workspaceDataKey && restorePersistedState == null;
    setWorkspaceState((current) => {
      const nextCurrent = restorePersistedState ?? current;
      return reconcileWorkspaceState(nextCurrent, queryModel, currentSessionId, { resetZoomWindow });
    });
    setHoveredSessionId(null);
    setHoveredSeriesKey(null);
    setPinnedSessionId((current) => reconcilePinnedSessionId(current, queryModel, currentSessionId));
    setSelection({
      endIndex: null,
      startIndex: null,
    });
    workspaceDataKeyRef.current = workspaceDataKey;
    workspaceStorageKeyRef.current = workspaceStorageKey;
    pendingWorkspaceStorageKeyRef.current = workspaceStorageKey;
    hydratedWorkspaceStorageKeyRef.current = null;
  }, [currentSessionId, queryModel, workspaceDataKey, workspaceStorageKey]);

  useEffect(() => {
    if (!queryModel || !workspaceStorageKey) {
      return;
    }
    if (pendingWorkspaceStorageKeyRef.current === workspaceStorageKey) {
      pendingWorkspaceStorageKeyRef.current = null;
      hydratedWorkspaceStorageKeyRef.current = workspaceStorageKey;
      return;
    }
    if (hydratedWorkspaceStorageKeyRef.current !== workspaceStorageKey) {
      return;
    }
    if (Object.keys(workspaceState.seriesConfig).length === 0) {
      return;
    }
    persistWorkspaceState(workspaceStorageKey, workspaceState);
  }, [queryModel, workspaceState, workspaceStorageKey]);

  const sessionMap = useMemo(
    () => new Map((metrics?.sessions ?? []).map((session) => [session.session_id, session])),
    [metrics],
  );
  const visibleSeries = useMemo(() => {
    if (!queryModel) {
      return [];
    }
    return queryModel.chartSeries.filter((series) => workspaceState.seriesConfig[series.key]?.visible);
  }, [queryModel, workspaceState.seriesConfig]);
  const primarySeriesKey = resolvePrimarySeriesKey(visibleSeries, workspaceState.primarySeriesKey);

  useEffect(() => {
    if (!primarySeriesKey || workspaceState.primarySeriesKey === primarySeriesKey) {
      return;
    }
    setWorkspaceState((current) => ({
      ...current,
      primarySeriesKey,
    }));
  }, [primarySeriesKey, workspaceState.primarySeriesKey]);

  const chartAnalysis = useMemo(() => {
    if (!queryModel || visibleSeries.length === 0) {
      return null;
    }
    return buildChartAnalysis({
      outlierMode: workspaceState.outlierMode,
      rows: queryModel.chartRows,
      series: visibleSeries,
    });
  }, [queryModel, visibleSeries, workspaceState.outlierMode]);

  const totalRows = queryModel?.chartRows.length ?? 0;
  const clampedWindow = clampZoomWindow(workspaceState.zoomWindow, totalRows);
  const visibleRows = queryModel?.chartRows.slice(clampedWindow.startIndex, clampedWindow.endIndex + 1) ?? [];
  const visibleChartRows = chartAnalysis?.chartData.slice(clampedWindow.startIndex, clampedWindow.endIndex + 1) ?? [];
  const seriesModeMap = useMemo(() => {
    const next = new Map<ProjectMetricSeriesKey, ProjectMetricsChartMode>();
    for (const series of visibleSeries) {
      next.set(series.key, workspaceState.seriesConfig[series.key]?.overrideMode ?? workspaceState.defaultMode);
    }
    return next;
  }, [visibleSeries, workspaceState.defaultMode, workspaceState.seriesConfig]);
  const pinnedRow = queryModel?.chartRows.find((row) => row.sessionId === pinnedSessionId) ?? null;
  const pinnedChartRow = chartAnalysis?.chartData.find((row) => row.sessionId === pinnedSessionId) ?? null;
  const pinnedSession = pinnedSessionId ? sessionMap.get(pinnedSessionId) ?? null : null;
  const anomalies = useMemo(() => {
    if (!chartAnalysis || !primarySeriesKey) {
      return [] as AnomalyGroup[];
    }
    return buildAnomalyGroups({
      chartRows: visibleChartRows,
      primarySeriesKey,
      series: visibleSeries,
      seriesModeMap,
      sessionsById: sessionMap,
    });
  }, [chartAnalysis, primarySeriesKey, seriesModeMap, sessionMap, visibleChartRows, visibleSeries]);
  const anomalyCount = anomalies.reduce((count, group) => count + group.items.length, 0);
  const visibleWindowSummary = buildWindowPulseSummary({
    anomalies,
    chartRows: visibleChartRows,
    rows: visibleRows,
    series: visibleSeries,
    sessionsById: sessionMap,
  });
  const detailState = pinnedSessionId ? detailCache[pinnedSessionId] : undefined;

  useEffect(() => {
    if (!pinnedSessionId) {
      return;
    }
    const cached = detailCache[pinnedSessionId];
    if (cached?.status === "loading" || cached?.status === "ready") {
      return;
    }

    let cancelled = false;
    setDetailCache((current) => ({
      ...current,
      [pinnedSessionId]: { status: "loading" },
    }));

    void loadProjectMetricsSessionDetailById(pinnedSessionId)
      .then((detail) => {
        if (!cancelled) {
          setDetailCache((current) => ({
            ...current,
            [pinnedSessionId]: { detail, status: "ready" },
          }));
        }
      })
      .catch((loadError) => {
        if (!cancelled) {
          setDetailCache((current) => ({
            ...current,
            [pinnedSessionId]: {
              error: extractErrorMessage(loadError),
              status: "error",
            },
          }));
        }
      });

    return () => {
      cancelled = true;
    };
  }, [pinnedSessionId]);

  function updateSeriesConfig(
    seriesKey: ProjectMetricSeriesKey,
    updater: (current: SeriesRuntimeConfig) => SeriesRuntimeConfig,
  ) {
    setWorkspaceState((current) => {
      const existing = current.seriesConfig[seriesKey];
      if (!existing) {
        return current;
      }
      const nextSeriesConfig = {
        ...current.seriesConfig,
        [seriesKey]: updater(existing),
      };
      return {
        ...current,
        seriesConfig: nextSeriesConfig,
      };
    });
  }

  function clearSelection() {
    setSelection({
      endIndex: null,
      startIndex: null,
    });
  }

  function focusRange(nextWindow: { endIndex: number; startIndex: number }) {
    setWorkspaceState((current) => ({
      ...current,
      zoomWindow: clampZoomWindow(nextWindow, totalRows),
    }));
  }

  function focusSession(sessionId: string) {
    setPinnedSessionId(sessionId);
    setWorkspaceState((current) => ({
      ...current,
      activeTab: "pinned-session",
    }));

    const row = queryModel?.chartRows.find((item) => item.sessionId === sessionId);
    if (!row) {
      return;
    }
    if (row.index < clampedWindow.startIndex || row.index > clampedWindow.endIndex) {
      focusRange(focusWindowAroundIndex(totalRows, Math.max(6, getWindowSize(clampedWindow)), row.index));
    }
  }

  function handleChartMouseDown(state: unknown) {
    const index = readActiveLabelIndex(state, totalRows);
    suppressDirectActivationRef.current = false;
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
    const chartRow = extractChartRow((state as { activePayload?: unknown } | null)?.activePayload);
    setHoveredSessionId(chartRow?.sessionId ?? null);

    if (selection.startIndex == null) {
      return;
    }

    const index = readActiveLabelIndex(state, totalRows);
    if (index == null) {
      return;
    }

    if (index !== selection.startIndex) {
      suppressDirectActivationRef.current = true;
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
    const nextWindow = resolveSelectionWindow(selection.startIndex, selection.endIndex, totalRows);
    if (nextWindow) {
      focusRange(nextWindow);
      queueMicrotask(() => {
        suppressDirectActivationRef.current = false;
      });
      clearSelection();
      return;
    }

    if (hoveredSessionId) {
      focusSession(hoveredSessionId);
    }
    suppressDirectActivationRef.current = false;
    clearSelection();
  }

  function handlePrimaryBarClick(payload: unknown) {
    if (suppressDirectActivationRef.current) {
      return;
    }
    const chartRow = extractChartRowFromActivationPayload(payload);
    if (chartRow) {
      focusSession(chartRow.sessionId);
    }
  }

  function handleChartClick(state: unknown) {
    if (suppressDirectActivationRef.current) {
      return;
    }
    const chartRow = extractChartRow((state as { activePayload?: unknown } | null)?.activePayload);
    if (chartRow) {
      focusSession(chartRow.sessionId);
    }
  }

  function handleChartWheel(event: ReactWheelEvent<HTMLDivElement>) {
    if (totalRows <= getWindowSize(clampedWindow)) {
      return;
    }

    const dominantDelta =
      Math.abs(event.deltaX) > Math.abs(event.deltaY) ? event.deltaX : event.deltaY;
    if (!Number.isFinite(dominantDelta) || dominantDelta === 0) {
      return;
    }

    event.preventDefault();
    const direction = dominantDelta > 0 ? 1 : -1;
    const step = pickWheelPanStep(getWindowSize(clampedWindow), dominantDelta);
    focusRange(shiftZoomWindow(clampedWindow, totalRows, direction * step));
  }

  const selectionPreview = resolveSelectionWindow(selection.startIndex, selection.endIndex, totalRows);
  const hasVisibleChartData = chartAnalysis
    ? visibleSeries.some((series) =>
        visibleChartRows.some((row) => getChartModeValue(row, seriesModeMap.get(series.key) ?? workspaceState.defaultMode, series.key) != null),
      )
    : false;
  const pinnedFlags = buildPinnedFlags({
    anomalies,
    chartRow: pinnedChartRow,
    row: pinnedRow,
    series: visibleSeries,
    session: pinnedSession,
  });

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-hidden" data-testid="project-metrics-shell">
      {error ? (
        <Alert variant="destructive">
          <AlertTriangle className="size-4" />
          <AlertTitle>Project metrics query failed</AlertTitle>
          <AlertDescription className="ui-selectable">{error}</AlertDescription>
        </Alert>
      ) : null}

      {loading ? (
        <Alert>
          <LoaderCircle className="size-4 animate-spin" />
          <AlertTitle>Project metrics loading</AlertTitle>
          <AlertDescription>Запрос обновляет workspace под текущий project/range/scope.</AlertDescription>
        </Alert>
      ) : null}

      {!selectedProjectKey && !loading ? (
        <Alert>
          <Search className="size-4" />
          <AlertTitle>Project not selected</AlertTitle>
          <AlertDescription>Выберите проект в shell header, чтобы открыть analytics workspace.</AlertDescription>
        </Alert>
      ) : null}

      {selectedProjectKey && !loading && !error && metrics && metrics.sessions.length === 0 ? (
        <Alert>
          <Search className="size-4" />
          <AlertTitle>No sessions for selected filter</AlertTitle>
          <AlertDescription>Для выбранного project/range/scope данных нет.</AlertDescription>
        </Alert>
      ) : null}

      {queryModel && metrics && metrics.sessions.length > 0 ? (
        <div
          className="grid min-h-0 flex-1 gap-4 overflow-hidden xl:grid-cols-[minmax(0,1.9fr)_minmax(340px,0.82fr)]"
          data-testid="project-metrics-split-view"
        >
          <Card className={cn(SURFACE_CARD_CLASS, "flex min-h-0 flex-col overflow-hidden")} size="sm">
            <div className="border-b border-border/50 px-3 py-3">
              {pinnedRow && pinnedSession ? (
                <div className="flex flex-wrap items-center justify-between gap-3" data-testid="project-metrics-pinned-summary">
                  <div className="flex min-w-0 flex-wrap items-center gap-2 text-sm text-foreground">
                    <span className="inline-flex items-center gap-1 font-medium">
                      <Pin className="size-3.5 text-[color:var(--accent-strong)]" />
                      {pinnedSession.factors.agent_role ?? "unknown-role"}
                    </span>
                    {pinnedFlags.map((flag) => (
                      <CompactFlag key={flag}>{flag}</CompactFlag>
                    ))}
                    <span className="text-muted-foreground">{formatTimestamp(pinnedRow.startedAt)}</span>
                    <span className="font-mono text-xs text-muted-foreground">{shortSessionId(pinnedRow.sessionId)}</span>
                  </div>
                  <div className="flex flex-wrap items-center justify-end gap-3 text-xs text-muted-foreground">
                    <SummaryMetric label="duration" value={pinnedRow.metrics.duration.formattedValue} />
                    <SummaryMetric label="tokens" value={pinnedRow.metrics.tokens.formattedValue} />
                    <SummaryMetric label="calls" value={pinnedRow.metrics.toolCalls.formattedValue} />
                    <SummaryMetric label="failures" value={pinnedRow.metrics.failures.formattedValue} />
                  </div>
                </div>
              ) : (
                <div className="text-sm text-muted-foreground">Click a chart point to pin a session.</div>
              )}
            </div>

            <CardContent className="flex min-h-0 flex-1 flex-col overflow-hidden pt-3">
              <div className="flex h-full min-h-0 flex-col overflow-y-auto pr-3" data-testid="project-metrics-main-scroll">
                <div className="flex min-h-full flex-1 flex-col gap-3">
                  {visibleSeries.length === 0 ? (
                    <Alert>
                      <Search className="size-4" />
                      <AlertTitle>No series selected</AlertTitle>
                      <AlertDescription>Включите хотя бы одну series во вкладке `Series`.</AlertDescription>
                    </Alert>
                  ) : !hasVisibleChartData ? (
                    <Alert>
                      <AlertTriangle className="size-4" />
                      <AlertTitle>Visible window has no chartable values</AlertTitle>
                      <AlertDescription>
                        В текущем окне нет значений для выбранных series и analytical modes.
                      </AlertDescription>
                    </Alert>
                  ) : (
                    <>
                      {selectionPreview ? (
                        <div className="rounded-xl border border-[color:var(--accent-strong)]/35 bg-[color:var(--accent-strong)]/10 px-3 py-2 text-xs text-foreground">
                          Selecting sessions {selectionPreview.startIndex + 1}-{selectionPreview.endIndex + 1}
                        </div>
                      ) : null}

                      <div
                        className="relative min-h-[32rem] flex-1 rounded-2xl border border-border/70 bg-muted/10 p-3 lg:min-h-[38rem] xl:min-h-[42rem]"
                        data-testid="project-metrics-chart-surface"
                        onWheel={handleChartWheel}
                      >
                        <div
                          className="absolute left-6 top-6 z-10 flex items-center gap-1 rounded-xl border border-border/70 bg-background/92 p-1 shadow-sm"
                          data-testid="project-metrics-chart-overlay-controls"
                        >
                          <Button
                            aria-label="Zoom in"
                            disabled={getWindowSize(clampedWindow) <= 2}
                            onClick={() =>
                              focusRange(zoomWindowIn(clampedWindow, totalRows, getWindowCenterIndex(clampedWindow)))
                            }
                            size="icon-xs"
                            type="button"
                            variant="ghost"
                          >
                            <ZoomIn className="size-3.5" />
                          </Button>
                          <Button
                            aria-label="Zoom out"
                            disabled={getWindowSize(clampedWindow) >= totalRows}
                            onClick={() =>
                              focusRange(zoomWindowOut(clampedWindow, totalRows, getWindowCenterIndex(clampedWindow)))
                            }
                            size="icon-xs"
                            type="button"
                            variant="ghost"
                          >
                            <ZoomOut className="size-3.5" />
                          </Button>
                          <Button
                            aria-label="Move earlier"
                            disabled={clampedWindow.startIndex === 0}
                            onClick={() =>
                              focusRange(
                                shiftZoomWindow(clampedWindow, totalRows, -Math.max(1, Math.floor(getWindowSize(clampedWindow) / 2))),
                              )
                            }
                            size="icon-xs"
                            type="button"
                            variant="ghost"
                          >
                            <ChevronLeft className="size-3.5" />
                          </Button>
                          <Button
                            aria-label="Move later"
                            disabled={clampedWindow.endIndex >= totalRows - 1}
                            onClick={() =>
                              focusRange(
                                shiftZoomWindow(clampedWindow, totalRows, Math.max(1, Math.floor(getWindowSize(clampedWindow) / 2))),
                              )
                            }
                            size="icon-xs"
                            type="button"
                            variant="ghost"
                          >
                            <ChevronRight className="size-3.5" />
                          </Button>
                        </div>

                        {visibleRows.length === 1 ? (
                          <SingleSessionChartState
                            row={visibleRows[0]}
                            series={visibleSeries}
                          />
                        ) : chartAnalysis ? (
                          <ResponsiveContainer height="100%" width="100%">
                            <ComposedChart
                              className="cursor-crosshair"
                              data={chartAnalysis.chartData}
                              margin={{ top: 28, right: 24, bottom: 16, left: 8 }}
                              onClick={handleChartClick}
                              onMouseDown={handleChartMouseDown}
                              onMouseLeave={() => setHoveredSessionId(null)}
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
                                />
                              ) : null}
                              <XAxis
                                allowDataOverflow
                                dataKey="index"
                                domain={[clampedWindow.startIndex, clampedWindow.endIndex]}
                                minTickGap={24}
                                stroke="currentColor"
                                strokeOpacity={0.45}
                                tick={{ fill: "currentColor", fontSize: 12 }}
                                tickFormatter={(value) => chartAnalysis.chartData[Number(value)]?.label ?? ""}
                                type="number"
                              />
                              <YAxis domain={[-0.05, 1.05]} hide />
                              <Tooltip
                                content={(props) => (
                                  <SummaryChartTooltip
                                    {...props}
                                    hoveredSessionId={hoveredSessionId}
                                    series={visibleSeries}
                                    seriesModeMap={seriesModeMap}
                                    workspaceState={workspaceState}
                                  />
                                )}
                              />

                              {primarySeriesKey ? (
                                <Bar
                                  barSize={Math.max(6, Math.min(18, Math.floor(420 / Math.max(visibleRows.length, 12))))}
                                  className="cursor-pointer"
                                  dataKey={(row: ChartDisplayRow) =>
                                    getChartModeValue(
                                      row,
                                      seriesModeMap.get(primarySeriesKey) ?? workspaceState.defaultMode,
                                      primarySeriesKey,
                                    )}
                                  data-testid="project-metrics-primary-bar"
                                  fill={visibleSeries.find((series) => series.key === primarySeriesKey)?.color ?? "#2563eb"}
                                  fillOpacity={PRIMARY_BAR_FILL_OPACITY}
                                  isAnimationActive={false}
                                  name={`${primarySeriesKey}:primary`}
                                  onClick={handlePrimaryBarClick}
                                  radius={[6, 6, 0, 0]}
                                  shape={(props) => (
                                    <PrimaryBarShape
                                      {...props}
                                      onActivateSession={focusSession}
                                    />
                                  )}
                                />
                              ) : null}

                              {visibleSeries
                                .filter((series) => series.key !== primarySeriesKey)
                                .map((series) => {
                                  const emphasized =
                                    workspaceState.seriesConfig[series.key]?.emphasized || hoveredSeriesKey === series.key;
                                  return (
                                    <Line
                                      activeDot={{ r: 5.5, strokeWidth: 2 }}
                                      connectNulls={false}
                                      dataKey={(row: ChartDisplayRow) =>
                                        getChartModeValue(
                                          row,
                                          seriesModeMap.get(series.key) ?? workspaceState.defaultMode,
                                          series.key,
                                        )}
                                      dot={visibleRows.length <= 18 ? (
                                        <SeriesDot
                                          currentSessionId={pinnedSessionId}
                                          onActivateSession={focusSession}
                                          payloadKey={series.key}
                                          stroke={series.color}
                                        />
                                      ) : false}
                                      isAnimationActive={false}
                                      key={`${series.key}:normalized`}
                                      pointerEvents="none"
                                      stroke={series.color}
                                      strokeOpacity={emphasized ? 0.95 : 0.38}
                                      strokeWidth={emphasized ? 2.8 : 1.6}
                                      type="monotone"
                                    />
                                  );
                                })}

                              {visibleSeries
                                .filter((series) => workspaceState.seriesConfig[series.key]?.showRawValues)
                                .map((series) => (
                                  <Line
                                    connectNulls={false}
                                    dataKey={(row: ChartDisplayRow) => row.rawNormalizedValues[series.key] ?? null}
                                    dot={false}
                                    isAnimationActive={false}
                                    key={`${series.key}:raw`}
                                    pointerEvents="none"
                                    stroke={series.color}
                                    strokeDasharray="5 5"
                                    strokeOpacity={0.22}
                                    strokeWidth={1.2}
                                    type="monotone"
                                  />
                                ))}
                            </ComposedChart>
                          </ResponsiveContainer>
                        ) : null}
                      </div>
                    </>
                  )}
                </div>
              </div>
            </CardContent>
          </Card>

          <Card className={cn(SURFACE_CARD_CLASS, "flex min-h-0 flex-col overflow-hidden")} data-testid="project-metrics-side-panel" size="sm">
            <CardContent className="flex min-h-0 flex-1 flex-col overflow-hidden pt-3">
              <Tabs
                className="flex min-h-0 flex-1 flex-col overflow-hidden"
                onValueChange={(value) =>
                  setWorkspaceState((current) => ({
                    ...current,
                    activeTab: value as ProjectMetricsSideTab,
                  }))}
                value={workspaceState.activeTab}
              >
                <TabsList className="h-auto w-full flex-wrap justify-start overflow-visible" variant="line">
                  <TabsTrigger className="h-auto flex-none px-2 py-1 text-xs sm:text-[13px]" value="window-pulse">Window pulse</TabsTrigger>
                  <TabsTrigger className="h-auto flex-none px-2 py-1 text-xs sm:text-[13px]" value="anomalies">
                    Anomalies
                    {anomalyCount > 0 ? (
                      <span className="ml-1 inline-flex rounded-full border border-rose-200/80 px-1.5 py-0.5 text-[10px] text-rose-700">
                        {anomalyCount}
                      </span>
                    ) : null}
                  </TabsTrigger>
                  <TabsTrigger className="h-auto flex-none px-2 py-1 text-xs sm:text-[13px]" value="series">Series</TabsTrigger>
                  <TabsTrigger className="h-auto flex-none px-2 py-1 text-xs sm:text-[13px]" value="chart">Chart</TabsTrigger>
                  <TabsTrigger className="h-auto flex-none px-2 py-1 text-xs sm:text-[13px]" value="pinned-session">Pinned session</TabsTrigger>
                </TabsList>

                <TabsContent className="flex min-h-0 flex-1 flex-col overflow-hidden pt-3" value="window-pulse">
                  <ScrollArea className="h-full min-h-0 flex-1" data-testid="project-metrics-side-scroll">
                    <WindowPulseTab summary={visibleWindowSummary} />
                  </ScrollArea>
                </TabsContent>

                <TabsContent className="flex min-h-0 flex-1 flex-col overflow-hidden pt-3" value="anomalies">
                  <ScrollArea className="h-full min-h-0 flex-1" data-testid="project-metrics-side-scroll">
                    <AnomaliesTab
                      anomalies={anomalies}
                      onFocusRange={focusRange}
                      onPinSession={focusSession}
                    />
                  </ScrollArea>
                </TabsContent>

                <TabsContent className="flex min-h-0 flex-1 flex-col overflow-hidden pt-3" value="series">
                  <ScrollArea className="h-full min-h-0 flex-1" data-testid="project-metrics-side-scroll">
                    <SeriesTab
                      chartRowsCount={totalRows}
                      hoveredSeriesKey={hoveredSeriesKey}
                      onHoveredSeriesKeyChange={setHoveredSeriesKey}
                      onSetPrimarySeries={(seriesKey) =>
                        setWorkspaceState((current) => ({
                          ...current,
                          primarySeriesKey: seriesKey,
                          seriesConfig: {
                            ...current.seriesConfig,
                            [seriesKey]: {
                              ...current.seriesConfig[seriesKey]!,
                              visible: true,
                            },
                          },
                        }))}
                      onUpdateSeriesConfig={updateSeriesConfig}
                      primarySeriesKey={primarySeriesKey}
                      series={queryModel.chartSeries}
                      seriesConfig={workspaceState.seriesConfig}
                    />
                  </ScrollArea>
                </TabsContent>

                <TabsContent className="flex min-h-0 flex-1 flex-col overflow-hidden pt-3" value="chart">
                  <ScrollArea className="h-full min-h-0 flex-1" data-testid="project-metrics-side-scroll">
                    <ChartTab
                      defaultMode={workspaceState.defaultMode}
                      normalizationLabel={PROJECT_METRICS_NORMALIZATION_META.label}
                      normalizationDescription={PROJECT_METRICS_NORMALIZATION_META.description}
                      onDefaultModeChange={(mode) =>
                        setWorkspaceState((current) => ({
                          ...current,
                          defaultMode: mode,
                        }))}
                      onOutlierModeChange={(mode) =>
                        setWorkspaceState((current) => ({
                          ...current,
                          outlierMode: mode,
                        }))}
                      outlierMode={workspaceState.outlierMode}
                    />
                  </ScrollArea>
                </TabsContent>

                <TabsContent className="flex min-h-0 flex-1 flex-col overflow-hidden pt-3" value="pinned-session">
                  <ScrollArea className="h-full min-h-0 flex-1" data-testid="project-metrics-side-scroll">
                    <PinnedSessionTab
                      detailState={detailState}
                      onOpenSession={onOpenSession}
                      pinnedChartRow={pinnedChartRow}
                      pinnedFlags={pinnedFlags}
                      pinnedRow={pinnedRow}
                      pinnedSession={pinnedSession}
                      series={queryModel.chartSeries}
                    />
                  </ScrollArea>
                </TabsContent>
              </Tabs>
            </CardContent>
          </Card>
        </div>
      ) : null}
    </div>
  );
}

function WindowPulseTab({
  summary,
}: {
  summary: {
    items: Array<{ hint: string; label: string; value: string }>;
    subtitle: string;
    title: string;
  };
}) {
  return (
    <div className="flex flex-col gap-3 pr-3">
      <div className="rounded-2xl border border-border/70 bg-muted/10 p-3">
        <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
          {summary.title}
        </div>
        <div className="mt-1 text-xs text-muted-foreground">{summary.subtitle}</div>
      </div>

      <div className="grid gap-2">
        {summary.items.map((item) => (
          <div className="rounded-2xl border border-border/70 bg-background/80 p-3" key={item.label}>
            <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
              {item.label}
            </div>
            <div className="mt-1 text-lg font-semibold tracking-[-0.03em] text-foreground">{item.value}</div>
            <div className="mt-1 text-xs text-muted-foreground">{item.hint}</div>
          </div>
        ))}
      </div>
    </div>
  );
}

function AnomaliesTab({
  anomalies,
  onFocusRange,
  onPinSession,
}: {
  anomalies: AnomalyGroup[];
  onFocusRange: (range: { endIndex: number; startIndex: number }) => void;
  onPinSession: (sessionId: string) => void;
}) {
  const [collapsed, setCollapsed] = useState<Record<AnomalyClass, boolean>>({
    Baseline: false,
    "Data issue": false,
    Metric: false,
    Session: false,
  });

  if (anomalies.length === 0) {
    return (
      <div className="rounded-2xl border border-border/70 bg-background/80 p-3 text-sm text-muted-foreground">
        Current window has no grouped anomalies.
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3 pr-3" data-testid="project-metrics-anomalies-tab">
      {anomalies.map((group) => (
        <div className="rounded-2xl border border-border/70 bg-background/80" key={group.className}>
          <button
            className="flex w-full items-center justify-between gap-3 px-3 py-3 text-left"
            onClick={() =>
              setCollapsed((current) => ({
                ...current,
                [group.className]: !current[group.className],
              }))}
            type="button"
          >
            <div>
              <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
                {group.className}
              </div>
              <div className="mt-1 text-sm text-foreground">
                {group.items.length} items · severity {group.severity}
              </div>
            </div>
            <Badge variant="outline">{collapsed[group.className] ? "collapsed" : "expanded"}</Badge>
          </button>

          {!collapsed[group.className] ? (
            <div className="flex flex-col gap-2 border-t border-border/60 px-3 py-3">
              {group.items.map((item) => (
                <div className="rounded-xl border border-border/70 bg-muted/15 p-3" key={item.id}>
                  <div className="flex items-start justify-between gap-3">
                    <div>
                      <div className="text-sm font-medium text-foreground">{item.title}</div>
                      <div className="mt-1 text-xs text-muted-foreground">{item.description}</div>
                    </div>
                    <Badge variant="outline">sev {item.severity}</Badge>
                  </div>
                  <div className="mt-3">
                    <Button
                      onClick={() => {
                        if (item.actionType === "focus-range" && item.range) {
                          onFocusRange(item.range);
                          return;
                        }
                        if (item.actionType === "pin-session" && item.sessionId) {
                          onPinSession(item.sessionId);
                        }
                      }}
                      size="xs"
                      type="button"
                      variant="outline"
                    >
                      {item.actionLabel}
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          ) : null}
        </div>
      ))}
    </div>
  );
}

function SeriesTab({
  chartRowsCount,
  hoveredSeriesKey,
  onHoveredSeriesKeyChange,
  onSetPrimarySeries,
  onUpdateSeriesConfig,
  primarySeriesKey,
  series,
  seriesConfig,
}: {
  chartRowsCount: number;
  hoveredSeriesKey: ProjectMetricSeriesKey | null;
  onHoveredSeriesKeyChange: (value: ProjectMetricSeriesKey | null) => void;
  onSetPrimarySeries: (seriesKey: ProjectMetricSeriesKey) => void;
  onUpdateSeriesConfig: (
    seriesKey: ProjectMetricSeriesKey,
    updater: (current: SeriesRuntimeConfig) => SeriesRuntimeConfig,
  ) => void;
  primarySeriesKey: ProjectMetricSeriesKey | null;
  series: ProjectMetricSeries[];
  seriesConfig: Partial<Record<ProjectMetricSeriesKey, SeriesRuntimeConfig>>;
}) {
  return (
    <div className="flex flex-col gap-3 pr-3" data-testid="project-metrics-series-tab">
      {(["operational", "tokens", "factors", "derived"] as ProjectMetricSeriesCategory[]).map((category) => {
        const categorySeries = series.filter((item) => item.category === category);
        if (categorySeries.length === 0) {
          return null;
        }

        return (
          <div className="rounded-2xl border border-border/70 bg-background/80 p-3" key={category}>
            <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
              {category}
            </div>
            <div className="mt-3 flex flex-col gap-2">
              {categorySeries.map((item) => {
                const config = seriesConfig[item.key];
                if (!config) {
                  return null;
                }
                const knownCount = Math.max(0, chartRowsCount - item.partialPoints - item.unknownPoints);
                return (
                  <div
                    className={cn(
                      "rounded-xl border px-3 py-3 transition",
                      hoveredSeriesKey === item.key ? "border-foreground/20 bg-muted/30" : "border-border/70 bg-muted/10",
                    )}
                    key={item.key}
                    onMouseEnter={() => onHoveredSeriesKeyChange(item.key)}
                    onMouseLeave={() => onHoveredSeriesKeyChange(null)}
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <div className="flex items-center gap-2 text-sm font-medium text-foreground">
                          <span
                            aria-hidden="true"
                            className="size-2.5 rounded-full"
                            style={{ backgroundColor: item.color }}
                          />
                          {item.label}
                        </div>
                        <div className="mt-1 text-xs text-muted-foreground">
                          known {knownCount} · partial {item.partialPoints} · unknown {item.unknownPoints}
                        </div>
                      </div>
                      <div className="flex flex-wrap items-center gap-1">
                        {primarySeriesKey === item.key ? (
                          <Badge>primary</Badge>
                        ) : null}
                        {config.emphasized ? (
                          <Badge variant="outline">emphasis</Badge>
                        ) : null}
                      </div>
                    </div>

                    <div className="mt-3 flex flex-wrap gap-2">
                      <Button
                        aria-pressed={config.visible}
                        onClick={() => onUpdateSeriesConfig(item.key, (current) => ({ ...current, visible: !current.visible }))}
                        size="xs"
                        type="button"
                        variant={config.visible ? "secondary" : "outline"}
                      >
                        {config.visible ? "Visible" : "Hidden"}
                      </Button>
                      <Button
                        onClick={() => onSetPrimarySeries(item.key)}
                        size="xs"
                        type="button"
                        variant={primarySeriesKey === item.key ? "secondary" : "outline"}
                      >
                        Make primary
                      </Button>
                      <Button
                        aria-pressed={config.emphasized}
                        onClick={() => onUpdateSeriesConfig(item.key, (current) => ({ ...current, emphasized: !current.emphasized }))}
                        size="xs"
                        type="button"
                        variant={config.emphasized ? "secondary" : "outline"}
                      >
                        Emphasize
                      </Button>
                      <Button
                        aria-pressed={config.showRawValues}
                        onClick={() => onUpdateSeriesConfig(item.key, (current) => ({ ...current, showRawValues: !current.showRawValues }))}
                        size="xs"
                        type="button"
                        variant={config.showRawValues ? "secondary" : "outline"}
                      >
                        Raw values
                      </Button>
                    </div>

                    <div className="mt-3 flex flex-wrap gap-1">
                      <ModeBadge
                        active={config.overrideMode == null}
                        label="Default"
                        onClick={() => onUpdateSeriesConfig(item.key, (current) => ({ ...current, overrideMode: null }))}
                      />
                      {PROJECT_METRICS_CHART_MODES.map((mode) => (
                        <ModeBadge
                          active={config.overrideMode === mode}
                          key={mode}
                          label={PROJECT_METRICS_CHART_MODE_META[mode].label}
                          onClick={() =>
                            onUpdateSeriesConfig(item.key, (current) => ({
                              ...current,
                              overrideMode: current.overrideMode === mode ? null : mode,
                            }))}
                        />
                      ))}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function ChartTab({
  defaultMode,
  normalizationDescription,
  normalizationLabel,
  onDefaultModeChange,
  onOutlierModeChange,
  outlierMode,
}: {
  defaultMode: ProjectMetricsChartMode;
  normalizationDescription: string;
  normalizationLabel: string;
  onDefaultModeChange: (mode: ProjectMetricsChartMode) => void;
  onOutlierModeChange: (mode: ChartOutlierMode) => void;
  outlierMode: ChartOutlierMode;
}) {
  return (
    <div className="flex flex-col gap-3 pr-3" data-testid="project-metrics-chart-tab">
      <div className="rounded-2xl border border-border/70 bg-background/80 p-3">
        <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
          Normalization
        </div>
        <div className="mt-1 text-sm font-medium text-foreground">{normalizationLabel}</div>
        <div className="mt-1 text-xs text-muted-foreground">{normalizationDescription}</div>
      </div>

      <div className="rounded-2xl border border-border/70 bg-background/80 p-3">
        <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
          Default analytical mode
        </div>
        <div className="mt-3 flex flex-wrap gap-2">
          {PROJECT_METRICS_CHART_MODES.map((mode) => (
            <Button
              aria-pressed={defaultMode === mode}
              key={mode}
              onClick={() => onDefaultModeChange(mode)}
              size="xs"
              type="button"
              variant={defaultMode === mode ? "secondary" : "outline"}
            >
              {PROJECT_METRICS_CHART_MODE_META[mode].label}
            </Button>
          ))}
        </div>
        <div className="mt-2 text-xs text-muted-foreground">
          {PROJECT_METRICS_CHART_MODE_META[defaultMode].helpText}
        </div>
      </div>

      <div className="rounded-2xl border border-border/70 bg-background/80 p-3">
        <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
          Outlier mode
        </div>
        <div className="mt-3 flex flex-wrap gap-2">
          {(["keep", "clamp", "exclude"] as ChartOutlierMode[]).map((mode) => (
            <Button
              aria-pressed={outlierMode === mode}
              key={mode}
              onClick={() => onOutlierModeChange(mode)}
              size="xs"
              type="button"
              variant={outlierMode === mode ? "secondary" : "outline"}
            >
              {describeOutlierMode(mode)}
            </Button>
          ))}
        </div>
      </div>
    </div>
  );
}

function PinnedSessionTab({
  detailState,
  onOpenSession,
  pinnedChartRow,
  pinnedFlags,
  pinnedRow,
  pinnedSession,
  series,
}: {
  detailState: LazyDetailState | undefined;
  onOpenSession: (sessionId: string) => void;
  pinnedChartRow: ChartDisplayRow | null;
  pinnedFlags: string[];
  pinnedRow: ProjectMetricsChartRow | null;
  pinnedSession: SessionMetrics | null;
  series: ProjectMetricSeries[];
}) {
  if (!pinnedRow || !pinnedSession) {
    return (
      <div className="rounded-2xl border border-border/70 bg-background/80 p-3 text-sm text-muted-foreground">
        Pin a session from the chart to inspect its full detail here.
      </div>
    );
  }

  const detail = detailState?.status === "ready" ? detailState.detail : null;

  return (
    <div className="flex flex-col gap-3 pr-3" data-testid="project-metrics-pinned-tab">
      <div className="rounded-2xl border border-border/70 bg-background/80 p-3">
        <div className="flex items-start justify-between gap-3">
          <div>
            <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
              Pinned session
            </div>
            <div className="mt-1 text-sm font-medium text-foreground">{formatTimestamp(pinnedRow.startedAt)}</div>
            <div className="mt-1 font-mono text-xs text-muted-foreground">{pinnedRow.sessionId}</div>
          </div>
          <Button onClick={() => onOpenSession(pinnedRow.sessionId)} size="xs" type="button" variant="outline">
            Open session
          </Button>
        </div>

        <div className="mt-3 flex flex-wrap gap-2">
          <CompactFlag>{pinnedSession.factors.agent_role ?? "unknown-role"}</CompactFlag>
          {pinnedFlags.map((flag) => (
            <CompactFlag key={flag}>{flag}</CompactFlag>
          ))}
        </div>

        <div className="mt-3 grid gap-2 sm:grid-cols-2">
          <PinnedMeta label="Duration" value={pinnedRow.metrics.duration.formattedValue} />
          <PinnedMeta label="Tokens" value={pinnedRow.metrics.tokens.formattedValue} />
          <PinnedMeta label="Tool calls" value={pinnedRow.metrics.toolCalls.formattedValue} />
          <PinnedMeta label="Failures" value={pinnedRow.metrics.failures.formattedValue} />
        </div>
      </div>

      <div className="rounded-2xl border border-border/70 bg-background/80 p-3">
        <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
          Metric values
        </div>
        <div className="mt-3 grid gap-2 sm:grid-cols-2">
          {series.map((seriesItem) => {
            const point = getProjectMetricPoint(pinnedRow, seriesItem.key);
            return (
              <div className="rounded-xl border border-border/70 bg-muted/10 p-3" key={seriesItem.key}>
                <div className="flex items-center justify-between gap-2">
                  <span className="flex items-center gap-2 text-sm font-medium text-foreground">
                    <span className="size-2.5 rounded-full" style={{ backgroundColor: seriesItem.color }} />
                    {seriesItem.shortLabel}
                  </span>
                  <CoveragePill coverage={point.coverage} />
                </div>
                <div className="mt-2 text-base font-semibold text-foreground">{point.formattedValue}</div>
                {pinnedChartRow ? (
                  <div className="mt-1 text-xs text-muted-foreground">
                    Normalized: {formatNormalizedValue(pinnedChartRow.rawNormalizedValues[seriesItem.key] ?? null)}
                  </div>
                ) : null}
              </div>
            );
          })}
        </div>
      </div>

      <div className="rounded-2xl border border-border/70 bg-background/80 p-3">
        <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
          Request context
        </div>
        {detailState?.status === "loading" ? (
          <div className="mt-3 inline-flex items-center gap-2 text-sm text-muted-foreground">
            <LoaderCircle className="size-4 animate-spin" />
            Loading pinned detail...
          </div>
        ) : detailState?.status === "error" ? (
          <div className="mt-3 text-sm text-rose-700">{detailState.error}</div>
        ) : (
          <div className="mt-3 space-y-3">
            <PinnedMeta label="Title" value={detail?.title ?? "unavailable"} />
            <PinnedTextBlock
              label="Start request"
              source={detail?.start_user_request_source ?? "unavailable"}
              value={detail?.start_user_request ?? null}
            />
            <PinnedTextBlock
              label="Task summary"
              source={detail?.task_summary_source ?? "unavailable"}
              value={detail?.task_summary ?? null}
            />
            <div className="grid gap-2 sm:grid-cols-2">
              <PinnedMeta label="Task class" value={detail?.task_class ?? "unavailable"} />
              <PinnedMeta label="Class confidence" value={detail?.task_class_confidence ?? "unknown"} />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function SummaryChartTooltip({
  active,
  hoveredSessionId,
  payload,
  series,
  seriesModeMap,
  workspaceState,
}: TooltipContentProps<any, any> & {
  hoveredSessionId: string | null;
  series: ProjectMetricSeries[];
  seriesModeMap: Map<ProjectMetricSeriesKey, ProjectMetricsChartMode>;
  workspaceState: ProjectMetricsWorkspaceState;
}) {
  const chartRow = extractChartRow(payload);
  if (!active || !chartRow) {
    return null;
  }

  return (
    <div className="w-80 rounded-xl border border-border/80 bg-card/95 p-3 shadow-2xl shadow-black/20 backdrop-blur">
      <div className="flex items-center justify-between gap-2">
        <div>
          <div className="text-sm font-medium text-foreground">{chartRow.row.label}</div>
          <div className="font-mono text-[11px] text-muted-foreground">{chartRow.row.sessionId}</div>
        </div>
        {hoveredSessionId === chartRow.sessionId ? (
          <Badge variant="outline">hover</Badge>
        ) : null}
      </div>
      <div className="mt-3 space-y-2">
        {series.map((seriesItem) => {
          const point = getProjectMetricPoint(chartRow.row, seriesItem.key);
          const activeMode = seriesModeMap.get(seriesItem.key) ?? workspaceState.defaultMode;
          const normalizedValue = getChartModeValue(chartRow, activeMode, seriesItem.key);
          return (
            <div className="space-y-1 text-xs" key={seriesItem.key}>
              <div className="flex items-center justify-between gap-3">
                <span className="flex items-center gap-2 text-muted-foreground">
                  <span className="size-2 rounded-full" style={{ backgroundColor: seriesItem.color }} />
                  {seriesItem.shortLabel}
                </span>
                <span className="text-right text-foreground">
                  {point.formattedValue} <span className="text-muted-foreground">· {point.coverage}</span>
                </span>
              </div>
              <div className="pl-4 text-[11px] text-muted-foreground">
                {PROJECT_METRICS_CHART_MODE_META[activeMode].label}: {formatNormalizedValue(normalizedValue)}
                {workspaceState.seriesConfig[seriesItem.key]?.showRawValues ? (
                  <span> · raw {formatNormalizedValue(chartRow.rawNormalizedValues[seriesItem.key] ?? null)}</span>
                ) : null}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function SingleSessionChartState({
  row,
  series,
}: {
  row: ProjectMetricsChartRow;
  series: ProjectMetricSeries[];
}) {
  return (
    <div
      className="flex h-full flex-col justify-between rounded-2xl border border-dashed border-border/70 bg-muted/10 p-6"
      data-testid="project-metrics-single-session-state"
    >
      <div className="space-y-2">
        <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
          Single-session window
        </div>
        <div className="text-2xl font-semibold tracking-[-0.03em] text-foreground">{row.label}</div>
        <div className="max-w-2xl text-sm text-muted-foreground">
          В текущем окне только одна сессия. Расширьте window, чтобы увидеть correlation chart.
        </div>
      </div>

      <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
        {series.map((item) => {
          const point = getProjectMetricPoint(row, item.key);
          return (
            <div className="rounded-xl border border-border/70 bg-background/80 p-4" key={item.key}>
              <div className="flex items-center justify-between gap-2">
                <span className="flex items-center gap-2 text-sm font-medium text-foreground">
                  <span className="size-2.5 rounded-full" style={{ backgroundColor: item.color }} />
                  {item.shortLabel}
                </span>
                <CoveragePill coverage={point.coverage} />
              </div>
              <div className="mt-3 text-xl font-semibold tracking-[-0.03em] text-foreground">
                {point.formattedValue}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function ModeBadge({
  active,
  label,
  onClick,
}: {
  active: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      className={cn(
        "rounded-full border px-2.5 py-1 text-xs font-medium transition",
        active
          ? "border-foreground/20 bg-muted text-foreground"
          : "border-border/70 bg-background text-muted-foreground hover:bg-muted/40 hover:text-foreground",
      )}
      onClick={onClick}
      type="button"
    >
      {label}
    </button>
  );
}

function CompactFlag({ children }: { children: ReactNode }) {
  return (
    <span className="inline-flex items-center rounded-full border border-border/70 bg-muted/20 px-2 py-0.5 text-[11px] text-foreground">
      {children}
    </span>
  );
}

function SummaryMetric({
  label,
  value,
}: {
  label: string;
  value: string;
}) {
  return (
    <span className="inline-flex items-center gap-1">
      <span className="uppercase tracking-[0.14em]">{label}</span>
      <span className="text-foreground">{value}</span>
    </span>
  );
}

function PinnedMeta({
  label,
  value,
}: {
  label: string;
  value: string;
}) {
  return (
    <div className="rounded-xl border border-border/70 bg-muted/10 p-3">
      <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
        {label}
      </div>
      <div className="mt-1 text-sm text-foreground">{value}</div>
    </div>
  );
}

function PinnedTextBlock({
  label,
  source,
  value,
}: {
  label: string;
  source: string;
  value: string | null;
}) {
  return (
    <div className="rounded-xl border border-border/70 bg-muted/10 p-3">
      <div className="flex items-center justify-between gap-2">
        <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
          {label}
        </div>
        <Badge variant="outline">{source}</Badge>
      </div>
      <div className="mt-2 whitespace-pre-wrap text-sm text-foreground">
        {value?.trim() ? value : "unavailable"}
      </div>
    </div>
  );
}

function CoveragePill({ coverage }: { coverage: MetricCoverage }) {
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

function SeriesDot({
  currentSessionId,
  onActivateSession,
  payloadKey,
  ...props
}: DotProps & {
  currentSessionId: string | null;
  onActivateSession: (value: string) => void;
  payload?: ChartDisplayRow;
  payloadKey: ProjectMetricSeriesKey;
}) {
  const row = props.payload as ChartDisplayRow | undefined;
  if (!row || typeof props.cx !== "number" || typeof props.cy !== "number") {
    return null;
  }

  const value = getChartModeValue(row, "trend", payloadKey);
  if (value == null) {
    return null;
  }

  const selected = row.sessionId === currentSessionId;

  return (
    <circle
      className="cursor-pointer"
      cx={props.cx}
      cy={props.cy}
      data-session-id={row.sessionId}
      fill={props.stroke ?? "currentColor"}
      fillOpacity={selected ? 1 : 0.92}
      onClick={() => onActivateSession(row.sessionId)}
      r={selected ? 5 : 3.5}
      stroke="var(--background)"
      strokeWidth={selected ? 2 : 1.5}
    />
  );
}

function PrimaryBarShape({
  onActivateSession,
  ...props
}: BarShapeProps & {
  onActivateSession: (sessionId: string) => void;
}) {
  const chartRow = extractChartRowFromActivationPayload(props);
  const sessionId = chartRow?.sessionId ?? null;
  const {
    background: _background,
    dataKey: _dataKey,
    index: _index,
    isActive: _isActive,
    option: _option,
    originalDataIndex: _originalDataIndex,
    parentViewBox: _parentViewBox,
    payload: _payload,
    stackedBarStart: _stackedBarStart,
    tooltipPosition: _tooltipPosition,
    value: _value,
    ...rectangleProps
  } = props as BarShapeProps & {
    dataKey?: unknown;
  };
  const x = typeof rectangleProps.x === "number" ? rectangleProps.x : 0;
  const y = typeof rectangleProps.y === "number" ? rectangleProps.y : 0;
  const width = typeof rectangleProps.width === "number" ? rectangleProps.width : 0;
  const height = typeof rectangleProps.height === "number" ? rectangleProps.height : 0;
  const hitboxWidth = Math.max(width, 14);
  const hitboxX = x - (hitboxWidth - width) / 2;
  const hitboxY = Math.min(y, y + height);
  const hitboxHeight = Math.max(Math.abs(height), 18);

  return (
    <g className="cursor-pointer">
      <rect
        className="cursor-pointer"
        data-testid="project-metrics-primary-bar"
        fill="currentColor"
        fillOpacity={0.001}
        height={hitboxHeight}
        onMouseDown={(event) => {
          event.preventDefault();
          event.stopPropagation();
        }}
        onMouseUp={(event) => {
          event.preventDefault();
          event.stopPropagation();
          if (sessionId) {
            onActivateSession(sessionId);
          }
        }}
        onClick={(event) => {
          event.stopPropagation();
          if (sessionId) {
            onActivateSession(sessionId);
          }
        }}
        pointerEvents="all"
        width={hitboxWidth}
        x={hitboxX}
        y={hitboxY}
      />
      <Rectangle
        {...rectangleProps}
        className={cn(rectangleProps.className, "cursor-pointer")}
        pointerEvents="none"
      />
    </g>
  );
}

function createEmptyWorkspaceState(): ProjectMetricsWorkspaceState {
  return {
    activeTab: "window-pulse",
    defaultMode: "trend",
    outlierMode: "clamp",
    primarySeriesKey: null,
    seriesConfig: {},
    zoomWindow: { endIndex: 0, startIndex: 0 },
  };
}

function createInitialWorkspaceState(
  viewModel: ProjectMetricsViewModel,
  currentSessionId: string | null,
): ProjectMetricsWorkspaceState {
  const defaultVisibleKeys = viewModel.defaultVisibleSeriesKeys;
  const primarySeriesKey = defaultVisibleKeys[0] ?? viewModel.chartSeries[0]?.key ?? null;
  const seriesConfig = Object.fromEntries(
    viewModel.chartSeries.map((series) => [
      series.key,
      {
        emphasized: false,
        overrideMode: null,
        showRawValues: false,
        visible: defaultVisibleKeys.includes(series.key),
      } satisfies SeriesRuntimeConfig,
    ]),
  ) as Partial<Record<ProjectMetricSeriesKey, SeriesRuntimeConfig>>;

  return {
    activeTab: "window-pulse",
    defaultMode: "trend",
    outlierMode: "clamp",
    primarySeriesKey,
    seriesConfig,
    zoomWindow: currentSessionId
      ? focusWindowAroundIndex(
          viewModel.chartRows.length,
          Math.min(Math.max(12, Math.ceil(viewModel.chartRows.length / 2)), viewModel.chartRows.length),
          viewModel.chartRows.find((row) => row.sessionId === currentSessionId)?.index ?? viewModel.chartRows.length - 1,
        )
      : viewModel.initialZoomWindow,
  };
}

function reconcileWorkspaceState(
  current: ProjectMetricsWorkspaceState,
  viewModel: ProjectMetricsViewModel,
  currentSessionId: string | null,
  options: { resetZoomWindow: boolean },
): ProjectMetricsWorkspaceState {
  if (Object.keys(current.seriesConfig).length === 0) {
    return createInitialWorkspaceState(viewModel, currentSessionId);
  }

  const initial = createInitialWorkspaceState(viewModel, currentSessionId);
  const seriesConfig = Object.fromEntries(
    viewModel.chartSeries.map((series) => {
      const preserved = current.seriesConfig[series.key];
      return [
        series.key,
        preserved
          ? {
              emphasized: preserved.emphasized,
              overrideMode: preserved.overrideMode,
              showRawValues: preserved.showRawValues,
              visible: preserved.visible,
            }
          : initial.seriesConfig[series.key]!,
      ];
    }),
  ) as Partial<Record<ProjectMetricSeriesKey, SeriesRuntimeConfig>>;

  const visibleSeriesKeys = viewModel.chartSeries
    .map((series) => series.key)
    .filter((seriesKey) => seriesConfig[seriesKey]?.visible);
  if (visibleSeriesKeys.length === 0 && initial.primarySeriesKey) {
    seriesConfig[initial.primarySeriesKey] = {
      ...(seriesConfig[initial.primarySeriesKey] ?? initial.seriesConfig[initial.primarySeriesKey]!),
      visible: true,
    };
  }

  return {
    activeTab: current.activeTab,
    defaultMode: current.defaultMode,
    outlierMode: current.outlierMode,
    primarySeriesKey: resolvePrimarySeriesKey(
      viewModel.chartSeries.filter((series) => seriesConfig[series.key]?.visible),
      current.primarySeriesKey,
    ),
    seriesConfig,
    zoomWindow: options.resetZoomWindow
      ? initial.zoomWindow
      : clampZoomWindow(current.zoomWindow, viewModel.chartRows.length),
  };
}

function buildWorkspaceDataKey(
  metrics: ProjectMetricsResponse,
  includeSpawnAgents: boolean,
) {
  return [
    metrics.project_key,
    metrics.scope_filter,
    includeSpawnAgents ? "with-spawn" : "without-spawn",
    metrics.contributing_session_ids.join("|"),
  ].join("::");
}

function buildWorkspaceStorageKey(
  metrics: ProjectMetricsResponse,
  includeSpawnAgents: boolean,
) {
  return [
    PROJECT_METRICS_WORKSPACE_STORAGE_PREFIX,
    metrics.project_key,
    metrics.scope_filter,
    includeSpawnAgents ? "with-spawn" : "without-spawn",
  ].join("::");
}

function loadPersistedWorkspaceState(
  storageKey: string,
  viewModel: ProjectMetricsViewModel,
  currentSessionId: string | null,
): ProjectMetricsWorkspaceState | null {
  if (typeof window === "undefined") {
    return null;
  }

  const rawValue = readWorkspaceStorage(storageKey);
  if (!rawValue) {
    return null;
  }

  try {
    const parsed = JSON.parse(rawValue) as unknown;
    return restorePersistedWorkspaceState(parsed, viewModel, currentSessionId);
  } catch {
    return null;
  }
}

function persistWorkspaceState(
  storageKey: string,
  workspaceState: ProjectMetricsWorkspaceState,
) {
  if (typeof window === "undefined") {
    return;
  }

  try {
    window.localStorage.setItem(
      storageKey,
      JSON.stringify({
        defaultMode: workspaceState.defaultMode,
        outlierMode: workspaceState.outlierMode,
        primarySeriesKey: workspaceState.primarySeriesKey,
        seriesConfig: workspaceState.seriesConfig,
        zoomWindow: workspaceState.zoomWindow,
      } satisfies PersistedProjectMetricsWorkspaceState),
    );
  } catch {
    // Ignore storage failures so chart interaction stays available in restricted runtimes.
  }
}

function readWorkspaceStorage(storageKey: string) {
  try {
    return window.localStorage.getItem(storageKey);
  } catch {
    return null;
  }
}

function restorePersistedWorkspaceState(
  value: unknown,
  viewModel: ProjectMetricsViewModel,
  currentSessionId: string | null,
): ProjectMetricsWorkspaceState | null {
  if (!value || typeof value !== "object") {
    return null;
  }

  const persisted = value as Partial<PersistedProjectMetricsWorkspaceState>;
  const initial = createInitialWorkspaceState(viewModel, currentSessionId);
  const seriesConfig = Object.fromEntries(
    viewModel.chartSeries.map((series) => {
      const restoredConfig = sanitizeSeriesRuntimeConfig(persisted.seriesConfig?.[series.key]);
      return [
        series.key,
        restoredConfig
          ? {
              ...initial.seriesConfig[series.key]!,
              ...restoredConfig,
            }
          : initial.seriesConfig[series.key]!,
      ];
    }),
  ) as Partial<Record<ProjectMetricSeriesKey, SeriesRuntimeConfig>>;

  return {
    ...initial,
    defaultMode: isProjectMetricsChartMode(persisted.defaultMode) ? persisted.defaultMode : initial.defaultMode,
    outlierMode: isChartOutlierMode(persisted.outlierMode) ? persisted.outlierMode : initial.outlierMode,
    primarySeriesKey:
      typeof persisted.primarySeriesKey === "string" ? persisted.primarySeriesKey as ProjectMetricSeriesKey : initial.primarySeriesKey,
    seriesConfig,
    zoomWindow: sanitizeZoomWindow(persisted.zoomWindow) ?? initial.zoomWindow,
  };
}

function sanitizeSeriesRuntimeConfig(value: unknown): Partial<SeriesRuntimeConfig> | null {
  if (!value || typeof value !== "object") {
    return null;
  }

  const candidate = value as Partial<SeriesRuntimeConfig>;
  const next: Partial<SeriesRuntimeConfig> = {};
  if (typeof candidate.visible === "boolean") {
    next.visible = candidate.visible;
  }
  if (typeof candidate.emphasized === "boolean") {
    next.emphasized = candidate.emphasized;
  }
  if (typeof candidate.showRawValues === "boolean") {
    next.showRawValues = candidate.showRawValues;
  }
  if (candidate.overrideMode == null) {
    next.overrideMode = null;
  } else if (isProjectMetricsChartMode(candidate.overrideMode)) {
    next.overrideMode = candidate.overrideMode;
  }

  return Object.keys(next).length > 0 ? next : null;
}

function sanitizeZoomWindow(value: unknown) {
  if (!value || typeof value !== "object") {
    return null;
  }

  const candidate = value as Partial<ProjectMetricsWorkspaceState["zoomWindow"]>;
  if (
    Number.isInteger(candidate.startIndex) &&
    Number.isInteger(candidate.endIndex) &&
    typeof candidate.startIndex === "number" &&
    typeof candidate.endIndex === "number"
  ) {
    return {
      startIndex: candidate.startIndex,
      endIndex: candidate.endIndex,
    };
  }

  return null;
}

function isProjectMetricsChartMode(value: unknown): value is ProjectMetricsChartMode {
  return PROJECT_METRICS_CHART_MODES.includes(value as ProjectMetricsChartMode);
}

function isChartOutlierMode(value: unknown): value is ChartOutlierMode {
  return value === "keep" || value === "clamp" || value === "exclude";
}

function resolveInitialPinnedSessionId(
  viewModel: ProjectMetricsViewModel,
  currentSessionId: string | null,
) {
  if (currentSessionId && viewModel.chartRows.some((row) => row.sessionId === currentSessionId)) {
    return currentSessionId;
  }
  return viewModel.chartRows.at(-1)?.sessionId ?? null;
}

function reconcilePinnedSessionId(
  currentPinnedSessionId: string | null,
  viewModel: ProjectMetricsViewModel,
  currentSessionId: string | null,
) {
  if (
    currentPinnedSessionId
    && viewModel.chartRows.some((row) => row.sessionId === currentPinnedSessionId)
  ) {
    return currentPinnedSessionId;
  }
  return resolveInitialPinnedSessionId(viewModel, currentSessionId);
}

function resolvePrimarySeriesKey(
  visibleSeries: ProjectMetricSeries[],
  requestedKey: ProjectMetricSeriesKey | null,
) {
  if (requestedKey && visibleSeries.some((series) => series.key === requestedKey)) {
    return requestedKey;
  }
  return visibleSeries[0]?.key ?? null;
}

function buildWindowPulseSummary({
  anomalies,
  chartRows,
  rows,
  series,
  sessionsById,
}: {
  anomalies: AnomalyGroup[];
  chartRows: ChartDisplayRow[];
  rows: ProjectMetricsChartRow[];
  series: ProjectMetricSeries[];
  sessionsById: Map<string, SessionMetrics>;
}) {
  const sessionCount = rows.length;
  const completedCount = rows.reduce((count, row) => {
    const session = sessionsById.get(row.sessionId);
    return count + (session?.outcome.outcome === "completed" ? 1 : 0);
  }, 0);
  const coverageKnownCount = rows.reduce((count, row) => count + (dominantCoverage(row, series) === "known" ? 1 : 0), 0);
  const totalTokens = rows.reduce((sum, row) => sum + (row.metrics.tokens.value ?? 0), 0);

  return {
    items: [
      {
        hint: `${sessionCount} sessions in the current window`,
        label: "Sessions",
        value: new Intl.NumberFormat("ru-RU").format(sessionCount),
      },
      {
        hint: `${completedCount} completed sessions`,
        label: "Completion",
        value: sessionCount > 0 ? `${Math.round((completedCount / sessionCount) * 100)}%` : "n/a",
      },
      {
        hint: `${coverageKnownCount} sessions have fully known chart values`,
        label: "Coverage",
        value: `${coverageKnownCount}/${sessionCount || 0} known`,
      },
      {
        hint: anomalies.length > 0 ? "Grouped anomaly review stays in the next tab" : "No grouped anomalies in view",
        label: "Anomalies",
        value: new Intl.NumberFormat("ru-RU").format(anomalies.reduce((count, group) => count + group.items.length, 0)),
      },
      {
        hint: "Visible token volume from the current window",
        label: "Tokens",
        value: formatProjectMetricSeriesValue("tokens", totalTokens),
      },
      {
        hint: chartRows.length > 0 ? `${chartRows[0]?.label} -> ${chartRows.at(-1)?.label}` : "n/a",
        label: "Window span",
        value: chartRows.length > 0 ? `${chartRows[0]?.label} -> ${chartRows.at(-1)?.label}` : "n/a",
      },
    ],
    subtitle: "Quick summary for the current visible chart window without reintroducing overview blocks into main area.",
    title: "Window pulse",
  };
}

function buildAnomalyGroups({
  chartRows,
  primarySeriesKey,
  series,
  seriesModeMap,
  sessionsById,
}: {
  chartRows: ChartDisplayRow[];
  primarySeriesKey: ProjectMetricSeriesKey;
  series: ProjectMetricSeries[];
  seriesModeMap: Map<ProjectMetricSeriesKey, ProjectMetricsChartMode>;
  sessionsById: Map<string, SessionMetrics>;
}) {
  const groups = new Map<AnomalyClass, AnomalyItem[]>([
    ["Metric", []],
    ["Baseline", []],
    ["Session", []],
    ["Data issue", []],
  ]);

  for (const chartRow of chartRows) {
    const row = chartRow.row;
    const session = sessionsById.get(row.sessionId);
    const metricOutliers = series.filter((seriesItem) => chartRow.outlierFlags[seriesItem.key]);
    if (metricOutliers.length > 0) {
      groups.get("Metric")?.push({
        actionLabel: "Pin session",
        actionType: "pin-session",
        className: "Metric",
        description: `Outlier: ${metricOutliers.map((item) => item.shortLabel).join(", ")}`,
        id: `metric:${row.sessionId}`,
        sessionId: row.sessionId,
        severity: Math.min(3, metricOutliers.length + 1),
        startedAt: row.startedAt,
        title: row.label,
      });
    }

    if (session) {
      const failureCount = sessionFailureCount(session);
      if (failureCount > 0 || session.outcome.outcome !== "completed") {
        groups.get("Session")?.push({
          actionLabel: "Pin session",
          actionType: "pin-session",
          className: "Session",
          description: `${session.outcome.outcome} · failures ${formatProjectMetricSeriesValue("failures", failureCount)}`,
          id: `session:${row.sessionId}`,
          sessionId: row.sessionId,
          severity: failureCount > 0 ? 3 : 2,
          startedAt: row.startedAt,
          title: row.label,
        });
      }

      const coverage = dominantCoverage(row, series);
      const dataReasons = [];
      if (coverage === "partial") {
        dataReasons.push("partial metric coverage");
      }
      if (coverage === "unknown") {
        dataReasons.push("unknown chart values");
      }
      if (session.project.state === "degraded") {
        dataReasons.push("degraded project identity");
      }
      if (session.session_scope === "unknown") {
        dataReasons.push("unknown scope");
      }
      if (dataReasons.length > 0) {
        groups.get("Data issue")?.push({
          actionLabel: "Pin session",
          actionType: "pin-session",
          className: "Data issue",
          description: dataReasons.join(" · "),
          id: `data:${row.sessionId}`,
          sessionId: row.sessionId,
          severity: dataReasons.length >= 2 ? 3 : 2,
          startedAt: row.startedAt,
          title: row.label,
        });
      }
    }
  }

  const primaryMode = seriesModeMap.get(primarySeriesKey) ?? "trend";
  for (let index = 1; index < chartRows.length; index += 1) {
    const previous = chartRows[index - 1];
    const current = chartRows[index];
    const previousValue = getChartModeValue(previous, primaryMode, primarySeriesKey);
    const currentValue = getChartModeValue(current, primaryMode, primarySeriesKey);
    if (previousValue == null || currentValue == null) {
      continue;
    }
    const jump = Math.abs(currentValue - previousValue);
    if (jump < 0.45) {
      continue;
    }
    groups.get("Baseline")?.push({
      actionLabel: "Focus range",
      actionType: "focus-range",
      className: "Baseline",
      description: `${PROJECT_METRICS_CHART_MODE_META[primaryMode].label} jump ${formatNormalizedValue(jump)}`,
      id: `baseline:${current.sessionId}:${index}`,
      range: {
        endIndex: Math.min(chartRows.at(-1)?.index ?? current.index, current.index + 1),
        startIndex: Math.max(chartRows[0]?.index ?? previous.index, previous.index - 1),
      },
      severity: jump >= 0.7 ? 3 : 2,
      startedAt: current.row.startedAt,
      title: `${previous.row.label} -> ${current.row.label}`,
    });
  }

  return Array.from(groups.entries())
    .map(([className, items]) => ({
      className,
      items: items.sort((left, right) => {
        return (
          right.severity - left.severity
          || (right.startedAt ?? "").localeCompare(left.startedAt ?? "")
          || left.id.localeCompare(right.id)
        );
      }),
      severity: items.reduce((max, item) => Math.max(max, item.severity), 0),
    }))
    .filter((group) => group.items.length > 0);
}

function buildPinnedFlags({
  anomalies,
  chartRow,
  row,
  series,
  session,
}: {
  anomalies: AnomalyGroup[];
  chartRow: ChartDisplayRow | null;
  row: ProjectMetricsChartRow | null;
  series: ProjectMetricSeries[];
  session: SessionMetrics | null;
}) {
  if (!row || !session) {
    return [];
  }

  const flags = new Set<string>();
  if (chartRow && series.some((seriesItem) => chartRow.outlierFlags[seriesItem.key])) {
    flags.add("anomaly");
  }
  if (
    anomalies.some((group) =>
      group.className === "Baseline"
      && group.items.some((item) => {
        const range = item.range;
        return range && row.index >= range.startIndex && row.index <= range.endIndex;
      }))
  ) {
    flags.add("baseline");
  }
  const coverage = dominantCoverage(row, series);
  if (coverage === "partial") {
    flags.add("partial");
  }
  if (coverage === "unknown" || session.session_scope === "unknown") {
    flags.add("unknown");
  }
  if (session.project.state === "degraded") {
    flags.add("degraded");
  }
  return Array.from(flags);
}

function dominantCoverage(row: ProjectMetricsChartRow, series: ProjectMetricSeries[]) {
  if (series.length === 0) {
    return "unknown" as MetricCoverage;
  }
  const coverages = series.map((item) => getProjectMetricPoint(row, item.key).coverage);
  if (coverages.includes("partial")) {
    return "partial";
  }
  if (coverages.every((coverage) => coverage === "unknown")) {
    return "unknown";
  }
  return "known";
}

function sessionFailureCount(session: SessionMetrics) {
  const failures = session.operations.failed_operations.coverage !== "unknown"
    ? session.operations.failed_operations
    : session.error_count;
  return failures.value ?? 0;
}

function shortSessionId(sessionId: string) {
  return sessionId.slice(0, 8);
}

function formatTimestamp(value: string | null) {
  if (!value) {
    return "n/a";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat("ru-RU", {
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    month: "2-digit",
  }).format(date);
}

function formatNormalizedValue(value: number | null) {
  if (value == null) {
    return "n/a";
  }
  return value.toFixed(2);
}

function describeRangePreset(preset: ProjectMetricsRangePreset) {
  switch (preset) {
    case "7d":
      return "Last 7 days";
    case "14d":
      return "Last 14 days";
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

function describeOutlierMode(value: ChartOutlierMode) {
  switch (value) {
    case "keep":
      return "Keep outliers";
    case "exclude":
      return "Exclude outliers";
    case "clamp":
    default:
      return "Clamp outliers";
  }
}

function readActiveLabelIndex(state: unknown, totalRows: number) {
  const activeLabel = (state as { activeLabel?: unknown } | null)?.activeLabel;
  if (typeof activeLabel !== "number" || !Number.isFinite(activeLabel)) {
    return null;
  }
  return Math.max(0, Math.min(totalRows - 1, Math.round(activeLabel)));
}

function extractChartRow(payload: unknown): ChartDisplayRow | null {
  if (!Array.isArray(payload) || payload.length === 0) {
    return null;
  }
  const item = payload[0] as { payload?: ChartDisplayRow } | undefined;
  return item?.payload ?? null;
}

function extractChartRowFromActivationPayload(payload: unknown): ChartDisplayRow | null {
  if (!payload || typeof payload !== "object") {
    return null;
  }
  const candidate = payload as { payload?: unknown; sessionId?: unknown };
  if (typeof candidate.sessionId === "string") {
    return candidate as ChartDisplayRow;
  }
  if (candidate.payload && typeof candidate.payload === "object") {
    const nested = candidate.payload as { sessionId?: unknown };
    if (typeof nested.sessionId === "string") {
      return candidate.payload as ChartDisplayRow;
    }
  }
  return null;
}

function clampZoomWindow(
  window: { endIndex: number; startIndex: number },
  totalRows: number,
) {
  if (totalRows <= 0) {
    return { endIndex: 0, startIndex: 0 };
  }
  const upperBound = totalRows - 1;
  const startIndex = Math.max(0, Math.min(window.startIndex, upperBound));
  const endIndex = Math.max(startIndex, Math.min(window.endIndex, upperBound));
  return { endIndex, startIndex };
}

function getWindowSize(window: { endIndex: number; startIndex: number }) {
  return Math.max(0, window.endIndex - window.startIndex + 1);
}

function getWindowCenterIndex(window: { endIndex: number; startIndex: number }) {
  return window.startIndex + Math.floor((getWindowSize(window) - 1) / 2);
}

function focusWindowAroundIndex(totalRows: number, size: number, focusIndex: number) {
  if (totalRows <= 0) {
    return { endIndex: 0, startIndex: 0 };
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

  return { endIndex, startIndex };
}

function shiftZoomWindow(
  window: { endIndex: number; startIndex: number },
  totalRows: number,
  delta: number,
) {
  if (totalRows <= 0) {
    return { endIndex: 0, startIndex: 0 };
  }

  const size = Math.max(1, Math.min(getWindowSize(window), totalRows));
  const maxStart = Math.max(0, totalRows - size);
  const startIndex = Math.max(0, Math.min(window.startIndex + delta, maxStart));
  return {
    endIndex: Math.min(totalRows - 1, startIndex + size - 1),
    startIndex,
  };
}

function zoomWindowIn(
  window: { endIndex: number; startIndex: number },
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
  window: { endIndex: number; startIndex: number },
  totalRows: number,
  focusIndex: number,
) {
  const currentSize = getWindowSize(window);
  if (currentSize >= totalRows) {
    return clampZoomWindow(window, totalRows);
  }

  const nextSize = Math.min(totalRows, Math.max(currentSize + 1, currentSize * 2));
  if (nextSize >= totalRows) {
    return { endIndex: totalRows - 1, startIndex: 0 };
  }
  return focusWindowAroundIndex(totalRows, nextSize, focusIndex);
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
    endIndex: nextEndIndex,
    startIndex: nextStartIndex,
  };
}

function pickWheelPanStep(windowSize: number, delta: number) {
  const normalized = Math.max(1, Math.ceil(Math.abs(delta) / 120));
  return Math.max(1, Math.min(normalized, Math.max(1, Math.floor(windowSize / 4))));
}

export {
  clampZoomWindow,
  focusWindowAroundIndex,
  resolveSelectionWindow,
  SeriesDot,
};

import type { SessionScopeFilter } from "@/backend";
import {
  createInitialProjectMetricsRange,
  type ProjectMetricsRangePreset,
  type ProjectMetricsRangeSelection,
} from "@/components/project-metrics";

export type ViewerRouteView = "session" | "project_metrics";

export type ViewerRouteState = {
  mainScreen: ViewerRouteView;
  projectMetricsRange: ProjectMetricsRangeSelection;
  projectMetricsScopeFilter: SessionScopeFilter;
  selectedProjectKey: string | null;
};

const ROUTE_PARAM_KEYS = ["view", "project", "period", "scope", "start", "end"] as const;

export function readViewerRoute(search: string): ViewerRouteState {
  const params = new URLSearchParams(search);
  const initialRange = createInitialProjectMetricsRange();
  const preset = parseProjectMetricsRangePreset(params.get("period")) ?? initialRange.preset;

  return {
    mainScreen: parseViewerRouteView(params.get("view")),
    selectedProjectKey: normalizeQueryValue(params.get("project")),
    projectMetricsScopeFilter: parseScopeFilter(params.get("scope")) ?? "all",
    projectMetricsRange: {
      preset,
      start: preset === "custom" ? normalizeQueryValue(params.get("start")) ?? "" : "",
      end: preset === "custom" ? normalizeQueryValue(params.get("end")) ?? "" : "",
    },
  };
}

export function buildViewerRouteSearch(
  currentSearch: string,
  state: ViewerRouteState,
): string {
  const params = new URLSearchParams(currentSearch);
  for (const key of ROUTE_PARAM_KEYS) {
    params.delete(key);
  }

  params.set("view", state.mainScreen === "project_metrics" ? "project" : "session");

  if (state.mainScreen === "project_metrics") {
    if (state.selectedProjectKey) {
      params.set("project", state.selectedProjectKey);
    }

    params.set("period", state.projectMetricsRange.preset);
    params.set("scope", state.projectMetricsScopeFilter);

    if (state.projectMetricsRange.preset === "custom") {
      if (state.projectMetricsRange.start) {
        params.set("start", state.projectMetricsRange.start);
      }
      if (state.projectMetricsRange.end) {
        params.set("end", state.projectMetricsRange.end);
      }
    }
  }

  const nextSearch = params.toString();
  return nextSearch ? `?${nextSearch}` : "";
}

function parseViewerRouteView(value: string | null): ViewerRouteView {
  return value === "project" || value === "project_metrics" ? "project_metrics" : "session";
}

function parseProjectMetricsRangePreset(value: string | null): ProjectMetricsRangePreset | null {
  return value === "7d"
    || value === "14d"
    || value === "30d"
    || value === "90d"
    || value === "all"
    || value === "custom"
    ? value
    : null;
}

function parseScopeFilter(value: string | null): SessionScopeFilter | null {
  return value === "all" || value === "main" || value === "subsession" ? value : null;
}

function normalizeQueryValue(value: string | null) {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

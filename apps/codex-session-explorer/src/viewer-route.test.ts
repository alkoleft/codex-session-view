import { describe, expect, it } from "vitest";

import { buildViewerRouteSearch, readViewerRoute } from "@/viewer-route";

describe("viewer-route", () => {
  it("reads project view state from URL params", () => {
    expect(
      readViewerRoute("?view=project&project=codex-worker-rs&period=30d&scope=main"),
    ).toEqual({
      mainScreen: "project_metrics",
      selectedProjectKey: "codex-worker-rs",
      projectMetricsRange: {
        preset: "30d",
        start: "",
        end: "",
      },
      projectMetricsScopeFilter: "main",
    });
  });

  it("keeps custom range details for project view", () => {
    expect(
      readViewerRoute(
        "?view=project&project=codex-worker-rs&period=custom&start=2026-04-01T12:00&end=2026-04-02T08:30&scope=subsession",
      ),
    ).toEqual({
      mainScreen: "project_metrics",
      selectedProjectKey: "codex-worker-rs",
      projectMetricsRange: {
        preset: "custom",
        start: "2026-04-01T12:00",
        end: "2026-04-02T08:30",
      },
      projectMetricsScopeFilter: "subsession",
    });
  });

  it("builds project view params and preserves unrelated query keys", () => {
    expect(
      buildViewerRouteSearch("?debug=1", {
        mainScreen: "project_metrics",
        selectedProjectKey: "codex-worker-rs",
        projectMetricsRange: {
          preset: "14d",
          start: "",
          end: "",
        },
        projectMetricsScopeFilter: "all",
      }),
    ).toBe("?debug=1&view=project&project=codex-worker-rs&period=14d&scope=all");
  });

  it("drops project-only params outside project view", () => {
    expect(
      buildViewerRouteSearch("?project=codex-worker-rs&period=30d&scope=main", {
        mainScreen: "session",
        selectedProjectKey: "codex-worker-rs",
        projectMetricsRange: {
          preset: "30d",
          start: "",
          end: "",
        },
        projectMetricsScopeFilter: "main",
      }),
    ).toBe("?view=session");
  });
});

## 1. Chart Stack and Series Model

- [x] 1.1 Confirm the target chart stack for `ProjectMetricsScreen`, add the dependency in `apps/codex-session-explorer/package.json`, and remove the current custom `MetricChart` path from the implementation plan.
- [x] 1.2 Extract a reusable chart-series registry in `project-metrics.ts` for supported metrics, labels, formatting, coverage metadata, default visibility, and toggle options.
- [x] 1.3 Extend the project metrics view-model with multi-series tooltip/legend payload and shared zoom-window state inputs needed by the new chart layer.

## 2. Interactive Summary Chart

- [x] 2.1 Replace the current SVG `MetricChart` renderer in `project-metrics-screen.tsx` with one interactive summary chart that supports responsive layout, hover/focus details, and chart legend.
- [x] 2.2 Add curated legend or control toggles so the user can show and hide supported operational and derived series without changing the backend query contract.
- [x] 2.3 Implement chart zoom for long time series and keep the visible window synchronized with the active set of visible series.
- [x] 2.4 Preserve click-through drill-down from a chart point to the source session in the current explorer workflow.

## 3. Coverage Semantics and UX States

- [x] 3.1 Render `known`, `partial`, and `unknown` points with explicit visual semantics, keeping `unknown` values as gaps or dedicated unknown markers.
- [x] 3.2 Show richer point details in tooltip or side-panel output, including formatted metric value, coverage state, and source session identity.
- [x] 3.3 Ensure empty/degraded/low-coverage states remain readable after the chart renderer migration, including cases where derived metrics are unavailable.

## 4. Validation

- [x] 4.1 Update or add UI tests for metric selection, zoom interaction, tooltip rendering, coverage markers, and session drill-down.
- [x] 4.2 Run `npm --prefix apps/codex-session-explorer test` and any additional targeted frontend checks required by the chosen chart library integration.
- [x] 4.3 Run `openspec validate enhance-project-metrics-charts --strict`.

import {
  getProjectMetricPoint,
  type ProjectMetricSeries,
  type ProjectMetricSeriesKey,
  type ProjectMetricsChartRow,
} from "@/components/project-metrics";

export type ChartOutlierMode = "keep" | "clamp" | "exclude";

export const PROJECT_METRICS_CHART_MODES = [
  "trend",
  "moving-average",
  "moving-median",
] as const;

export type ProjectMetricsChartMode = (typeof PROJECT_METRICS_CHART_MODES)[number];

export const PROJECT_METRICS_CHART_OVERLAYS = [
  "raw-values",
  "project-median",
] as const;

export type ProjectMetricsChartOverlayKey = (typeof PROJECT_METRICS_CHART_OVERLAYS)[number];

export type ProjectMetricsChartModeMeta = {
  key: ProjectMetricsChartMode;
  label: string;
  primaryValueLabel: string;
  description: string;
  helpText: string;
};

export type ProjectMetricsChartOverlayMeta = {
  key: ProjectMetricsChartOverlayKey;
  label: string;
  valueLabel: string;
  description: string;
  seriesLabelSuffix: string;
};

export const PROJECT_METRICS_CHART_MODE_META: Record<ProjectMetricsChartMode, ProjectMetricsChartModeMeta> = {
  trend: {
    key: "trend",
    label: "Trend",
    primaryValueLabel: "Trend",
    description: "Robust Theil-Sen line across the full project window.",
    helpText: "Trend uses a Theil-Sen line across the full project query window and keeps unknown points as gaps.",
  },
  "moving-average": {
    key: "moving-average",
    label: "Moving average",
    primaryValueLabel: "Moving average",
    description: "Adaptive rolling average over processed values.",
    helpText: "Moving average smooths processed values with an adaptive window while preserving unknown gaps.",
  },
  "moving-median": {
    key: "moving-median",
    label: "Moving median",
    primaryValueLabel: "Moving median",
    description: "Adaptive rolling median over processed values.",
    helpText: "Moving median uses the same adaptive window as moving average but stays more resistant to spikes.",
  },
};

export const PROJECT_METRICS_CHART_OVERLAY_META: Record<
  ProjectMetricsChartOverlayKey,
  ProjectMetricsChartOverlayMeta
> = {
  "raw-values": {
    key: "raw-values",
    label: "Raw values",
    valueLabel: "Raw values",
    description: "Show processed session values on top of the active analytic mode.",
    seriesLabelSuffix: "raw",
  },
  "project-median": {
    key: "project-median",
    label: "Project median",
    valueLabel: "Project median",
    description: "Show a project-wide median baseline computed from the full query window.",
    seriesLabelSuffix: "median",
  },
};

type ChartModeSeriesValues = Record<ProjectMetricsChartMode, Partial<Record<ProjectMetricSeriesKey, number | null>>>;

export type ChartDisplayRow = {
  index: number;
  label: string;
  modeValues: ChartModeSeriesValues;
  outlierFlags: Partial<Record<ProjectMetricSeriesKey, boolean>>;
  processedValues: Partial<Record<ProjectMetricSeriesKey, number | null>>;
  row: ProjectMetricsChartRow;
  sessionId: string;
};

export type ChartSeriesAnalytics = {
  availableCount: number;
  lowerFence: number | null;
  outlierCount: number;
  processedCount: number;
  projectMedian: number | null;
  smoothingWindowSize: number;
  upperFence: number | null;
};

export type ChartAnalysis = {
  chartData: ChartDisplayRow[];
  seriesAnalytics: Partial<Record<ProjectMetricSeriesKey, ChartSeriesAnalytics>>;
};

export function buildChartAnalysis({
  outlierMode,
  rows,
  series,
}: {
  outlierMode: ChartOutlierMode;
  rows: ProjectMetricsChartRow[];
  series: ProjectMetricSeries[];
}): ChartAnalysis {
  const chartData: ChartDisplayRow[] = rows.map((row) => ({
    index: row.index,
    label: row.label,
    modeValues: createEmptyModeValueMap(),
    outlierFlags: {},
    processedValues: {},
    row,
    sessionId: row.sessionId,
  }));
  const seriesAnalytics: Partial<Record<ProjectMetricSeriesKey, ChartSeriesAnalytics>> = {};

  for (const seriesItem of series) {
    const rawValues = rows.map((row) => getProjectMetricPoint(row, seriesItem.key).value);
    const numericValues = rawValues.filter((value): value is number => value != null);
    const fences = computeOutlierFences(numericValues);
    const processedValues = rawValues.map((value) => applyOutlierMode(value, fences, outlierMode));
    const smoothingWindowSize = pickSmoothingWindowSize(processedValues);
    const trendValues = computeTheilSenTrend(processedValues);
    const movingAverageValues = computeRollingAverage(processedValues, smoothingWindowSize);
    const movingMedianValues = computeRollingMedian(processedValues, smoothingWindowSize);
    const projectMedian = computeQuantile(
      processedValues.filter((value): value is number => value != null),
      0.5,
    );
    const outlierCount = rawValues.reduce<number>((count, value) => (
      isOutlierValue(value, fences) ? count + 1 : count
    ), 0);

    seriesAnalytics[seriesItem.key] = {
      availableCount: numericValues.length,
      lowerFence: fences.lowerFence,
      outlierCount,
      processedCount: processedValues.filter((value): value is number => value != null).length,
      projectMedian,
      smoothingWindowSize,
      upperFence: fences.upperFence,
    };

    for (let index = 0; index < rows.length; index += 1) {
      const dataRow = chartData[index];
      dataRow.processedValues[seriesItem.key] = processedValues[index] ?? null;
      dataRow.modeValues.trend[seriesItem.key] = trendValues[index] ?? null;
      dataRow.modeValues["moving-average"][seriesItem.key] = movingAverageValues[index] ?? null;
      dataRow.modeValues["moving-median"][seriesItem.key] = movingMedianValues[index] ?? null;
      dataRow.outlierFlags[seriesItem.key] = isOutlierValue(rawValues[index], fences);
    }
  }

  return {
    chartData,
    seriesAnalytics,
  };
}

export function getChartModeValue(
  row: ChartDisplayRow,
  mode: ProjectMetricsChartMode,
  seriesKey: ProjectMetricSeriesKey,
) {
  return row.modeValues[mode][seriesKey] ?? null;
}

function createEmptyModeValueMap(): ChartModeSeriesValues {
  return {
    trend: {},
    "moving-average": {},
    "moving-median": {},
  };
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

function pickSmoothingWindowSize(values: Array<number | null>) {
  const availableCount = values.filter((value): value is number => value != null).length;
  if (availableCount <= 1) {
    return 1;
  }

  let size = availableCount <= 4
    ? availableCount
    : Math.max(3, Math.min(9, Math.round(availableCount / 4)));

  if (size > availableCount) {
    size = availableCount;
  }
  if (size % 2 === 0) {
    size -= 1;
  }
  return Math.max(1, size);
}

function computeRollingAverage(values: Array<number | null>, windowSize: number) {
  if (values.length === 0) {
    return [];
  }
  if (windowSize <= 1) {
    return [...values];
  }

  const halfWindow = Math.floor(windowSize / 2);
  return values.map((value, index) => {
    if (value == null) {
      return null;
    }
    const start = Math.max(0, index - halfWindow);
    const end = Math.min(values.length - 1, index + halfWindow);
    const slice = values
      .slice(start, end + 1)
      .filter((item): item is number => item != null);
    if (slice.length === 0) {
      return null;
    }
    return slice.reduce((sum, item) => sum + item, 0) / slice.length;
  });
}

function computeRollingMedian(values: Array<number | null>, windowSize: number) {
  if (values.length === 0) {
    return [];
  }
  if (windowSize <= 1) {
    return [...values];
  }

  const halfWindow = Math.floor(windowSize / 2);
  return values.map((value, index) => {
    if (value == null) {
      return null;
    }
    const start = Math.max(0, index - halfWindow);
    const end = Math.min(values.length - 1, index + halfWindow);
    const slice = values
      .slice(start, end + 1)
      .filter((item): item is number => item != null);
    return computeQuantile(slice, 0.5);
  });
}

function computeTheilSenTrend(values: Array<number | null>) {
  if (values.length === 0) {
    return [];
  }

  const knownPoints = values.flatMap((value, index) => (
    value == null ? [] : [{ x: index, y: value }]
  ));

  if (knownPoints.length === 0) {
    return values.map(() => null);
  }
  if (knownPoints.length === 1) {
    const singleValue = knownPoints[0]?.y ?? null;
    return values.map((value) => (value == null ? null : singleValue));
  }

  const slopes: number[] = [];
  for (let leftIndex = 0; leftIndex < knownPoints.length; leftIndex += 1) {
    for (let rightIndex = leftIndex + 1; rightIndex < knownPoints.length; rightIndex += 1) {
      const leftPoint = knownPoints[leftIndex];
      const rightPoint = knownPoints[rightIndex];
      const deltaX = rightPoint.x - leftPoint.x;
      if (deltaX === 0) {
        continue;
      }
      slopes.push((rightPoint.y - leftPoint.y) / deltaX);
    }
  }

  const slope = computeQuantile(slopes, 0.5);
  if (slope == null) {
    return values.map(() => null);
  }

  const intercept = computeQuantile(
    knownPoints.map((point) => point.y - slope * point.x),
    0.5,
  );
  if (intercept == null) {
    return values.map(() => null);
  }

  return values.map((value, index) => (
    value == null ? null : intercept + slope * index
  ));
}

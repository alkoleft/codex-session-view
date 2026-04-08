import type { EventEntry, PlanStepEntry } from "@/backend";

const PLAN_UPDATE = "plan.update";

export type PlanUpdateRenderData = {
  explanation: string | null;
  steps: PlanStepEntry[];
};

export function planUpdateRenderData(event: EventEntry): PlanUpdateRenderData | null {
  if (event.event_type !== PLAN_UPDATE) {
    return null;
  }

  return buildPlanUpdateRenderData(event.plan_explanation, event.plan_steps);
}

export function mergedPlanUpdateRenderData(
  call: EventEntry,
  result: EventEntry,
): PlanUpdateRenderData | null {
  if (call.event_type !== PLAN_UPDATE && result.event_type !== PLAN_UPDATE) {
    return null;
  }

  return buildPlanUpdateRenderData(
    preferredPlanExplanation(result.plan_explanation, call.plan_explanation),
    preferredPlanSteps(result.plan_steps, call.plan_steps),
  );
}

function buildPlanUpdateRenderData(
  explanation: string | null | undefined,
  steps: PlanStepEntry[] | null | undefined,
): PlanUpdateRenderData | null {
  const normalizedExplanation = normalizePlanExplanation(explanation);
  const normalizedSteps = normalizePlanSteps(steps);

  if (!normalizedExplanation && normalizedSteps.length === 0) {
    return null;
  }

  return {
    explanation: normalizedExplanation,
    steps: normalizedSteps,
  };
}

function preferredPlanExplanation(
  preferred: string | null | undefined,
  fallback: string | null | undefined,
) {
  return normalizePlanExplanation(preferred) ?? normalizePlanExplanation(fallback);
}

function preferredPlanSteps(
  preferred: PlanStepEntry[] | null | undefined,
  fallback: PlanStepEntry[] | null | undefined,
) {
  const preferredSteps = normalizePlanSteps(preferred);
  if (preferredSteps.length > 0) {
    return preferredSteps;
  }

  return normalizePlanSteps(fallback);
}

function normalizePlanExplanation(value: string | null | undefined) {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

function normalizePlanSteps(steps: PlanStepEntry[] | null | undefined): PlanStepEntry[] {
  if (!steps?.length) {
    return [];
  }

  return steps.flatMap((step) => {
    const normalizedStep = step.step.trim();
    if (!normalizedStep) {
      return [];
    }

    return [{
      step: normalizedStep,
      status: normalizePlanStatus(step.status),
    }];
  });
}

function normalizePlanStatus(value: string | null | undefined) {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

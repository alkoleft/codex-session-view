export type PatchApplyStatusData = {
  variant: "success" | "failure" | "unknown";
  detailText: string | null;
  ariaLabel: string;
};

export function patchApplyStatus(
  status: string | null | undefined,
  phase: string | null | undefined,
): PatchApplyStatusData {
  const normalizedStatus = normalizePatchApplyText(status);
  if (normalizedStatus) {
    const statusKey = normalizedStatus.toLowerCase();

    if (["completed", "success", "succeeded"].includes(statusKey)) {
      return {
        variant: "success",
        detailText: null,
        ariaLabel: "Patch apply завершён успешно",
      };
    }

    if (["failed", "error"].includes(statusKey)) {
      return {
        variant: "failure",
        detailText: normalizedStatus,
        ariaLabel: `Patch apply завершён с ошибкой: ${normalizedStatus}`,
      };
    }

    return {
      variant: "unknown",
      detailText: normalizedStatus,
      ariaLabel: `Patch apply status: ${normalizedStatus}`,
    };
  }

  const normalizedPhase = normalizePatchApplyText(phase);
  if (normalizedPhase) {
    return {
      variant: "unknown",
      detailText: normalizedPhase,
      ariaLabel: `Patch apply phase: ${normalizedPhase}`,
    };
  }

  return {
    variant: "unknown",
    detailText: null,
    ariaLabel: "Patch apply status неизвестен",
  };
}

function normalizePatchApplyText(value: string | null | undefined) {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

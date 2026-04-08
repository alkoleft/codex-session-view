import { describe, expect, it } from "vitest";

import { patchApplyStatus } from "@/components/session-event-list-patch";

describe("patchApplyStatus", () => {
  it("returns success state for completed patch apply", () => {
    expect(patchApplyStatus("completed", "completed")).toEqual({
      variant: "success",
      detailText: null,
      ariaLabel: "Patch apply завершён успешно",
    });
  });

  it("returns failure state for failed patch apply", () => {
    expect(patchApplyStatus("failed", "completed")).toEqual({
      variant: "failure",
      detailText: "failed",
      ariaLabel: "Patch apply завершён с ошибкой: failed",
    });
  });

  it("falls back to phase when terminal status is absent", () => {
    expect(patchApplyStatus(null, "started")).toEqual({
      variant: "unknown",
      detailText: "started",
      ariaLabel: "Patch apply phase: started",
    });
  });
});

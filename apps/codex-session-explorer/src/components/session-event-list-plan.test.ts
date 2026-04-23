import { describe, expect, it } from "vitest";

import type { EventEntry } from "@/backend";
import {
  mergedPlanUpdateRenderData,
  planUpdateRenderData,
} from "@/components/session-event-list-plan";

function makeEvent(overrides: Partial<EventEntry> = {}): EventEntry {
  return {
    event_id: "run-1:1",
    parent_event_id: null,
    seq: 1,
    ts: "2026-04-08T12:00:00Z",
    actor_type: "agent",
    thread_id: "root-thread",
    subagent_nickname: null,
    event_type: "todo.update",
    raw_type: "response_item",
    parse_status: "parsed",
    duplicate_of: null,
    summary: "",
    category: "Lifecycle",
    meta_type: null,
    turn_id: null,
    model_context_window: null,
    collaboration_mode_kind: null,
    last_agent_message: null,
    plan_explanation: null,
    plan_steps: [],
    tool_name: null,
    receiver_thread_ids: [],
    operation_id: null,
    operation_kind: null,
    operation_root_event_id: null,
    operation_revision: null,
    operation_started_seq: null,
    operation_terminal_seq: null,
    operation_last_seq: null,
    operation_is_preferred_terminal: false,
    operation_status: null,
    phase: null,
    aggregated_output: null,
    output_value: null,
    shell_command: null,
    shell_exit_code: null,
    shell_workdir: null,
    shell_cwd: null,
    shell_yield_time_ms: null,
    shell_max_output_tokens: null,
    shell_login: null,
    shell_tty: null,
    shell_binary: null,
    shell_process_id: null,
    shell_source: null,
    shell_duration_ns: null,
    shell_original_token_count: null,
    shell_formatted_output: null,
    shell_parsed_commands: [],
    summary_pairs: [],
    input_tokens: null,
    cached_input_tokens: null,
    output_tokens: null,
    reasoning_output_tokens: null,
    total_tokens: null,
    spawn_agent: null,
    user_input_request: null,
    runtime_context_pairs: [],
    patch_apply_status: null,
    patch_apply_input: null,
    patch_apply_changes: [],
    ...overrides,
  };
}

describe("planUpdateRenderData", () => {
  it("returns structured explanation and steps for todo.update from update_plan", () => {
    expect(planUpdateRenderData(makeEvent({
      plan_explanation: "  sync state  ",
      plan_steps: [
        { step: " Inspect ", status: "completed" },
        { step: "Patch", status: " in_progress " },
      ],
    }))).toEqual({
      explanation: "sync state",
      steps: [
        { step: "Inspect", status: "completed" },
        { step: "Patch", status: "in_progress" },
      ],
    });
  });

  it("returns null when todo.update has no renderable plan content", () => {
    expect(planUpdateRenderData(makeEvent({
      plan_explanation: "   ",
      plan_steps: [{ step: "   ", status: "pending" }],
    }))).toBeNull();
  });
});

describe("mergedPlanUpdateRenderData", () => {
  it("prefers completed explanation and steps when available", () => {
    expect(mergedPlanUpdateRenderData(
      makeEvent({
        phase: "started",
        plan_explanation: "draft",
        plan_steps: [{ step: "Inspect", status: "completed" }],
      }),
      makeEvent({
        seq: 2,
        phase: "completed",
        plan_explanation: "done",
        plan_steps: [
          { step: "Inspect", status: "completed" },
          { step: "Patch", status: "completed" },
        ],
      }),
    )).toEqual({
      explanation: "done",
      steps: [
        { step: "Inspect", status: "completed" },
        { step: "Patch", status: "completed" },
      ],
    });
  });

  it("falls back to started payload when completed payload is empty", () => {
    expect(mergedPlanUpdateRenderData(
      makeEvent({
        phase: "started",
        plan_explanation: "sync state",
        plan_steps: [{ step: "Inspect", status: "completed" }],
      }),
      makeEvent({
        seq: 2,
        phase: "completed",
        plan_explanation: " ",
        plan_steps: [],
      }),
    )).toEqual({
      explanation: "sync state",
      steps: [{ step: "Inspect", status: "completed" }],
    });
  });
});

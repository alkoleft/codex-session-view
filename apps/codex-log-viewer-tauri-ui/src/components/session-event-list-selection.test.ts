import { describe, expect, it } from "vitest";

import type { EventEntry, EventNode, TimelineItem } from "@/backend";
import { sortTimelineItemsForRender } from "@/components/session-event-list-order";
import { preferredTerminalChildIndexFromSnapshot } from "@/components/session-event-list-selection";

function makeEvent(overrides: Partial<EventEntry> = {}): EventEntry {
  return {
    event_id: "run-1:1",
    parent_event_id: null,
    seq: 1,
    ts: "2026-04-08T12:00:00Z",
    actor_type: "agent",
    thread_id: "root-thread",
    subagent_nickname: null,
    event_type: "shell.call",
    raw_type: "item.started",
    parse_status: "parsed",
    duplicate_of: null,
    summary: "",
    category: "Command",
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

function makeNode(eventOverrides: Partial<EventEntry>, children: TimelineItem[] = []): EventNode {
  return {
    event: makeEvent(eventOverrides),
    children,
  };
}

function eventItem(eventOverrides: Partial<EventEntry>): TimelineItem {
  return {
    Event: makeNode(eventOverrides),
  };
}

describe("preferredTerminalChildIndexFromSnapshot", () => {
  it("returns the snapshot-selected terminal child", () => {
    const node = makeNode(
      {
        event_id: "run-1:2",
        operation_root_event_id: "run-1:2",
        operation_terminal_seq: 4,
      },
      [
        eventItem({
          event_id: "run-1:3",
          seq: 3,
          operation_root_event_id: "run-1:2",
        }),
        eventItem({
          event_id: "run-1:4",
          seq: 4,
          operation_root_event_id: "run-1:2",
          operation_is_preferred_terminal: true,
        }),
      ],
    );

    expect(preferredTerminalChildIndexFromSnapshot(node)).toBe(1);
  });

  it("falls back to the node event_id when operation_root_event_id is absent", () => {
    const node = makeNode(
      {
        event_id: "run-1:2",
        operation_terminal_seq: 3,
      },
      [
        eventItem({
          event_id: "run-1:3",
          seq: 3,
          operation_root_event_id: "run-1:2",
          operation_is_preferred_terminal: true,
        }),
      ],
    );

    expect(preferredTerminalChildIndexFromSnapshot(node)).toBe(0);
  });

  it("ignores children from a different operation root", () => {
    const node = makeNode(
      {
        event_id: "run-1:2",
        operation_root_event_id: "run-1:2",
        operation_terminal_seq: 4,
      },
      [
        eventItem({
          event_id: "run-1:4",
          seq: 4,
          operation_root_event_id: "run-1:99",
          operation_is_preferred_terminal: true,
        }),
      ],
    );

    expect(preferredTerminalChildIndexFromSnapshot(node)).toBeNull();
  });
});

describe("sortTimelineItemsForRender", () => {
  it("orders atomic events by seq and lifecycle events by terminal seq", () => {
    const items = sortTimelineItemsForRender([
      eventItem({
        event_id: "run-1:25",
        event_type: "shell.call",
        seq: 25,
        operation_terminal_seq: 33,
      }),
      eventItem({
        event_id: "run-1:28",
        event_type: "info.tokens",
        seq: 28,
      }),
      eventItem({
        event_id: "run-1:24",
        event_type: "shell.call",
        seq: 24,
        operation_terminal_seq: 31,
      }),
      eventItem({
        event_id: "run-1:35",
        event_type: "agent.reasoning",
        seq: 35,
      }),
    ]);

    expect(items.map((item) => ("Event" in item ? item.Event.event.event_id : null))).toEqual([
      "run-1:28",
      "run-1:24",
      "run-1:25",
      "run-1:35",
    ]);
  });
});

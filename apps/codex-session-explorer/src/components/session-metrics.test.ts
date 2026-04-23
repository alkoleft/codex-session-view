import { describe, expect, it } from "vitest";

import type {
  EventEntry,
  IndexedSessionSummary,
  LoadedSession,
  ThreadNode,
  TimelineItem,
  SessionPreview,
} from "@/backend";
import {
  buildSessionMetricsViewModel,
  formatDurationBetween,
} from "@/components/session-metrics";

function makeEvent(overrides: Partial<EventEntry> = {}): EventEntry {
  return {
    event_id: "run-1:1",
    parent_event_id: null,
    seq: 1,
    ts: "2026-04-08T12:00:00Z",
    actor_type: "agent",
    thread_id: "root-thread",
    subagent_nickname: null,
    event_type: "message.agent",
    raw_type: "response_item",
    parse_status: "parsed",
    duplicate_of: null,
    summary: "",
    category: "Message",
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

function eventItem(
  eventOverrides: Partial<EventEntry>,
  children: TimelineItem[] = [],
): TimelineItem {
  return {
    Event: {
      event: makeEvent(eventOverrides),
      children,
    },
  };
}

function threadItem(overrides: Partial<ThreadNode>, items: TimelineItem[] = []): TimelineItem {
  return {
    Thread: makeThread(overrides, items),
  };
}

function makeThread(overrides: Partial<ThreadNode>, items: TimelineItem[] = []): ThreadNode {
  return {
    thread_id: "root-thread",
    parent_event_id: null,
    is_root: false,
    status: null,
    role: null,
    nickname: null,
    cwd: null,
    event_count: items.length,
    child_thread_count: items.filter((item) => "Thread" in item).length,
    items,
    ...overrides,
  };
}

function makeLoadedSession(rootItems: TimelineItem[], orphanEvents: EventEntry[] = []): LoadedSession {
  return {
    session_ref: "sessions/root.jsonl",
    session_id: "root",
    tail_cursor: {
      session_ref: "sessions/root.jsonl",
      offset: 0,
      file_identity: {
        device: 1,
        inode: 1,
        size: 0,
        modified_unix_ms: 0,
      },
      pending_fragment: [],
      call_names: {},
      resolved_parent_thread_id: null,
      next_seq: 1,
      recent_dedup_keys: [],
    },
    tree: {
      source_path: "sessions/root.jsonl",
      task_id: "task-1",
      run_id: "run-1",
      event_count: rootItems.length + orphanEvents.length,
      thread_count: 3,
      root_thread_id: "root-thread",
      roots: [
        makeThread(
          {
            thread_id: "root-thread",
            is_root: true,
            status: "running",
          },
          rootItems,
        ),
      ],
      orphan_events: orphanEvents,
    },
  };
}

function makePreview(overrides: Partial<SessionPreview> = {}): SessionPreview {
  return {
    session_ref: "sessions/root.jsonl",
    session_id: "root",
    event_count: 12,
    first_ts: "2026-04-08T12:00:00Z",
    last_ts: "2026-04-08T12:08:05Z",
    indexed_summary: null,
    tail_cursor: {
      session_ref: "sessions/root.jsonl",
      offset: 0,
      file_identity: {
        device: 1,
        inode: 1,
        size: 0,
        modified_unix_ms: 0,
      },
      pending_fragment: [],
      call_names: {},
      resolved_parent_thread_id: null,
      next_seq: 1,
      recent_dedup_keys: [],
    },
    recent_events: [],
    ...overrides,
  };
}

function makeSummary(overrides: Partial<IndexedSessionSummary> = {}): IndexedSessionSummary {
  return {
    session_id: "root",
    updated_at: "2026-04-08T12:09:00Z",
    thread_name: "Add metrics panel",
    thread_name_source: "summary",
    cwd: "/repo",
    agent_name: "codex",
    tokens_used: 4321,
    created_at: "2026-04-08T12:00:00Z",
    source: null,
    model_provider: "openai",
    sandbox_policy_kind: "workspace-write",
    approval_mode: "never",
    has_user_event: true,
    archived: false,
    archived_at: null,
    git_sha: null,
    git_branch: null,
    git_origin_url: null,
    cli_version: null,
    agent_role: null,
    memory_mode: null,
    model: "gpt-5.4",
    reasoning_effort: "medium",
    agent_path: null,
    ...overrides,
  };
}

function metricValue(label: string, metrics: ReturnType<typeof buildSessionMetricsViewModel>) {
  return metrics.items.find((item) => item.label === label)?.value;
}

describe("formatDurationBetween", () => {
  it("formats short and long durations", () => {
    expect(formatDurationBetween("2026-04-08T12:00:00Z", "2026-04-08T12:00:42Z")).toBe("42s");
    expect(formatDurationBetween("2026-04-08T12:00:00Z", "2026-04-08T12:12:05Z")).toBe(
      "12m 05s",
    );
    expect(formatDurationBetween("2026-04-08T12:00:00Z", "2026-04-08T13:08:05Z")).toBe(
      "1h 08m",
    );
  });

  it("returns n/a for invalid boundaries", () => {
    expect(formatDurationBetween(null, "2026-04-08T12:00:42Z")).toBe("n/a");
    expect(formatDurationBetween("bad-ts", "2026-04-08T12:00:42Z")).toBe("n/a");
    expect(formatDurationBetween("2026-04-08T12:01:00Z", "2026-04-08T12:00:42Z")).toBe("n/a");
  });
});

describe("buildSessionMetricsViewModel", () => {
  it("deduplicates lifecycle events, excludes file.change and counts nested aggregates", () => {
    const session = makeLoadedSession(
      [
        eventItem({
          event_id: "run-1:1",
          seq: 1,
          event_type: "tool.call",
          tool_name: "search_query",
          operation_id: "tool-1",
          operation_kind: "tool",
        }, [
          eventItem({
            event_id: "run-1:2",
            seq: 2,
            event_type: "tool.result",
            tool_name: "search_query",
            operation_id: "tool-1",
            operation_kind: "tool",
            operation_is_preferred_terminal: true,
            operation_status: "completed",
          }),
        ]),
        eventItem({
          event_id: "run-1:3",
          seq: 3,
          event_type: "file.change",
          tool_name: "apply_patch",
          operation_id: "file-1",
          operation_kind: "file.change",
          operation_is_preferred_terminal: true,
          operation_status: "completed",
        }),
        eventItem({
          event_id: "run-1:4",
          seq: 4,
          event_type: "message.agent",
        }),
        eventItem({
          event_id: "run-1:5",
          seq: 5,
          event_type: "collab.spawn_agent",
          tool_name: "spawn_agent",
          operation_id: "spawn-1",
          operation_kind: "collab.spawn_agent",
          operation_is_preferred_terminal: true,
          operation_status: "completed",
        }, [
          threadItem(
            {
              thread_id: "sub-1",
              is_root: false,
            },
            [
              eventItem({
                event_id: "run-1:6",
                seq: 6,
                actor_type: "subagent",
                thread_id: "sub-1",
                event_type: "tool.call",
                tool_name: "exec_command",
                operation_id: "tool-2",
                operation_kind: "shell",
              }, [
                eventItem({
                  event_id: "run-1:7",
                  seq: 7,
                  actor_type: "subagent",
                  thread_id: "sub-1",
                  event_type: "tool.result",
                  tool_name: "exec_command",
                  operation_id: "tool-2",
                  operation_kind: "shell",
                  operation_is_preferred_terminal: true,
                  operation_status: "failed",
                }),
              ]),
            ],
          ),
        ]),
      ],
      [
        makeEvent({
          event_id: "run-1:8",
          seq: 8,
          event_type: "error",
          thread_id: null,
          operation_id: null,
          operation_kind: null,
        }),
      ],
    );

    const metrics = buildSessionMetricsViewModel({
      selectedPreview: makePreview({ event_count: 8 }),
      selectedIndexedSummary: makeSummary({ tokens_used: 4321 }),
      selectedLoadedSession: session,
    });

    expect(metricValue("Time worked", metrics)).toBe("8m 05s");
    expect(metricValue("Tool calls", metrics)).toBe("3");
    expect(metricValue("Errors", metrics)).toBe("2");
    expect(metricValue("Events", metrics)).toBe("8");
    expect(metricValue("Unique tools", metrics)).toBe("3");
    expect(metricValue("Threads", metrics)).toBe("3");
    expect(metricValue("Spawn agent", metrics)).toBe("1");
    expect(metricValue("Successes", metrics)).toBe("2");
    expect(metricValue("Success rate", metrics)).toBe("66,7%");
    expect(metricValue("Error rate", metrics)).toBe("25%");
    expect(metricValue("Messages", metrics)).toBe("1");
    expect(metricValue("Tokens", metrics)).toBe("4 321");
  });

  it("keeps operations separate across scopes when operation ids repeat", () => {
    const session = makeLoadedSession(
      [
        eventItem({
          event_id: "run-1:1",
          seq: 1,
          thread_id: "thread-a",
          event_type: "tool.call",
          tool_name: "search_query",
          operation_id: "shared-op",
          operation_kind: "tool",
          operation_root_event_id: "root-a",
        }, [
          eventItem({
            event_id: "run-1:2",
            seq: 2,
            thread_id: "thread-a",
            event_type: "tool.result",
            tool_name: "search_query",
            operation_id: "shared-op",
            operation_kind: "tool",
            operation_root_event_id: "root-a",
            operation_is_preferred_terminal: true,
            operation_status: "completed",
          }),
        ]),
        eventItem({
          event_id: "run-1:3",
          seq: 3,
          thread_id: "thread-b",
          event_type: "tool.call",
          tool_name: "exec_command",
          operation_id: "shared-op",
          operation_kind: "tool",
          operation_root_event_id: "root-b",
        }, [
          eventItem({
            event_id: "run-1:4",
            seq: 4,
            thread_id: "thread-b",
            event_type: "tool.result",
            tool_name: "exec_command",
            operation_id: "shared-op",
            operation_kind: "tool",
            operation_root_event_id: "root-b",
            operation_is_preferred_terminal: true,
            operation_status: "failed",
          }),
        ]),
      ],
    );

    const metrics = buildSessionMetricsViewModel({
      selectedPreview: makePreview({ event_count: 4 }),
      selectedIndexedSummary: makeSummary({ tokens_used: 100 }),
      selectedLoadedSession: session,
    });

    expect(metricValue("Tool calls", metrics)).toBe("2");
    expect(metricValue("Errors", metrics)).toBe("1");
    expect(metricValue("Spawn agent", metrics)).toBe("0");
    expect(metricValue("Unique tools", metrics)).toBe("2");
    expect(metricValue("Successes", metrics)).toBe("1");
    expect(metricValue("Success rate", metrics)).toBe("50%");
    expect(metricValue("Error rate", metrics)).toBe("50%");
  });

  it("includes collab operations in tool metrics", () => {
    const session = makeLoadedSession([
      eventItem({
        event_id: "run-1:1",
        seq: 1,
        event_type: "collab.spawn_agent",
        tool_name: "spawn_agent",
        operation_id: "spawn-1",
        operation_kind: "collab.spawn_agent",
        operation_is_preferred_terminal: true,
        operation_status: "completed",
      }),
      eventItem({
        event_id: "run-1:2",
        seq: 2,
        event_type: "tool.call",
        tool_name: "exec_command",
        operation_id: "tool-1",
        operation_kind: "shell",
      }),
      eventItem({
        event_id: "run-1:3",
        seq: 3,
        event_type: "tool.result",
        tool_name: "exec_command",
        operation_id: "tool-1",
        operation_kind: "shell",
        operation_is_preferred_terminal: true,
        operation_status: "completed",
      }),
    ]);

    const metrics = buildSessionMetricsViewModel({
      selectedPreview: makePreview({ event_count: 3 }),
      selectedIndexedSummary: makeSummary({ tokens_used: 10 }),
      selectedLoadedSession: session,
    });

    expect(metricValue("Tool calls", metrics)).toBe("2");
    expect(metricValue("Unique tools", metrics)).toBe("2");
    expect(metricValue("Spawn agent", metrics)).toBe("1");
  });

  it("counts collab spawn_agent failures as errors without adding tool calls", () => {
    const session = makeLoadedSession([
      eventItem({
        event_id: "run-1:1",
        seq: 1,
        event_type: "collab.spawn_agent",
        tool_name: "spawn_agent",
        operation_id: "spawn-1",
        operation_kind: "collab.spawn_agent",
        operation_is_preferred_terminal: true,
        operation_status: "failed",
      }),
      eventItem({
        event_id: "run-1:2",
        seq: 2,
        event_type: "tool.call",
        tool_name: "exec_command",
        operation_id: "tool-1",
        operation_kind: "shell",
      }),
      eventItem({
        event_id: "run-1:3",
        seq: 3,
        event_type: "tool.result",
        tool_name: "exec_command",
        operation_id: "tool-1",
        operation_kind: "shell",
        operation_is_preferred_terminal: true,
        operation_status: "completed",
      }),
    ]);

    const metrics = buildSessionMetricsViewModel({
      selectedPreview: makePreview({ event_count: 3 }),
      selectedIndexedSummary: makeSummary({ tokens_used: 10 }),
      selectedLoadedSession: session,
    });

    expect(metricValue("Errors", metrics)).toBe("1");
    expect(metricValue("Failed ops", metrics)).toBe("1");
    expect(metricValue("Tool calls", metrics)).toBe("2");
    expect(metricValue("Unique tools", metrics)).toBe("2");
    expect(metricValue("Spawn agent", metrics)).toBe("1");
    expect(metricValue("Success rate", metrics)).toBe("50%");
    expect(metricValue("Error rate", metrics)).toBe("50%");
  });

  it("counts phased spawn_agent lifecycle as a single spawn call", () => {
    const session = makeLoadedSession([
      eventItem({
        event_id: "run-1:1",
        seq: 1,
        event_type: "collab.spawn_agent",
        tool_name: "spawn_agent",
        operation_id: "spawn-1",
        operation_kind: "collab.spawn_agent",
        operation_root_event_id: "spawn-root",
      }, [
        eventItem({
          event_id: "run-1:2",
          seq: 2,
          event_type: "collab.spawn_agent",
          tool_name: "spawn_agent",
          operation_id: "spawn-1",
          operation_kind: "collab.spawn_agent",
          operation_root_event_id: "spawn-root",
          operation_is_preferred_terminal: true,
          operation_status: "completed",
        }),
      ]),
      eventItem({
        event_id: "run-1:3",
        seq: 3,
        event_type: "tool.call",
        tool_name: "exec_command",
        operation_id: "tool-1",
        operation_kind: "shell",
      }),
      eventItem({
        event_id: "run-1:4",
        seq: 4,
        event_type: "tool.result",
        tool_name: "exec_command",
        operation_id: "tool-1",
        operation_kind: "shell",
        operation_is_preferred_terminal: true,
        operation_status: "completed",
      }),
    ]);

    const metrics = buildSessionMetricsViewModel({
      selectedPreview: makePreview({ event_count: 4 }),
      selectedIndexedSummary: makeSummary({ tokens_used: 10 }),
      selectedLoadedSession: session,
    });

    expect(metricValue("Spawn agent", metrics)).toBe("1");
    expect(metricValue("Tool calls", metrics)).toBe("2");
    expect(metricValue("Successes", metrics)).toBe("2");
  });

  it("counts failed file.change operations in error metrics but not tool metrics", () => {
    const session = makeLoadedSession([
      eventItem({
        event_id: "run-1:1",
        seq: 1,
        event_type: "file.change",
        tool_name: "apply_patch",
        operation_id: "file-1",
        operation_kind: "file.change",
        operation_is_preferred_terminal: true,
        operation_status: "failed",
      }),
      eventItem({
        event_id: "run-1:2",
        seq: 2,
        event_type: "tool.call",
        tool_name: "exec_command",
        operation_id: "tool-1",
        operation_kind: "shell",
      }),
      eventItem({
        event_id: "run-1:3",
        seq: 3,
        event_type: "tool.result",
        tool_name: "exec_command",
        operation_id: "tool-1",
        operation_kind: "shell",
        operation_is_preferred_terminal: true,
        operation_status: "completed",
      }),
    ]);

    const metrics = buildSessionMetricsViewModel({
      selectedPreview: makePreview({ event_count: 3 }),
      selectedIndexedSummary: makeSummary({ tokens_used: 10 }),
      selectedLoadedSession: session,
    });

    expect(metricValue("Errors", metrics)).toBe("1");
    expect(metricValue("Failed ops", metrics)).toBe("1");
    expect(metricValue("Tool calls", metrics)).toBe("1");
    expect(metricValue("Unique tools", metrics)).toBe("1");
    expect(metricValue("Successes", metrics)).toBe("1");
  });

  it("returns n/a for loaded-only aggregates when full tree is absent", () => {
    const metrics = buildSessionMetricsViewModel({
      selectedPreview: makePreview({
        first_ts: null,
        last_ts: null,
      }),
      selectedIndexedSummary: makeSummary({ tokens_used: null }),
      selectedLoadedSession: null,
    });

    expect(metricValue("Time worked", metrics)).toBe("n/a");
    expect(metricValue("Tool calls", metrics)).toBe("n/a");
    expect(metricValue("Errors", metrics)).toBe("n/a");
    expect(metricValue("Threads", metrics)).toBe("n/a");
    expect(metricValue("Tokens", metrics)).toBe("n/a");
  });

  it("shows message counts for legacy loaded sessions without operation metadata", () => {
    const session = makeLoadedSession([
      eventItem({
        event_id: "run-1:1",
        seq: 1,
        event_type: "tool.call",
        tool_name: "search_query",
      }),
      eventItem({
        event_id: "run-1:2",
        seq: 2,
        event_type: "tool.result",
        tool_name: "search_query",
      }),
      eventItem({
        event_id: "run-1:3",
        seq: 3,
        event_type: "shell.call",
        tool_name: "exec_command",
        shell_command: "echo hello",
      }),
      eventItem({
        event_id: "run-1:4",
        seq: 4,
        event_type: "shell.result",
        tool_name: "exec_command",
        shell_exit_code: 0,
      }),
      eventItem({
        event_id: "run-1:5",
        seq: 5,
        event_type: "message.agent",
      }),
    ]);

    const metrics = buildSessionMetricsViewModel({
      selectedPreview: makePreview({ event_count: 5 }),
      selectedIndexedSummary: makeSummary({ tokens_used: 10 }),
      selectedLoadedSession: session,
    });

    expect(metricValue("Tool calls", metrics)).toBe("n/a");
    expect(metricValue("Errors", metrics)).toBe("0");
    expect(metricValue("Failed ops", metrics)).toBe("n/a");
    expect(metricValue("Spawn agent", metrics)).toBe("n/a");
    expect(metricValue("Unique tools", metrics)).toBe("n/a");
    expect(metricValue("Messages", metrics)).toBe("1");
  });

  it("falls back to n/a only for operation-dependent metrics when all relevant operations lack metadata", () => {
    const session = makeLoadedSession([
      eventItem({
        event_id: "run-1:1",
        seq: 1,
        event_type: "tool.call",
        tool_name: "search_query",
      }),
      eventItem({
        event_id: "run-1:2",
        seq: 2,
        event_type: "patch.apply",
        tool_name: "apply_patch",
        patch_apply_status: "failed",
      }),
      eventItem({
        event_id: "run-1:3",
        seq: 3,
        event_type: "mcp.call",
        tool_name: "filesystem.read",
      }),
      eventItem({
        event_id: "run-1:4",
        seq: 4,
        event_type: "message.agent",
      }),
    ]);

    const metrics = buildSessionMetricsViewModel({
      selectedPreview: makePreview({ event_count: 3 }),
      selectedIndexedSummary: makeSummary({ tokens_used: 10 }),
      selectedLoadedSession: session,
    });

    expect(metricValue("Tool calls", metrics)).toBe("n/a");
    expect(metricValue("Errors", metrics)).toBe("0");
    expect(metricValue("Failed ops", metrics)).toBe("n/a");
    expect(metricValue("Messages", metrics)).toBe("1");
  });

  it("keeps Messages available in mixed legacy and new operation data when operation metrics fall back to n/a", () => {
    const session = makeLoadedSession([
      eventItem({
        event_id: "run-1:1",
        seq: 1,
        event_type: "tool.call",
        tool_name: "search_query",
        operation_id: "tool-1",
        operation_kind: "tool",
      }),
      eventItem({
        event_id: "run-1:2",
        seq: 2,
        event_type: "tool.result",
        tool_name: "search_query",
        operation_id: "tool-1",
        operation_kind: "tool",
        operation_is_preferred_terminal: true,
        operation_status: "completed",
      }),
      eventItem({
        event_id: "run-1:3",
        seq: 3,
        event_type: "patch.apply",
        tool_name: "apply_patch",
      }),
      eventItem({
        event_id: "run-1:4",
        seq: 4,
        event_type: "mcp.call",
        tool_name: "filesystem.read",
      }),
      eventItem({
        event_id: "run-1:5",
        seq: 5,
        event_type: "message.agent",
      }),
    ]);

    const metrics = buildSessionMetricsViewModel({
      selectedPreview: makePreview({ event_count: 5 }),
      selectedIndexedSummary: makeSummary({ tokens_used: 10 }),
      selectedLoadedSession: session,
    });

    expect(metricValue("Tool calls", metrics)).toBe("1");
    expect(metricValue("Errors", metrics)).toBe("0");
    expect(metricValue("Failed ops", metrics)).toBe("0");
    expect(metricValue("Unique tools", metrics)).toBe("1");
    expect(metricValue("Messages", metrics)).toBe("1");
  });

  it("counts direct error events even when operation metadata is partial", () => {
    const session = makeLoadedSession([
      eventItem({
        event_id: "run-1:1",
        seq: 1,
        event_type: "error",
        operation_id: null,
        operation_kind: null,
      }),
      eventItem({
        event_id: "run-1:2",
        seq: 2,
        event_type: "tool.call",
        tool_name: "search_query",
        operation_id: "tool-1",
        operation_kind: "tool",
      }),
      eventItem({
        event_id: "run-1:3",
        seq: 3,
        event_type: "tool.result",
        tool_name: "search_query",
        operation_id: "tool-1",
        operation_kind: "tool",
        operation_is_preferred_terminal: true,
        operation_status: "completed",
      }),
    ]);

    const metrics = buildSessionMetricsViewModel({
      selectedPreview: makePreview({ event_count: 3 }),
      selectedIndexedSummary: makeSummary({ tokens_used: 10 }),
      selectedLoadedSession: session,
    });

    expect(metricValue("Tool calls", metrics)).toBe("1");
    expect(metricValue("Errors", metrics)).toBe("1");
    expect(metricValue("Failed ops", metrics)).toBe("0");
    expect(metricValue("Success rate", metrics)).toBe("100%");
  });
});

import { describe, expect, it } from "vitest";

import type {
  EventEntry,
  EventNode,
  LoadedSession,
  ThreadNode,
  TimelineItem,
} from "@/backend";
import {
  agentSelectionThreadIdForEvent,
  buildAgentGraphViewModel,
  preferredAgentThreadId,
} from "@/components/agent-thread-view-model";

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

function eventItem(
  eventOverrides: Partial<EventEntry>,
  children: TimelineItem[] = [],
): TimelineItem {
  return {
    Event: makeNode(eventOverrides, children),
  };
}

function threadItem(
  threadOverrides: Partial<ThreadNode>,
  items: TimelineItem[] = [],
): TimelineItem {
  return {
    Thread: makeThread(threadOverrides, items),
  };
}

function makeThread(
  overrides: Partial<ThreadNode>,
  items: TimelineItem[] = [],
): ThreadNode {
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

function makeSession(rootItems: TimelineItem[]): LoadedSession {
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
      event_count: 0,
      thread_count: 1,
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
      orphan_events: [],
    },
  };
}

describe("buildAgentGraphViewModel", () => {
  it("builds a closed agent with prompt, steps and terminal-first status", () => {
    const session = makeSession([
      eventItem({
        event_id: "run-1:1",
        event_type: "collab.spawn_agent",
        seq: 1,
        phase: "started",
        spawn_agent: {
          prompt: "Review the parser changes and summarize the risks.",
          requested_agent_type: "reviewer",
          model: "gpt-5.3-codex",
          reasoning_effort: "high",
          receiver_thread_id: "sub-1",
          receiver_nickname: "Ada",
          receiver_role: "reviewer",
          receiver_status: "pending_init",
        },
      }),
      threadItem(
        {
          thread_id: "sub-1",
          is_root: false,
          nickname: "Ada",
          role: "reviewer",
          status: "completed",
        },
        [
          eventItem({
            event_id: "run-1:3",
            actor_type: "subagent",
            thread_id: "sub-1",
            event_type: "task.started",
            seq: 3,
          }),
          eventItem({
            event_id: "run-1:4",
            actor_type: "subagent",
            thread_id: "sub-1",
            event_type: "message.assistant",
            seq: 4,
            summary: "inspected parser invariants",
          }),
          eventItem({
            event_id: "run-1:5",
            actor_type: "subagent",
            thread_id: "sub-1",
            event_type: "task.completed",
            seq: 5,
            last_agent_message: "Parser review complete",
          }),
        ],
      ),
      eventItem({
        event_id: "run-1:6",
        event_type: "collab.close_agent",
        seq: 6,
        receiver_thread_ids: ["sub-1"],
      }),
    ]);

    const graph = buildAgentGraphViewModel(session);
    expect(graph.agents).toHaveLength(1);
    expect(graph.agents[0]).toMatchObject({
      threadId: "sub-1",
      displayLabel: "Ada · reviewer",
      status: "closed",
      spawnEventId: "run-1:1",
      promptPreview: "Review the parser changes and summarize the risks.",
      waitEventIds: [],
      closeEventIds: ["run-1:6"],
      lastMeaningfulText: "Parser review complete",
    });
    expect(graph.agents[0].steps.map((step) => step.kind)).toEqual([
      "spawn",
      "activity",
      "completed",
      "closed",
    ]);
  });

  it("tracks duplicate spawn and unbound collab events without backend fallbacks", () => {
    const session = makeSession([
      eventItem({
        event_id: "run-1:1",
        event_type: "collab.spawn_agent",
        seq: 1,
        spawn_agent: {
          prompt: "First spawn",
          requested_agent_type: "reviewer",
          model: null,
          reasoning_effort: null,
          receiver_thread_id: "sub-1",
          receiver_nickname: "Ada",
          receiver_role: "reviewer",
          receiver_status: null,
        },
      }),
      eventItem({
        event_id: "run-1:2",
        event_type: "collab.spawn_agent",
        seq: 2,
        spawn_agent: {
          prompt: "Duplicate spawn",
          requested_agent_type: "reviewer",
          model: null,
          reasoning_effort: null,
          receiver_thread_id: "sub-1",
          receiver_nickname: "Ada",
          receiver_role: "reviewer",
          receiver_status: null,
        },
      }),
      eventItem({
        event_id: "run-1:3",
        event_type: "collab.wait",
        seq: 3,
      }),
      threadItem(
        {
          thread_id: "sub-1",
          is_root: false,
          nickname: "Ada",
          role: "reviewer",
          status: "running",
        },
        [
          eventItem({
            event_id: "run-1:4",
            actor_type: "subagent",
            thread_id: "sub-1",
            event_type: "message.assistant",
            seq: 4,
            summary: "working",
          }),
        ],
      ),
    ]);

    const graph = buildAgentGraphViewModel(session);
    expect(graph.unboundEventIds).toEqual(["run-1:3"]);
    expect(graph.agents[0].spawnEventId).toBe("run-1:1");
    expect(graph.agents[0].unboundEventIds).toEqual(["run-1:2"]);
  });

  it("derives parent thread ids for nested subagents and respects failed-over-close priority", () => {
    const session = makeSession([
      eventItem({
        event_id: "run-1:1",
        event_type: "collab.spawn_agent",
        seq: 1,
        spawn_agent: {
          prompt: "Spawn reviewer",
          requested_agent_type: "reviewer",
          model: null,
          reasoning_effort: null,
          receiver_thread_id: "sub-1",
          receiver_nickname: "Ada",
          receiver_role: "reviewer",
          receiver_status: null,
        },
      }),
      threadItem(
        {
          thread_id: "sub-1",
          is_root: false,
          nickname: "Ada",
          role: "reviewer",
          status: "running",
        },
        [
          eventItem({
            event_id: "run-1:2",
            actor_type: "subagent",
            thread_id: "sub-1",
            event_type: "task.started",
            seq: 2,
          }),
          eventItem({
            event_id: "run-1:3",
            actor_type: "subagent",
            thread_id: "sub-1",
            event_type: "agent.failed",
            seq: 3,
            summary: "tool timeout",
          }),
          threadItem(
            {
              thread_id: "sub-2",
              is_root: false,
              nickname: "Halley",
              role: "worker",
              status: "running",
            },
            [
              eventItem({
                event_id: "run-1:4",
                actor_type: "subagent",
                thread_id: "sub-2",
                event_type: "message.assistant",
                seq: 4,
                summary: "nested work",
              }),
            ],
          ),
        ],
      ),
      eventItem({
        event_id: "run-1:5",
        event_type: "collab.close_agent",
        seq: 5,
        receiver_thread_ids: ["sub-1"],
      }),
    ]);

    const graph = buildAgentGraphViewModel(session);
    expect(graph.byThreadId["sub-1"]).toMatchObject({
      parentThreadId: "root-thread",
      status: "failed",
    });
    expect(graph.byThreadId["sub-2"]).toMatchObject({
      parentThreadId: "sub-1",
      status: "running",
    });
  });

  it("discovers subagent threads anchored under event children", () => {
    const session = makeSession([
      eventItem({
        event_id: "run-1:1",
        event_type: "collab.spawn_agent",
        seq: 1,
        spawn_agent: {
          prompt: "Spawn Franklin",
          requested_agent_type: "reviewer",
          model: null,
          reasoning_effort: null,
          receiver_thread_id: "sub-1",
          receiver_nickname: "Franklin",
          receiver_role: "reviewer",
          receiver_status: null,
        },
      }, [
        threadItem(
          {
            thread_id: "sub-1",
            is_root: false,
            nickname: "Franklin",
            role: "reviewer",
            status: "running",
          },
          [
            eventItem({
              event_id: "run-1:2",
              actor_type: "subagent",
              thread_id: "sub-1",
              event_type: "message.assistant",
              seq: 2,
              summary: "review in progress",
            }),
          ],
        ),
      ]),
    ]);

    const graph = buildAgentGraphViewModel(session);
    expect(graph.agents).toHaveLength(1);
    expect(graph.byThreadId["sub-1"]).toMatchObject({
      parentThreadId: "root-thread",
      spawnEventId: "run-1:1",
      status: "running",
    });
  });
});

describe("agent selection helpers", () => {
  it("prefers active agents and resolves single-target selection from events", () => {
    const session = makeSession([
      eventItem({
        event_id: "run-1:1",
        event_type: "collab.spawn_agent",
        seq: 1,
        spawn_agent: {
          prompt: "Spawn Ada",
          requested_agent_type: "reviewer",
          model: null,
          reasoning_effort: null,
          receiver_thread_id: "sub-1",
          receiver_nickname: "Ada",
          receiver_role: "reviewer",
          receiver_status: null,
        },
      }),
      threadItem(
        {
          thread_id: "sub-1",
          is_root: false,
          nickname: "Ada",
          role: "reviewer",
          status: "running",
        },
        [
          eventItem({
            event_id: "run-1:2",
            actor_type: "subagent",
            thread_id: "sub-1",
            event_type: "message.assistant",
            seq: 2,
            summary: "working",
          }),
        ],
      ),
    ]);

    const graph = buildAgentGraphViewModel(session);
    expect(preferredAgentThreadId(graph)).toBe("sub-1");
    expect(agentSelectionThreadIdForEvent(makeEvent({
      actor_type: "subagent",
      thread_id: "sub-1",
      event_type: "message.assistant",
    }))).toBe("sub-1");
    expect(agentSelectionThreadIdForEvent(makeEvent({
      event_type: "collab.wait",
      receiver_thread_ids: ["sub-1", "sub-2"],
    }))).toBeNull();
    expect(agentSelectionThreadIdForEvent(makeEvent({
      event_type: "collab.close_agent",
      receiver_thread_ids: ["sub-1"],
    }))).toBe("sub-1");
  });
});

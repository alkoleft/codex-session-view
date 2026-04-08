import { trackedInvoke } from "@/lib/tauri";

export type DetectCodexHomeResponse = {
  detected_home: string | null;
};

export type ResolvedCodexHome = {
  root: string;
  sessions_dir: string;
  session_index_path: string;
};

export type InitializeCodexHomeResponse = {
  resolved_home: ResolvedCodexHome;
};

export type SessionSummary = {
  session_id: string;
  session_ref: string;
  updated_at: string;
  thread_name: string | null;
  cwd: string | null;
  is_active_like: boolean;
  index_status: string;
};

export type SessionDiagnostic = {
  kind: string;
  message: string;
  session_id: string | null;
  session_refs: string[];
};

export type SessionCatalogPage = {
  items: SessionSummary[];
  next_cursor: string | null;
  diagnostics: SessionDiagnostic[];
};

export type IndexedSessionSummary = {
  session_id: string;
  updated_at: string;
  thread_name: string | null;
  thread_name_source: string | null;
  cwd: string | null;
  agent_name: string | null;
  tokens_used: number | null;
  created_at: string | null;
  source: string | null;
  model_provider: string | null;
  sandbox_policy_kind: string | null;
  approval_mode: string | null;
  has_user_event: boolean | null;
  archived: boolean | null;
  archived_at: string | null;
  git_sha: string | null;
  git_branch: string | null;
  git_origin_url: string | null;
  cli_version: string | null;
  agent_role: string | null;
  memory_mode: string | null;
  model: string | null;
  reasoning_effort: string | null;
  agent_path: string | null;
};

export type IndexedSessionCatalogPage = {
  items: IndexedSessionSummary[];
  next_cursor: string | null;
  diagnostics: SessionDiagnostic[];
};

export type ListSessionsArgs = {
  query?: string;
  cursor?: string | null;
  limit?: number;
};

export type FileIdentity = {
  device: number | null;
  inode: number | null;
  size: number;
  modified_unix_ms: number | null;
};

export type TailCursor = {
  session_ref: string;
  offset: number;
  file_identity: FileIdentity;
  pending_fragment: number[];
  call_names: Record<string, string>;
  resolved_parent_thread_id: string | null;
  next_seq: number;
  recent_dedup_keys: string[];
};

export type SpawnAgentEntry = {
  prompt: string | null;
  requested_agent_type: string | null;
  model: string | null;
  reasoning_effort: string | null;
  receiver_thread_id: string | null;
  receiver_nickname: string | null;
  receiver_role: string | null;
  receiver_status: string | null;
};

export type UserInputOptionEntry = {
  label: string;
  description: string | null;
};

export type UserInputQuestionEntry = {
  header: string | null;
  id: string | null;
  question: string | null;
  options: UserInputOptionEntry[];
  answers: string[];
};

export type UserInputAnswerEntry = {
  id: string;
  answers: string[];
};

export type UserInputRequestEntry = {
  questions: UserInputQuestionEntry[];
  extra_answers: UserInputAnswerEntry[];
};

export type PatchApplyChangeEntry = {
  path: string;
  change_type: string | null;
  unified_diff: string | null;
  move_path: string | null;
};

export type ShellParsedCommandEntry = {
  kind: string | null;
  command: string | null;
  query: string | null;
  name: string | null;
  path: string | null;
};

export type EventEntry = {
  event_id: string;
  parent_event_id: string | null;
  seq: number;
  ts: string;
  actor_type: string | null;
  thread_id: string | null;
  subagent_nickname: string | null;
  event_type: string;
  raw_type: string;
  parse_status: string;
  duplicate_of: string | null;
  summary: string;
  category: string;
  meta_type: string | null;
  turn_id: string | null;
  model_context_window: string | null;
  collaboration_mode_kind: string | null;
  last_agent_message: string | null;
  tool_name: string | null;
  receiver_thread_ids: string[];
  operation_id: string | null;
  operation_kind: string | null;
  operation_root_event_id: string | null;
  operation_revision: number | null;
  operation_started_seq: number | null;
  operation_terminal_seq: number | null;
  operation_last_seq: number | null;
  operation_is_preferred_terminal: boolean;
  phase: string | null;
  aggregated_output: string | null;
  output_value: unknown;
  shell_command: string | null;
  shell_exit_code: number | null;
  shell_workdir: string | null;
  shell_cwd: string | null;
  shell_yield_time_ms: number | null;
  shell_max_output_tokens: number | null;
  shell_login: boolean | null;
  shell_tty: boolean | null;
  shell_binary: string | null;
  shell_process_id: string | null;
  shell_source: string | null;
  shell_duration_ns: number | null;
  shell_original_token_count: number | null;
  shell_formatted_output: string | null;
  shell_parsed_commands: ShellParsedCommandEntry[];
  summary_pairs: Array<[string, string]>;
  input_tokens: number | null;
  cached_input_tokens: number | null;
  output_tokens: number | null;
  reasoning_output_tokens: number | null;
  total_tokens: number | null;
  spawn_agent: SpawnAgentEntry | null;
  user_input_request: UserInputRequestEntry | null;
  runtime_context_pairs: Array<[string, string]>;
  patch_apply_status: string | null;
  patch_apply_input: string | null;
  patch_apply_changes: PatchApplyChangeEntry[];
};

export type EventNode = {
  event: EventEntry;
  children: TimelineItem[];
};

export type ThreadNode = {
  thread_id: string;
  parent_event_id: string | null;
  is_root: boolean;
  status: string | null;
  role: string | null;
  nickname: string | null;
  cwd: string | null;
  event_count: number;
  child_thread_count: number;
  items: TimelineItem[];
};

export type TimelineItem = { Event: EventNode } | { Thread: ThreadNode };

export type EventTree = {
  source_path: string;
  task_id: string;
  run_id: string;
  event_count: number;
  thread_count: number;
  root_thread_id: string;
  roots: ThreadNode[];
  orphan_events: EventEntry[];
};

export type EventRecord = {
  schema_version: number;
  ts: string;
  task_id: string;
  run_id: string;
  seq: number;
  event_type: string;
  raw_type: string;
  parse_status: string;
  payload: Record<string, unknown>;
};

export type SessionPreviewEvent = {
  seq: number;
  ts: string;
  event_type: string;
  raw_type: string;
  summary: string;
  category: string;
  text: string | null;
};

export type SessionPreview = {
  session_ref: string;
  session_id: string;
  event_count: number;
  first_ts: string | null;
  last_ts: string | null;
  indexed_summary: IndexedSessionSummary | null;
  tail_cursor: TailCursor;
  recent_events: SessionPreviewEvent[];
};

export type LoadedSession = {
  session_ref: string;
  session_id: string;
  tree: EventTree;
  tail_cursor: TailCursor;
};

export type TailResult = {
  events: EventRecord[];
  next_cursor: TailCursor;
  reset: boolean;
  raw_bytes: number[];
  tool_counts: Record<string, number>;
  subagent_counts: Record<string, number>;
};

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

function hasTauriRuntime() {
  return typeof window !== "undefined" && typeof window.__TAURI_INTERNALS__ !== "undefined";
}

async function invokeBackend<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!hasTauriRuntime()) {
    throw new Error("Desktop backend is available only inside the Tauri app runtime.");
  }
  return trackedInvoke<T>(command, args);
}

export function extractErrorMessage(error: unknown) {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === "string") {
    return error;
  }
  return "Unexpected viewer backend error.";
}

export function detectCodexHome() {
  return invokeBackend<DetectCodexHomeResponse>("detect_codex_home");
}

export function initializeCodexHome() {
  return invokeBackend<InitializeCodexHomeResponse>("initialize_codex_home");
}

export function listSessions(args: ListSessionsArgs = {}) {
  const query =
    args.query && args.query.trim().length > 0 ? args.query.trim() : null;
  return invokeBackend<SessionCatalogPage>("list_sessions", {
    limit: args.limit ?? 40,
    query,
    cursor: args.cursor ?? null,
  });
}

export function listIndexedSessions(args: ListSessionsArgs = {}) {
  const query =
    args.query && args.query.trim().length > 0 ? args.query.trim() : null;
  return invokeBackend<IndexedSessionCatalogPage>("list_indexed_sessions", {
    limit: args.limit ?? 40,
    query,
    cursor: args.cursor ?? null,
  });
}

export function loadSessionPreview(sessionRef: string) {
  return invokeBackend<SessionPreview>("load_session_preview", {
    sessionRef,
    eventLimit: 80,
  });
}

export function loadSession(sessionRef: string) {
  return invokeBackend<LoadedSession>("load_session", {
    sessionRef,
    textLimit: 120,
  });
}

export function loadSessionPreviewById(sessionId: string) {
  return invokeBackend<SessionPreview>("load_session_preview_by_id", {
    sessionId,
    eventLimit: 80,
  });
}

export function tailSession(sessionRef: string, tailCursor: TailCursor) {
  return invokeBackend<TailResult>("tail_session", {
    sessionRef,
    tailCursor,
  });
}

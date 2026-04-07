import { invoke } from "@tauri-apps/api/core";

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
  message_text: string | null;
  tool_name: string | null;
  phase: string | null;
  shell_command: string | null;
  shell_exit_code: number | null;
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

export type TimelineItem =
  | { Event: EventNode }
  | { Thread: ThreadNode };

export type EventTree = {
  source_path: string;
  task_id: string;
  run_id: string;
  event_count: number;
  thread_count: number;
  root_thread_id: string;
  roots: ThreadNode[];
  orphan_events: EventEntry[];
  standalone_startup_metadata: Record<string, unknown> | null;
};

export type LoadedSession = {
  session_ref: string;
  session_id: string;
  tree: EventTree;
  tail_cursor: TailCursor;
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
  tail_cursor: TailCursor;
  recent_events: SessionPreviewEvent[];
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
  return invoke<T>(command, args);
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

export function loadSession(sessionRef: string) {
  return invokeBackend<LoadedSession>("load_session", {
    sessionRef,
    textLimit: 240,
  });
}

export function loadSessionPreview(sessionRef: string) {
  return invokeBackend<SessionPreview>("load_session_preview", {
    sessionRef,
    eventLimit: 80,
  });
}

export function tailSession(sessionRef: string, tailCursor: TailCursor) {
  return invokeBackend<TailResult>("tail_session", {
    sessionRef,
    tailCursor,
  });
}

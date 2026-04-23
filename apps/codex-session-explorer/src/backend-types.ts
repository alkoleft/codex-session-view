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
  exactSessionId?: string | null;
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

export type PlanStepEntry = {
  step: string;
  status: string | null;
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
  plan_explanation: string | null;
  plan_steps: PlanStepEntry[];
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
  operation_status: string | null;
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

export type MetricCoverage = "known" | "partial" | "unknown";

export type MetricSource =
  | "normalized_events"
  | "operation_projection"
  | "event_tree"
  | "indexed_session_metadata"
  | "session_metadata"
  | "derived"
  | "unavailable";

export type CoveredMetric<T> = {
  value: T | null;
  coverage: MetricCoverage;
  source: MetricSource;
};

export type ProjectIdentity = {
  project_key: string;
  state: "normal" | "degraded";
  cwd: string | null;
  normalized_cwd: string | null;
  git_origin_url: string | null;
  git_branch: string | null;
  git_sha: string | null;
};

export type FactorMetadata = {
  model: string | null;
  reasoning_effort: string | null;
  cli_version: string | null;
  sandbox_policy_kind: string | null;
  approval_mode: string | null;
  agent_role: string | null;
  skills_count: CoveredMetric<number>;
  mcp_server_count: CoveredMetric<number>;
  mcp_call_count: CoveredMetric<number>;
  start_context_size: CoveredMetric<number>;
};

export type SessionOutcome = "completed" | "failed" | "aborted" | "interrupted" | "unknown";

export type OutcomeSummary = {
  outcome: SessionOutcome;
  coverage: MetricCoverage;
  error_type: string | null;
};

export type ToolCommandCategory =
  | "search"
  | "edit"
  | "web_search"
  | "test"
  | "build"
  | "git"
  | "filesystem"
  | "mcp"
  | "collaboration"
  | "other";

export type ToolCategoryMetrics = {
  category: ToolCommandCategory;
  count: number;
  failures: number;
  duration_ms: CoveredMetric<number>;
  token_contribution: CoveredMetric<number>;
};

export type DurationBreakdown = {
  total_ms: CoveredMetric<number>;
  generation_ms: CoveredMetric<number>;
  tool_ms: CoveredMetric<number>;
  shell_ms: CoveredMetric<number>;
  mcp_ms: CoveredMetric<number>;
  spawn_agent_ms: CoveredMetric<number>;
  idle_unknown_ms: CoveredMetric<number>;
};

export type TokenLedger = {
  total: CoveredMetric<number>;
  input: CoveredMetric<number>;
  output: CoveredMetric<number>;
  cached_input: CoveredMetric<number>;
  reasoning_output: CoveredMetric<number>;
  tool_call: CoveredMetric<number>;
  task: CoveredMetric<number>;
  spawn_agent: CoveredMetric<number>;
};

export type OperationMetrics = {
  operation_count: CoveredMetric<number>;
  successful_operations: CoveredMetric<number>;
  failed_operations: CoveredMetric<number>;
  tool_calls: CoveredMetric<number>;
  shell_calls: CoveredMetric<number>;
  mcp_calls: CoveredMetric<number>;
  collaboration_calls: CoveredMetric<number>;
  spawn_agent_calls: CoveredMetric<number>;
};

export type SessionMetrics = {
  session_id: string;
  metrics_schema_version: number;
  source_projection_version: number;
  computed_at: string;
  started_at: string | null;
  ended_at: string | null;
  project: ProjectIdentity;
  factors: FactorMetadata;
  outcome: OutcomeSummary;
  event_count: CoveredMetric<number>;
  thread_count: CoveredMetric<number>;
  message_count: CoveredMetric<number>;
  error_count: CoveredMetric<number>;
  abort_count: CoveredMetric<number>;
  failure_count: CoveredMetric<number>;
  operations: OperationMetrics;
  duration: DurationBreakdown;
  token_ledger: TokenLedger;
  tool_breakdown: ToolCategoryMetrics[];
  task_metrics: {
    task_count: CoveredMetric<number>;
    turn_count: CoveredMetric<number>;
    agent_work_item_count: CoveredMetric<number>;
  };
  business_review: {
    review_cycles: CoveredMetric<number>;
    review_findings: CoveredMetric<number>;
  };
  context: {
    start_context_size: CoveredMetric<number>;
    context_growth: CoveredMetric<number>;
    compaction_events: CoveredMetric<number>;
    context_compression: CoveredMetric<number>;
  };
  quality: {
    feedback_score: CoveredMetric<number>;
    evaluator_result_count: CoveredMetric<number>;
    guardrail_trigger_count: CoveredMetric<number>;
    handoff_count: CoveredMetric<number>;
  };
  baseline: {
    coverage: MetricCoverage;
    token_usage_delta: CoveredMetric<number>;
    duration_delta_ms: CoveredMetric<number>;
    error_rate_delta: CoveredMetric<number>;
    outcome_rate_delta: CoveredMetric<number>;
  };
  derived_efficiency: {
    tokens_per_successful_session: CoveredMetric<number>;
    tokens_per_accepted_task: CoveredMetric<number>;
    review_findings_per_1k_tokens: CoveredMetric<number>;
  };
};

export type SessionMetricsQuery = {
  project_key: string;
  start_ts: string | null;
  end_ts: string | null;
  include_spawn_agents: boolean;
};

export type ProjectMetricsResponse = {
  project_key: string;
  session_count: number;
  contributing_session_ids: string[];
  sessions: SessionMetrics[];
  token_ledger: TokenLedger;
  duration_ms: CoveredMetric<number>;
  baseline: SessionMetrics["baseline"];
  derived_efficiency: SessionMetrics["derived_efficiency"];
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
  metrics?: SessionMetrics | null;
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

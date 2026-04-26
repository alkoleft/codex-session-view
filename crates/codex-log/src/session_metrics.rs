use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::events::operation_stream::{
    project_operation_stream, OperationKind, OperationProjection, OperationSnapshot,
};
use crate::events::record::EventRecord;
use crate::events::types::{
    AGENT_ABORTED, AGENT_COMPLETED, AGENT_FAILED, AGENT_REASONING, CONTEXT_COMPACTED, ERROR,
    INFO_TOKENS, MCP_CALL, MESSAGE_AGENT, MESSAGE_COMMENTARY, MESSAGE_USER, RUNTIME_CONTEXT,
    SHELL_CALL, TASK_COMPLETED, TASK_STARTED,
};
use crate::session::{IndexedSessionSummary, StateThreadSummary};
use crate::tree::{EventTree, TimelineItem};
use crate::util::{hash8, normalize_path, utc_now_iso};

pub const METRICS_SCHEMA_VERSION: u32 = 5;
pub const METRICS_PROJECTION_VERSION: u32 = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MetricCoverage {
    #[default]
    Unknown,
    Known,
    Partial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoveredMetric<T> {
    pub value: Option<T>,
    pub coverage: MetricCoverage,
    pub source: MetricSource,
}

impl<T> CoveredMetric<T> {
    pub fn known(value: T, source: MetricSource) -> Self {
        Self {
            value: Some(value),
            coverage: MetricCoverage::Known,
            source,
        }
    }

    pub fn partial(value: T, source: MetricSource) -> Self {
        Self {
            value: Some(value),
            coverage: MetricCoverage::Partial,
            source,
        }
    }

    pub fn unknown() -> Self {
        Self {
            value: None,
            coverage: MetricCoverage::Unknown,
            source: MetricSource::Unavailable,
        }
    }
}

impl<T> Default for CoveredMetric<T> {
    fn default() -> Self {
        Self::unknown()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MetricSource {
    NormalizedEvents,
    OperationProjection,
    EventTree,
    IndexedSessionMetadata,
    SessionMetadata,
    Derived,
    #[default]
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectIdentityState {
    Normal,
    Degraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionScope {
    #[default]
    Unknown,
    Main,
    Subsession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionScopeFilter {
    #[default]
    All,
    Main,
    Subsession,
}

impl SessionScopeFilter {
    fn matches(self, scope: SessionScope) -> bool {
        match self {
            Self::All => true,
            Self::Main => scope == SessionScope::Main,
            Self::Subsession => scope == SessionScope::Subsession,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SessionScopeCounts {
    pub main: u64,
    pub subsession: u64,
    pub unknown: u64,
}

impl SessionScopeCounts {
    pub fn record(&mut self, scope: SessionScope) {
        match scope {
            SessionScope::Main => self.main += 1,
            SessionScope::Subsession => self.subsession += 1,
            SessionScope::Unknown => self.unknown += 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectIdentity {
    pub project_key: String,
    pub state: ProjectIdentityState,
    pub cwd: Option<String>,
    pub normalized_cwd: Option<String>,
    pub git_origin_url: Option<String>,
    pub git_branch: Option<String>,
    pub git_sha: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactorMetadata {
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub cli_version: Option<String>,
    pub sandbox_policy_kind: Option<String>,
    pub approval_mode: Option<String>,
    pub agent_role: Option<String>,
    pub skills_count: CoveredMetric<u64>,
    pub mcp_server_count: CoveredMetric<u64>,
    pub mcp_call_count: CoveredMetric<u64>,
    pub start_context_size: CoveredMetric<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionOutcome {
    Completed,
    Failed,
    Aborted,
    Interrupted,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomeSummary {
    pub outcome: SessionOutcome,
    pub coverage: MetricCoverage,
    pub error_type: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCommandCategory {
    Search,
    Edit,
    WebSearch,
    Test,
    Build,
    Git,
    Filesystem,
    Mcp,
    Collaboration,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCategoryMetrics {
    pub category: ToolCommandCategory,
    pub count: u64,
    pub failures: u64,
    pub duration_ms: CoveredMetric<u64>,
    pub token_contribution: CoveredMetric<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurationBreakdown {
    pub total_ms: CoveredMetric<u64>,
    pub generation_ms: CoveredMetric<u64>,
    pub tool_ms: CoveredMetric<u64>,
    pub shell_ms: CoveredMetric<u64>,
    pub mcp_ms: CoveredMetric<u64>,
    pub spawn_agent_ms: CoveredMetric<u64>,
    pub idle_unknown_ms: CoveredMetric<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenLedger {
    pub total: CoveredMetric<u64>,
    pub input: CoveredMetric<u64>,
    pub output: CoveredMetric<u64>,
    pub cached_input: CoveredMetric<u64>,
    #[serde(default = "unknown_u64_metric")]
    pub reasoning_output: CoveredMetric<u64>,
    pub tool_call: CoveredMetric<u64>,
    pub task: CoveredMetric<u64>,
    pub spawn_agent: CoveredMetric<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationMetrics {
    pub operation_count: CoveredMetric<u64>,
    pub successful_operations: CoveredMetric<u64>,
    pub failed_operations: CoveredMetric<u64>,
    pub tool_calls: CoveredMetric<u64>,
    pub shell_calls: CoveredMetric<u64>,
    pub mcp_calls: CoveredMetric<u64>,
    pub collaboration_calls: CoveredMetric<u64>,
    pub spawn_agent_calls: CoveredMetric<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskMetrics {
    pub task_count: CoveredMetric<u64>,
    pub turn_count: CoveredMetric<u64>,
    pub agent_work_item_count: CoveredMetric<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskClass {
    Implementation,
    Review,
    Analysis,
    Planning,
    Approval,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskClassSource {
    CollaborationMode,
    AgentRole,
    RequestedAgentType,
    ReceiverRole,
    AmbiguousSignals,
    #[default]
    Unclassified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskClassConfidence {
    Confident,
    Partial,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UsedSkillsMetrics {
    #[serde(default)]
    pub identifiers: Vec<String>,
    #[serde(default)]
    pub count: CoveredMetric<u64>,
    #[serde(default)]
    pub coverage: MetricCoverage,
    #[serde(default)]
    pub source: MetricSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UsedSkillRollupEntry {
    pub identifier: String,
    pub usage_count: u64,
    pub session_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UsedSkillsRollup {
    #[serde(default)]
    pub skills: Vec<UsedSkillRollupEntry>,
    #[serde(default)]
    pub count: CoveredMetric<u64>,
    #[serde(default)]
    pub coverage: MetricCoverage,
    #[serde(default)]
    pub source: MetricSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProjectFactorRollups {
    #[serde(default = "unknown_u64_metric")]
    pub start_context_size: CoveredMetric<u64>,
    #[serde(default = "unknown_u64_metric")]
    pub skills_count: CoveredMetric<u64>,
    #[serde(default = "unknown_u64_metric")]
    pub mcp_server_count: CoveredMetric<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProjectOperationRollups {
    #[serde(default = "unknown_u64_metric")]
    pub spawn_agent_calls: CoveredMetric<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProjectTaskRollups {
    #[serde(default = "unknown_u64_metric")]
    pub task_count: CoveredMetric<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TaskFactRawSignals {
    pub agent_role: Option<String>,
    pub requested_agent_type: Option<String>,
    pub receiver_role: Option<String>,
    pub collaboration_mode_kind: Option<String>,
    pub model_context_window: Option<String>,
    pub actor_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskMetricsFact {
    pub analytic_key: String,
    pub session_id: String,
    pub project_key: String,
    #[serde(default)]
    pub session_scope: SessionScope,
    pub run_task_id: String,
    pub turn_id: String,
    pub thread_id: Option<String>,
    pub parent_thread_id: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub started_seq: u64,
    pub ended_seq: Option<u64>,
    pub outcome: OutcomeSummary,
    pub token_ledger: TokenLedger,
    pub duration: DurationBreakdown,
    pub operations: OperationMetrics,
    #[serde(default)]
    pub task_class: TaskClass,
    #[serde(default)]
    pub task_class_source: TaskClassSource,
    #[serde(default)]
    pub task_class_confidence: TaskClassConfidence,
    #[serde(default)]
    pub raw_signals: TaskFactRawSignals,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusinessReviewMetrics {
    pub review_cycles: CoveredMetric<u64>,
    pub review_findings: CoveredMetric<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextMetrics {
    pub start_context_size: CoveredMetric<u64>,
    pub context_growth: CoveredMetric<i64>,
    pub compaction_events: CoveredMetric<u64>,
    #[serde(default = "unknown_u64_metric")]
    pub context_compression: CoveredMetric<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityMetrics {
    pub feedback_score: CoveredMetric<i64>,
    pub evaluator_result_count: CoveredMetric<u64>,
    pub guardrail_trigger_count: CoveredMetric<u64>,
    pub handoff_count: CoveredMetric<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedEfficiencyMetrics {
    pub tokens_per_successful_session: CoveredMetric<f64>,
    pub tokens_per_accepted_task: CoveredMetric<f64>,
    pub review_findings_per_1k_tokens: CoveredMetric<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineComparison {
    pub coverage: MetricCoverage,
    pub token_usage_delta: CoveredMetric<i64>,
    pub duration_delta_ms: CoveredMetric<i64>,
    pub error_rate_delta: CoveredMetric<f64>,
    pub outcome_rate_delta: CoveredMetric<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMetrics {
    pub session_id: String,
    pub metrics_schema_version: u32,
    pub source_projection_version: u32,
    pub computed_at: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub project: ProjectIdentity,
    #[serde(default)]
    pub session_scope: SessionScope,
    pub factors: FactorMetadata,
    pub outcome: OutcomeSummary,
    pub event_count: CoveredMetric<u64>,
    pub thread_count: CoveredMetric<u64>,
    pub message_count: CoveredMetric<u64>,
    pub error_count: CoveredMetric<u64>,
    pub abort_count: CoveredMetric<u64>,
    pub failure_count: CoveredMetric<u64>,
    pub operations: OperationMetrics,
    pub duration: DurationBreakdown,
    pub token_ledger: TokenLedger,
    pub tool_breakdown: Vec<ToolCategoryMetrics>,
    pub task_metrics: TaskMetrics,
    #[serde(default)]
    pub task_facts: Vec<TaskMetricsFact>,
    #[serde(default)]
    pub used_skills: UsedSkillsMetrics,
    pub business_review: BusinessReviewMetrics,
    pub context: ContextMetrics,
    pub quality: QualityMetrics,
    pub baseline: BaselineComparison,
    pub derived_efficiency: DerivedEfficiencyMetrics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnAgentAggregation {
    Include,
    Exclude,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMetricsQuery {
    pub project_key: String,
    pub start_ts: Option<String>,
    pub end_ts: Option<String>,
    pub include_spawn_agents: bool,
    #[serde(default)]
    pub session_scope_filter: SessionScopeFilter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectMetricsResponse {
    pub project_key: String,
    pub session_count: u64,
    pub contributing_session_ids: Vec<String>,
    #[serde(default)]
    pub scope_filter: SessionScopeFilter,
    #[serde(default)]
    pub available_scope_counts: SessionScopeCounts,
    pub sessions: Vec<SessionMetrics>,
    pub token_ledger: TokenLedger,
    pub duration_ms: CoveredMetric<u64>,
    #[serde(default)]
    pub factors: ProjectFactorRollups,
    #[serde(default)]
    pub operations: ProjectOperationRollups,
    #[serde(default)]
    pub task_metrics: ProjectTaskRollups,
    #[serde(default)]
    pub task_facts: Vec<TaskMetricsFact>,
    #[serde(default)]
    pub used_skills: UsedSkillsRollup,
    pub baseline: BaselineComparison,
    pub derived_efficiency: DerivedEfficiencyMetrics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionDetailTextSource {
    IndexedFirstUserMessage,
    IndexedTitle,
    MessageUser,
    TaskStarted,
    #[default]
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectMetricsSessionDetail {
    pub session_id: String,
    pub session_ref: String,
    pub title: Option<String>,
    pub start_user_request: Option<String>,
    pub start_user_request_source: SessionDetailTextSource,
    pub task_summary: Option<String>,
    pub task_summary_source: SessionDetailTextSource,
    pub agent_role: Option<String>,
    pub task_class: Option<TaskClass>,
    pub task_class_confidence: TaskClassConfidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricsRebuildVersion {
    pub metrics_schema_version: u32,
    pub source_projection_version: u32,
}

impl MetricsRebuildVersion {
    pub fn current() -> Self {
        Self {
            metrics_schema_version: METRICS_SCHEMA_VERSION,
            source_projection_version: METRICS_PROJECTION_VERSION,
        }
    }
}

pub trait MaterializedMetricsStore {
    fn store_session_metrics(&self, metrics: &SessionMetrics) -> AppResult<()>;

    fn load_session_metrics(&self, session_id: &str) -> AppResult<Option<SessionMetrics>>;

    fn needs_rebuild(&self, session_id: &str, version: MetricsRebuildVersion) -> AppResult<bool>;

    fn store_session_detail(&self, detail: &ProjectMetricsSessionDetail) -> AppResult<()>;

    fn load_session_detail(
        &self,
        session_id: &str,
    ) -> AppResult<Option<ProjectMetricsSessionDetail>>;

    fn needs_detail_rebuild(
        &self,
        session_id: &str,
        version: MetricsRebuildVersion,
    ) -> AppResult<bool>;

    fn query_project_metrics(
        &self,
        query: &SessionMetricsQuery,
    ) -> AppResult<ProjectMetricsResponse>;
}

pub fn extract_project_identity(summary: Option<&IndexedSessionSummary>) -> ProjectIdentity {
    let cwd = summary.and_then(|value| cleaned(value.cwd.as_deref()));
    let normalized_cwd = cwd.as_deref().map(|value| {
        normalize_path(Path::new(value))
            .to_string_lossy()
            .into_owned()
    });
    let git_origin_url = summary.and_then(|value| cleaned(value.git_origin_url.as_deref()));
    let git_branch = summary.and_then(|value| cleaned(value.git_branch.as_deref()));
    let git_sha = summary.and_then(|value| cleaned(value.git_sha.as_deref()));

    let state = if normalized_cwd.is_some() {
        ProjectIdentityState::Normal
    } else {
        ProjectIdentityState::Degraded
    };
    let key_material = match (&normalized_cwd, &git_origin_url, &git_branch) {
        (Some(cwd), Some(origin), Some(branch)) => format!("{origin}|{branch}|{cwd}"),
        (Some(cwd), Some(origin), None) => format!("{origin}|{cwd}"),
        (Some(cwd), None, Some(branch)) => format!("{cwd}|{branch}"),
        (Some(cwd), None, None) => cwd.clone(),
        (None, _, _) => summary
            .map(|value| value.session_id.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(|value| format!("degraded-session:{value}"))
            .unwrap_or_else(|| "degraded-unknown".to_string()),
    };
    let prefix = if matches!(state, ProjectIdentityState::Degraded) {
        "degraded"
    } else {
        "project"
    };

    ProjectIdentity {
        project_key: format!("{prefix}:{}", hash8(&key_material)),
        state,
        cwd,
        normalized_cwd,
        git_origin_url,
        git_branch,
        git_sha,
    }
}

pub fn classify_session_scope(summary: Option<&IndexedSessionSummary>) -> SessionScope {
    let Some(source) = summary.and_then(|value| cleaned(value.source.as_deref())) else {
        return SessionScope::Main;
    };
    let trimmed = source.trim();
    if trimmed.is_empty() || !trimmed.starts_with('{') {
        return SessionScope::Main;
    }

    let parsed: Value = match serde_json::from_str(trimmed) {
        Ok(value) => value,
        Err(_) => return SessionScope::Unknown,
    };

    let Some(subagent) = parsed.get("subagent") else {
        return SessionScope::Main;
    };
    let Some(thread_spawn) = subagent.get("thread_spawn") else {
        return SessionScope::Unknown;
    };
    let Some(thread_spawn) = thread_spawn.as_object() else {
        return SessionScope::Unknown;
    };

    match thread_spawn
        .get("parent_thread_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(_) => SessionScope::Subsession,
        None => SessionScope::Unknown,
    }
}

pub fn compute_session_metrics(
    session_id: &str,
    events: &[EventRecord],
    tree: Option<&EventTree>,
    indexed_summary: Option<&IndexedSessionSummary>,
) -> SessionMetrics {
    let operation_projection = project_operation_stream(events);
    let op_durations = operation_durations(events, &operation_projection);
    let first_ts = events
        .first()
        .and_then(|event| cleaned(Some(event.ts.as_str())));
    let last_ts = events
        .last()
        .and_then(|event| cleaned(Some(event.ts.as_str())));
    let total_duration = duration_between(first_ts.as_deref(), last_ts.as_deref());
    let token_ledger = build_token_ledger(events, indexed_summary);
    let task_count = events
        .iter()
        .filter(|event| event.event_type == TASK_STARTED)
        .count() as u64;
    let compaction_count = events
        .iter()
        .filter(|event| event.event_type == CONTEXT_COMPACTED)
        .count() as u64;
    let message_count = events
        .iter()
        .filter(|event| {
            matches!(
                event.event_type.as_str(),
                MESSAGE_USER | MESSAGE_AGENT | MESSAGE_COMMENTARY | AGENT_REASONING
            )
        })
        .count() as u64;
    let error_count = events
        .iter()
        .filter(|event| event.event_type == ERROR)
        .count() as u64;
    let abort_count = events
        .iter()
        .filter(|event| event.event_type == AGENT_ABORTED)
        .count() as u64;
    let failure_count = events
        .iter()
        .filter(|event| event.event_type == AGENT_FAILED)
        .count() as u64;
    let outcome = classify_session_outcome(events);
    let factors = build_factor_metadata(events, indexed_summary);
    let operations = build_operation_metrics(&operation_projection);
    let tool_breakdown = build_tool_breakdown(events, &operation_projection, &op_durations);
    let duration = build_duration_breakdown(total_duration, &op_durations);
    let task_facts = extract_task_metrics_facts(
        session_id,
        &extract_project_identity(indexed_summary),
        classify_session_scope(indexed_summary),
        events,
        &operation_projection,
        &op_durations,
        indexed_summary.and_then(|value| cleaned(value.agent_role.as_deref())),
    );
    let review_cycles = count_review_cycles(events, tree);
    let review_findings = count_review_findings(events);
    let thread_count = tree
        .map(|value| CoveredMetric::known(value.thread_count as u64, MetricSource::EventTree))
        .unwrap_or_else(CoveredMetric::unknown);
    let context = ContextMetrics {
        start_context_size: factors.start_context_size.clone(),
        context_growth: CoveredMetric::unknown(),
        compaction_events: CoveredMetric::known(compaction_count, MetricSource::NormalizedEvents),
        context_compression: CoveredMetric::known(compaction_count, MetricSource::NormalizedEvents),
    };
    let task_metrics = TaskMetrics {
        task_count: if task_count > 0 {
            CoveredMetric::known(task_count, MetricSource::NormalizedEvents)
        } else {
            CoveredMetric::unknown()
        },
        turn_count: count_distinct_payload_field(events, "turn_id")
            .map(|value| CoveredMetric::known(value, MetricSource::NormalizedEvents))
            .unwrap_or_else(CoveredMetric::unknown),
        agent_work_item_count: count_agent_work_items(events, tree),
    };
    let used_skills = extract_used_skills(events);
    let business_review = BusinessReviewMetrics {
        review_cycles,
        review_findings,
    };
    let quality = QualityMetrics {
        feedback_score: CoveredMetric::unknown(),
        evaluator_result_count: CoveredMetric::unknown(),
        guardrail_trigger_count: CoveredMetric::unknown(),
        handoff_count: CoveredMetric::unknown(),
    };
    let derived_efficiency = build_derived_efficiency(&token_ledger, &outcome, &business_review);

    SessionMetrics {
        session_id: session_id.to_string(),
        metrics_schema_version: METRICS_SCHEMA_VERSION,
        source_projection_version: METRICS_PROJECTION_VERSION,
        computed_at: utc_now_iso(),
        started_at: first_ts,
        ended_at: last_ts,
        project: extract_project_identity(indexed_summary),
        session_scope: classify_session_scope(indexed_summary),
        factors,
        outcome,
        event_count: CoveredMetric::known(events.len() as u64, MetricSource::NormalizedEvents),
        thread_count,
        message_count: CoveredMetric::known(message_count, MetricSource::NormalizedEvents),
        error_count: CoveredMetric::known(error_count, MetricSource::NormalizedEvents),
        abort_count: CoveredMetric::known(abort_count, MetricSource::NormalizedEvents),
        failure_count: CoveredMetric::known(failure_count, MetricSource::NormalizedEvents),
        operations,
        duration,
        token_ledger,
        tool_breakdown,
        task_metrics,
        task_facts,
        used_skills,
        business_review,
        context,
        quality,
        baseline: BaselineComparison {
            coverage: MetricCoverage::Unknown,
            token_usage_delta: CoveredMetric::unknown(),
            duration_delta_ms: CoveredMetric::unknown(),
            error_rate_delta: CoveredMetric::unknown(),
            outcome_rate_delta: CoveredMetric::unknown(),
        },
        derived_efficiency,
    }
}

pub fn build_project_metrics_session_detail(
    session_id: &str,
    session_ref: &str,
    events: &[EventRecord],
    indexed_summary: Option<&IndexedSessionSummary>,
    state_thread_summary: Option<&StateThreadSummary>,
    metrics: &SessionMetrics,
) -> ProjectMetricsSessionDetail {
    let (start_user_request, start_user_request_source) =
        resolve_start_user_request(events, state_thread_summary);
    let (task_summary, task_summary_source) =
        resolve_task_summary(events, state_thread_summary, indexed_summary);
    let primary_task_fact = select_primary_task_fact(&metrics.task_facts);

    ProjectMetricsSessionDetail {
        session_id: session_id.to_string(),
        session_ref: session_ref.to_string(),
        title: state_thread_summary.and_then(|summary| summary.title.clone()),
        start_user_request,
        start_user_request_source,
        task_summary,
        task_summary_source,
        agent_role: indexed_summary
            .and_then(|summary| cleaned(summary.agent_role.as_deref()))
            .or_else(|| cleaned(metrics.factors.agent_role.as_deref())),
        task_class: primary_task_fact
            .and_then(|fact| (fact.task_class != TaskClass::Unknown).then_some(fact.task_class)),
        task_class_confidence: primary_task_fact
            .map(|fact| fact.task_class_confidence)
            .unwrap_or(TaskClassConfidence::Unknown),
    }
}

pub fn apply_spawn_agent_mode(
    metrics: &SessionMetrics,
    mode: SpawnAgentAggregation,
) -> SessionMetrics {
    if matches!(mode, SpawnAgentAggregation::Include) {
        return metrics.clone();
    }

    let mut adjusted = metrics.clone();
    if let (Some(total), Some(spawn)) = (
        adjusted.token_ledger.total.value,
        adjusted.token_ledger.spawn_agent.value,
    ) {
        adjusted.token_ledger.total.value = Some(total.saturating_sub(spawn));
    }
    if let (Some(total), Some(spawn)) = (
        adjusted.duration.total_ms.value,
        adjusted.duration.spawn_agent_ms.value,
    ) {
        adjusted.duration.total_ms.value = Some(total.saturating_sub(spawn));
    }
    adjusted
}

fn resolve_start_user_request(
    events: &[EventRecord],
    state_thread_summary: Option<&StateThreadSummary>,
) -> (Option<String>, SessionDetailTextSource) {
    if let Some(text) =
        state_thread_summary.and_then(|summary| cleaned(summary.first_user_message.as_deref()))
    {
        return (Some(text), SessionDetailTextSource::IndexedFirstUserMessage);
    }

    if let Some(text) = first_event_text(events, MESSAGE_USER, &["text", "message", "summary"]) {
        return (Some(text), SessionDetailTextSource::MessageUser);
    }

    (None, SessionDetailTextSource::Unavailable)
}

fn resolve_task_summary(
    events: &[EventRecord],
    state_thread_summary: Option<&StateThreadSummary>,
    indexed_summary: Option<&IndexedSessionSummary>,
) -> (Option<String>, SessionDetailTextSource) {
    if let Some(text) = state_thread_summary.and_then(|summary| cleaned(summary.title.as_deref())) {
        return (Some(text), SessionDetailTextSource::IndexedTitle);
    }

    if let Some(text) = first_event_text(
        events,
        TASK_STARTED,
        &["title", "task", "prompt", "message", "summary"],
    ) {
        return (Some(text), SessionDetailTextSource::TaskStarted);
    }

    if let Some(text) = indexed_summary.and_then(|summary| cleaned(summary.thread_name.as_deref()))
    {
        return (Some(text), SessionDetailTextSource::IndexedTitle);
    }

    (None, SessionDetailTextSource::Unavailable)
}

fn first_event_text(events: &[EventRecord], event_type: &str, keys: &[&str]) -> Option<String> {
    events.iter().find_map(|event| {
        if event.event_type != event_type {
            return None;
        }
        keys.iter()
            .find_map(|key| payload_string(event.payload.as_object(), key))
    })
}

fn select_primary_task_fact(task_facts: &[TaskMetricsFact]) -> Option<&TaskMetricsFact> {
    task_facts
        .iter()
        .filter(|fact| fact.parent_thread_id.is_none())
        .min_by(|left, right| {
            left.started_seq
                .cmp(&right.started_seq)
                .then_with(|| left.analytic_key.cmp(&right.analytic_key))
        })
        .or_else(|| {
            task_facts.iter().min_by(|left, right| {
                left.started_seq
                    .cmp(&right.started_seq)
                    .then_with(|| left.analytic_key.cmp(&right.analytic_key))
            })
        })
}

pub struct SqliteSessionMetricsStore {
    conn: Connection,
}

pub type SessionMetricsStore = SqliteSessionMetricsStore;

impl SqliteSessionMetricsStore {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)
            .map_err(|err| AppError::Runner(format!("metrics storage open failed: {err}")))?;
        let store = Self { conn };
        store.initialize()?;
        Ok(store)
    }

    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|err| AppError::Runner(format!("metrics storage open failed: {err}")))?;
        let store = Self { conn };
        store.initialize()?;
        Ok(store)
    }

    fn initialize(&self) -> AppResult<()> {
        self.conn
            .execute_batch(
                r#"
                create table if not exists session_metrics (
                    session_id text primary key not null,
                    project_key text not null,
                    started_at text,
                    ended_at text,
                    metrics_schema_version integer not null,
                    source_projection_version integer not null,
                    payload_json text not null,
                    updated_at text not null
                );
                create index if not exists idx_session_metrics_project_time
                    on session_metrics(project_key, started_at, session_id);
                create table if not exists session_metric_details (
                    session_id text primary key not null,
                    metrics_schema_version integer not null,
                    source_projection_version integer not null,
                    payload_json text not null,
                    updated_at text not null
                );
                "#,
            )
            .map_err(|err| AppError::Runner(format!("metrics storage init failed: {err}")))?;
        Ok(())
    }

    pub fn upsert_session_metrics(&self, metrics: &SessionMetrics) -> AppResult<()> {
        self.store_session_metrics(metrics)
    }

    pub fn get_session_metrics(&self, session_id: &str) -> AppResult<Option<SessionMetrics>> {
        self.load_session_metrics(session_id)
    }

    pub fn is_stale(
        &self,
        session_id: &str,
        metrics_schema_version: u32,
        source_projection_version: u32,
    ) -> AppResult<bool> {
        self.needs_rebuild(
            session_id,
            MetricsRebuildVersion {
                metrics_schema_version,
                source_projection_version,
            },
        )
    }

    pub fn list_project_sessions(
        &self,
        query: &SessionMetricsQuery,
    ) -> AppResult<ProjectMetricsResponse> {
        self.query_project_metrics(query)
    }

    pub fn get_session_detail(
        &self,
        session_id: &str,
    ) -> AppResult<Option<ProjectMetricsSessionDetail>> {
        self.load_session_detail(session_id)
    }
}

impl MaterializedMetricsStore for SqliteSessionMetricsStore {
    fn store_session_metrics(&self, metrics: &SessionMetrics) -> AppResult<()> {
        let payload = serde_json::to_string(metrics)
            .map_err(|err| AppError::Runner(format!("metrics serialization failed: {err}")))?;
        self.conn
            .execute(
                r#"
                insert into session_metrics (
                    session_id, project_key, started_at, ended_at,
                    metrics_schema_version, source_projection_version,
                    payload_json, updated_at
                )
                values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                on conflict(session_id) do update set
                    project_key = excluded.project_key,
                    started_at = excluded.started_at,
                    ended_at = excluded.ended_at,
                    metrics_schema_version = excluded.metrics_schema_version,
                    source_projection_version = excluded.source_projection_version,
                    payload_json = excluded.payload_json,
                    updated_at = excluded.updated_at
                "#,
                params![
                    metrics.session_id,
                    metrics.project.project_key,
                    metrics.started_at,
                    metrics.ended_at,
                    metrics.metrics_schema_version,
                    metrics.source_projection_version,
                    payload,
                    utc_now_iso()
                ],
            )
            .map_err(|err| AppError::Runner(format!("metrics upsert failed: {err}")))?;
        Ok(())
    }

    fn load_session_metrics(&self, session_id: &str) -> AppResult<Option<SessionMetrics>> {
        let mut stmt = self
            .conn
            .prepare("select payload_json from session_metrics where session_id = ?1")
            .map_err(|err| AppError::Runner(format!("metrics query prepare failed: {err}")))?;
        let result = stmt.query_row(params![session_id], |row| row.get::<_, String>(0));
        match result {
            Ok(payload) => serde_json::from_str::<SessionMetrics>(&payload)
                .map(Some)
                .map_err(|err| AppError::Runner(format!("metrics deserialization failed: {err}"))),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(AppError::Runner(format!("metrics query failed: {err}"))),
        }
    }

    fn needs_rebuild(&self, session_id: &str, version: MetricsRebuildVersion) -> AppResult<bool> {
        let mut stmt = self
            .conn
            .prepare(
                "select metrics_schema_version, source_projection_version from session_metrics where session_id = ?1",
            )
            .map_err(|err| AppError::Runner(format!("metrics stale query prepare failed: {err}")))?;
        let result = stmt.query_row(params![session_id], |row| {
            Ok((row.get::<_, u32>(0)?, row.get::<_, u32>(1)?))
        });
        match result {
            Ok((schema, projection)) => Ok(schema != version.metrics_schema_version
                || projection != version.source_projection_version),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(true),
            Err(err) => Err(AppError::Runner(format!(
                "metrics stale query failed: {err}"
            ))),
        }
    }

    fn store_session_detail(&self, detail: &ProjectMetricsSessionDetail) -> AppResult<()> {
        let payload = serde_json::to_string(detail)
            .map_err(|err| AppError::Runner(format!("detail serialization failed: {err}")))?;
        self.conn
            .execute(
                r#"
                insert into session_metric_details (
                    session_id, metrics_schema_version, source_projection_version, payload_json, updated_at
                )
                values (?1, ?2, ?3, ?4, ?5)
                on conflict(session_id) do update set
                    metrics_schema_version = excluded.metrics_schema_version,
                    source_projection_version = excluded.source_projection_version,
                    payload_json = excluded.payload_json,
                    updated_at = excluded.updated_at
                "#,
                params![
                    detail.session_id,
                    METRICS_SCHEMA_VERSION,
                    METRICS_PROJECTION_VERSION,
                    payload,
                    utc_now_iso()
                ],
            )
            .map_err(|err| AppError::Runner(format!("detail upsert failed: {err}")))?;
        Ok(())
    }

    fn load_session_detail(
        &self,
        session_id: &str,
    ) -> AppResult<Option<ProjectMetricsSessionDetail>> {
        let mut stmt = self
            .conn
            .prepare("select payload_json from session_metric_details where session_id = ?1")
            .map_err(|err| AppError::Runner(format!("detail query prepare failed: {err}")))?;
        let result = stmt.query_row(params![session_id], |row| row.get::<_, String>(0));
        match result {
            Ok(payload) => serde_json::from_str::<ProjectMetricsSessionDetail>(&payload)
                .map(Some)
                .map_err(|err| AppError::Runner(format!("detail deserialization failed: {err}"))),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(AppError::Runner(format!("detail query failed: {err}"))),
        }
    }

    fn needs_detail_rebuild(
        &self,
        session_id: &str,
        version: MetricsRebuildVersion,
    ) -> AppResult<bool> {
        let mut stmt = self
            .conn
            .prepare(
                "select metrics_schema_version, source_projection_version from session_metric_details where session_id = ?1",
            )
            .map_err(|err| AppError::Runner(format!("detail stale query prepare failed: {err}")))?;
        let result = stmt.query_row(params![session_id], |row| {
            Ok((row.get::<_, u32>(0)?, row.get::<_, u32>(1)?))
        });
        match result {
            Ok((schema, projection)) => Ok(schema != version.metrics_schema_version
                || projection != version.source_projection_version),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(true),
            Err(err) => Err(AppError::Runner(format!(
                "detail stale query failed: {err}"
            ))),
        }
    }

    fn query_project_metrics(
        &self,
        query: &SessionMetricsQuery,
    ) -> AppResult<ProjectMetricsResponse> {
        let mut stmt = self
            .conn
            .prepare(
                r#"
                select payload_json
                from session_metrics
                where project_key = ?1
                  and (?2 is null or coalesce(started_at, ended_at, '') >= ?2)
                  and (?3 is null or coalesce(started_at, ended_at, '') <= ?3)
                order by coalesce(started_at, ended_at, ''), session_id
                "#,
            )
            .map_err(|err| AppError::Runner(format!("metrics list prepare failed: {err}")))?;
        let rows = stmt
            .query_map(
                params![query.project_key, query.start_ts, query.end_ts],
                |row| row.get::<_, String>(0),
            )
            .map_err(|err| AppError::Runner(format!("metrics list failed: {err}")))?;
        let mode = if query.include_spawn_agents {
            SpawnAgentAggregation::Include
        } else {
            SpawnAgentAggregation::Exclude
        };
        let mut sessions = Vec::new();
        let mut available_scope_counts = SessionScopeCounts::default();
        for row in rows {
            let payload =
                row.map_err(|err| AppError::Runner(format!("metrics row failed: {err}")))?;
            let metrics = serde_json::from_str::<SessionMetrics>(&payload).map_err(|err| {
                AppError::Runner(format!("metrics deserialization failed: {err}"))
            })?;
            available_scope_counts.record(metrics.session_scope);
            if query.session_scope_filter.matches(metrics.session_scope) {
                sessions.push(apply_spawn_agent_mode(&metrics, mode));
            }
        }
        Ok(aggregate_project_metrics(
            query.project_key.clone(),
            sessions,
            query.session_scope_filter,
            available_scope_counts,
        ))
    }
}

pub fn aggregate_project_metrics(
    project_key: String,
    sessions: Vec<SessionMetrics>,
    scope_filter: SessionScopeFilter,
    available_scope_counts: SessionScopeCounts,
) -> ProjectMetricsResponse {
    let mut contributing_session_ids = Vec::new();
    for session in &sessions {
        contributing_session_ids.push(session.session_id.clone());
    }
    let token_ledger = TokenLedger {
        total: sum_u64_metrics(sessions.iter().map(|session| &session.token_ledger.total)),
        input: sum_u64_metrics(sessions.iter().map(|session| &session.token_ledger.input)),
        output: sum_u64_metrics(sessions.iter().map(|session| &session.token_ledger.output)),
        cached_input: sum_u64_metrics(
            sessions
                .iter()
                .map(|session| &session.token_ledger.cached_input),
        ),
        reasoning_output: sum_u64_metrics(
            sessions
                .iter()
                .map(|session| &session.token_ledger.reasoning_output),
        ),
        tool_call: sum_u64_metrics(
            sessions
                .iter()
                .map(|session| &session.token_ledger.tool_call),
        ),
        task: sum_u64_metrics(sessions.iter().map(|session| &session.token_ledger.task)),
        spawn_agent: sum_u64_metrics(
            sessions
                .iter()
                .map(|session| &session.token_ledger.spawn_agent),
        ),
    };
    let duration_ms = sum_u64_metrics(sessions.iter().map(|session| &session.duration.total_ms));
    let factors = ProjectFactorRollups {
        start_context_size: sum_u64_metrics(
            sessions
                .iter()
                .map(|session| &session.factors.start_context_size),
        ),
        skills_count: sum_u64_metrics(sessions.iter().map(|session| &session.factors.skills_count)),
        mcp_server_count: sum_u64_metrics(
            sessions
                .iter()
                .map(|session| &session.factors.mcp_server_count),
        ),
    };
    let operations = ProjectOperationRollups {
        spawn_agent_calls: sum_u64_metrics(
            sessions
                .iter()
                .map(|session| &session.operations.spawn_agent_calls),
        ),
    };
    let task_metrics = ProjectTaskRollups {
        task_count: sum_u64_metrics(
            sessions
                .iter()
                .map(|session| &session.task_metrics.task_count),
        ),
    };
    let mut task_facts = sessions
        .iter()
        .flat_map(|session| session.task_facts.iter().cloned())
        .collect::<Vec<_>>();
    task_facts.sort_by(|left, right| {
        left.started_at
            .cmp(&right.started_at)
            .then_with(|| left.session_id.cmp(&right.session_id))
            .then_with(|| left.analytic_key.cmp(&right.analytic_key))
    });
    let used_skills = aggregate_used_skills(&sessions);
    ProjectMetricsResponse {
        project_key,
        session_count: sessions.len() as u64,
        contributing_session_ids,
        scope_filter,
        available_scope_counts,
        sessions,
        token_ledger,
        duration_ms,
        factors,
        operations,
        task_metrics,
        task_facts,
        used_skills,
        baseline: BaselineComparison {
            coverage: MetricCoverage::Unknown,
            token_usage_delta: CoveredMetric::unknown(),
            duration_delta_ms: CoveredMetric::unknown(),
            error_rate_delta: CoveredMetric::unknown(),
            outcome_rate_delta: CoveredMetric::unknown(),
        },
        derived_efficiency: DerivedEfficiencyMetrics {
            tokens_per_successful_session: CoveredMetric::unknown(),
            tokens_per_accepted_task: CoveredMetric::unknown(),
            review_findings_per_1k_tokens: CoveredMetric::unknown(),
        },
    }
}

fn sum_u64_metrics<'a>(
    metrics: impl IntoIterator<Item = &'a CoveredMetric<u64>>,
) -> CoveredMetric<u64> {
    let mut sum = 0u64;
    let mut has_value = false;
    let mut all_known = true;
    for metric in metrics {
        if let Some(value) = metric.value {
            has_value = true;
            sum = sum.saturating_add(value);
        }
        if metric.coverage != MetricCoverage::Known {
            all_known = false;
        }
    }
    if !has_value {
        return CoveredMetric::unknown();
    }
    if all_known {
        CoveredMetric::known(sum, MetricSource::Derived)
    } else {
        CoveredMetric::partial(sum, MetricSource::Derived)
    }
}

fn covered_u64_metric(
    value: u64,
    coverage: MetricCoverage,
    source: MetricSource,
) -> CoveredMetric<u64> {
    match coverage {
        MetricCoverage::Known => CoveredMetric::known(value, source),
        MetricCoverage::Partial => CoveredMetric::partial(value, source),
        MetricCoverage::Unknown => CoveredMetric::unknown(),
    }
}

fn build_factor_metadata(
    events: &[EventRecord],
    indexed_summary: Option<&IndexedSessionSummary>,
) -> FactorMetadata {
    FactorMetadata {
        model: indexed_summary.and_then(|value| cleaned(value.model.as_deref())),
        reasoning_effort: indexed_summary
            .and_then(|value| cleaned(value.reasoning_effort.as_deref())),
        cli_version: indexed_summary.and_then(|value| cleaned(value.cli_version.as_deref())),
        sandbox_policy_kind: indexed_summary
            .and_then(|value| cleaned(value.sandbox_policy_kind.as_deref())),
        approval_mode: indexed_summary.and_then(|value| cleaned(value.approval_mode.as_deref())),
        agent_role: indexed_summary.and_then(|value| cleaned(value.agent_role.as_deref())),
        skills_count: resolve_enabled_skills_count(events),
        mcp_server_count: count_runtime_array(events, "mcp_servers"),
        mcp_call_count: CoveredMetric::known(
            events
                .iter()
                .filter(|event| event.event_type == MCP_CALL)
                .count() as u64,
            MetricSource::NormalizedEvents,
        ),
        start_context_size: resolve_start_context_size(events),
    }
}

fn build_operation_metrics(projection: &OperationProjection) -> OperationMetrics {
    if projection.snapshots.is_empty() {
        return OperationMetrics {
            operation_count: CoveredMetric::unknown(),
            successful_operations: CoveredMetric::unknown(),
            failed_operations: CoveredMetric::unknown(),
            tool_calls: CoveredMetric::unknown(),
            shell_calls: CoveredMetric::unknown(),
            mcp_calls: CoveredMetric::unknown(),
            collaboration_calls: CoveredMetric::unknown(),
            spawn_agent_calls: CoveredMetric::unknown(),
        };
    }
    let count_kind = |kind: OperationKind| {
        projection
            .snapshots
            .iter()
            .filter(|snapshot| snapshot.key.kind == kind)
            .count() as u64
    };
    let collaboration_calls = projection
        .snapshots
        .iter()
        .filter(|snapshot| snapshot.key.kind.as_str().starts_with("collab."))
        .count() as u64;
    let failed = projection
        .snapshots
        .iter()
        .filter(|snapshot| {
            matches!(
                snapshot.last_status.as_deref(),
                Some("failed" | "error" | "cancelled")
            )
        })
        .count() as u64;
    let successful = projection
        .snapshots
        .iter()
        .filter(|snapshot| snapshot.last_status.as_deref() == Some("completed"))
        .count() as u64;

    OperationMetrics {
        operation_count: CoveredMetric::known(
            projection.snapshots.len() as u64,
            MetricSource::OperationProjection,
        ),
        successful_operations: CoveredMetric::known(successful, MetricSource::OperationProjection),
        failed_operations: CoveredMetric::known(failed, MetricSource::OperationProjection),
        tool_calls: CoveredMetric::known(
            projection
                .snapshots
                .iter()
                .filter(|snapshot| snapshot.key.kind != OperationKind::FileChange)
                .count() as u64,
            MetricSource::OperationProjection,
        ),
        shell_calls: CoveredMetric::known(
            count_kind(OperationKind::Shell),
            MetricSource::OperationProjection,
        ),
        mcp_calls: CoveredMetric::known(
            count_kind(OperationKind::Mcp),
            MetricSource::OperationProjection,
        ),
        collaboration_calls: CoveredMetric::known(
            collaboration_calls,
            MetricSource::OperationProjection,
        ),
        spawn_agent_calls: CoveredMetric::known(
            count_kind(OperationKind::CollabSpawnAgent),
            MetricSource::OperationProjection,
        ),
    }
}

fn build_duration_breakdown(
    total_duration: Option<u64>,
    op_durations: &BTreeMap<String, OperationDuration>,
) -> DurationBreakdown {
    let mut tool = 0u64;
    let mut shell = 0u64;
    let mut mcp = 0u64;
    let mut spawn = 0u64;
    for duration in op_durations.values() {
        match duration.kind {
            OperationKind::Shell => shell += duration.duration_ms,
            OperationKind::Mcp => mcp += duration.duration_ms,
            OperationKind::CollabSpawnAgent => spawn += duration.duration_ms,
            _ => tool += duration.duration_ms,
        }
    }
    let attributed = tool
        .saturating_add(shell)
        .saturating_add(mcp)
        .saturating_add(spawn);
    let total =
        total_duration.map(|value| CoveredMetric::known(value, MetricSource::NormalizedEvents));
    DurationBreakdown {
        total_ms: total.clone().unwrap_or_else(CoveredMetric::unknown),
        generation_ms: CoveredMetric::unknown(),
        tool_ms: if op_durations.is_empty() {
            CoveredMetric::unknown()
        } else {
            CoveredMetric::partial(tool, MetricSource::OperationProjection)
        },
        shell_ms: if op_durations.is_empty() {
            CoveredMetric::unknown()
        } else {
            CoveredMetric::partial(shell, MetricSource::OperationProjection)
        },
        mcp_ms: if op_durations.is_empty() {
            CoveredMetric::unknown()
        } else {
            CoveredMetric::partial(mcp, MetricSource::OperationProjection)
        },
        spawn_agent_ms: if op_durations.is_empty() {
            CoveredMetric::unknown()
        } else {
            CoveredMetric::partial(spawn, MetricSource::OperationProjection)
        },
        idle_unknown_ms: match total_duration {
            Some(total) => {
                CoveredMetric::partial(total.saturating_sub(attributed), MetricSource::Derived)
            }
            None => CoveredMetric::unknown(),
        },
    }
}

fn build_token_ledger(
    events: &[EventRecord],
    indexed_summary: Option<&IndexedSessionSummary>,
) -> TokenLedger {
    if let Some(ledger) = build_scoped_token_ledger(events) {
        return ledger;
    }

    if let Some(snapshot) = latest_unscoped_token_snapshot(events) {
        return TokenLedger {
            total: snapshot
                .total
                .map(|value| CoveredMetric::partial(value, MetricSource::NormalizedEvents))
                .unwrap_or_else(CoveredMetric::unknown),
            input: snapshot
                .input
                .map(|value| CoveredMetric::partial(value, MetricSource::NormalizedEvents))
                .unwrap_or_else(CoveredMetric::unknown),
            output: snapshot
                .output
                .map(|value| CoveredMetric::partial(value, MetricSource::NormalizedEvents))
                .unwrap_or_else(CoveredMetric::unknown),
            cached_input: snapshot
                .cached_input
                .map(|value| CoveredMetric::partial(value, MetricSource::NormalizedEvents))
                .unwrap_or_else(CoveredMetric::unknown),
            reasoning_output: snapshot
                .reasoning_output
                .map(|value| CoveredMetric::partial(value, MetricSource::NormalizedEvents))
                .unwrap_or_else(CoveredMetric::unknown),
            tool_call: CoveredMetric::unknown(),
            task: CoveredMetric::unknown(),
            spawn_agent: CoveredMetric::unknown(),
        };
    }

    if let Some(value) = indexed_summary.and_then(|summary| summary.tokens_used) {
        return TokenLedger {
            total: CoveredMetric::partial(value, MetricSource::IndexedSessionMetadata),
            input: CoveredMetric::unknown(),
            output: CoveredMetric::unknown(),
            cached_input: CoveredMetric::unknown(),
            reasoning_output: CoveredMetric::unknown(),
            tool_call: CoveredMetric::unknown(),
            task: CoveredMetric::unknown(),
            spawn_agent: CoveredMetric::unknown(),
        };
    }

    TokenLedger {
        total: CoveredMetric::unknown(),
        input: CoveredMetric::unknown(),
        output: CoveredMetric::unknown(),
        cached_input: CoveredMetric::unknown(),
        reasoning_output: CoveredMetric::unknown(),
        tool_call: CoveredMetric::unknown(),
        task: CoveredMetric::unknown(),
        spawn_agent: CoveredMetric::unknown(),
    }
}

#[derive(Debug, Clone, Default)]
struct ThreadParentState {
    parent_thread_id: Option<String>,
    ambiguous: bool,
}

fn build_scoped_token_ledger(events: &[EventRecord]) -> Option<TokenLedger> {
    let latest_snapshots = latest_token_snapshots_by_thread(events);
    if latest_snapshots.is_empty() {
        return None;
    }

    let all_snapshots = latest_snapshots.values().collect::<Vec<_>>();
    let parent_states = thread_parent_states(events);
    let parentless_thread_count = latest_snapshots
        .keys()
        .filter(|thread_id| {
            parent_states
                .get(*thread_id)
                .and_then(|state| state.parent_thread_id.as_deref())
                .is_none()
        })
        .count();
    let hierarchy_partial = has_unscoped_token_snapshot(events)
        || parent_states.values().any(|state| state.ambiguous)
        || parentless_thread_count > 1;
    let child_snapshots = latest_snapshots
        .iter()
        .filter_map(|(thread_id, snapshot)| {
            parent_states
                .get(thread_id)
                .and_then(|state| state.parent_thread_id.as_ref())
                .map(|_| snapshot)
        })
        .collect::<Vec<_>>();

    let total = aggregate_token_snapshot_metric(
        &all_snapshots,
        has_unscoped_token_snapshot(events),
        MetricSource::NormalizedEvents,
        |snapshot| snapshot.total,
    );
    let spawn_agent =
        aggregate_child_token_metric(&child_snapshots, hierarchy_partial, |snapshot| {
            snapshot.total
        });

    Some(TokenLedger {
        total: total.clone(),
        input: aggregate_token_snapshot_metric(
            &all_snapshots,
            has_unscoped_token_snapshot(events),
            MetricSource::NormalizedEvents,
            |snapshot| snapshot.input,
        ),
        output: aggregate_token_snapshot_metric(
            &all_snapshots,
            has_unscoped_token_snapshot(events),
            MetricSource::NormalizedEvents,
            |snapshot| snapshot.output,
        ),
        cached_input: aggregate_token_snapshot_metric(
            &all_snapshots,
            has_unscoped_token_snapshot(events),
            MetricSource::NormalizedEvents,
            |snapshot| snapshot.cached_input,
        ),
        reasoning_output: aggregate_token_snapshot_metric(
            &all_snapshots,
            has_unscoped_token_snapshot(events),
            MetricSource::NormalizedEvents,
            |snapshot| snapshot.reasoning_output,
        ),
        tool_call: CoveredMetric::unknown(),
        task: subtract_u64_metrics(&total, &spawn_agent, MetricSource::Derived),
        spawn_agent,
    })
}

fn aggregate_token_snapshot_metric(
    snapshots: &[&TokenSnapshot],
    force_partial: bool,
    source: MetricSource,
    select: impl Fn(&TokenSnapshot) -> Option<u64>,
) -> CoveredMetric<u64> {
    let mut sum = 0u64;
    let mut has_value = false;
    let mut all_have_values = true;

    for snapshot in snapshots {
        if let Some(value) = select(snapshot) {
            has_value = true;
            sum = sum.saturating_add(value);
        } else {
            all_have_values = false;
        }
    }

    if !has_value {
        return CoveredMetric::unknown();
    }

    if all_have_values && !force_partial {
        CoveredMetric::known(sum, source)
    } else {
        CoveredMetric::partial(sum, source)
    }
}

fn aggregate_child_token_metric(
    snapshots: &[&TokenSnapshot],
    force_partial: bool,
    select: impl Fn(&TokenSnapshot) -> Option<u64>,
) -> CoveredMetric<u64> {
    if snapshots.is_empty() {
        return if force_partial {
            CoveredMetric::partial(0, MetricSource::Derived)
        } else {
            CoveredMetric::known(0, MetricSource::Derived)
        };
    }

    aggregate_token_snapshot_metric(snapshots, force_partial, MetricSource::Derived, select)
}

fn subtract_u64_metrics(
    total: &CoveredMetric<u64>,
    excluded: &CoveredMetric<u64>,
    source: MetricSource,
) -> CoveredMetric<u64> {
    let (Some(total_value), Some(excluded_value)) = (total.value, excluded.value) else {
        return CoveredMetric::unknown();
    };

    let coverage =
        if total.coverage == MetricCoverage::Known && excluded.coverage == MetricCoverage::Known {
            MetricCoverage::Known
        } else {
            MetricCoverage::Partial
        };

    covered_u64_metric(total_value.saturating_sub(excluded_value), coverage, source)
}

fn latest_token_snapshots_by_thread(events: &[EventRecord]) -> BTreeMap<String, TokenSnapshot> {
    let mut snapshots = BTreeMap::new();
    for event in events
        .iter()
        .filter(|event| event.event_type == INFO_TOKENS)
    {
        let Some(thread_id) = event_thread_id(event) else {
            continue;
        };
        let Some(snapshot) = token_snapshot_from_payload(event.payload.as_object()) else {
            continue;
        };
        snapshots.insert(thread_id, snapshot);
    }
    snapshots
}

fn latest_unscoped_token_snapshot(events: &[EventRecord]) -> Option<TokenSnapshot> {
    let mut snapshot = None;
    for event in events
        .iter()
        .filter(|event| event.event_type == INFO_TOKENS)
    {
        if event_thread_id(event).is_some() {
            continue;
        }
        snapshot = token_snapshot_from_payload(event.payload.as_object());
    }
    snapshot
}

fn has_unscoped_token_snapshot(events: &[EventRecord]) -> bool {
    latest_unscoped_token_snapshot(events).is_some()
}

fn thread_parent_states(events: &[EventRecord]) -> BTreeMap<String, ThreadParentState> {
    let mut states = BTreeMap::<String, ThreadParentState>::new();
    for event in events {
        let Some(thread_id) = event_thread_id(event) else {
            continue;
        };
        let Some(parent_thread_id) = payload_string(event.payload.as_object(), "parent_thread_id")
        else {
            continue;
        };
        let state = states.entry(thread_id).or_default();
        match state.parent_thread_id.as_deref() {
            None => state.parent_thread_id = Some(parent_thread_id),
            Some(current) if current == parent_thread_id => {}
            Some(_) => state.ambiguous = true,
        }
    }
    states
}

fn build_tool_breakdown(
    events: &[EventRecord],
    projection: &OperationProjection,
    op_durations: &BTreeMap<String, OperationDuration>,
) -> Vec<ToolCategoryMetrics> {
    let event_by_seq = events
        .iter()
        .map(|event| (event.seq, event))
        .collect::<BTreeMap<_, _>>();
    let mut by_category: BTreeMap<ToolCommandCategory, (u64, u64, u64)> = BTreeMap::new();
    for snapshot in &projection.snapshots {
        let event = snapshot
            .started_seq
            .and_then(|seq| event_by_seq.get(&seq))
            .or_else(|| event_by_seq.get(&snapshot.last_seq));
        let category = event
            .map(|event| classify_tool_category(snapshot, event))
            .unwrap_or(ToolCommandCategory::Other);
        let failed = matches!(
            snapshot.last_status.as_deref(),
            Some("failed" | "error" | "cancelled")
        );
        let key = operation_key(snapshot);
        let duration = op_durations
            .get(&key)
            .map(|value| value.duration_ms)
            .unwrap_or(0);
        let entry = by_category.entry(category).or_default();
        entry.0 += 1;
        if failed {
            entry.1 += 1;
        }
        entry.2 += duration;
    }
    by_category
        .into_iter()
        .map(
            |(category, (count, failures, duration_ms))| ToolCategoryMetrics {
                category,
                count,
                failures,
                duration_ms: CoveredMetric::partial(duration_ms, MetricSource::OperationProjection),
                token_contribution: CoveredMetric::unknown(),
            },
        )
        .collect()
}

fn classify_tool_category(
    snapshot: &OperationSnapshot,
    event: &EventRecord,
) -> ToolCommandCategory {
    match snapshot.key.kind {
        OperationKind::Mcp => return ToolCommandCategory::Mcp,
        OperationKind::WebSearch | OperationKind::WebOpen => return ToolCommandCategory::WebSearch,
        OperationKind::FileChange | OperationKind::PatchApply => return ToolCommandCategory::Edit,
        OperationKind::CollabSpawnAgent
        | OperationKind::CollabSendInput
        | OperationKind::CollabWait
        | OperationKind::CollabCloseAgent
        | OperationKind::CollabResumeAgent => return ToolCommandCategory::Collaboration,
        _ => {}
    }
    let payload = event.payload.as_object();
    let tool_name = payload
        .and_then(|obj| obj.get("tool_name").or_else(|| obj.get("name")))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let shell = payload
        .and_then(|obj| obj.get("command"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let haystack = format!("{tool_name} {shell}");
    if haystack.contains("search") || haystack.contains("rg ") || haystack.starts_with("rg") {
        ToolCommandCategory::Search
    } else if haystack.contains("web") {
        ToolCommandCategory::WebSearch
    } else if haystack.contains("apply_patch")
        || haystack.contains("sed -i")
        || haystack.contains("edit")
    {
        ToolCommandCategory::Edit
    } else if haystack.contains("test")
        || haystack.contains("pytest")
        || haystack.contains("vitest")
    {
        ToolCommandCategory::Test
    } else if haystack.contains("build") || haystack.contains("cargo check") {
        ToolCommandCategory::Build
    } else if haystack.contains("git ") || haystack.starts_with("git") {
        ToolCommandCategory::Git
    } else if haystack.contains("ls ") || haystack.contains("find ") || haystack.contains("cat ") {
        ToolCommandCategory::Filesystem
    } else {
        ToolCommandCategory::Other
    }
}

fn classify_session_outcome(events: &[EventRecord]) -> OutcomeSummary {
    if let Some(event) = events
        .iter()
        .rev()
        .find(|event| event.event_type == AGENT_ABORTED)
    {
        return OutcomeSummary {
            outcome: if error_text(event).as_deref() == Some("interrupted") {
                SessionOutcome::Interrupted
            } else {
                SessionOutcome::Aborted
            },
            coverage: MetricCoverage::Known,
            error_type: error_text(event),
        };
    }
    if let Some(event) = events
        .iter()
        .rev()
        .find(|event| event.event_type == AGENT_FAILED)
    {
        return OutcomeSummary {
            outcome: SessionOutcome::Failed,
            coverage: MetricCoverage::Known,
            error_type: error_text(event),
        };
    }
    if events
        .iter()
        .any(|event| event.event_type == AGENT_COMPLETED || event.event_type == TASK_COMPLETED)
    {
        return OutcomeSummary {
            outcome: SessionOutcome::Completed,
            coverage: MetricCoverage::Known,
            error_type: None,
        };
    }
    OutcomeSummary {
        outcome: SessionOutcome::Unknown,
        coverage: MetricCoverage::Unknown,
        error_type: None,
    }
}

#[derive(Debug, Clone)]
struct OperationDuration {
    kind: OperationKind,
    duration_ms: u64,
}

fn operation_durations(
    events: &[EventRecord],
    projection: &OperationProjection,
) -> BTreeMap<String, OperationDuration> {
    let ts_by_seq = events
        .iter()
        .filter_map(|event| parse_ts(&event.ts).map(|ts| (event.seq, ts)))
        .collect::<BTreeMap<_, _>>();
    let mut out = BTreeMap::new();
    for snapshot in &projection.snapshots {
        let Some(started_seq) = snapshot.started_seq else {
            continue;
        };
        let Some(terminal_seq) = snapshot.terminal_seq else {
            continue;
        };
        let (Some(started), Some(ended)) =
            (ts_by_seq.get(&started_seq), ts_by_seq.get(&terminal_seq))
        else {
            continue;
        };
        if let Ok(duration) = (*ended - *started).to_std() {
            out.insert(
                operation_key(snapshot),
                OperationDuration {
                    kind: snapshot.key.kind,
                    duration_ms: duration.as_millis() as u64,
                },
            );
        }
    }
    out
}

fn count_runtime_array(events: &[EventRecord], key: &str) -> CoveredMetric<u64> {
    runtime_context_values(events, key)
        .into_iter()
        .find_map(|value| value.as_array())
        .map(|items| CoveredMetric::known(items.len() as u64, MetricSource::NormalizedEvents))
        .unwrap_or_else(CoveredMetric::unknown)
}

fn resolve_enabled_skills_count(events: &[EventRecord]) -> CoveredMetric<u64> {
    first_message_available_skills_count(events)
        .or_else(|| {
            runtime_context_values(events, "skills")
                .into_iter()
                .find_map(|value| value.as_array())
                .map(|items| items.len() as u64)
        })
        .map(|value| CoveredMetric::known(value, MetricSource::NormalizedEvents))
        .unwrap_or_else(CoveredMetric::unknown)
}

fn runtime_context_values<'a>(events: &'a [EventRecord], key: &str) -> Vec<&'a Value> {
    let mut values = Vec::new();
    for event in events
        .iter()
        .filter(|event| event.event_type == RUNTIME_CONTEXT)
    {
        let Some(payload) = event.payload.as_object() else {
            continue;
        };
        if let Some(value) = payload.get(key) {
            values.push(value);
        }
        if let Some(value) = payload
            .get("context")
            .and_then(Value::as_object)
            .and_then(|context| context.get(key))
        {
            values.push(value);
        }
    }
    values
}

fn resolve_start_context_size(events: &[EventRecord]) -> CoveredMetric<u64> {
    runtime_context_values(events, "start_context_size")
        .into_iter()
        .find_map(Value::as_u64)
        .or_else(|| first_nonempty_tokens_start_context(events))
        .or_else(|| events.iter().find_map(start_context_size_from_event))
        .map(|value| CoveredMetric::known(value, MetricSource::NormalizedEvents))
        .unwrap_or_else(CoveredMetric::unknown)
}

fn first_message_available_skills_count(events: &[EventRecord]) -> Option<u64> {
    let message_text = events
        .iter()
        .find(|event| event.event_type.starts_with("message."))
        .and_then(|event| event.payload.as_object())
        .and_then(|payload| payload.get("text"))
        .and_then(Value::as_str)?;
    let skills = extract_available_skills_from_message_text(message_text);
    (!skills.is_empty()).then_some(skills.len() as u64)
}

fn extract_available_skills_from_message_text(text: &str) -> Vec<String> {
    let mut collecting = false;
    let mut skills = BTreeSet::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if !collecting {
            if trimmed == "### Available skills" {
                collecting = true;
            }
            continue;
        }

        if let Some(skill) = parse_available_skill_line(trimmed) {
            skills.insert(skill);
            continue;
        }

        if !trimmed.is_empty() {
            break;
        }
    }

    skills.into_iter().collect()
}

fn parse_available_skill_line(line: &str) -> Option<String> {
    let item = line.strip_prefix("- ")?.trim();
    if item.is_empty() {
        return None;
    }

    let identifier = item
        .split_once(':')
        .map(|(value, _)| value)
        .unwrap_or(item)
        .trim()
        .trim_matches('`');
    let identifier = identifier
        .split_whitespace()
        .next()
        .unwrap_or(identifier)
        .trim();
    (!identifier.is_empty()).then_some(identifier.to_string())
}

fn extract_used_skills(events: &[EventRecord]) -> UsedSkillsMetrics {
    let mut identifiers = BTreeSet::new();

    for event in events {
        if matches!(
            event.event_type.as_str(),
            MESSAGE_USER | MESSAGE_AGENT | MESSAGE_COMMENTARY | AGENT_REASONING
        ) {
            if let Some(text) = event
                .payload
                .as_object()
                .and_then(|payload| payload.get("text"))
                .and_then(Value::as_str)
            {
                if let Some(identifier) = extract_explicit_skill_identifier_from_message_text(text) {
                    identifiers.insert(identifier);
                }
            }
        }

        if event.event_type != SHELL_CALL {
            continue;
        }

        let Some(payload) = event.payload.as_object() else {
            continue;
        };
        let Some(items) = payload.get("skill_identifiers").and_then(Value::as_array) else {
            continue;
        };
        for identifier in items.iter().filter_map(Value::as_str).map(str::trim) {
            if !identifier.is_empty() {
                identifiers.insert(identifier.to_string());
            }
        }
    }

    let identifiers = identifiers.into_iter().collect::<Vec<_>>();
    if identifiers.is_empty() {
        UsedSkillsMetrics::default()
    } else {
        UsedSkillsMetrics {
            count: CoveredMetric::known(identifiers.len() as u64, MetricSource::NormalizedEvents),
            identifiers,
            coverage: MetricCoverage::Known,
            source: MetricSource::NormalizedEvents,
        }
    }
}

fn extract_explicit_skill_identifier_from_message_text(text: &str) -> Option<String> {
    let skill_start = text.find("<skill>")?;
    let skill_end = text[skill_start..].find("</skill>")?;
    let block = &text[skill_start..skill_start + skill_end];
    let name_start = block.find("<name>")?;
    let after_name_start = name_start + "<name>".len();
    let name_end = block[after_name_start..].find("</name>")?;
    let identifier = block[after_name_start..after_name_start + name_end].trim();
    (!identifier.is_empty()).then_some(identifier.to_string())
}

fn aggregate_used_skills(sessions: &[SessionMetrics]) -> UsedSkillsRollup {
    let mut skills = BTreeMap::<String, (u64, u64)>::new();
    let mut has_known = false;
    let mut all_known = !sessions.is_empty();
    for session in sessions {
        if session.used_skills.coverage == MetricCoverage::Unknown {
            all_known = false;
            continue;
        }
        has_known = true;
        if session.used_skills.coverage != MetricCoverage::Known {
            all_known = false;
        }
        for identifier in &session.used_skills.identifiers {
            let entry = skills.entry(identifier.clone()).or_default();
            entry.0 += 1;
            entry.1 += 1;
        }
    }
    if !has_known {
        return UsedSkillsRollup::default();
    }
    let unique_skill_count = skills.len() as u64;
    UsedSkillsRollup {
        skills: skills
            .into_iter()
            .map(
                |(identifier, (usage_count, session_count))| UsedSkillRollupEntry {
                    identifier,
                    usage_count,
                    session_count,
                },
            )
            .collect(),
        count: if all_known {
            CoveredMetric::known(unique_skill_count, MetricSource::Derived)
        } else {
            CoveredMetric::partial(unique_skill_count, MetricSource::Derived)
        },
        coverage: if all_known {
            MetricCoverage::Known
        } else {
            MetricCoverage::Partial
        },
        source: MetricSource::Derived,
    }
}

#[derive(Debug, Clone)]
struct TaskGroupingCandidate {
    fact: TaskMetricsFact,
    activity_end_seq: u64,
    interval_coverage: MetricCoverage,
}

#[derive(Debug, Clone, Copy)]
struct TaskClassSignal {
    class: TaskClass,
    source: TaskClassSource,
}

fn extract_task_metrics_facts(
    session_id: &str,
    project: &ProjectIdentity,
    session_scope: SessionScope,
    events: &[EventRecord],
    projection: &OperationProjection,
    op_durations: &BTreeMap<String, OperationDuration>,
    fallback_agent_role: Option<String>,
) -> Vec<TaskMetricsFact> {
    let Some(last_seq) = events.last().map(|event| event.seq) else {
        return Vec::new();
    };

    let mut candidates =
        task_grouping_candidates(session_id, project, session_scope, events, last_seq);
    if candidates.is_empty() {
        return Vec::new();
    }

    let token_points = token_points_by_thread(events);
    for candidate in &mut candidates {
        candidate.fact.raw_signals =
            collect_task_raw_signals(events, candidate, fallback_agent_role.as_deref());
        let classification = classify_task_from_raw_signals(&candidate.fact.raw_signals);
        candidate.fact.task_class = classification.class;
        candidate.fact.task_class_source = classification.source;
        candidate.fact.task_class_confidence = classification.confidence;

        let task_coverage = candidate.interval_coverage;
        let local_token_ledger = build_task_token_ledger(
            &token_points,
            candidate.fact.thread_id.as_deref(),
            candidate.fact.started_seq,
            candidate.activity_end_seq,
            task_coverage,
        );
        let snapshots = task_operation_snapshots(
            projection,
            candidate.fact.thread_id.as_deref(),
            candidate.fact.started_seq,
            candidate.activity_end_seq,
        );

        candidate.fact.operations = snapshots
            .as_deref()
            .map(|items| build_task_operation_metrics(items, task_coverage))
            .unwrap_or_else(unknown_operation_metrics);
        candidate.fact.duration =
            build_task_duration_metrics(candidate, snapshots.as_deref(), op_durations);
        candidate.fact.token_ledger = local_token_ledger;
    }

    let child_map = direct_task_children(&candidates);
    let mut facts = candidates
        .into_iter()
        .map(|candidate| candidate.fact)
        .collect::<Vec<_>>();
    for index in (0..facts.len()).rev() {
        let child_indices = child_map.get(&index).cloned().unwrap_or_default();
        let child_total_metric = sum_u64_metrics(
            child_indices
                .iter()
                .map(|child| &facts[*child].token_ledger.total),
        );
        let child_input_metric = sum_u64_metrics(
            child_indices
                .iter()
                .map(|child| &facts[*child].token_ledger.input),
        );
        let child_output_metric = sum_u64_metrics(
            child_indices
                .iter()
                .map(|child| &facts[*child].token_ledger.output),
        );
        let child_cached_input_metric = sum_u64_metrics(
            child_indices
                .iter()
                .map(|child| &facts[*child].token_ledger.cached_input),
        );
        let child_reasoning_metric = sum_u64_metrics(
            child_indices
                .iter()
                .map(|child| &facts[*child].token_ledger.reasoning_output),
        );
        let child_duration_metric = sum_u64_metrics(
            child_indices
                .iter()
                .map(|child| &facts[*child].duration.total_ms),
        );

        let spawn_token_metric = child_total_metric.clone();
        let spawn_duration_metric = child_duration_metric;

        let local_total = facts[index].token_ledger.total.clone();
        let local_input = facts[index].token_ledger.input.clone();
        let local_output = facts[index].token_ledger.output.clone();
        let local_cached_input = facts[index].token_ledger.cached_input.clone();
        let local_reasoning = facts[index].token_ledger.reasoning_output.clone();

        facts[index].token_ledger.total = sum_u64_metrics([&local_total, &child_total_metric]);
        facts[index].token_ledger.input = sum_u64_metrics([&local_input, &child_input_metric]);
        facts[index].token_ledger.output = sum_u64_metrics([&local_output, &child_output_metric]);
        facts[index].token_ledger.cached_input =
            sum_u64_metrics([&local_cached_input, &child_cached_input_metric]);
        facts[index].token_ledger.reasoning_output =
            sum_u64_metrics([&local_reasoning, &child_reasoning_metric]);
        facts[index].token_ledger.task = local_total;
        facts[index].token_ledger.spawn_agent = spawn_token_metric;
        facts[index].duration.spawn_agent_ms = spawn_duration_metric;
    }

    facts.sort_by(|left, right| {
        left.started_at
            .cmp(&right.started_at)
            .then_with(|| left.started_seq.cmp(&right.started_seq))
            .then_with(|| left.analytic_key.cmp(&right.analytic_key))
    });
    facts
}

fn task_grouping_candidates(
    session_id: &str,
    project: &ProjectIdentity,
    session_scope: SessionScope,
    events: &[EventRecord],
    last_seq: u64,
) -> Vec<TaskGroupingCandidate> {
    let start_indices = events
        .iter()
        .enumerate()
        .filter(|(_, event)| event.event_type == TASK_STARTED)
        .collect::<Vec<_>>();
    let mut candidates = Vec::new();
    for (position, (_, start_event)) in start_indices.iter().enumerate() {
        let Some(turn_id): Option<String> =
            payload_string(start_event.payload.as_object(), "turn_id")
        else {
            continue;
        };
        let thread_id = event_thread_id(start_event);
        let parent_thread_id = payload_string(start_event.payload.as_object(), "parent_thread_id");
        let analytic_key = canonical_task_analytic_key(
            session_id,
            start_event.task_id.as_str(),
            turn_id.as_str(),
            thread_id.as_deref(),
        );
        let terminal = events
            .iter()
            .skip_while(|event| event.seq <= start_event.seq)
            .find(|event| is_task_terminal_for(event, turn_id.as_str(), thread_id.as_deref()));
        let next_same_thread_start = start_indices
            .iter()
            .skip(position + 1)
            .map(|(_, event)| *event)
            .find(|event| {
                payload_string(event.payload.as_object(), "turn_id").as_deref()
                    != Some(turn_id.as_str())
                    && event_thread_id(event) == thread_id
            });
        let activity_end_seq = terminal
            .map(|event| event.seq)
            .or_else(|| next_same_thread_start.map(|event| event.seq.saturating_sub(1)))
            .unwrap_or(last_seq);
        let interval_coverage = if terminal.is_some() {
            MetricCoverage::Known
        } else if activity_end_seq > start_event.seq {
            MetricCoverage::Partial
        } else {
            MetricCoverage::Unknown
        };
        let outcome = terminal
            .map(classify_task_terminal_outcome)
            .unwrap_or(OutcomeSummary {
                outcome: SessionOutcome::Unknown,
                coverage: MetricCoverage::Unknown,
                error_type: None,
            });
        let started_at = start_event.ts.clone();
        let ended_at = terminal.map(|event| event.ts.clone());
        let ended_seq = terminal.map(|event| event.seq);

        candidates.push(TaskGroupingCandidate {
            fact: TaskMetricsFact {
                analytic_key,
                session_id: session_id.to_string(),
                project_key: project.project_key.clone(),
                session_scope,
                run_task_id: start_event.task_id.clone(),
                turn_id,
                thread_id,
                parent_thread_id,
                started_at,
                ended_at,
                started_seq: start_event.seq,
                ended_seq,
                outcome,
                token_ledger: unknown_token_ledger(),
                duration: unknown_duration_breakdown(),
                operations: unknown_operation_metrics(),
                task_class: TaskClass::Unknown,
                task_class_source: TaskClassSource::Unclassified,
                task_class_confidence: TaskClassConfidence::Unknown,
                raw_signals: extract_raw_signals_from_payload(start_event.payload.as_object()),
            },
            activity_end_seq,
            interval_coverage,
        });
    }
    candidates
}

fn direct_task_children(candidates: &[TaskGroupingCandidate]) -> BTreeMap<usize, Vec<usize>> {
    let mut children = BTreeMap::<usize, Vec<usize>>::new();
    for (parent_index, parent) in candidates.iter().enumerate() {
        let Some(parent_thread_id) = parent.fact.thread_id.as_deref() else {
            continue;
        };
        for (child_index, child) in candidates.iter().enumerate() {
            if parent_index == child_index {
                continue;
            }
            if child.fact.parent_thread_id.as_deref() != Some(parent_thread_id) {
                continue;
            }
            if child.fact.started_seq < parent.fact.started_seq
                || child.fact.started_seq > parent.activity_end_seq
            {
                continue;
            }
            children.entry(parent_index).or_default().push(child_index);
        }
    }
    children
}

fn canonical_task_analytic_key(
    session_id: &str,
    run_task_id: &str,
    turn_id: &str,
    thread_id: Option<&str>,
) -> String {
    format!(
        "task:{}|{}|{}|{}",
        session_id.trim(),
        run_task_id.trim(),
        turn_id.trim(),
        thread_id.unwrap_or("unknown")
    )
}

fn event_thread_id(event: &EventRecord) -> Option<String> {
    payload_string(event.payload.as_object(), "thread_id")
        .or_else(|| payload_string(event.payload.as_object(), "sender_thread_id"))
}

fn is_task_terminal_for(event: &EventRecord, turn_id: &str, thread_id: Option<&str>) -> bool {
    if !matches!(
        event.event_type.as_str(),
        TASK_COMPLETED | AGENT_ABORTED | AGENT_FAILED
    ) {
        return false;
    }
    if payload_string(event.payload.as_object(), "turn_id").as_deref() != Some(turn_id) {
        return false;
    }
    let event_thread_id = event_thread_id(event);
    match thread_id {
        Some(expected) => event_thread_id.as_deref() == Some(expected),
        None => true,
    }
}

fn classify_task_terminal_outcome(event: &EventRecord) -> OutcomeSummary {
    match event.event_type.as_str() {
        TASK_COMPLETED => OutcomeSummary {
            outcome: SessionOutcome::Completed,
            coverage: MetricCoverage::Known,
            error_type: None,
        },
        AGENT_ABORTED => OutcomeSummary {
            outcome: if error_text(event).as_deref() == Some("interrupted") {
                SessionOutcome::Interrupted
            } else {
                SessionOutcome::Aborted
            },
            coverage: MetricCoverage::Known,
            error_type: error_text(event),
        },
        AGENT_FAILED => OutcomeSummary {
            outcome: SessionOutcome::Failed,
            coverage: MetricCoverage::Known,
            error_type: error_text(event),
        },
        _ => OutcomeSummary {
            outcome: SessionOutcome::Unknown,
            coverage: MetricCoverage::Unknown,
            error_type: None,
        },
    }
}

fn collect_task_raw_signals(
    events: &[EventRecord],
    candidate: &TaskGroupingCandidate,
    fallback_agent_role: Option<&str>,
) -> TaskFactRawSignals {
    let mut signals = candidate.fact.raw_signals.clone();
    for event in events.iter().filter(|event| {
        event.seq >= candidate.fact.started_seq
            && event.seq <= candidate.activity_end_seq
            && task_event_matches_thread(event, candidate.fact.thread_id.as_deref())
    }) {
        merge_task_raw_signals(&mut signals, event.payload.as_object());
    }
    if signals.agent_role.is_none() && candidate.fact.parent_thread_id.is_none() {
        signals.agent_role = fallback_agent_role.map(str::to_string);
    }
    signals
}

fn task_event_matches_thread(event: &EventRecord, thread_id: Option<&str>) -> bool {
    match thread_id {
        Some(expected) => event_thread_id(event).as_deref() == Some(expected),
        None => true,
    }
}

fn extract_raw_signals_from_payload(
    payload: Option<&serde_json::Map<String, Value>>,
) -> TaskFactRawSignals {
    let mut signals = TaskFactRawSignals::default();
    merge_task_raw_signals(&mut signals, payload);
    signals
}

fn merge_task_raw_signals(
    signals: &mut TaskFactRawSignals,
    payload: Option<&serde_json::Map<String, Value>>,
) {
    if signals.agent_role.is_none() {
        signals.agent_role = payload_string(payload, "agent_role");
    }
    if signals.requested_agent_type.is_none() {
        signals.requested_agent_type = payload_string(payload, "requested_agent_type");
    }
    if signals.receiver_role.is_none() {
        signals.receiver_role = payload_string(payload, "receiver_role")
            .or_else(|| payload_string(payload, "new_agent_role"))
            .or_else(|| payload_string(payload, "receiver_agent_role"));
    }
    if signals.collaboration_mode_kind.is_none() {
        signals.collaboration_mode_kind = payload_string(payload, "collaboration_mode_kind");
    }
    if signals.model_context_window.is_none() {
        signals.model_context_window = payload_string(payload, "model_context_window");
    }
    if signals.actor_type.is_none() {
        signals.actor_type = payload_string(payload, "actor_type");
    }
}

fn build_task_token_ledger(
    token_points: &BTreeMap<String, Vec<(u64, TokenSnapshot)>>,
    thread_id: Option<&str>,
    start_seq: u64,
    end_seq: u64,
    coverage: MetricCoverage,
) -> TokenLedger {
    let Some(thread_id) = thread_id else {
        return unknown_token_ledger();
    };
    let Some(points) = token_points.get(thread_id) else {
        return unknown_token_ledger();
    };
    let before_point = points.iter().rev().find(|(seq, _)| *seq < start_seq);
    let end_point = points.iter().rev().find(|(seq, _)| *seq <= end_seq);
    let Some((_, end_snapshot)) = end_point else {
        return unknown_token_ledger();
    };
    let before_snapshot = before_point.map(|(_, snapshot)| *snapshot);
    let before_exists = before_point.is_some();

    let total = token_snapshot_delta_metric(
        end_snapshot.total,
        before_snapshot.and_then(|snapshot| snapshot.total),
        before_exists,
        coverage,
    );
    TokenLedger {
        total: total.clone(),
        input: token_snapshot_delta_metric(
            end_snapshot.input,
            before_snapshot.and_then(|snapshot| snapshot.input),
            before_exists,
            coverage,
        ),
        output: token_snapshot_delta_metric(
            end_snapshot.output,
            before_snapshot.and_then(|snapshot| snapshot.output),
            before_exists,
            coverage,
        ),
        cached_input: token_snapshot_delta_metric(
            end_snapshot.cached_input,
            before_snapshot.and_then(|snapshot| snapshot.cached_input),
            before_exists,
            coverage,
        ),
        reasoning_output: token_snapshot_delta_metric(
            end_snapshot.reasoning_output,
            before_snapshot.and_then(|snapshot| snapshot.reasoning_output),
            before_exists,
            coverage,
        ),
        tool_call: CoveredMetric::unknown(),
        task: total,
        spawn_agent: CoveredMetric::unknown(),
    }
}

#[derive(Debug, Clone, Copy)]
struct ClassifiedTask {
    class: TaskClass,
    source: TaskClassSource,
    confidence: TaskClassConfidence,
}

fn classify_task_from_raw_signals(signals: &TaskFactRawSignals) -> ClassifiedTask {
    let ordered = [
        classify_task_signal(
            signals.collaboration_mode_kind.as_deref(),
            TaskClassSource::CollaborationMode,
        ),
        classify_task_signal(signals.agent_role.as_deref(), TaskClassSource::AgentRole),
        classify_task_signal(
            signals.requested_agent_type.as_deref(),
            TaskClassSource::RequestedAgentType,
        ),
        classify_task_signal(
            signals.receiver_role.as_deref(),
            TaskClassSource::ReceiverRole,
        ),
    ];

    let mut resolved = Vec::new();
    for signal in ordered.into_iter().flatten() {
        if !resolved
            .iter()
            .any(|known: &TaskClassSignal| known.class == signal.class)
        {
            resolved.push(signal);
        }
    }

    match resolved.as_slice() {
        [] => ClassifiedTask {
            class: TaskClass::Unknown,
            source: TaskClassSource::Unclassified,
            confidence: TaskClassConfidence::Unknown,
        },
        [single] => ClassifiedTask {
            class: single.class,
            source: single.source,
            confidence: TaskClassConfidence::Confident,
        },
        _ => ClassifiedTask {
            class: TaskClass::Unknown,
            source: TaskClassSource::AmbiguousSignals,
            confidence: TaskClassConfidence::Partial,
        },
    }
}

fn classify_task_signal(value: Option<&str>, source: TaskClassSource) -> Option<TaskClassSignal> {
    let normalized = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())?;
    let class = match normalized.as_str() {
        "implementation" | "implementer" | "worker" => TaskClass::Implementation,
        "review" | "review_loop" | "reviewer" => TaskClass::Review,
        "analysis" | "docs_researcher" | "explorer" | "research" | "researcher" => {
            TaskClass::Analysis
        }
        "plan" | "planner" | "planning" => TaskClass::Planning,
        "approval" | "approver" => TaskClass::Approval,
        _ => return None,
    };
    Some(TaskClassSignal { class, source })
}

fn token_points_by_thread(events: &[EventRecord]) -> BTreeMap<String, Vec<(u64, TokenSnapshot)>> {
    let mut points = BTreeMap::<String, Vec<(u64, TokenSnapshot)>>::new();
    for event in events
        .iter()
        .filter(|event| event.event_type == INFO_TOKENS)
    {
        let Some(thread_id) = event_thread_id(event) else {
            continue;
        };
        let Some(snapshot) = token_snapshot_from_payload(event.payload.as_object()) else {
            continue;
        };
        points
            .entry(thread_id)
            .or_default()
            .push((event.seq, snapshot));
    }
    points
}

fn token_snapshot_from_payload(
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<TokenSnapshot> {
    let payload = payload?;
    let snapshot = TokenSnapshot {
        input: payload.get("input_tokens").and_then(Value::as_u64),
        cached_input: payload.get("cached_input_tokens").and_then(Value::as_u64),
        output: payload.get("output_tokens").and_then(Value::as_u64),
        reasoning_output: payload
            .get("reasoning_output_tokens")
            .and_then(Value::as_u64),
        total: payload.get("total_tokens").and_then(Value::as_u64),
    };
    (snapshot.input.is_some()
        || snapshot.cached_input.is_some()
        || snapshot.output.is_some()
        || snapshot.reasoning_output.is_some()
        || snapshot.total.is_some())
    .then_some(snapshot)
}

fn token_snapshot_delta_metric(
    end_value: Option<u64>,
    before_value: Option<u64>,
    before_exists: bool,
    coverage: MetricCoverage,
) -> CoveredMetric<u64> {
    let Some(end_value) = end_value else {
        return CoveredMetric::unknown();
    };
    let baseline = if before_exists {
        let Some(before_value) = before_value else {
            return CoveredMetric::unknown();
        };
        before_value
    } else {
        0
    };
    if end_value < baseline {
        return CoveredMetric::unknown();
    }
    covered_u64_metric(
        end_value.saturating_sub(baseline),
        coverage,
        MetricSource::NormalizedEvents,
    )
}

fn task_operation_snapshots<'a>(
    projection: &'a OperationProjection,
    thread_id: Option<&str>,
    start_seq: u64,
    end_seq: u64,
) -> Option<Vec<&'a OperationSnapshot>> {
    let thread_id = thread_id?;
    Some(
        projection
            .snapshots
            .iter()
            .filter(|snapshot| snapshot.key.scope.thread_id.as_deref() == Some(thread_id))
            .filter(|snapshot| {
                let anchor_seq = snapshot.started_seq.unwrap_or(snapshot.last_seq);
                anchor_seq >= start_seq && anchor_seq <= end_seq
            })
            .collect(),
    )
}

fn build_task_operation_metrics(
    snapshots: &[&OperationSnapshot],
    coverage: MetricCoverage,
) -> OperationMetrics {
    let count_kind = |kind: OperationKind| {
        snapshots
            .iter()
            .filter(|snapshot| snapshot.key.kind == kind)
            .count() as u64
    };
    let collaboration_calls = snapshots
        .iter()
        .filter(|snapshot| snapshot.key.kind.as_str().starts_with("collab."))
        .count() as u64;
    let failed = snapshots
        .iter()
        .filter(|snapshot| {
            matches!(
                snapshot.last_status.as_deref(),
                Some("failed" | "error" | "cancelled")
            )
        })
        .count() as u64;
    let successful = snapshots
        .iter()
        .filter(|snapshot| snapshot.last_status.as_deref() == Some("completed"))
        .count() as u64;

    OperationMetrics {
        operation_count: covered_u64_metric(
            snapshots.len() as u64,
            coverage,
            MetricSource::OperationProjection,
        ),
        successful_operations: covered_u64_metric(
            successful,
            coverage,
            MetricSource::OperationProjection,
        ),
        failed_operations: covered_u64_metric(failed, coverage, MetricSource::OperationProjection),
        tool_calls: covered_u64_metric(
            snapshots
                .iter()
                .filter(|snapshot| snapshot.key.kind != OperationKind::FileChange)
                .count() as u64,
            coverage,
            MetricSource::OperationProjection,
        ),
        shell_calls: covered_u64_metric(
            count_kind(OperationKind::Shell),
            coverage,
            MetricSource::OperationProjection,
        ),
        mcp_calls: covered_u64_metric(
            count_kind(OperationKind::Mcp),
            coverage,
            MetricSource::OperationProjection,
        ),
        collaboration_calls: covered_u64_metric(
            collaboration_calls,
            coverage,
            MetricSource::OperationProjection,
        ),
        spawn_agent_calls: covered_u64_metric(
            count_kind(OperationKind::CollabSpawnAgent),
            coverage,
            MetricSource::OperationProjection,
        ),
    }
}

fn build_task_duration_metrics(
    candidate: &TaskGroupingCandidate,
    snapshots: Option<&[&OperationSnapshot]>,
    op_durations: &BTreeMap<String, OperationDuration>,
) -> DurationBreakdown {
    let explicit_total = candidate.fact.ended_at.as_deref().and_then(|ended_at| {
        duration_between(Some(candidate.fact.started_at.as_str()), Some(ended_at))
    });
    let mut tool_ms = 0u64;
    let mut shell_ms = 0u64;
    let mut mcp_ms = 0u64;
    if let Some(snapshots) = snapshots {
        for snapshot in snapshots {
            let duration_ms = op_durations
                .get(&operation_key(snapshot))
                .map(|duration| duration.duration_ms)
                .unwrap_or(0);
            match snapshot.key.kind {
                OperationKind::Shell => shell_ms = shell_ms.saturating_add(duration_ms),
                OperationKind::Mcp => mcp_ms = mcp_ms.saturating_add(duration_ms),
                _ => tool_ms = tool_ms.saturating_add(duration_ms),
            }
        }
    }
    DurationBreakdown {
        total_ms: explicit_total
            .map(|value| CoveredMetric::known(value, MetricSource::NormalizedEvents))
            .unwrap_or_else(CoveredMetric::unknown),
        generation_ms: CoveredMetric::unknown(),
        tool_ms: covered_u64_metric(
            tool_ms,
            candidate.interval_coverage,
            MetricSource::OperationProjection,
        ),
        shell_ms: covered_u64_metric(
            shell_ms,
            candidate.interval_coverage,
            MetricSource::OperationProjection,
        ),
        mcp_ms: covered_u64_metric(
            mcp_ms,
            candidate.interval_coverage,
            MetricSource::OperationProjection,
        ),
        spawn_agent_ms: CoveredMetric::unknown(),
        idle_unknown_ms: CoveredMetric::unknown(),
    }
}

fn unknown_operation_metrics() -> OperationMetrics {
    OperationMetrics {
        operation_count: CoveredMetric::unknown(),
        successful_operations: CoveredMetric::unknown(),
        failed_operations: CoveredMetric::unknown(),
        tool_calls: CoveredMetric::unknown(),
        shell_calls: CoveredMetric::unknown(),
        mcp_calls: CoveredMetric::unknown(),
        collaboration_calls: CoveredMetric::unknown(),
        spawn_agent_calls: CoveredMetric::unknown(),
    }
}

fn unknown_duration_breakdown() -> DurationBreakdown {
    DurationBreakdown {
        total_ms: CoveredMetric::unknown(),
        generation_ms: CoveredMetric::unknown(),
        tool_ms: CoveredMetric::unknown(),
        shell_ms: CoveredMetric::unknown(),
        mcp_ms: CoveredMetric::unknown(),
        spawn_agent_ms: CoveredMetric::unknown(),
        idle_unknown_ms: CoveredMetric::unknown(),
    }
}

fn unknown_token_ledger() -> TokenLedger {
    TokenLedger {
        total: CoveredMetric::unknown(),
        input: CoveredMetric::unknown(),
        output: CoveredMetric::unknown(),
        cached_input: CoveredMetric::unknown(),
        reasoning_output: CoveredMetric::unknown(),
        tool_call: CoveredMetric::unknown(),
        task: CoveredMetric::unknown(),
        spawn_agent: CoveredMetric::unknown(),
    }
}

fn count_distinct_payload_field(events: &[EventRecord], key: &str) -> Option<u64> {
    let values = events
        .iter()
        .filter_map(|event| event.payload.as_object())
        .filter_map(|payload| payload.get(key))
        .filter_map(Value::as_str)
        .filter_map(|value| cleaned(Some(value)))
        .collect::<BTreeSet<_>>();
    (!values.is_empty()).then_some(values.len() as u64)
}

fn count_agent_work_items(events: &[EventRecord], tree: Option<&EventTree>) -> CoveredMetric<u64> {
    if let Some(tree) = tree {
        return CoveredMetric::known(tree.thread_count as u64, MetricSource::EventTree);
    }
    count_distinct_payload_field(events, "thread_id")
        .map(|value| CoveredMetric::partial(value, MetricSource::NormalizedEvents))
        .unwrap_or_else(CoveredMetric::unknown)
}

fn count_review_cycles(events: &[EventRecord], tree: Option<&EventTree>) -> CoveredMetric<u64> {
    let mut count = events
        .iter()
        .filter(|event| {
            event
                .payload
                .as_object()
                .and_then(|payload| {
                    payload
                        .get("agent_role")
                        .or_else(|| payload.get("receiver_role"))
                })
                .and_then(Value::as_str)
                .map(|value| value.eq_ignore_ascii_case("reviewer"))
                .unwrap_or(false)
        })
        .count() as u64;
    if count == 0 {
        if let Some(tree) = tree {
            count = count_reviewer_threads(tree);
        }
    }
    if count > 0 {
        CoveredMetric::partial(count, MetricSource::NormalizedEvents)
    } else {
        CoveredMetric::unknown()
    }
}

fn count_reviewer_threads(tree: &EventTree) -> u64 {
    fn walk(items: &[TimelineItem], count: &mut u64) {
        for item in items {
            match item {
                TimelineItem::Thread(thread) => {
                    if thread.role.as_deref() == Some("reviewer") {
                        *count += 1;
                    }
                    walk(&thread.items, count);
                }
                TimelineItem::Event(node) => walk(&node.children, count),
            }
        }
    }
    let mut count = 0;
    for root in &tree.roots {
        if root.role.as_deref() == Some("reviewer") {
            count += 1;
        }
        walk(&root.items, &mut count);
    }
    count
}

fn count_review_findings(events: &[EventRecord]) -> CoveredMetric<u64> {
    let count = events
        .iter()
        .filter_map(|event| event.payload.as_object())
        .filter_map(|payload| payload.get("artifact_path").or_else(|| payload.get("path")))
        .filter_map(Value::as_str)
        .filter(|value| value.contains("review") || value.contains("finding"))
        .count() as u64;
    if count > 0 {
        CoveredMetric::partial(count, MetricSource::NormalizedEvents)
    } else {
        CoveredMetric::unknown()
    }
}

fn build_derived_efficiency(
    token_ledger: &TokenLedger,
    outcome: &OutcomeSummary,
    review: &BusinessReviewMetrics,
) -> DerivedEfficiencyMetrics {
    let tokens_per_successful_session = if outcome.outcome == SessionOutcome::Completed {
        token_ledger
            .total
            .value
            .map(|tokens| CoveredMetric::known(tokens as f64, MetricSource::Derived))
            .unwrap_or_else(CoveredMetric::unknown)
    } else {
        CoveredMetric::unknown()
    };
    let review_findings_per_1k_tokens =
        match (review.review_findings.value, token_ledger.total.value) {
            (Some(findings), Some(tokens)) if tokens > 0 => CoveredMetric::partial(
                findings as f64 * 1000.0 / tokens as f64,
                MetricSource::Derived,
            ),
            _ => CoveredMetric::unknown(),
        };
    DerivedEfficiencyMetrics {
        tokens_per_successful_session,
        tokens_per_accepted_task: CoveredMetric::unknown(),
        review_findings_per_1k_tokens,
    }
}

fn duration_between(start: Option<&str>, end: Option<&str>) -> Option<u64> {
    let (Some(start), Some(end)) = (start.and_then(parse_ts), end.and_then(parse_ts)) else {
        return None;
    };
    (end - start)
        .to_std()
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

fn parse_ts(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn operation_key(snapshot: &OperationSnapshot) -> String {
    format!(
        "{}|{}|{}|{}",
        snapshot.key.kind.as_str(),
        snapshot.key.scope.run_id,
        snapshot.key.scope.thread_id.as_deref().unwrap_or(""),
        snapshot.key.operation_id
    )
}

fn start_context_size_from_event(event: &EventRecord) -> Option<u64> {
    event
        .payload
        .as_object()
        .and_then(|payload| {
            payload
                .get("start_context_size")
                .or_else(|| payload.get("context_size"))
        })
        .and_then(Value::as_u64)
}

#[derive(Debug, Clone, Copy, Default)]
struct TokenSnapshot {
    input: Option<u64>,
    cached_input: Option<u64>,
    output: Option<u64>,
    reasoning_output: Option<u64>,
    total: Option<u64>,
}

fn first_nonempty_tokens_start_context(events: &[EventRecord]) -> Option<u64> {
    events
        .iter()
        .filter(|event| event.event_type == INFO_TOKENS)
        .filter_map(|event| event.payload.as_object())
        .find_map(|payload| {
            payload
                .get("input_tokens")
                .and_then(Value::as_u64)
                .filter(|value| *value > 0)
                .or_else(|| {
                    payload
                        .get("total_tokens")
                        .and_then(Value::as_u64)
                        .filter(|value| *value > 0)
                })
        })
}

fn error_text(event: &EventRecord) -> Option<String> {
    event.payload.as_object().and_then(|payload| {
        ["error_type", "reason", "error", "message"]
            .iter()
            .find_map(|key| payload.get(*key).and_then(Value::as_str))
            .and_then(|value| cleaned(Some(value)))
    })
}

fn cleaned(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn payload_string(payload: Option<&serde_json::Map<String, Value>>, key: &str) -> Option<String> {
    payload
        .and_then(|obj| obj.get(key))
        .and_then(Value::as_str)
        .and_then(|value| cleaned(Some(value)))
}

fn unknown_u64_metric() -> CoveredMetric<u64> {
    CoveredMetric::unknown()
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tempfile::tempdir;

    use super::*;
    use crate::events::types::{
        AGENT_SESSION, COLLAB_SPAWN_AGENT, SHELL_CALL, SHELL_RESULT, TOOL_CALL, TOOL_RESULT,
    };

    fn event(seq: u64, event_type: &str, ts: &str, payload: Value) -> EventRecord {
        EventRecord {
            schema_version: 1,
            ts: ts.to_string(),
            task_id: "task-1".to_string(),
            run_id: "run-1".to_string(),
            seq,
            event_type: event_type.to_string(),
            raw_type: event_type.to_string(),
            parse_status: "parsed".to_string(),
            payload,
        }
    }

    fn summary() -> IndexedSessionSummary {
        IndexedSessionSummary {
            session_id: "session-1".to_string(),
            updated_at: "2026-04-23T10:02:00Z".to_string(),
            thread_name: Some("metrics".to_string()),
            thread_name_source: Some("summary".to_string()),
            cwd: Some("/repo/project".to_string()),
            agent_name: Some("codex".to_string()),
            tokens_used: Some(999),
            created_at: Some("2026-04-23T10:00:00Z".to_string()),
            source: Some("sqlite".to_string()),
            model_provider: Some("openai".to_string()),
            sandbox_policy_kind: Some("workspace-write".to_string()),
            approval_mode: Some("never".to_string()),
            has_user_event: Some(true),
            archived: Some(false),
            archived_at: None,
            git_sha: Some("abc123".to_string()),
            git_branch: Some("main".to_string()),
            git_origin_url: Some("https://example.test/repo.git".to_string()),
            cli_version: Some("codex-cli 1.2.3".to_string()),
            agent_role: Some("default".to_string()),
            memory_mode: None,
            model: Some("gpt-5.4".to_string()),
            reasoning_effort: Some("medium".to_string()),
            agent_path: None,
        }
    }

    #[test]
    fn extracts_project_identity_with_degraded_bucket() {
        let identity = extract_project_identity(Some(&summary()));
        assert_eq!(identity.state, ProjectIdentityState::Normal);
        assert!(identity.project_key.starts_with("project:"));
        assert_eq!(identity.git_branch.as_deref(), Some("main"));

        let mut incomplete = summary();
        incomplete.cwd = None;
        let degraded = extract_project_identity(Some(&incomplete));
        assert_eq!(degraded.state, ProjectIdentityState::Degraded);
        assert!(degraded.project_key.starts_with("degraded:"));
    }

    #[test]
    fn classifies_session_scope_from_indexed_metadata() {
        let main = classify_session_scope(Some(&summary()));
        assert_eq!(main, SessionScope::Main);

        let mut subsession = summary();
        subsession.source =
            Some(r#"{"subagent":{"thread_spawn":{"parent_thread_id":"root-thread"}}}"#.to_string());
        assert_eq!(
            classify_session_scope(Some(&subsession)),
            SessionScope::Subsession
        );

        let mut unknown = summary();
        unknown.source = Some(r#"{"subagent":{"thread_spawn":{}}}"#.to_string());
        assert_eq!(
            classify_session_scope(Some(&unknown)),
            SessionScope::Unknown
        );
    }

    #[test]
    fn computes_session_metrics_from_normalized_events_and_operation_projection() {
        let events = vec![
            event(
                1,
                MESSAGE_USER,
                "2026-04-23T10:00:00Z",
                json!({"text": "run tests", "thread_id": "root", "turn_id": "turn-1"}),
            ),
            event(
                2,
                SHELL_CALL,
                "2026-04-23T10:00:02Z",
                json!({"call_id": "shell-1", "thread_id": "root", "tool_name": "exec_command", "command": "cargo test"}),
            ),
            event(
                3,
                SHELL_RESULT,
                "2026-04-23T10:00:07Z",
                json!({"call_id": "shell-1", "thread_id": "root", "status": "failed"}),
            ),
            event(
                4,
                TOOL_CALL,
                "2026-04-23T10:00:08Z",
                json!({"tool_use_id": "tool-1", "thread_id": "root", "tool_name": "search_query"}),
            ),
            event(
                5,
                TOOL_RESULT,
                "2026-04-23T10:00:09Z",
                json!({"tool_use_id": "tool-1", "thread_id": "root", "status": "completed", "tool_name": "search_query"}),
            ),
            event(
                6,
                INFO_TOKENS,
                "2026-04-23T10:00:10Z",
                json!({"input_tokens": 100, "cached_input_tokens": 20, "output_tokens": 30, "reasoning_output_tokens": 7, "total_tokens": 150}),
            ),
            event(7, ERROR, "2026-04-23T10:00:11Z", json!({"message": "boom"})),
            event(
                8,
                AGENT_FAILED,
                "2026-04-23T10:00:12Z",
                json!({"error_type": "tool_error"}),
            ),
        ];
        let metrics = compute_session_metrics("session-1", &events, None, Some(&summary()));

        assert_eq!(metrics.event_count.value, Some(8));
        assert_eq!(metrics.message_count.value, Some(1));
        assert_eq!(metrics.operations.operation_count.value, Some(2));
        assert_eq!(metrics.operations.tool_calls.value, Some(2));
        assert_eq!(metrics.operations.shell_calls.value, Some(1));
        assert_eq!(metrics.operations.failed_operations.value, Some(1));
        assert_eq!(metrics.error_count.value, Some(1));
        assert_eq!(metrics.failure_count.value, Some(1));
        assert_eq!(metrics.token_ledger.total.value, Some(150));
        assert_eq!(metrics.token_ledger.input.value, Some(100));
        assert_eq!(metrics.token_ledger.cached_input.value, Some(20));
        assert_eq!(metrics.token_ledger.reasoning_output.value, Some(7));
        assert_eq!(metrics.context.start_context_size.value, Some(100));
        assert_eq!(metrics.duration.total_ms.value, Some(12_000));
        assert_eq!(metrics.duration.shell_ms.value, Some(5_000));
        assert_eq!(metrics.outcome.outcome, SessionOutcome::Failed);
        assert!(metrics
            .tool_breakdown
            .iter()
            .any(|item| item.category == ToolCommandCategory::Test && item.failures == 1));
        assert!(metrics
            .tool_breakdown
            .iter()
            .any(|item| item.category == ToolCommandCategory::Search && item.count == 1));
    }

    #[test]
    fn unknown_token_and_duration_values_are_not_zero() {
        let events = vec![event(
            1,
            MESSAGE_USER,
            "2026-04-23T10:00:00Z",
            json!({"text": "hello"}),
        )];
        let mut indexed = summary();
        indexed.tokens_used = None;
        let metrics = compute_session_metrics("session-1", &events, None, Some(&indexed));

        assert_eq!(metrics.token_ledger.total.value, None);
        assert_eq!(metrics.token_ledger.total.coverage, MetricCoverage::Unknown);
        assert_eq!(metrics.duration.shell_ms.value, None);
        assert_eq!(metrics.duration.shell_ms.coverage, MetricCoverage::Unknown);
        assert_eq!(metrics.operations.operation_count.value, None);
    }

    #[test]
    fn token_ledger_uses_latest_cumulative_snapshot_instead_of_sum() {
        let events = vec![
            event(
                1,
                INFO_TOKENS,
                "2026-04-23T10:00:00Z",
                json!({"input_tokens": 10, "cached_input_tokens": 5, "output_tokens": 2, "reasoning_output_tokens": 1, "total_tokens": 17}),
            ),
            event(
                2,
                INFO_TOKENS,
                "2026-04-23T10:00:01Z",
                json!({"input_tokens": 16, "cached_input_tokens": 7, "output_tokens": 4, "reasoning_output_tokens": 3, "total_tokens": 27}),
            ),
        ];

        let metrics = compute_session_metrics("session-1", &events, None, Some(&summary()));
        assert_eq!(metrics.token_ledger.input.value, Some(16));
        assert_eq!(metrics.token_ledger.cached_input.value, Some(7));
        assert_eq!(metrics.token_ledger.output.value, Some(4));
        assert_eq!(metrics.token_ledger.reasoning_output.value, Some(3));
        assert_eq!(metrics.token_ledger.total.value, Some(27));
        assert_eq!(metrics.context.start_context_size.value, Some(10));
    }

    #[test]
    fn scoped_session_token_ledger_sums_latest_snapshot_per_thread() {
        let events = vec![
            event(
                1,
                INFO_TOKENS,
                "2026-04-23T10:00:00Z",
                json!({"thread_id": "root", "input_tokens": 10, "output_tokens": 8, "reasoning_output_tokens": 2, "total_tokens": 18}),
            ),
            event(
                2,
                INFO_TOKENS,
                "2026-04-23T10:00:01Z",
                json!({"thread_id": "sub-1", "parent_thread_id": "root", "input_tokens": 7, "cached_input_tokens": 3, "output_tokens": 5, "total_tokens": 15}),
            ),
            event(
                3,
                INFO_TOKENS,
                "2026-04-23T10:00:02Z",
                json!({"thread_id": "sub-1", "parent_thread_id": "root", "input_tokens": 9, "cached_input_tokens": 4, "output_tokens": 7, "total_tokens": 20}),
            ),
        ];

        let metrics = compute_session_metrics("session-1", &events, None, Some(&summary()));
        assert_eq!(metrics.token_ledger.total.value, Some(38));
        assert_eq!(metrics.token_ledger.total.coverage, MetricCoverage::Known);
        assert_eq!(metrics.token_ledger.input.value, Some(19));
        assert_eq!(metrics.token_ledger.cached_input.value, Some(4));
        assert_eq!(metrics.token_ledger.output.value, Some(15));
        assert_eq!(metrics.token_ledger.reasoning_output.value, Some(2));
        assert_eq!(metrics.token_ledger.task.value, Some(18));
        assert_eq!(metrics.token_ledger.task.coverage, MetricCoverage::Known);
        assert_eq!(metrics.token_ledger.spawn_agent.value, Some(20));
        assert_eq!(
            metrics.token_ledger.spawn_agent.coverage,
            MetricCoverage::Known
        );
    }

    #[test]
    fn scoped_session_token_ledger_keeps_unscoped_snapshots_partial() {
        let events = vec![
            event(
                1,
                INFO_TOKENS,
                "2026-04-23T10:00:00Z",
                json!({"thread_id": "root", "input_tokens": 10, "output_tokens": 8, "total_tokens": 18}),
            ),
            event(
                2,
                INFO_TOKENS,
                "2026-04-23T10:00:01Z",
                json!({"input_tokens": 14, "output_tokens": 11, "total_tokens": 25}),
            ),
        ];

        let metrics = compute_session_metrics("session-1", &events, None, Some(&summary()));
        assert_eq!(metrics.token_ledger.total.value, Some(18));
        assert_eq!(metrics.token_ledger.total.coverage, MetricCoverage::Partial);
        assert_eq!(metrics.token_ledger.task.value, Some(18));
        assert_eq!(metrics.token_ledger.task.coverage, MetricCoverage::Partial);
        assert_eq!(metrics.token_ledger.spawn_agent.value, Some(0));
        assert_eq!(
            metrics.token_ledger.spawn_agent.coverage,
            MetricCoverage::Partial
        );
    }

    fn assert_materialized_metrics_store_contract(store: &dyn MaterializedMetricsStore) {
        let first = compute_session_metrics(
            "session-1",
            &[event(1, AGENT_COMPLETED, "2026-04-23T10:00:00Z", json!({}))],
            None,
            Some(&summary()),
        );
        let mut second_summary = summary();
        second_summary.session_id = "session-2".to_string();
        let second = compute_session_metrics(
            "session-2",
            &[event(1, AGENT_COMPLETED, "2026-04-23T09:00:00Z", json!({}))],
            None,
            Some(&second_summary),
        );
        store
            .store_session_metrics(&first)
            .expect("first upsert should succeed");
        store
            .store_session_metrics(&second)
            .expect("second upsert should succeed");

        assert!(!store
            .needs_rebuild("session-1", MetricsRebuildVersion::current(),)
            .expect("stale query"));
        assert!(store
            .needs_rebuild(
                "session-1",
                MetricsRebuildVersion {
                    metrics_schema_version: METRICS_SCHEMA_VERSION + 1,
                    source_projection_version: METRICS_PROJECTION_VERSION,
                },
            )
            .expect("stale query"));
        let loaded = store
            .load_session_metrics("session-1")
            .expect("get should succeed")
            .expect("session should exist");
        assert_eq!(loaded.project.project_key, first.project.project_key);
        let detail = ProjectMetricsSessionDetail {
            session_id: "session-1".to_string(),
            session_ref: "2026/04/23/rollout-session-1.jsonl".to_string(),
            title: Some("Implement parser fix".to_string()),
            start_user_request: Some("Fix the parser regression".to_string()),
            start_user_request_source: SessionDetailTextSource::IndexedFirstUserMessage,
            task_summary: Some("Implement parser fix".to_string()),
            task_summary_source: SessionDetailTextSource::IndexedTitle,
            agent_role: Some("worker".to_string()),
            task_class: Some(TaskClass::Implementation),
            task_class_confidence: TaskClassConfidence::Confident,
        };
        store
            .store_session_detail(&detail)
            .expect("detail upsert should succeed");
        assert!(!store
            .needs_detail_rebuild("session-1", MetricsRebuildVersion::current())
            .expect("detail stale query"));
        let loaded_detail = store
            .load_session_detail("session-1")
            .expect("detail get should succeed")
            .expect("detail should exist");
        assert_eq!(loaded_detail.start_user_request, detail.start_user_request);

        let project = store
            .query_project_metrics(&SessionMetricsQuery {
                project_key: first.project.project_key.clone(),
                start_ts: None,
                end_ts: None,
                include_spawn_agents: true,
                session_scope_filter: SessionScopeFilter::All,
            })
            .expect("project query");
        assert_eq!(
            project.contributing_session_ids,
            vec!["session-2", "session-1"]
        );
        assert_eq!(project.scope_filter, SessionScopeFilter::All);
    }

    #[test]
    fn sqlite_metrics_store_satisfies_materialized_metrics_store_contract() {
        let dir = tempdir().expect("tempdir should exist");
        let store = SessionMetricsStore::open(&dir.path().join("metrics.sqlite"))
            .expect("store should open");

        assert_materialized_metrics_store_contract(&store);
    }

    #[test]
    fn project_metrics_session_detail_prefers_indexed_request_and_title() {
        let events = vec![
            event(
                1,
                MESSAGE_USER,
                "2026-04-23T10:00:00Z",
                json!({"text": "Fallback user message"}),
            ),
            event(
                2,
                TASK_STARTED,
                "2026-04-23T10:00:01Z",
                json!({"title": "Fallback task title"}),
            ),
            event(3, AGENT_COMPLETED, "2026-04-23T10:00:02Z", json!({})),
        ];
        let metrics = compute_session_metrics("session-1", &events, None, Some(&summary()));
        let detail = build_project_metrics_session_detail(
            "session-1",
            "2026/04/23/rollout-session-1.jsonl",
            &events,
            Some(&summary()),
            Some(&StateThreadSummary {
                title: Some("Indexed task title".to_string()),
                first_user_message: Some("Indexed request".to_string()),
            }),
            &metrics,
        );

        assert_eq!(
            detail.start_user_request.as_deref(),
            Some("Indexed request")
        );
        assert_eq!(
            detail.start_user_request_source,
            SessionDetailTextSource::IndexedFirstUserMessage
        );
        assert_eq!(detail.task_summary.as_deref(), Some("Indexed task title"));
        assert_eq!(
            detail.task_summary_source,
            SessionDetailTextSource::IndexedTitle
        );
    }

    #[test]
    fn exclude_spawn_agent_mode_preserves_unknowns_and_subtracts_known_contributions() {
        let mut metrics = compute_session_metrics(
            "session-1",
            &[event(1, AGENT_COMPLETED, "2026-04-23T10:00:00Z", json!({}))],
            None,
            Some(&summary()),
        );
        metrics.token_ledger.total = CoveredMetric::known(200, MetricSource::NormalizedEvents);
        metrics.token_ledger.spawn_agent = CoveredMetric::known(50, MetricSource::NormalizedEvents);
        metrics.duration.total_ms = CoveredMetric::known(1_000, MetricSource::NormalizedEvents);
        metrics.duration.spawn_agent_ms =
            CoveredMetric::known(400, MetricSource::OperationProjection);

        let adjusted = apply_spawn_agent_mode(&metrics, SpawnAgentAggregation::Exclude);
        assert_eq!(adjusted.token_ledger.total.value, Some(150));
        assert_eq!(adjusted.duration.total_ms.value, Some(600));
    }

    #[test]
    fn start_context_uses_explicit_runtime_context_before_tokens_and_legacy_fields() {
        let events = vec![
            event(
                1,
                RUNTIME_CONTEXT,
                "2026-04-23T10:00:00Z",
                json!({"context": {"start_context_size": 321}, "skills": ["openspec-apply-change"], "mcp_servers": ["filesystem"]}),
            ),
            event(
                2,
                INFO_TOKENS,
                "2026-04-23T10:00:01Z",
                json!({"input_tokens": 100, "total_tokens": 150}),
            ),
            event(
                3,
                MESSAGE_USER,
                "2026-04-23T10:00:02Z",
                json!({"start_context_size": 80}),
            ),
        ];

        let metrics = compute_session_metrics("session-1", &events, None, Some(&summary()));
        assert_eq!(metrics.context.start_context_size.value, Some(321));
        assert_eq!(metrics.factors.skills_count.value, Some(1));
        assert_eq!(metrics.factors.mcp_server_count.value, Some(1));
    }

    #[test]
    fn start_context_falls_back_to_first_nonempty_tokens_before_legacy_fields() {
        let events = vec![
            event(
                1,
                INFO_TOKENS,
                "2026-04-23T10:00:00Z",
                json!({"input_tokens": 0, "total_tokens": 0}),
            ),
            event(
                2,
                INFO_TOKENS,
                "2026-04-23T10:00:01Z",
                json!({"input_tokens": 42, "total_tokens": 100}),
            ),
            event(
                3,
                MESSAGE_USER,
                "2026-04-23T10:00:02Z",
                json!({"start_context_size": 80}),
            ),
        ];

        let metrics = compute_session_metrics("session-1", &events, None, Some(&summary()));
        assert_eq!(metrics.context.start_context_size.value, Some(42));
    }

    #[test]
    fn project_rollups_aggregate_token_breakdown_counts_and_used_skills_without_zero_fill() {
        let mut first = compute_session_metrics(
            "session-1",
            &[event(1, AGENT_COMPLETED, "2026-04-23T10:00:00Z", json!({}))],
            None,
            Some(&summary()),
        );
        first.token_ledger.input = CoveredMetric::known(10, MetricSource::NormalizedEvents);
        first.token_ledger.output = CoveredMetric::known(20, MetricSource::NormalizedEvents);
        first.token_ledger.cached_input = CoveredMetric::known(5, MetricSource::NormalizedEvents);
        first.token_ledger.reasoning_output =
            CoveredMetric::known(3, MetricSource::NormalizedEvents);
        first.token_ledger.task = CoveredMetric::known(11, MetricSource::NormalizedEvents);
        first.token_ledger.spawn_agent = CoveredMetric::known(7, MetricSource::NormalizedEvents);
        first.factors.skills_count = CoveredMetric::known(2, MetricSource::NormalizedEvents);
        first.factors.mcp_server_count = CoveredMetric::known(1, MetricSource::NormalizedEvents);
        first.factors.start_context_size =
            CoveredMetric::known(100, MetricSource::NormalizedEvents);
        first.task_metrics.task_count = CoveredMetric::known(4, MetricSource::NormalizedEvents);
        first.operations.spawn_agent_calls =
            CoveredMetric::known(1, MetricSource::OperationProjection);
        first.used_skills = UsedSkillsMetrics {
            identifiers: vec!["openspec-apply-change".to_string()],
            count: CoveredMetric::known(1, MetricSource::NormalizedEvents),
            coverage: MetricCoverage::Known,
            source: MetricSource::NormalizedEvents,
        };

        let mut second = compute_session_metrics(
            "session-2",
            &[event(1, AGENT_COMPLETED, "2026-04-23T11:00:00Z", json!({}))],
            None,
            Some(&summary()),
        );
        second.session_id = "session-2".to_string();
        second.token_ledger.input = CoveredMetric::known(30, MetricSource::NormalizedEvents);
        second.token_ledger.output = CoveredMetric::unknown();
        second.token_ledger.cached_input = CoveredMetric::known(10, MetricSource::NormalizedEvents);
        second.token_ledger.reasoning_output = CoveredMetric::unknown();
        second.token_ledger.task = CoveredMetric::unknown();
        second.token_ledger.spawn_agent = CoveredMetric::known(2, MetricSource::NormalizedEvents);
        second.factors.skills_count = CoveredMetric::unknown();
        second.factors.mcp_server_count = CoveredMetric::known(2, MetricSource::NormalizedEvents);
        second.factors.start_context_size =
            CoveredMetric::known(50, MetricSource::NormalizedEvents);
        second.task_metrics.task_count = CoveredMetric::unknown();
        second.operations.spawn_agent_calls =
            CoveredMetric::known(2, MetricSource::OperationProjection);
        second.used_skills = UsedSkillsMetrics {
            identifiers: vec!["openspec-apply-change".to_string(), "shadcn".to_string()],
            count: CoveredMetric::known(2, MetricSource::NormalizedEvents),
            coverage: MetricCoverage::Known,
            source: MetricSource::NormalizedEvents,
        };

        let project = aggregate_project_metrics(
            "project:test".to_string(),
            vec![first, second],
            SessionScopeFilter::All,
            SessionScopeCounts::default(),
        );

        assert_eq!(project.token_ledger.input.value, Some(40));
        assert_eq!(project.token_ledger.output.value, Some(20));
        assert_eq!(
            project.token_ledger.output.coverage,
            MetricCoverage::Partial
        );
        assert_eq!(project.token_ledger.cached_input.value, Some(15));
        assert_eq!(project.token_ledger.reasoning_output.value, Some(3));
        assert_eq!(
            project.token_ledger.reasoning_output.coverage,
            MetricCoverage::Partial
        );
        assert_eq!(project.token_ledger.task.value, Some(11));
        assert_eq!(project.token_ledger.task.coverage, MetricCoverage::Partial);
        assert_eq!(project.token_ledger.spawn_agent.value, Some(9));
        assert_eq!(project.factors.start_context_size.value, Some(150));
        assert_eq!(project.factors.skills_count.value, Some(2));
        assert_eq!(
            project.factors.skills_count.coverage,
            MetricCoverage::Partial
        );
        assert_eq!(project.factors.mcp_server_count.value, Some(3));
        assert_eq!(project.task_metrics.task_count.value, Some(4));
        assert_eq!(
            project.task_metrics.task_count.coverage,
            MetricCoverage::Partial
        );
        assert_eq!(project.operations.spawn_agent_calls.value, Some(3));
        assert_eq!(project.used_skills.coverage, MetricCoverage::Known);
        assert_eq!(project.used_skills.count.value, Some(2));
        assert_eq!(project.used_skills.count.coverage, MetricCoverage::Known);
        assert_eq!(project.used_skills.skills.len(), 2);
        assert_eq!(
            project.used_skills.skills[0].identifier,
            "openspec-apply-change"
        );
        assert_eq!(project.used_skills.skills[0].usage_count, 2);
        assert_eq!(project.used_skills.skills[0].session_count, 2);
    }

    #[test]
    fn query_project_metrics_respects_include_spawn_agents_for_session_and_project_totals() {
        let dir = tempdir().expect("tempdir should exist");
        let store = SessionMetricsStore::open(&dir.path().join("metrics.sqlite"))
            .expect("store should open");

        let mut metrics = compute_session_metrics(
            "session-1",
            &[event(1, AGENT_COMPLETED, "2026-04-23T10:00:00Z", json!({}))],
            None,
            Some(&summary()),
        );
        metrics.project.project_key = "project:test".to_string();
        metrics.token_ledger.total = CoveredMetric::known(200, MetricSource::NormalizedEvents);
        metrics.token_ledger.spawn_agent = CoveredMetric::known(50, MetricSource::NormalizedEvents);
        metrics.duration.total_ms = CoveredMetric::known(1_000, MetricSource::NormalizedEvents);
        metrics.duration.spawn_agent_ms =
            CoveredMetric::known(400, MetricSource::OperationProjection);
        store
            .upsert_session_metrics(&metrics)
            .expect("upsert should succeed");

        let included = store
            .list_project_sessions(&SessionMetricsQuery {
                project_key: "project:test".to_string(),
                start_ts: None,
                end_ts: None,
                include_spawn_agents: true,
                session_scope_filter: SessionScopeFilter::All,
            })
            .expect("project query should succeed");
        let excluded = store
            .list_project_sessions(&SessionMetricsQuery {
                project_key: "project:test".to_string(),
                start_ts: None,
                end_ts: None,
                include_spawn_agents: false,
                session_scope_filter: SessionScopeFilter::All,
            })
            .expect("project query should succeed");

        assert_eq!(included.sessions[0].token_ledger.total.value, Some(200));
        assert_eq!(excluded.sessions[0].token_ledger.total.value, Some(150));
        assert_eq!(included.token_ledger.total.value, Some(200));
        assert_eq!(excluded.token_ledger.total.value, Some(150));
        assert_eq!(included.duration_ms.value, Some(1_000));
        assert_eq!(excluded.duration_ms.value, Some(600));
        assert_eq!(excluded.token_ledger.spawn_agent.value, Some(50));
    }

    #[test]
    fn task_facts_require_turn_identity_for_canonical_key() {
        let events = vec![
            event(
                1,
                TASK_STARTED,
                "2026-04-23T10:00:00Z",
                json!({"thread_id": "root", "collaboration_mode_kind": "untracked"}),
            ),
            event(
                2,
                TASK_COMPLETED,
                "2026-04-23T10:00:01Z",
                json!({"thread_id": "root"}),
            ),
            event(
                3,
                TASK_STARTED,
                "2026-04-23T10:00:02Z",
                json!({"thread_id": "root", "turn_id": "turn-2", "collaboration_mode_kind": "review"}),
            ),
            event(
                4,
                TASK_COMPLETED,
                "2026-04-23T10:00:03Z",
                json!({"thread_id": "root", "turn_id": "turn-2"}),
            ),
        ];

        let metrics = compute_session_metrics("session-1", &events, None, Some(&summary()));
        assert_eq!(metrics.task_facts.len(), 1);
        let fact = &metrics.task_facts[0];
        assert_eq!(fact.analytic_key, "task:session-1|task-1|turn-2|root");
        assert_eq!(fact.turn_id, "turn-2");
        assert_eq!(
            fact.raw_signals.collaboration_mode_kind.as_deref(),
            Some("review")
        );
    }

    #[test]
    fn task_facts_group_by_thread_and_roll_up_spawn_contribution() {
        let events = vec![
            event(
                1,
                TASK_STARTED,
                "2026-04-23T10:00:00Z",
                json!({"thread_id": "root", "turn_id": "turn-root", "collaboration_mode_kind": "interactive"}),
            ),
            event(
                2,
                SHELL_CALL,
                "2026-04-23T10:00:01Z",
                json!({"call_id": "shell-1", "thread_id": "root", "tool_name": "exec_command", "command": "cargo test"}),
            ),
            event(
                3,
                SHELL_RESULT,
                "2026-04-23T10:00:02Z",
                json!({"call_id": "shell-1", "thread_id": "root", "status": "completed"}),
            ),
            event(
                4,
                COLLAB_SPAWN_AGENT,
                "2026-04-23T10:00:03Z",
                json!({
                    "call_id": "spawn-1",
                    "thread_id": "root",
                    "sender_thread_id": "root",
                    "phase": "started",
                    "requested_agent_type": "reviewer"
                }),
            ),
            event(
                5,
                COLLAB_SPAWN_AGENT,
                "2026-04-23T10:00:04Z",
                json!({
                    "call_id": "spawn-1",
                    "thread_id": "root",
                    "sender_thread_id": "root",
                    "phase": "completed",
                    "status": "completed",
                    "requested_agent_type": "reviewer",
                    "new_thread_id": "sub-1",
                    "new_agent_role": "reviewer"
                }),
            ),
            event(
                6,
                TASK_STARTED,
                "2026-04-23T10:00:05Z",
                json!({
                    "thread_id": "sub-1",
                    "parent_thread_id": "root",
                    "turn_id": "turn-sub",
                    "collaboration_mode_kind": "review_loop"
                }),
            ),
            event(
                7,
                AGENT_SESSION,
                "2026-04-23T10:00:06Z",
                json!({"thread_id": "sub-1", "parent_thread_id": "root", "agent_role": "reviewer"}),
            ),
            event(
                8,
                INFO_TOKENS,
                "2026-04-23T10:00:07Z",
                json!({"thread_id": "sub-1", "input_tokens": 7, "output_tokens": 5, "total_tokens": 12}),
            ),
            event(
                9,
                TASK_COMPLETED,
                "2026-04-23T10:00:08Z",
                json!({"thread_id": "sub-1", "parent_thread_id": "root", "turn_id": "turn-sub"}),
            ),
            event(
                10,
                INFO_TOKENS,
                "2026-04-23T10:00:09Z",
                json!({"thread_id": "root", "input_tokens": 10, "output_tokens": 8, "reasoning_output_tokens": 2, "total_tokens": 18}),
            ),
            event(
                11,
                TASK_COMPLETED,
                "2026-04-23T10:00:10Z",
                json!({"thread_id": "root", "turn_id": "turn-root"}),
            ),
        ];

        let metrics = compute_session_metrics("session-1", &events, None, Some(&summary()));
        assert_eq!(metrics.task_facts.len(), 2);

        let parent = metrics
            .task_facts
            .iter()
            .find(|fact| fact.turn_id == "turn-root")
            .expect("parent task fact should exist");
        assert_eq!(parent.operations.shell_calls.value, Some(1));
        assert_eq!(parent.operations.spawn_agent_calls.value, Some(1));
        assert_eq!(parent.duration.total_ms.value, Some(10_000));
        assert_eq!(parent.duration.spawn_agent_ms.value, Some(3_000));
        assert_eq!(parent.token_ledger.task.value, Some(18));
        assert_eq!(parent.token_ledger.spawn_agent.value, Some(12));
        assert_eq!(parent.token_ledger.total.value, Some(30));
        assert_eq!(parent.token_ledger.input.value, Some(17));
        assert_eq!(parent.token_ledger.output.value, Some(13));
        assert_eq!(
            parent.raw_signals.requested_agent_type.as_deref(),
            Some("reviewer")
        );
        assert_eq!(
            parent.raw_signals.receiver_role.as_deref(),
            Some("reviewer")
        );
        assert_eq!(metrics.token_ledger.total.value, Some(30));
        assert_eq!(metrics.token_ledger.input.value, Some(17));
        assert_eq!(metrics.token_ledger.output.value, Some(13));
        assert_eq!(metrics.token_ledger.reasoning_output.value, Some(2));
        assert_eq!(metrics.token_ledger.task.value, Some(18));
        assert_eq!(metrics.token_ledger.spawn_agent.value, Some(12));

        let child = metrics
            .task_facts
            .iter()
            .find(|fact| fact.turn_id == "turn-sub")
            .expect("child task fact should exist");
        assert_eq!(child.parent_thread_id.as_deref(), Some("root"));
        assert_eq!(child.duration.total_ms.value, Some(3_000));
        assert_eq!(child.token_ledger.total.value, Some(12));
        assert_eq!(child.raw_signals.agent_role.as_deref(), Some("reviewer"));
        assert_eq!(
            child.raw_signals.collaboration_mode_kind.as_deref(),
            Some("review_loop")
        );
    }

    #[test]
    fn task_facts_keep_unknown_metrics_when_boundaries_are_open() {
        let events = vec![
            event(
                1,
                TASK_STARTED,
                "2026-04-23T10:00:00Z",
                json!({"thread_id": "root", "turn_id": "turn-open", "collaboration_mode_kind": "interactive"}),
            ),
            event(
                2,
                MESSAGE_AGENT,
                "2026-04-23T10:00:01Z",
                json!({"thread_id": "root", "text": "still working"}),
            ),
        ];

        let metrics = compute_session_metrics("session-1", &events, None, Some(&summary()));
        assert_eq!(metrics.task_facts.len(), 1);
        let fact = &metrics.task_facts[0];
        assert_eq!(fact.outcome.outcome, SessionOutcome::Unknown);
        assert_eq!(fact.duration.total_ms.value, None);
        assert_eq!(fact.token_ledger.total.value, None);
        assert_eq!(fact.token_ledger.spawn_agent.value, None);
    }

    #[test]
    fn task_classification_maps_raw_signals_to_semantic_classes() {
        let cases = [
            (
                json!({"thread_id": "root", "turn_id": "turn-implementation", "agent_role": "worker"}),
                TaskClass::Implementation,
                TaskClassSource::AgentRole,
            ),
            (
                json!({"thread_id": "root", "turn_id": "turn-review", "requested_agent_type": "reviewer"}),
                TaskClass::Review,
                TaskClassSource::RequestedAgentType,
            ),
            (
                json!({"thread_id": "root", "turn_id": "turn-analysis", "receiver_role": "docs_researcher"}),
                TaskClass::Analysis,
                TaskClassSource::ReceiverRole,
            ),
            (
                json!({"thread_id": "root", "turn_id": "turn-planning", "collaboration_mode_kind": "planning"}),
                TaskClass::Planning,
                TaskClassSource::CollaborationMode,
            ),
            (
                json!({"thread_id": "root", "turn_id": "turn-approval", "collaboration_mode_kind": "approval"}),
                TaskClass::Approval,
                TaskClassSource::CollaborationMode,
            ),
        ];

        for (payload, expected_class, expected_source) in cases {
            let turn_id = payload["turn_id"].clone();
            let metrics = compute_session_metrics(
                "session-1",
                &[
                    event(1, TASK_STARTED, "2026-04-23T10:00:00Z", payload),
                    event(
                        2,
                        TASK_COMPLETED,
                        "2026-04-23T10:00:01Z",
                        json!({"thread_id": "root", "turn_id": turn_id}),
                    ),
                ],
                None,
                Some(&summary()),
            );
            let fact = metrics
                .task_facts
                .first()
                .expect("task fact should be materialized");
            assert_eq!(fact.task_class, expected_class);
            assert_eq!(fact.task_class_source, expected_source);
            assert_eq!(fact.task_class_confidence, TaskClassConfidence::Confident);
        }
    }

    #[test]
    fn task_classification_keeps_ambiguous_cases_unknown() {
        let metrics = compute_session_metrics(
            "session-1",
            &[
                event(
                    1,
                    TASK_STARTED,
                    "2026-04-23T10:00:00Z",
                    json!({
                        "thread_id": "root",
                        "turn_id": "turn-1",
                        "agent_role": "worker",
                        "requested_agent_type": "reviewer"
                    }),
                ),
                event(
                    2,
                    TASK_COMPLETED,
                    "2026-04-23T10:00:01Z",
                    json!({"thread_id": "root", "turn_id": "turn-1"}),
                ),
            ],
            None,
            Some(&summary()),
        );
        let fact = metrics
            .task_facts
            .first()
            .expect("task fact should be materialized");
        assert_eq!(fact.task_class, TaskClass::Unknown);
        assert_eq!(fact.task_class_source, TaskClassSource::AmbiguousSignals);
        assert_eq!(fact.task_class_confidence, TaskClassConfidence::Partial);
    }

    #[test]
    fn task_classification_leaves_unmapped_interactive_cases_unknown() {
        let metrics = compute_session_metrics(
            "session-1",
            &[
                event(
                    1,
                    TASK_STARTED,
                    "2026-04-23T10:00:00Z",
                    json!({
                        "thread_id": "root",
                        "turn_id": "turn-1",
                        "collaboration_mode_kind": "interactive",
                        "agent_role": "default"
                    }),
                ),
                event(
                    2,
                    TASK_COMPLETED,
                    "2026-04-23T10:00:01Z",
                    json!({"thread_id": "root", "turn_id": "turn-1"}),
                ),
            ],
            None,
            Some(&summary()),
        );
        let fact = metrics
            .task_facts
            .first()
            .expect("task fact should be materialized");
        assert_eq!(fact.task_class, TaskClass::Unknown);
        assert_eq!(fact.task_class_source, TaskClassSource::Unclassified);
        assert_eq!(fact.task_class_confidence, TaskClassConfidence::Unknown);
    }

    #[test]
    fn used_skills_stay_unknown_without_explicit_markers() {
        let metrics = compute_session_metrics(
            "session-1",
            &[event(
                1,
                RUNTIME_CONTEXT,
                "2026-04-23T10:00:00Z",
                json!({"skills": ["openspec-apply-change"]}),
            )],
            None,
            Some(&summary()),
        );

        assert!(metrics.used_skills.identifiers.is_empty());
        assert_eq!(metrics.used_skills.count.value, None);
        assert_eq!(metrics.used_skills.coverage, MetricCoverage::Unknown);
    }

    #[test]
    fn used_skills_count_deduplicates_repeated_markers_within_session() {
        let metrics = compute_session_metrics(
            "session-1",
            &[
                event(
                    1,
                    SHELL_CALL,
                    "2026-04-23T10:00:01Z",
                    json!({
                        "thread_id": "root",
                        "tool_name": "exec_command",
                        "skill_identifiers": ["openspec-apply-change", "shadcn"]
                    }),
                ),
                event(
                    2,
                    SHELL_CALL,
                    "2026-04-23T10:00:02Z",
                    json!({
                        "thread_id": "root",
                        "tool_name": "exec_command",
                        "skill_identifiers": ["shadcn", "openspec-apply-change"]
                    }),
                ),
            ],
            None,
            Some(&summary()),
        );

        assert_eq!(
            metrics.used_skills.identifiers,
            vec![
                "openspec-apply-change".to_string(),
                "shadcn".to_string(),
            ]
        );
        assert_eq!(metrics.used_skills.count.value, Some(2));
        assert_eq!(metrics.used_skills.count.coverage, MetricCoverage::Known);
    }

    #[test]
    fn enabled_skills_come_from_first_session_message_available_skills_block() {
        let metrics = compute_session_metrics(
            "session-1",
            &[
                event(
                    1,
                    MESSAGE_USER,
                    "2026-04-23T10:00:00Z",
                    json!({
                        "text": "## Skills\n### Available skills\n- openspec-apply-change: apply OpenSpec tasks\n- shadcn: manage shadcn components\n\n### How to use skills\n- ..."
                    }),
                ),
                event(
                    2,
                    RUNTIME_CONTEXT,
                    "2026-04-23T10:00:01Z",
                    json!({"skills": ["openspec-explore"]}),
                ),
            ],
            None,
            Some(&summary()),
        );

        assert_eq!(metrics.factors.skills_count.value, Some(2));
    }

    #[test]
    fn enabled_skills_stay_tied_to_first_runtime_context_list() {
        let metrics = compute_session_metrics(
            "session-1",
            &[
                event(
                    1,
                    RUNTIME_CONTEXT,
                    "2026-04-23T10:00:00Z",
                    json!({"skills": ["openspec-apply-change", "shadcn"]}),
                ),
                event(
                    2,
                    SHELL_CALL,
                    "2026-04-23T10:00:01Z",
                    json!({
                        "thread_id": "root",
                        "tool_name": "exec_command",
                        "command": "sed -n '1,80p' /tmp/skills/openspec-explore/SKILL.md",
                        "skill_identifiers": ["openspec-explore"]
                    }),
                ),
                event(
                    3,
                    RUNTIME_CONTEXT,
                    "2026-04-23T10:00:02Z",
                    json!({"skills": ["openspec-explore"]}),
                ),
            ],
            None,
            Some(&summary()),
        );

        assert_eq!(metrics.factors.skills_count.value, Some(2));
        assert_eq!(
            metrics.used_skills.identifiers,
            vec!["openspec-explore".to_string()]
        );
        assert_eq!(metrics.used_skills.count.value, Some(1));
    }

    #[test]
    fn enabled_skills_fall_back_to_runtime_context_when_first_message_has_no_skills_block() {
        let metrics = compute_session_metrics(
            "session-1",
            &[
                event(
                    1,
                    MESSAGE_USER,
                    "2026-04-23T10:00:00Z",
                    json!({
                        "text": "Обычное пользовательское сообщение без списка skills"
                    }),
                ),
                event(
                    2,
                    RUNTIME_CONTEXT,
                    "2026-04-23T10:00:01Z",
                    json!({"skills": ["openspec-apply-change", "shadcn"]}),
                ),
            ],
            None,
            Some(&summary()),
        );

        assert_eq!(metrics.factors.skills_count.value, Some(2));
    }

    #[test]
    fn explicit_skill_usage_stays_separate_from_enabled_skills_count() {
        let metrics = compute_session_metrics(
            "session-1",
            &[
                event(
                    1,
                    RUNTIME_CONTEXT,
                    "2026-04-23T10:00:00Z",
                    json!({"skills": ["openspec-apply-change"]}),
                ),
                event(
                    2,
                    SHELL_CALL,
                    "2026-04-23T10:00:01Z",
                    json!({
                        "thread_id": "root",
                        "tool_name": "exec_command",
                        "command": "cat /tmp/skills/shadcn/SKILL.md",
                        "skill_identifiers": ["shadcn"]
                    }),
                ),
            ],
            None,
            Some(&summary()),
        );

        assert_eq!(metrics.factors.skills_count.value, Some(1));
        assert_eq!(metrics.used_skills.coverage, MetricCoverage::Known);
        assert_eq!(metrics.used_skills.identifiers, vec!["shadcn".to_string()]);
        assert_eq!(metrics.used_skills.count.value, Some(1));
    }

    #[test]
    fn explicit_skill_message_counts_as_used_skill_without_rewriting_enabled_skills() {
        let metrics = compute_session_metrics(
            "session-1",
            &[
                event(
                    1,
                    MESSAGE_USER,
                    "2026-04-23T10:00:00Z",
                    json!({
                        "text": "## Skills\n### Available skills\n- openspec-apply-change\n- shadcn\n"
                    }),
                ),
                event(
                    2,
                    MESSAGE_USER,
                    "2026-04-23T10:00:01Z",
                    json!({
                        "text": "<skill>\n<name>openspec-explore</name>\n<path>/tmp/skills/openspec-explore/SKILL.md</path>\n</skill>"
                    }),
                ),
            ],
            None,
            Some(&summary()),
        );

        assert_eq!(metrics.factors.skills_count.value, Some(2));
        assert_eq!(
            metrics.used_skills.identifiers,
            vec!["openspec-explore".to_string()]
        );
        assert_eq!(metrics.used_skills.count.value, Some(1));
        assert_eq!(metrics.used_skills.coverage, MetricCoverage::Known);
    }

    #[test]
    fn shell_result_output_skill_paths_do_not_count_as_used_skills() {
        let metrics = compute_session_metrics(
            "session-1",
            &[
                event(
                    1,
                    SHELL_CALL,
                    "2026-04-23T10:00:00Z",
                    json!({
                        "thread_id": "root",
                        "tool_name": "exec_command",
                        "command": "rg -n \"skill\" ."
                    }),
                ),
                event(
                    2,
                    SHELL_RESULT,
                    "2026-04-23T10:00:01Z",
                    json!({
                        "thread_id": "root",
                        "tool_name": "exec_command",
                        "output": "quoted block (file: /home/alko/.codex/skills/.system/openai-docs/SKILL.md)"
                    }),
                ),
            ],
            None,
            Some(&summary()),
        );

        assert!(metrics.used_skills.identifiers.is_empty());
        assert_eq!(metrics.used_skills.count.value, None);
        assert_eq!(metrics.used_skills.coverage, MetricCoverage::Unknown);
    }

    #[test]
    fn quoted_available_skills_blocks_do_not_count_as_skill_usage() {
        let metrics = compute_session_metrics(
            "session-1",
            &[
                event(
                    1,
                    RUNTIME_CONTEXT,
                    "2026-04-23T10:00:00Z",
                    json!({"skills": ["openspec-apply-change", "shadcn"]}),
                ),
                event(
                    2,
                    MESSAGE_USER,
                    "2026-04-23T10:00:01Z",
                    json!({
                        "text": "<skills_instructions>\n### Available skills\n- openspec-apply-change\n- shadcn\n</skills_instructions>"
                    }),
                ),
            ],
            None,
            Some(&summary()),
        );

        assert_eq!(metrics.factors.skills_count.value, Some(2));
        assert!(metrics.used_skills.identifiers.is_empty());
        assert_eq!(metrics.used_skills.count.value, None);
        assert_eq!(metrics.used_skills.coverage, MetricCoverage::Unknown);
    }

    #[test]
    fn project_used_skills_count_keeps_partial_coverage_when_some_sessions_are_unknown() {
        let mut known = compute_session_metrics(
            "session-1",
            &[event(1, AGENT_COMPLETED, "2026-04-23T10:00:00Z", json!({}))],
            None,
            Some(&summary()),
        );
        known.used_skills = UsedSkillsMetrics {
            identifiers: vec!["openspec-apply-change".to_string(), "shadcn".to_string()],
            count: CoveredMetric::known(2, MetricSource::NormalizedEvents),
            coverage: MetricCoverage::Known,
            source: MetricSource::NormalizedEvents,
        };

        let unknown = compute_session_metrics(
            "session-2",
            &[event(1, AGENT_COMPLETED, "2026-04-23T11:00:00Z", json!({}))],
            None,
            Some(&summary()),
        );

        let project = aggregate_project_metrics(
            "project:test".to_string(),
            vec![known, unknown],
            SessionScopeFilter::All,
            SessionScopeCounts::default(),
        );

        assert_eq!(project.used_skills.count.value, Some(2));
        assert_eq!(project.used_skills.count.coverage, MetricCoverage::Partial);
        assert_eq!(project.used_skills.coverage, MetricCoverage::Partial);
    }

    #[test]
    fn deserializes_legacy_metrics_without_new_fields() {
        let legacy = serde_json::json!({
            "session_id": "session-1",
            "metrics_schema_version": 1,
            "source_projection_version": 1,
            "computed_at": "2026-04-23T18:18:37Z",
            "started_at": "2026-04-23T18:18:00Z",
            "ended_at": "2026-04-23T18:18:37Z",
            "project": {
                "project_key": "project:test",
                "state": "normal",
                "cwd": "/repo",
                "normalized_cwd": "/repo",
                "git_origin_url": null,
                "git_branch": null,
                "git_sha": null
            },
            "factors": {
                "model": "gpt-5.4",
                "reasoning_effort": "medium",
                "cli_version": null,
                "sandbox_policy_kind": null,
                "approval_mode": null,
                "agent_role": null,
                "skills_count": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "mcp_server_count": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "mcp_call_count": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "start_context_size": {"value": null, "coverage": "unknown", "source": "unavailable"}
            },
            "outcome": {"outcome": "completed", "coverage": "known", "error_type": null},
            "event_count": {"value": 1, "coverage": "known", "source": "normalized_events"},
            "thread_count": {"value": 1, "coverage": "known", "source": "event_tree"},
            "message_count": {"value": 1, "coverage": "known", "source": "normalized_events"},
            "error_count": {"value": 0, "coverage": "known", "source": "normalized_events"},
            "abort_count": {"value": 0, "coverage": "known", "source": "normalized_events"},
            "failure_count": {"value": 0, "coverage": "known", "source": "normalized_events"},
            "operations": {
                "operation_count": {"value": 0, "coverage": "unknown", "source": "unavailable"},
                "successful_operations": {"value": 0, "coverage": "unknown", "source": "unavailable"},
                "failed_operations": {"value": 0, "coverage": "unknown", "source": "unavailable"},
                "tool_calls": {"value": 0, "coverage": "unknown", "source": "unavailable"},
                "shell_calls": {"value": 0, "coverage": "unknown", "source": "unavailable"},
                "mcp_calls": {"value": 0, "coverage": "unknown", "source": "unavailable"},
                "collaboration_calls": {"value": 0, "coverage": "unknown", "source": "unavailable"},
                "spawn_agent_calls": {"value": 0, "coverage": "unknown", "source": "unavailable"}
            },
            "duration": {
                "total_ms": {"value": 1000, "coverage": "known", "source": "normalized_events"},
                "generation_ms": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "tool_ms": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "shell_ms": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "mcp_ms": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "spawn_agent_ms": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "idle_unknown_ms": {"value": null, "coverage": "unknown", "source": "unavailable"}
            },
            "token_ledger": {
                "total": {"value": 10, "coverage": "known", "source": "normalized_events"},
                "input": {"value": 5, "coverage": "known", "source": "normalized_events"},
                "output": {"value": 5, "coverage": "known", "source": "normalized_events"},
                "cached_input": {"value": 0, "coverage": "known", "source": "normalized_events"},
                "tool_call": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "task": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "spawn_agent": {"value": null, "coverage": "unknown", "source": "unavailable"}
            },
            "tool_breakdown": [],
            "task_metrics": {
                "task_count": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "turn_count": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "agent_work_item_count": {"value": null, "coverage": "unknown", "source": "unavailable"}
            },
            "business_review": {
                "review_cycles": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "review_findings": {"value": null, "coverage": "unknown", "source": "unavailable"}
            },
            "context": {
                "start_context_size": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "context_growth": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "compaction_events": {"value": 0, "coverage": "known", "source": "normalized_events"}
            },
            "quality": {
                "feedback_score": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "evaluator_result_count": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "guardrail_trigger_count": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "handoff_count": {"value": null, "coverage": "unknown", "source": "unavailable"}
            },
            "baseline": {
                "coverage": "unknown",
                "token_usage_delta": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "duration_delta_ms": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "error_rate_delta": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "outcome_rate_delta": {"value": null, "coverage": "unknown", "source": "unavailable"}
            },
            "derived_efficiency": {
                "tokens_per_successful_session": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "tokens_per_accepted_task": {"value": null, "coverage": "unknown", "source": "unavailable"},
                "review_findings_per_1k_tokens": {"value": null, "coverage": "unknown", "source": "unavailable"}
            }
        });

        let metrics: SessionMetrics =
            serde_json::from_value(legacy).expect("legacy metrics should deserialize");
        assert_eq!(metrics.token_ledger.reasoning_output.value, None);
        assert_eq!(
            metrics.token_ledger.reasoning_output.coverage,
            MetricCoverage::Unknown
        );
        assert_eq!(metrics.context.context_compression.value, None);
        assert_eq!(
            metrics.context.context_compression.coverage,
            MetricCoverage::Unknown
        );
        assert_eq!(metrics.used_skills.count.value, None);
        assert_eq!(metrics.used_skills.count.coverage, MetricCoverage::Unknown);
    }
}

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
    INFO_TOKENS, MCP_CALL, MESSAGE_AGENT, MESSAGE_COMMENTARY, MESSAGE_USER, TASK_COMPLETED,
    TASK_STARTED,
};
use crate::session::IndexedSessionSummary;
use crate::tree::{EventTree, TimelineItem};
use crate::util::{hash8, normalize_path, utc_now_iso};

pub const METRICS_SCHEMA_VERSION: u32 = 1;
pub const METRICS_PROJECTION_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricCoverage {
    Known,
    Partial,
    Unknown,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricSource {
    NormalizedEvents,
    OperationProjection,
    EventTree,
    IndexedSessionMetadata,
    SessionMetadata,
    Derived,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectIdentityState {
    Normal,
    Degraded,
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectMetricsResponse {
    pub project_key: String,
    pub session_count: u64,
    pub contributing_session_ids: Vec<String>,
    pub sessions: Vec<SessionMetrics>,
    pub token_ledger: TokenLedger,
    pub duration_ms: CoveredMetric<u64>,
    pub baseline: BaselineComparison,
    pub derived_efficiency: DerivedEfficiencyMetrics,
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

pub fn compute_session_metrics(
    session_id: &str,
    events: &[EventRecord],
    tree: Option<&EventTree>,
    indexed_summary: Option<&IndexedSessionSummary>,
) -> SessionMetrics {
    let operation_projection = project_operation_stream(events);
    let first_ts = events
        .first()
        .and_then(|event| cleaned(Some(event.ts.as_str())));
    let last_ts = events
        .last()
        .and_then(|event| cleaned(Some(event.ts.as_str())));
    let total_duration = duration_between(first_ts.as_deref(), last_ts.as_deref());
    let op_durations = operation_durations(events, &operation_projection);
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

pub struct SessionMetricsStore {
    conn: Connection,
}

impl SessionMetricsStore {
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
                "#,
            )
            .map_err(|err| AppError::Runner(format!("metrics storage init failed: {err}")))?;
        Ok(())
    }

    pub fn upsert_session_metrics(&self, metrics: &SessionMetrics) -> AppResult<()> {
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

    pub fn get_session_metrics(&self, session_id: &str) -> AppResult<Option<SessionMetrics>> {
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

    pub fn is_stale(
        &self,
        session_id: &str,
        metrics_schema_version: u32,
        source_projection_version: u32,
    ) -> AppResult<bool> {
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
            Ok((schema, projection)) => {
                Ok(schema != metrics_schema_version || projection != source_projection_version)
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(true),
            Err(err) => Err(AppError::Runner(format!(
                "metrics stale query failed: {err}"
            ))),
        }
    }

    pub fn list_project_sessions(
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
        for row in rows {
            let payload =
                row.map_err(|err| AppError::Runner(format!("metrics row failed: {err}")))?;
            let metrics = serde_json::from_str::<SessionMetrics>(&payload).map_err(|err| {
                AppError::Runner(format!("metrics deserialization failed: {err}"))
            })?;
            sessions.push(apply_spawn_agent_mode(&metrics, mode));
        }
        Ok(aggregate_project_metrics(
            query.project_key.clone(),
            sessions,
        ))
    }
}

pub fn aggregate_project_metrics(
    project_key: String,
    sessions: Vec<SessionMetrics>,
) -> ProjectMetricsResponse {
    let mut total_tokens = 0u64;
    let mut total_tokens_coverage: Option<MetricCoverage> = None;
    let mut duration_ms = 0u64;
    let mut duration_coverage: Option<MetricCoverage> = None;
    let mut contributing_session_ids = Vec::new();
    for session in &sessions {
        contributing_session_ids.push(session.session_id.clone());
        if let Some(value) = session.token_ledger.total.value {
            total_tokens += value;
            total_tokens_coverage = Some(match total_tokens_coverage {
                Some(current) => current.max(session.token_ledger.total.coverage),
                None => session.token_ledger.total.coverage,
            });
        }
        if let Some(value) = session.duration.total_ms.value {
            duration_ms += value;
            duration_coverage = Some(match duration_coverage {
                Some(current) => current.max(session.duration.total_ms.coverage),
                None => session.duration.total_ms.coverage,
            });
        }
    }
    ProjectMetricsResponse {
        project_key,
        session_count: sessions.len() as u64,
        contributing_session_ids,
        sessions,
        token_ledger: TokenLedger {
            total: covered_sum(total_tokens, total_tokens_coverage, MetricSource::Derived),
            input: CoveredMetric::unknown(),
            output: CoveredMetric::unknown(),
            cached_input: CoveredMetric::unknown(),
            reasoning_output: CoveredMetric::unknown(),
            tool_call: CoveredMetric::unknown(),
            task: CoveredMetric::unknown(),
            spawn_agent: CoveredMetric::unknown(),
        },
        duration_ms: covered_sum(duration_ms, duration_coverage, MetricSource::Derived),
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

fn covered_sum(
    value: u64,
    coverage: Option<MetricCoverage>,
    source: MetricSource,
) -> CoveredMetric<u64> {
    match coverage {
        Some(MetricCoverage::Known) => CoveredMetric::known(value, source),
        Some(MetricCoverage::Partial) => CoveredMetric::partial(value, source),
        Some(MetricCoverage::Unknown) | None => CoveredMetric::unknown(),
    }
}

fn build_factor_metadata(
    events: &[EventRecord],
    indexed_summary: Option<&IndexedSessionSummary>,
) -> FactorMetadata {
    let runtime_context = events
        .iter()
        .find_map(|event| event.payload.as_object())
        .and_then(|payload| payload.get("context"))
        .and_then(Value::as_object);
    FactorMetadata {
        model: indexed_summary.and_then(|value| cleaned(value.model.as_deref())),
        reasoning_effort: indexed_summary
            .and_then(|value| cleaned(value.reasoning_effort.as_deref())),
        cli_version: indexed_summary.and_then(|value| cleaned(value.cli_version.as_deref())),
        sandbox_policy_kind: indexed_summary
            .and_then(|value| cleaned(value.sandbox_policy_kind.as_deref())),
        approval_mode: indexed_summary.and_then(|value| cleaned(value.approval_mode.as_deref())),
        agent_role: indexed_summary.and_then(|value| cleaned(value.agent_role.as_deref())),
        skills_count: count_runtime_array(events, "skills"),
        mcp_server_count: count_runtime_array(events, "mcp_servers"),
        mcp_call_count: CoveredMetric::known(
            events
                .iter()
                .filter(|event| event.event_type == MCP_CALL)
                .count() as u64,
            MetricSource::NormalizedEvents,
        ),
        start_context_size: runtime_context
            .and_then(|payload| payload.get("start_context_size"))
            .and_then(Value::as_u64)
            .or_else(|| first_nonempty_tokens_start_context(events))
            .or_else(|| events.iter().find_map(start_context_size_from_event))
            .map(|value| CoveredMetric::known(value, MetricSource::NormalizedEvents))
            .unwrap_or_else(CoveredMetric::unknown),
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
    if let Some(snapshot) = latest_token_snapshot(events) {
        return TokenLedger {
            total: snapshot
                .total
                .map(|value| CoveredMetric::known(value, MetricSource::NormalizedEvents))
                .unwrap_or_else(CoveredMetric::unknown),
            input: snapshot
                .input
                .map(|value| CoveredMetric::known(value, MetricSource::NormalizedEvents))
                .unwrap_or_else(CoveredMetric::unknown),
            output: snapshot
                .output
                .map(|value| CoveredMetric::known(value, MetricSource::NormalizedEvents))
                .unwrap_or_else(CoveredMetric::unknown),
            cached_input: snapshot
                .cached_input
                .map(|value| CoveredMetric::known(value, MetricSource::NormalizedEvents))
                .unwrap_or_else(CoveredMetric::unknown),
            reasoning_output: snapshot
                .reasoning_output
                .map(|value| CoveredMetric::known(value, MetricSource::NormalizedEvents))
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
    events
        .iter()
        .filter_map(|event| event.payload.as_object())
        .find_map(|payload| payload.get(key).and_then(Value::as_array))
        .map(|items| CoveredMetric::known(items.len() as u64, MetricSource::NormalizedEvents))
        .unwrap_or_else(CoveredMetric::unknown)
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

fn latest_token_snapshot(events: &[EventRecord]) -> Option<TokenSnapshot> {
    let mut snapshot = TokenSnapshot::default();
    let mut seen = false;
    for event in events
        .iter()
        .filter(|event| event.event_type == INFO_TOKENS)
    {
        let Some(payload) = event.payload.as_object() else {
            continue;
        };
        seen = true;
        snapshot.input = payload.get("input_tokens").and_then(Value::as_u64);
        snapshot.cached_input = payload.get("cached_input_tokens").and_then(Value::as_u64);
        snapshot.output = payload.get("output_tokens").and_then(Value::as_u64);
        snapshot.reasoning_output = payload
            .get("reasoning_output_tokens")
            .and_then(Value::as_u64);
        snapshot.total = payload.get("total_tokens").and_then(Value::as_u64);
    }
    seen.then_some(snapshot)
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

fn unknown_u64_metric() -> CoveredMetric<u64> {
    CoveredMetric::unknown()
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tempfile::tempdir;

    use super::*;
    use crate::events::types::{SHELL_CALL, SHELL_RESULT, TOOL_CALL, TOOL_RESULT};

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
    fn storage_upserts_detects_stale_versions_and_lists_project_chronologically() {
        let dir = tempdir().expect("tempdir should exist");
        let store = SessionMetricsStore::open(&dir.path().join("metrics.sqlite"))
            .expect("store should open");
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
        store.upsert_session_metrics(&first).expect("first upsert");
        store
            .upsert_session_metrics(&second)
            .expect("second upsert");

        assert!(!store
            .is_stale(
                "session-1",
                METRICS_SCHEMA_VERSION,
                METRICS_PROJECTION_VERSION
            )
            .expect("stale query"));
        assert!(store
            .is_stale(
                "session-1",
                METRICS_SCHEMA_VERSION + 1,
                METRICS_PROJECTION_VERSION
            )
            .expect("stale query"));
        let loaded = store
            .get_session_metrics("session-1")
            .expect("get should succeed")
            .expect("session should exist");
        assert_eq!(loaded.project.project_key, first.project.project_key);

        let project = store
            .list_project_sessions(&SessionMetricsQuery {
                project_key: first.project.project_key.clone(),
                start_ts: None,
                end_ts: None,
                include_spawn_agents: true,
            })
            .expect("project query");
        assert_eq!(
            project.contributing_session_ids,
            vec!["session-2", "session-1"]
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
    }
}

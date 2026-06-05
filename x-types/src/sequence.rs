use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::ids::{BlockId, Namespace, SequenceId, TenantId};

/// Lifecycle status for sequences: Draft → Staging → Production → Unpublished.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SequenceStatus {
    Draft,
    Staging,
    #[default]
    Production,
    Unpublished,
}

impl std::fmt::Display for SequenceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Draft => f.write_str("draft"),
            Self::Staging => f.write_str("staging"),
            Self::Production => f.write_str("production"),
            Self::Unpublished => f.write_str("unpublished"),
        }
    }
}

impl std::str::FromStr for SequenceStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "draft" => Ok(Self::Draft),
            "staging" => Ok(Self::Staging),
            "production" => Ok(Self::Production),
            "unpublished" => Ok(Self::Unpublished),
            other => Err(format!("unknown sequence status: {other}")),
        }
    }
}

impl SequenceStatus {
    pub fn valid_transitions(self) -> &'static [SequenceStatus] {
        match self {
            Self::Draft => &[Self::Staging, Self::Unpublished],
            Self::Staging => &[Self::Production, Self::Unpublished],
            Self::Production => &[Self::Unpublished],
            Self::Unpublished => &[],
        }
    }

    pub fn can_transition_to(self, target: Self) -> bool {
        self.valid_transitions().contains(&target)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceDefinition {
    pub id: SequenceId,
    pub tenant_id: TenantId,
    pub namespace: Namespace,
    pub name: String,
    pub version: i32,
    /// If true, this version is deprecated. New instances should use a newer version.
    /// Running instances bound to this version continue unaffected.
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default)]
    pub status: SequenceStatus,
    pub blocks: Vec<BlockDefinition>,
    /// Lifecycle interceptors (before/after step, on-signal, on-complete, on-failure).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interceptors: Option<crate::interceptor::InterceptorDef>,
    pub created_at: DateTime<Utc>,
}

/// A block is either a leaf (step) or a composite (parallel, race, etc.).
///
/// This recursive enum IS the workflow DSL.
/// Each variant wraps its definition in `Box<T>` so the enum itself stays a
/// single word. `StepDef` is large (14 fields with many `Option<...>`), and
/// without boxing every `BlockDefinition` — even `SubSequence`, which is small
/// — paid the full size. `Box<T>` is transparent to both `serde` (the default
/// impl delegates to the inner type so wire format is unchanged) and `utoipa`
/// (which forwards `` through `Box<T>`), so this is an internal
/// representation change only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockDefinition {
    Step(Box<StepDef>),
    Parallel(Box<ParallelDef>),
    Race(Box<RaceDef>),
    Loop(Box<LoopDef>),
    ForEach(Box<ForEachDef>),
    Router(Box<RouterDef>),
    TryCatch(Box<TryCatchDef>),
    /// Invoke another sequence as a sub-workflow.
    SubSequence(Box<SubSequenceDef>),
    /// A/B split: route traffic to one of several variants by weight.
    ABSplit(Box<ABSplitDef>),
    /// Cancellation scope: child blocks cannot be cancelled by external cancel signals.
    /// Provides subtree-level non-cancellability (Temporal-style structured concurrency).
    CancellationScope(Box<CancellationScopeDef>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepDef {
    pub id: BlockId,
    pub handler: String,
    #[serde(default)]
    pub params: serde_json::Value,
    pub delay: Option<DelaySpec>,
    pub retry: Option<RetryPolicy>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::serde_duration_opt"
    )]
    pub timeout: Option<Duration>,
    /// If set, this step consumes a rate limit token for the given resource key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit_key: Option<String>,
    /// If set, only execute during the specified time window (per instance timezone).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_window: Option<SendWindow>,
    /// Restrict which context sections this step can access. If omitted, all sections visible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_access: Option<ContextAccess>,
    /// If false, this step will not be cancelled when the instance receives a cancel signal.
    /// Used for cleanup/finalization steps that must complete.
    #[serde(default = "default_true_seq")]
    pub cancellable: bool,
    /// If set, this step pauses execution and waits for human input via a signal.
    /// The signal name is `human_input:{block_id}`. Contains optional timeout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_for_input: Option<HumanInputDef>,
    /// Named task queue for routing to dedicated worker pools.
    /// If omitted, uses the default queue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_name: Option<String>,
    /// SLA deadline: maximum wall-clock time from when this step starts running.
    /// If breached, the escalation handler is invoked and the step is failed.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::serde_duration_opt"
    )]
    pub deadline: Option<Duration>,
    /// Handler to invoke when the SLA deadline is breached.
    /// If omitted but deadline is set, the step simply fails on breach.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_deadline_breach: Option<EscalationDef>,
    /// Fallback handler to invoke when the primary handler's circuit breaker
    /// is `Open`. When set, step dispatch re-targets to this handler instead
    /// of deferring the instance for the cooldown window; when unset, the
    /// legacy behaviour applies (defer to `now + remaining_cooldown_secs`).
    /// Uses the same params + context as the primary handler; its own
    /// failures are tracked under its own breaker key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_handler: Option<String>,
    /// If set, cache step output under this key in the instance KV state.
    /// On subsequent executions, if a cached value exists for the resolved key,
    /// the handler is skipped and the cached value is returned directly.
    /// The key is template-resolved before lookup (e.g. `"rate_{{ data.currency }}"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelaySpec {
    #[serde(with = "crate::serde_duration")]
    pub duration: Duration,
    #[serde(default)]
    pub business_days_only: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::serde_duration_opt"
    )]
    pub jitter: Option<Duration>,
    /// Holiday dates (YYYY-MM-DD) to skip when `business_days_only` is true.
    /// Merged with `context.config.holidays` at runtime for tenant-level calendars.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holidays: Vec<String>,
    /// Fire at a specific local wall-clock time (ISO 8601 `NaiveDateTime`,
    /// e.g. `"2026-03-08T02:30:00"`). When set, `duration` is ignored and
    /// the engine converts this local time to UTC using the step-level
    /// `timezone` (or the instance timezone as fallback). DST transitions
    /// are handled by rolling forward to the next valid local time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fire_at_local: Option<String>,
    /// Timezone for `fire_at_local` (IANA, e.g. `"America/New_York"`).
    /// Falls back to the instance's timezone if omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

/// Time window during which a step is allowed to execute.
/// Hours are in 24h format relative to the instance's timezone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendWindow {
    /// Start hour (0-23). Defaults to 9.
    #[serde(default = "default_window_start")]
    pub start_hour: u8,
    /// End hour (0-23, exclusive). Defaults to 17.
    #[serde(default = "default_window_end")]
    pub end_hour: u8,
    /// Days of week allowed (0=Mon .. 6=Sun). Empty means all days.
    #[serde(default)]
    pub days: Vec<u8>,
}

const fn default_window_start() -> u8 {
    9
}

const fn default_window_end() -> u8 {
    17
}

/// Controls which context sections a step handler can see.
/// When set, only the listed sections are passed to the handler.
///
/// `data` supports field-level granularity via [`FieldAccess`]; other sections
/// are all-or-nothing because they are small by design (`config`, `runtime`)
/// or append-only streams (`audit`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextAccess {
    /// Controls read access to `context.data`. Accepts legacy `true`/`false`,
    /// the keywords `"all"`/`"none"`, or an explicit `{"fields": [..]}` list.
    #[serde(default)]
    pub data: FieldAccess,
    /// Allow reading `context.config`.
    #[serde(default = "default_true_seq")]
    pub config: bool,
    /// Allow reading `context.audit`.
    #[serde(default)]
    pub audit: bool,
    /// Allow reading `context.runtime`.
    #[serde(default)]
    pub runtime: bool,
}

const fn default_true_seq() -> bool {
    true
}

/// Field-level access control for a context section.
///
/// Backward-compatible with the legacy boolean form: pre-M3 sequence
/// definitions wrote `{"data": true}` and are still accepted. New sequences
/// can opt into selective fetch with `{"data": {"fields": ["user_id"]}}`,
/// which lets the scheduler preload only the required fields and skip the
/// rest when hydrating externalized context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FieldAccess {
    /// Legacy boolean: `true` = all fields, `false` = no fields.
    Bool(bool),
    /// Explicit field list: only the listed top-level keys of `context.data`
    /// are visible to the handler.
    Fields { fields: Vec<String> },
    /// String keyword: `"all"` or `"none"`.
    Keyword(AccessKeyword),
}

/// String form of [`FieldAccess`] for human-authored YAML/JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccessKeyword {
    All,
    None,
}

impl Default for FieldAccess {
    fn default() -> Self {
        // Matches the old `default_true_seq` behavior for ContextAccess.data.
        Self::Bool(true)
    }
}

impl FieldAccess {
    /// Canonical "grant everything" value.
    pub const ALL: Self = Self::Bool(true);
    /// Canonical "grant nothing" value.
    pub const NONE: Self = Self::Bool(false);

    /// Return `true` if access to `key` is permitted.
    #[must_use]
    pub fn allows(&self, key: &str) -> bool {
        match self {
            Self::Bool(b) => *b,
            Self::Keyword(AccessKeyword::All) => true,
            Self::Keyword(AccessKeyword::None) => false,
            Self::Fields { fields } => fields.iter().any(|f| f == key),
        }
    }

    /// Return `true` if _any_ field is permitted. Used by code paths that
    /// want to skip work entirely when the handler cannot read the section.
    #[must_use]
    pub const fn allows_any(&self) -> bool {
        match self {
            Self::Bool(b) => *b,
            Self::Keyword(AccessKeyword::All) => true,
            Self::Keyword(AccessKeyword::None) => false,
            Self::Fields { fields } => !fields.is_empty(),
        }
    }

    /// Return the explicit field list when this is a `Fields` variant; `None`
    /// for `All`/`None` (caller must fall back to full fetch or skip).
    #[must_use]
    pub const fn required_fields(&self) -> Option<&[String]> {
        match self {
            Self::Fields { fields } => Some(fields.as_slice()),
            _ => None,
        }
    }
}

/// One option presented to the human reviewer in advanced mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HumanChoice {
    /// Display text shown in the UI.
    pub label: String,
    /// Stable identifier stored in context and used for router matching.
    pub value: String,
}

/// Configuration for human-in-the-loop steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanInputDef {
    /// Prompt or instructions for the human reviewer.
    #[serde(default)]
    pub prompt: String,
    /// Timeout in seconds before the step fails or escalates.
    /// If omitted, waits indefinitely.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::serde_duration_opt"
    )]
    pub timeout: Option<Duration>,
    /// If set and timeout expires, send a signal to this escalation target
    /// instead of failing the step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escalation_handler: Option<String>,
    /// Choices the human can pick from. If `None`, the engine applies the
    /// default yes/no preset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<HumanChoice>>,
    /// Context-variable name under which the picked value is stored
    /// (`context.data[store_as]`). If `None`, the block id is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_as: Option<String>,
    /// When true, the reviewer can attach a free-text comment to their decision.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_comment: bool,
}

impl HumanInputDef {
    /// Structural validation for a single `human_review` definition.
    ///
    /// Rules:
    /// - If `choices` is `Some`, the vector must be non-empty.
    /// - If `choices` is `Some`, every `HumanChoice::value` must be unique
    ///   (router targets rely on a stable 1:1 value → branch mapping).
    /// - If `store_as` is `Some`, the string must be non-empty — an empty
    ///   key would collide with the block-id fallback and is almost
    ///   certainly a client mistake.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(choices) = &self.choices {
            if choices.is_empty() {
                return Err("human_review: `choices` must be non-empty when provided".into());
            }
            let mut seen = std::collections::HashSet::new();
            for c in choices {
                if !seen.insert(c.value.as_str()) {
                    return Err(format!(
                        "human_review: duplicate choice value `{}`",
                        c.value
                    ));
                }
            }
        }
        if let Some(s) = &self.store_as {
            if s.is_empty() {
                return Err("human_review: `store_as` must be non-empty".into());
            }
        }
        Ok(())
    }

    /// Return the choices to present to the human. Defaults to Yes/No when
    /// `choices` is `None`.
    #[must_use]
    pub fn effective_choices(&self) -> Vec<HumanChoice> {
        match &self.choices {
            Some(c) => c.clone(),
            None => vec![
                HumanChoice {
                    label: "Yes".into(),
                    value: "yes".into(),
                },
                HumanChoice {
                    label: "No".into(),
                    value: "no".into(),
                },
            ],
        }
    }
}

/// Action to take when an SLA deadline is breached.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationDef {
    /// Handler name to invoke on breach (e.g. `"notify_slack"`, `"send_alert"`).
    pub handler: String,
    /// Parameters passed to the escalation handler.
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    #[serde(with = "crate::serde_duration")]
    pub initial_backoff: Duration,
    #[serde(with = "crate::serde_duration")]
    pub max_backoff: Duration,
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
}

const fn default_backoff_multiplier() -> f64 {
    2.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelDef {
    pub id: BlockId,
    pub branches: Vec<Vec<BlockDefinition>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaceDef {
    pub id: BlockId,
    pub branches: Vec<Vec<BlockDefinition>>,
    #[serde(default)]
    pub semantics: RaceSemantics,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RaceSemantics {
    #[default]
    FirstToResolve,
    FirstToSucceed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TryCatchDef {
    pub id: BlockId,
    pub try_block: Vec<BlockDefinition>,
    pub catch_block: Vec<BlockDefinition>,
    #[serde(default)]
    pub finally_block: Option<Vec<BlockDefinition>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopDef {
    pub id: BlockId,
    pub condition: String,
    pub body: Vec<BlockDefinition>,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub break_on: Option<String>,
    #[serde(default)]
    pub continue_on_error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_interval: Option<u64>,
}

const fn default_max_iterations() -> u32 {
    1000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForEachDef {
    pub id: BlockId,
    pub collection: String,
    #[serde(default = "default_item_var")]
    pub item_var: String,
    pub body: Vec<BlockDefinition>,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
}

fn default_item_var() -> String {
    "item".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterDef {
    pub id: BlockId,
    pub routes: Vec<Route>,
    #[serde(default)]
    pub default: Option<Vec<BlockDefinition>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub condition: String,
    pub blocks: Vec<BlockDefinition>,
}

/// Invoke another sequence as a child workflow.
/// The child instance is created and linked; the parent waits for completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubSequenceDef {
    pub id: BlockId,
    /// Name of the sequence to invoke (resolved by tenant + namespace + name).
    pub sequence_name: String,
    /// Optional specific version. If omitted, uses the latest non-deprecated version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<i32>,
    /// Input data to pass as the child instance's initial context data.
    #[serde(default)]
    pub input: serde_json::Value,
}

/// A/B split: deterministically route each instance to one of several
/// weighted variants. The chosen variant is persisted in the block output
/// so re-executions always follow the same path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ABSplitDef {
    pub id: BlockId,
    /// Weighted variants. Weights are relative (e.g. 70 + 30 = 100%).
    pub variants: Vec<ABVariant>,
}

/// Cancellation scope: wraps child blocks in a non-cancellable boundary.
///
/// When a cancel signal is received, blocks inside a `CancellationScope`
/// continue executing until completion. The cancel takes effect only after
/// all scoped blocks finish.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancellationScopeDef {
    pub id: BlockId,
    /// Child blocks protected from cancellation.
    pub blocks: Vec<BlockDefinition>,
}

/// One arm of an A/B split.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ABVariant {
    /// Human-readable label (e.g. "control", `"variant_a"`).
    pub name: String,
    /// Relative weight. Higher = more traffic.
    pub weight: u32,
    /// Blocks to execute when this variant is chosen.
    pub blocks: Vec<BlockDefinition>,
}

/// Validation error produced by [`SequenceDefinition::validate`].
#[derive(Debug, Clone, thiserror::Error)]
pub enum SequenceValidationError {
    #[error("duplicate block id: {0}")]
    DuplicateBlockId(String),
    #[error("invalid human_review on block `{block_id}`: {message}")]
    InvalidHumanInput { block_id: String, message: String },
    #[error("block `{block_id}`: {message}")]
    InvalidBlock { block_id: String, message: String },
}

/// Known built-in handler names shipped with the engine. Used for
/// create-time validation warnings when a sequence references an
/// unknown handler (likely a typo).
pub const BUILTIN_HANDLER_NAMES: &[&str] = &[
    "noop",
    "log",
    "sleep",
    "fail",
    "http_request",
    "llm_call",
    "tool_call",
    "mcp_call",
    "agent",
    "embed",
    "memory_store",
    "memory_search",
    "human_review",
    "self_modify",
    "emit_event",
    "send_signal",
    "query_instance",
    "set_state",
    "get_state",
    "delete_state",
    "transform",
    "assert",
    "merge_state",
    "blob_put",
    "blob_get",
];

impl SequenceDefinition {
    /// Structural validation performed at submit time (before the sequence
    /// reaches storage).
    pub fn validate(&self) -> Result<(), SequenceValidationError> {
        if self.blocks.is_empty() {
            return Err(SequenceValidationError::InvalidBlock {
                block_id: "(root)".into(),
                message: "sequence has no blocks".into(),
            });
        }
        let mut seen = std::collections::HashSet::new();
        for block in &self.blocks {
            validate_block(block, &mut seen)?;
        }
        Ok(())
    }

    /// Collect all handler names referenced by Step blocks in the sequence.
    pub fn handler_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for block in &self.blocks {
            collect_handler_names(block, &mut names);
        }
        names.sort();
        names.dedup();
        names
    }

    /// Check for handler names that are not in the built-in list and
    /// return suggestions using fuzzy matching.
    pub fn unknown_handler_warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        for name in self.handler_names() {
            if !BUILTIN_HANDLER_NAMES.contains(&name.as_str()) {
                warnings.push(format!(
                    "unknown handler \"{name}\" — not a built-in; ensure a custom handler is registered"
                ));
            }
        }
        warnings
    }
}

fn collect_handler_names(block: &BlockDefinition, names: &mut Vec<String>) {
    match block {
        BlockDefinition::Step(s) => {
            names.push(s.handler.clone());
        }
        BlockDefinition::Parallel(p) => {
            for branch in &p.branches {
                for b in branch {
                    collect_handler_names(b, names);
                }
            }
        }
        BlockDefinition::Race(r) => {
            for branch in &r.branches {
                for b in branch {
                    collect_handler_names(b, names);
                }
            }
        }
        BlockDefinition::Loop(l) => {
            for b in &l.body {
                collect_handler_names(b, names);
            }
        }
        BlockDefinition::ForEach(fe) => {
            for b in &fe.body {
                collect_handler_names(b, names);
            }
        }
        BlockDefinition::Router(r) => {
            for route in &r.routes {
                for b in &route.blocks {
                    collect_handler_names(b, names);
                }
            }
            if let Some(default) = &r.default {
                for b in default {
                    collect_handler_names(b, names);
                }
            }
        }
        BlockDefinition::TryCatch(tc) => {
            for b in &tc.try_block {
                collect_handler_names(b, names);
            }
            for b in &tc.catch_block {
                collect_handler_names(b, names);
            }
            if let Some(finally) = &tc.finally_block {
                for b in finally {
                    collect_handler_names(b, names);
                }
            }
        }
        BlockDefinition::SubSequence(_) => {}
        BlockDefinition::ABSplit(ab) => {
            for variant in &ab.variants {
                for b in &variant.blocks {
                    collect_handler_names(b, names);
                }
            }
        }
        BlockDefinition::CancellationScope(cs) => {
            for b in &cs.blocks {
                collect_handler_names(b, names);
            }
        }
    }
}

fn block_err(id: &str, msg: impl Into<String>) -> SequenceValidationError {
    SequenceValidationError::InvalidBlock {
        block_id: id.into(),
        message: msg.into(),
    }
}

fn check_id(
    id: &BlockId,
    seen: &mut std::collections::HashSet<String>,
) -> Result<(), SequenceValidationError> {
    if id.as_str().is_empty() {
        return Err(block_err("(empty)", "block id must not be empty"));
    }
    if !seen.insert(id.as_str().to_owned()) {
        return Err(SequenceValidationError::DuplicateBlockId(
            id.as_str().to_owned(),
        ));
    }
    Ok(())
}

fn validate_step(
    s: &StepDef,
    seen: &mut std::collections::HashSet<String>,
) -> Result<(), SequenceValidationError> {
    check_id(&s.id, seen)?;
    let id = s.id.as_str();

    if s.handler.is_empty() {
        return Err(block_err(id, "handler name must not be empty"));
    }

    if let Some(retry) = &s.retry {
        if retry.max_attempts == 0 {
            return Err(block_err(id, "retry.max_attempts must be > 0"));
        }
        if retry.backoff_multiplier <= 0.0 {
            return Err(block_err(id, "retry.backoff_multiplier must be > 0"));
        }
        if retry.initial_backoff > retry.max_backoff {
            return Err(block_err(
                id,
                "retry.initial_backoff must be <= retry.max_backoff",
            ));
        }
    }

    if let Some(sw) = &s.send_window {
        if sw.start_hour > 23 {
            return Err(block_err(id, "send_window.start_hour must be 0-23"));
        }
        if sw.end_hour > 23 {
            return Err(block_err(id, "send_window.end_hour must be 0-23"));
        }
        if sw.start_hour == sw.end_hour {
            return Err(block_err(
                id,
                "send_window.start_hour must differ from end_hour",
            ));
        }
        for &d in &sw.days {
            if d > 6 {
                return Err(block_err(
                    id,
                    format!("send_window.days value {d} out of range 0-6"),
                ));
            }
        }
    }

    if let Some(human) = &s.wait_for_input {
        human
            .validate()
            .map_err(|message| SequenceValidationError::InvalidHumanInput {
                block_id: id.to_string(),
                message,
            })?;
    }

    Ok(())
}

fn validate_branches(
    id: &BlockId,
    label: &str,
    branches: &[Vec<BlockDefinition>],
    seen: &mut std::collections::HashSet<String>,
) -> Result<(), SequenceValidationError> {
    check_id(id, seen)?;
    if branches.is_empty() {
        return Err(block_err(
            id.as_str(),
            format!("{label} must have at least one branch"),
        ));
    }
    for branch in branches {
        for b in branch {
            validate_block(b, seen)?;
        }
    }
    Ok(())
}

fn validate_ab_split(
    ab: &ABSplitDef,
    seen: &mut std::collections::HashSet<String>,
) -> Result<(), SequenceValidationError> {
    check_id(&ab.id, seen)?;
    if ab.variants.len() < 2 {
        return Err(block_err(
            ab.id.as_str(),
            "ab_split must have at least 2 variants",
        ));
    }
    let total_weight: u32 = ab
        .variants
        .iter()
        .fold(0u32, |acc, v| acc.saturating_add(v.weight));
    if total_weight == 0 {
        return Err(block_err(
            ab.id.as_str(),
            "ab_split total weight must be > 0",
        ));
    }
    let mut names_seen = std::collections::HashSet::new();
    for v in &ab.variants {
        if v.name.trim().is_empty() {
            return Err(block_err(
                ab.id.as_str(),
                "ab_split variant name must not be empty",
            ));
        }
        if !names_seen.insert(&v.name) {
            return Err(block_err(
                ab.id.as_str(),
                format!("ab_split duplicate variant name `{}`", v.name),
            ));
        }
        for b in &v.blocks {
            validate_block(b, seen)?;
        }
    }
    Ok(())
}

fn validate_children(
    blocks: &[BlockDefinition],
    seen: &mut std::collections::HashSet<String>,
) -> Result<(), SequenceValidationError> {
    for b in blocks {
        validate_block(b, seen)?;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_block(
    block: &BlockDefinition,
    seen: &mut std::collections::HashSet<String>,
) -> Result<(), SequenceValidationError> {
    match block {
        BlockDefinition::Step(s) => validate_step(s, seen),
        BlockDefinition::Parallel(p) => validate_branches(&p.id, "parallel", &p.branches, seen),
        BlockDefinition::Race(r) => validate_branches(&r.id, "race", &r.branches, seen),

        BlockDefinition::Loop(l) => {
            check_id(&l.id, seen)?;
            if l.condition.trim().is_empty() {
                return Err(block_err(l.id.as_str(), "loop condition must not be empty"));
            }
            if l.body.is_empty() {
                return Err(block_err(l.id.as_str(), "loop body must not be empty"));
            }
            if l.max_iterations == 0 {
                return Err(block_err(l.id.as_str(), "loop max_iterations must be > 0"));
            }
            validate_children(&l.body, seen)
        }

        BlockDefinition::ForEach(fe) => {
            check_id(&fe.id, seen)?;
            if fe.collection.trim().is_empty() {
                return Err(block_err(
                    fe.id.as_str(),
                    "for_each collection must not be empty",
                ));
            }
            if fe.body.is_empty() {
                return Err(block_err(fe.id.as_str(), "for_each body must not be empty"));
            }
            if fe.item_var.trim().is_empty() {
                return Err(block_err(
                    fe.id.as_str(),
                    "for_each item_var must not be empty",
                ));
            }
            if fe.max_iterations == 0 {
                return Err(block_err(
                    fe.id.as_str(),
                    "for_each max_iterations must be > 0",
                ));
            }
            validate_children(&fe.body, seen)
        }

        BlockDefinition::Router(r) => {
            check_id(&r.id, seen)?;
            if r.routes.is_empty() && r.default.is_none() {
                return Err(block_err(
                    r.id.as_str(),
                    "router must have at least one route or a default",
                ));
            }
            for route in &r.routes {
                if route.condition.trim().is_empty() {
                    return Err(block_err(
                        r.id.as_str(),
                        "router route condition must not be empty",
                    ));
                }
                validate_children(&route.blocks, seen)?;
            }
            if let Some(default) = &r.default {
                validate_children(default, seen)?;
            }
            Ok(())
        }

        BlockDefinition::TryCatch(tc) => {
            check_id(&tc.id, seen)?;
            if tc.try_block.is_empty() {
                return Err(block_err(
                    tc.id.as_str(),
                    "try_catch try_block must not be empty",
                ));
            }
            validate_children(&tc.try_block, seen)?;
            validate_children(&tc.catch_block, seen)?;
            if let Some(finally) = &tc.finally_block {
                validate_children(finally, seen)?;
            }
            Ok(())
        }

        BlockDefinition::SubSequence(s) => {
            check_id(&s.id, seen)?;
            if s.sequence_name.trim().is_empty() {
                return Err(block_err(
                    s.id.as_str(),
                    "sub_sequence sequence_name must not be empty",
                ));
            }
            Ok(())
        }

        BlockDefinition::ABSplit(ab) => validate_ab_split(ab, seen),

        BlockDefinition::CancellationScope(cs) => {
            check_id(&cs.id, seen)?;
            if cs.blocks.is_empty() {
                return Err(block_err(
                    cs.id.as_str(),
                    "cancellation_scope must have at least one block",
                ));
            }
            validate_children(&cs.blocks, seen)
        }
    }
}


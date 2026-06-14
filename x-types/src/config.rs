use serde::{Deserialize, Serialize};

/// A wrapper for sensitive strings that redacts the value in Debug and Serialize output.
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(transparent)]
pub struct SecretString(String);

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretString(\"[REDACTED]\")")
    }
}

impl SecretString {
    pub fn new(s: String) -> Self {
        Self(s)
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<String> for SecretString {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for SecretString {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// Top-level configuration. Layered: TOML file -> env vars -> CLI flags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EngineConfig {
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub engine: SchedulerConfig,
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default = "default_database_url")]
    pub url: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    #[serde(default)]
    pub run_migrations: bool,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            url: default_database_url(),
            max_connections: default_max_connections(),
            run_migrations: false,
        }
    }
}

fn default_backend() -> String {
    "postgres".to_string()
}

fn default_database_url() -> String {
    String::new()
}

const fn default_max_connections() -> u32 {
    64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    #[serde(default = "default_tick_interval_ms")]
    pub tick_interval_ms: u64,
    #[serde(default = "default_batch_size")]
    pub batch_size: u32,
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_steps: u32,
    #[serde(default = "default_grace_period")]
    pub shutdown_grace_period_secs: u64,
    #[serde(default = "default_stale_threshold")]
    pub stale_instance_threshold_secs: u64,
    #[serde(default)]
    pub webhooks: WebhookConfig,
    #[serde(default = "default_max_context_bytes")]
    pub max_context_bytes: u32,
    #[serde(default)]
    pub externalize_output_threshold: u32,
    /// Threshold in bytes above which a context.data field is automatically
    /// offloaded to the artifact store (S3/MinIO/local FS) instead of being
    /// stored inline or in `externalized_state`. When 0 (default), offloading
    /// is disabled — the engine falls back to `externalize_output_threshold`
    /// for the relational-DB externalization path.
    ///
    /// Requires an artifact store to be configured; if absent, offload
    /// attempts fail with `Unsupported` and are logged as warnings.
    #[serde(default)]
    pub context_offload_threshold: u32,
    #[serde(default)]
    pub max_instances_per_tenant: u32,
    #[serde(default)]
    pub max_steps_per_instance: u32,
    #[serde(default)]
    pub cron_tick_secs: u64,
    #[serde(default)]
    pub stale_reaper_tick_secs: u64,
    #[serde(default)]
    pub node_reaper_tick_secs: u64,
    #[serde(default)]
    pub node_reaper_stale_secs: u64,
    #[serde(default)]
    pub worker_reaper_tick_secs: u64,
    #[serde(default)]
    pub worker_reaper_stale_secs: u64,
    #[serde(default)]
    pub artifact_retention_secs: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            tick_interval_ms: default_tick_interval_ms(),
            batch_size: default_batch_size(),
            max_concurrent_steps: default_max_concurrent(),
            shutdown_grace_period_secs: default_grace_period(),
            stale_instance_threshold_secs: default_stale_threshold(),
            webhooks: WebhookConfig::default(),
            max_context_bytes: default_max_context_bytes(),
            externalize_output_threshold: 0,
            context_offload_threshold: 0,
            max_instances_per_tenant: 0,
            max_steps_per_instance: 0,
            cron_tick_secs: 10,
            stale_reaper_tick_secs: default_stale_reaper_tick_secs(),
            node_reaper_tick_secs: 60,
            node_reaper_stale_secs: 120,
            worker_reaper_tick_secs: 30,
            worker_reaper_stale_secs: 60,
            artifact_retention_secs: 0,
        }
    }
}

const fn default_tick_interval_ms() -> u64 {
    100
}

const fn default_batch_size() -> u32 {
    256
}

const fn default_max_concurrent() -> u32 {
    128
}

const fn default_grace_period() -> u64 {
    30
}

const fn default_stale_threshold() -> u64 {
    300
}

const fn default_max_context_bytes() -> u32 {
    256 * 1024
}

const fn default_stale_reaper_tick_secs() -> u64 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default = "default_webhook_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_webhook_max_retries")]
    pub max_retries: u32,
    #[serde(default)]
    pub secret: Option<String>,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            urls: Vec::new(),
            timeout_secs: default_webhook_timeout(),
            max_retries: default_webhook_max_retries(),
            secret: None,
        }
    }
}

const fn default_webhook_timeout() -> u64 {
    10
}

const fn default_webhook_max_retries() -> u32 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    #[serde(default = "default_http_addr")]
    pub http_addr: String,
    #[serde(default)]
    pub cors_origins: String,
    #[serde(default)]
    pub max_concurrent_requests: u32,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            http_addr: default_http_addr(),
            cors_origins: String::new(),
            max_concurrent_requests: 0,
        }
    }
}

fn default_http_addr() -> String {
    "127.0.0.1:8080".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default)]
    pub json: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            json: false,
        }
    }
}

fn default_log_level() -> String {
    "info".to_string()
}

impl EngineConfig {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if self.engine.tick_interval_ms == 0 {
            errors.push("engine.tick_interval_ms must be > 0".into());
        }
        if self.engine.batch_size == 0 {
            errors.push("engine.batch_size must be > 0".into());
        }
        if self.engine.max_concurrent_steps == 0 {
            errors.push("engine.max_concurrent_steps must be > 0".into());
        }
        if self.engine.stale_instance_threshold_secs == 0 {
            errors.push("engine.stale_instance_threshold_secs must be > 0".into());
        }
        if self.engine.stale_instance_threshold_secs * 1000 <= self.engine.tick_interval_ms {
            errors.push("engine.stale_instance_threshold_secs must be greater than tick_interval_ms".into());
        }
        match self.database.backend.as_str() {
            "postgres" | "sqlite" => {}
            other => errors.push(format!("database.backend must be 'postgres' or 'sqlite', got {other:?}")),
        }
        match self.logging.level.as_str() {
            "trace" | "debug" | "info" | "warn" | "error" => {}
            other => errors.push(format!("logging.level must be one of trace/debug/info/warn/error, got {other:?}")),
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_config_defaults() {
        let cfg = EngineConfig::default();
        assert_eq!(cfg.database.backend, "postgres");
        assert_eq!(cfg.database.max_connections, 64);
        assert!(!cfg.database.run_migrations);
        assert_eq!(cfg.engine.tick_interval_ms, 100);
        assert_eq!(cfg.engine.batch_size, 256);
        assert_eq!(cfg.engine.max_concurrent_steps, 128);
        assert_eq!(cfg.engine.shutdown_grace_period_secs, 30);
        assert_eq!(cfg.engine.stale_instance_threshold_secs, 300);
        assert_eq!(cfg.api.http_addr, "127.0.0.1:8080");
        assert_eq!(cfg.logging.level, "info");
        assert!(!cfg.logging.json);
    }

    #[test]
    fn validate_default_config_passes() {
        assert!(EngineConfig::default().validate().is_ok());
    }
}

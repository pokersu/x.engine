pub mod api_key;
pub mod artifact;
pub mod audit;
pub mod checkpoint;
pub mod circuit_breaker;
pub mod cluster;
pub mod config;
pub mod context;
pub mod credential;
pub mod cron;
pub mod dedupe;
pub mod encryption;
pub mod error;
pub mod execution;
pub mod filter;
pub mod ids;
pub mod instance;
pub mod interceptor;
pub mod output;
pub mod plugin;
pub mod pool;
pub mod rate_limit;
pub mod rollback;
pub mod sequence;
pub mod suggest;
pub mod session;
pub mod signal;
pub mod trigger;
pub mod worker;
pub mod worker_filter;

// Re-exports for convenience.
pub use config::EngineConfig;
pub use error::{StepError, StorageError};
pub use ids::*;
pub use instance::{InstanceState, Priority, TaskInstance};

/// Shared serde default value functions used across types.
pub mod serde_defaults {
    #[must_use]
    pub fn yes() -> bool {
        true
    }

    #[must_use]
    pub fn default_namespace() -> String {
        "default".to_string()
    }
}

/// Serde helper: serialize/deserialize `std::time::Duration` as milliseconds (u64).
pub mod serde_duration {
    use serde::{de::Error as _, Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(dur: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(u64::try_from(dur.as_millis()).unwrap_or(u64::MAX))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let n = serde_json::Number::deserialize(d)?;
        ms_from_number(&n)
            .map(Duration::from_millis)
            .map_err(D::Error::custom)
    }

    fn ms_from_number(n: &serde_json::Number) -> Result<u64, String> {
        if let Some(ms) = n.as_u64() {
            return Ok(ms);
        }
        if let Some(signed) = n.as_i64() {
            return Err(format!("duration must be non-negative (got {signed} ms)"))
        }
        let f = n.as_f64().ok_or_else(|| format!("duration {n} is not a finite number"))?;
        if !f.is_finite() {
            return Err(format!("duration {f} is not finite"));
        }
        if f < 0.0 {
            return Err(format!("duration must be non-negative (got {f}s)"));
        }
        let ms = f * 1000.0;
        #[allow(clippy::cast_precision_loss)]
        let max_as_f64 = u64::MAX as f64;
        if ms >= max_as_f64 {
            return Ok(u64::MAX);
        }
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        Ok(ms as u64)
    }

    pub(super) fn parse_ms(n: &serde_json::Number) -> Result<u64, String> {
        ms_from_number(n)
    }
}

/// Serde helper: serialize/deserialize `Option<Duration>` as optional milliseconds.
pub mod serde_duration_opt {
    use serde::{de::Error as _, Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(dur: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
        match dur {
            Some(d) => s.serialize_some(&u64::try_from(d.as_millis()).unwrap_or(u64::MAX)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        let val: Option<serde_json::Value> = Option::deserialize(d)?;
        match val {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(serde_json::Value::Number(n)) => super::serde_duration::parse_ms(&n)
                .map(|ms| Some(Duration::from_millis(ms)))
                .map_err(D::Error::custom),
            Some(other) => Err(D::Error::custom(format!(
                "expected number or null for duration, got {other}"
            ))),
        }
    }
}

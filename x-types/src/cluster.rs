use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterNode {
    pub id: Uuid,
    pub name: String,
    pub status: NodeStatus,
    pub registered_at: DateTime<Utc>,
    pub last_heartbeat_at: DateTime<Utc>,
    pub drain: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    Active,
    Stopped,
}

impl std::fmt::Display for NodeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => f.write_str("active"),
            Self::Stopped => f.write_str("stopped"),
        }
    }
}

impl std::str::FromStr for NodeStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "stopped" => Ok(Self::Stopped),
            other => Err(format!("unknown node status: {other}")),
        }
    }
}

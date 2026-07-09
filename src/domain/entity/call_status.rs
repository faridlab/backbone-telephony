use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "call_status", rename_all = "snake_case")]
pub enum CallStatus {
    Ringing,
    Answered,
    Completed,
    Missed,
    Failed,
}

impl std::fmt::Display for CallStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ringing => write!(f, "ringing"),
            Self::Answered => write!(f, "answered"),
            Self::Completed => write!(f, "completed"),
            Self::Missed => write!(f, "missed"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

impl FromStr for CallStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ringing" => Ok(Self::Ringing),
            "answered" => Ok(Self::Answered),
            "completed" => Ok(Self::Completed),
            "missed" => Ok(Self::Missed),
            "failed" => Ok(Self::Failed),
            _ => Err(format!("Unknown CallStatus variant: {}", s)),
        }
    }
}

impl Default for CallStatus {
    fn default() -> Self {
        Self::Ringing
    }
}

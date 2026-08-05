use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::CallDirection;
use super::CallStatus;
use super::AuditMetadata;

/// Strongly-typed ID for Call
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CallId(pub Uuid);

impl CallId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for CallId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for CallId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for CallId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<CallId> for Uuid {
    fn from(id: CallId) -> Self { id.0 }
}

impl AsRef<Uuid> for CallId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for CallId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Call {
    pub id: Uuid,
    pub company_id: Uuid,
    pub direction: CallDirection,
    pub from_number: String,
    pub to_number: String,
    pub party_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub status: CallStatus,
    pub external_id: Option<String>,
    pub subject_type: Option<String>,
    pub subject_id: Option<Uuid>,
    pub started_at: DateTime<Utc>,
    pub answered_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub duration_seconds: i32,
    pub recording_url: Option<String>,
    pub notes: Option<String>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl Call {
    /// Create a builder for Call
    pub fn builder() -> CallBuilder {
        CallBuilder::default()
    }

    /// Create a new Call with required fields
    pub fn new(company_id: Uuid, direction: CallDirection, from_number: String, to_number: String, status: CallStatus, started_at: DateTime<Utc>, duration_seconds: i32) -> Self {
        Self {
            id: Uuid::new_v4(),
            company_id,
            direction,
            from_number,
            to_number,
            party_id: None,
            agent_id: None,
            status,
            external_id: None,
            subject_type: None,
            subject_id: None,
            started_at,
            answered_at: None,
            ended_at: None,
            duration_seconds,
            recording_url: None,
            notes: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> CallId {
        CallId(self.id)
    }

    /// Get when this entity was created
    pub fn created_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.created_at.as_ref()
    }

    /// Get when this entity was last updated
    pub fn updated_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.updated_at.as_ref()
    }

    /// Check if this entity is soft deleted
    pub fn is_deleted(&self) -> bool {
        self.metadata.deleted_at.is_some()
    }

    /// Check if this entity is active (not deleted)
    pub fn is_active(&self) -> bool {
        self.metadata.deleted_at.is_none()
    }

    /// Get when this entity was deleted
    pub fn deleted_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.deleted_at.as_ref()
    }

    /// Get who created this entity
    pub fn created_by(&self) -> Option<&Uuid> {
        self.metadata.created_by.as_ref()
    }

    /// Get who last updated this entity
    pub fn updated_by(&self) -> Option<&Uuid> {
        self.metadata.updated_by.as_ref()
    }

    /// Get who deleted this entity
    pub fn deleted_by(&self) -> Option<&Uuid> {
        self.metadata.deleted_by.as_ref()
    }

    /// Get the current status
    pub fn status(&self) -> &CallStatus {
        &self.status
    }


    // ==========================================================
    // Fluent Setters (with_* for optional fields)
    // ==========================================================

    /// Set the party_id field (chainable)
    pub fn with_party_id(mut self, value: Uuid) -> Self {
        self.party_id = Some(value);
        self
    }

    /// Set the agent_id field (chainable)
    pub fn with_agent_id(mut self, value: Uuid) -> Self {
        self.agent_id = Some(value);
        self
    }

    /// Set the external_id field (chainable)
    pub fn with_external_id(mut self, value: String) -> Self {
        self.external_id = Some(value);
        self
    }

    /// Set the subject_type field (chainable)
    pub fn with_subject_type(mut self, value: String) -> Self {
        self.subject_type = Some(value);
        self
    }

    /// Set the subject_id field (chainable)
    pub fn with_subject_id(mut self, value: Uuid) -> Self {
        self.subject_id = Some(value);
        self
    }

    /// Set the answered_at field (chainable)
    pub fn with_answered_at(mut self, value: DateTime<Utc>) -> Self {
        self.answered_at = Some(value);
        self
    }

    /// Set the ended_at field (chainable)
    pub fn with_ended_at(mut self, value: DateTime<Utc>) -> Self {
        self.ended_at = Some(value);
        self
    }

    /// Set the recording_url field (chainable)
    pub fn with_recording_url(mut self, value: String) -> Self {
        self.recording_url = Some(value);
        self
    }

    /// Set the notes field (chainable)
    pub fn with_notes(mut self, value: String) -> Self {
        self.notes = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "company_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.company_id = v; }
                }
                "direction" => {
                    if let Ok(v) = serde_json::from_value(value) { self.direction = v; }
                }
                "from_number" => {
                    if let Ok(v) = serde_json::from_value(value) { self.from_number = v; }
                }
                "to_number" => {
                    if let Ok(v) = serde_json::from_value(value) { self.to_number = v; }
                }
                "party_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.party_id = v; }
                }
                "agent_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.agent_id = v; }
                }
                "status" => {
                    if let Ok(v) = serde_json::from_value(value) { self.status = v; }
                }
                "external_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.external_id = v; }
                }
                "subject_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.subject_type = v; }
                }
                "subject_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.subject_id = v; }
                }
                "started_at" => {
                    if let Ok(v) = serde_json::from_value(value) { self.started_at = v; }
                }
                "answered_at" => {
                    if let Ok(v) = serde_json::from_value(value) { self.answered_at = v; }
                }
                "ended_at" => {
                    if let Ok(v) = serde_json::from_value(value) { self.ended_at = v; }
                }
                "duration_seconds" => {
                    if let Ok(v) = serde_json::from_value(value) { self.duration_seconds = v; }
                }
                "recording_url" => {
                    if let Ok(v) = serde_json::from_value(value) { self.recording_url = v; }
                }
                "notes" => {
                    if let Ok(v) = serde_json::from_value(value) { self.notes = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for Call {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "Call"
    }
}

impl backbone_core::PersistentEntity for Call {
    fn entity_id(&self) -> String {
        self.id.to_string()
    }
    fn set_entity_id(&mut self, id: String) {
        if let Ok(uuid) = uuid::Uuid::parse_str(&id) {
            self.id = uuid;
        }
    }
    fn created_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.created_at
    }
    fn set_created_at(&mut self, ts: chrono::DateTime<chrono::Utc>) {
        self.metadata.created_at = Some(ts);
    }
    fn updated_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.updated_at
    }
    fn set_updated_at(&mut self, ts: chrono::DateTime<chrono::Utc>) {
        self.metadata.updated_at = Some(ts);
    }
    fn deleted_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.deleted_at
    }
    fn set_deleted_at(&mut self, ts: Option<chrono::DateTime<chrono::Utc>>) {
        self.metadata.deleted_at = ts;
    }
}

impl backbone_orm::EntityRepoMeta for Call {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("company_id".to_string(), "uuid".to_string());
        m.insert("party_id".to_string(), "uuid".to_string());
        m.insert("agent_id".to_string(), "uuid".to_string());
        m.insert("subject_id".to_string(), "uuid".to_string());
        m.insert("direction".to_string(), "call_direction".to_string());
        m.insert("status".to_string(), "call_status".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["from_number", "to_number"]
    }
    fn company_field() -> Option<&'static str> {
        Some("company_id")
    }
}

/// Builder for Call entity
///
/// Provides a fluent API for constructing Call instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct CallBuilder {
    company_id: Option<Uuid>,
    direction: Option<CallDirection>,
    from_number: Option<String>,
    to_number: Option<String>,
    party_id: Option<Uuid>,
    agent_id: Option<Uuid>,
    status: Option<CallStatus>,
    external_id: Option<String>,
    subject_type: Option<String>,
    subject_id: Option<Uuid>,
    started_at: Option<DateTime<Utc>>,
    answered_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    duration_seconds: Option<i32>,
    recording_url: Option<String>,
    notes: Option<String>,
}

impl CallBuilder {
    /// Set the company_id field (required)
    pub fn company_id(mut self, value: Uuid) -> Self {
        self.company_id = Some(value);
        self
    }

    /// Set the direction field (required)
    pub fn direction(mut self, value: CallDirection) -> Self {
        self.direction = Some(value);
        self
    }

    /// Set the from_number field (required)
    pub fn from_number(mut self, value: String) -> Self {
        self.from_number = Some(value);
        self
    }

    /// Set the to_number field (required)
    pub fn to_number(mut self, value: String) -> Self {
        self.to_number = Some(value);
        self
    }

    /// Set the party_id field (optional)
    pub fn party_id(mut self, value: Uuid) -> Self {
        self.party_id = Some(value);
        self
    }

    /// Set the agent_id field (optional)
    pub fn agent_id(mut self, value: Uuid) -> Self {
        self.agent_id = Some(value);
        self
    }

    /// Set the status field (default: `CallStatus::default()`)
    pub fn status(mut self, value: CallStatus) -> Self {
        self.status = Some(value);
        self
    }

    /// Set the external_id field (optional)
    pub fn external_id(mut self, value: String) -> Self {
        self.external_id = Some(value);
        self
    }

    /// Set the subject_type field (optional)
    pub fn subject_type(mut self, value: String) -> Self {
        self.subject_type = Some(value);
        self
    }

    /// Set the subject_id field (optional)
    pub fn subject_id(mut self, value: Uuid) -> Self {
        self.subject_id = Some(value);
        self
    }

    /// Set the started_at field (default: `Utc::now()`)
    pub fn started_at(mut self, value: DateTime<Utc>) -> Self {
        self.started_at = Some(value);
        self
    }

    /// Set the answered_at field (optional)
    pub fn answered_at(mut self, value: DateTime<Utc>) -> Self {
        self.answered_at = Some(value);
        self
    }

    /// Set the ended_at field (optional)
    pub fn ended_at(mut self, value: DateTime<Utc>) -> Self {
        self.ended_at = Some(value);
        self
    }

    /// Set the duration_seconds field (default: `0`)
    pub fn duration_seconds(mut self, value: i32) -> Self {
        self.duration_seconds = Some(value);
        self
    }

    /// Set the recording_url field (optional)
    pub fn recording_url(mut self, value: String) -> Self {
        self.recording_url = Some(value);
        self
    }

    /// Set the notes field (optional)
    pub fn notes(mut self, value: String) -> Self {
        self.notes = Some(value);
        self
    }

    /// Build the Call entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<Call, String> {
        let company_id = self.company_id.ok_or_else(|| "company_id is required".to_string())?;
        let direction = self.direction.ok_or_else(|| "direction is required".to_string())?;
        let from_number = self.from_number.ok_or_else(|| "from_number is required".to_string())?;
        let to_number = self.to_number.ok_or_else(|| "to_number is required".to_string())?;

        Ok(Call {
            id: Uuid::new_v4(),
            company_id,
            direction,
            from_number,
            to_number,
            party_id: self.party_id,
            agent_id: self.agent_id,
            status: self.status.unwrap_or(CallStatus::default()),
            external_id: self.external_id,
            subject_type: self.subject_type,
            subject_id: self.subject_id,
            started_at: self.started_at.unwrap_or(Utc::now()),
            answered_at: self.answered_at,
            ended_at: self.ended_at,
            duration_seconds: self.duration_seconds.unwrap_or(0),
            recording_url: self.recording_url,
            notes: self.notes,
            metadata: AuditMetadata::default(),
        })
    }
}

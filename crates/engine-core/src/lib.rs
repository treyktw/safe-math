//! Shared, vendor-neutral contracts for every engine in this workspace.
//!
//! Engine crates depend on this crate; the application depends on engine
//! crates. This crate deliberately has no database, HTTP, queue, or model SDK.

#![forbid(unsafe_code)]

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use uuid::Uuid;

pub type EngineResult<T> = Result<T, EngineError>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EngineId(String);

impl EngineId {
    pub fn new(value: impl Into<String>) -> EngineResult<Self> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(EngineError::invalid_input(
                "engine_id_empty",
                "engine id must not be empty",
            ));
        }
        if !trimmed.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        }) {
            return Err(EngineError::invalid_input(
                "engine_id_invalid",
                "engine id may only contain ASCII letters, numbers, dashes, underscores, and dots",
            ));
        }
        Ok(Self(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EngineId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    Math,
    Graphing,
    Visualization,
    Manim,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineRuntime {
    InProcess,
    ExternalProcess,
    Container,
    RemoteApi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostClass {
    Free,
    Low,
    Medium,
    High,
    Variable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineMetadata {
    pub id: EngineId,
    pub kind: EngineKind,
    pub name: String,
    pub version: String,
    pub runtime: EngineRuntime,
    pub cost_class: CostClass,
    pub capabilities: BTreeSet<String>,
    pub concurrency_limit: Option<usize>,
    pub attributes: BTreeMap<String, Value>,
}

impl EngineMetadata {
    pub fn new(id: EngineId, kind: EngineKind, name: impl Into<String>) -> Self {
        Self {
            id,
            kind,
            name: name.into(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            runtime: EngineRuntime::InProcess,
            cost_class: CostClass::Free,
            capabilities: BTreeSet::new(),
            concurrency_limit: None,
            attributes: BTreeMap::new(),
        }
    }

    pub fn with_capabilities(
        mut self,
        capabilities: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.capabilities
            .extend(capabilities.into_iter().map(Into::into));
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineHealth {
    pub status: HealthStatus,
    pub checked_at: DateTime<Utc>,
    pub latency_ms: u64,
    pub message: Option<String>,
    pub details: BTreeMap<String, Value>,
}

impl EngineHealth {
    pub fn healthy(latency_ms: u64) -> Self {
        Self {
            status: HealthStatus::Healthy,
            checked_at: Utc::now(),
            latency_ms,
            message: None,
            details: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineRequestContext {
    pub request_id: Uuid,
    pub workspace_id: Uuid,
    pub actor_id: Option<String>,
    pub deadline: Option<DateTime<Utc>>,
    pub trace_parent: Option<String>,
    pub labels: BTreeMap<String, String>,
}

impl EngineRequestContext {
    pub fn new(workspace_id: Uuid) -> Self {
        Self {
            request_id: Uuid::new_v4(),
            workspace_id,
            actor_id: None,
            deadline: None,
            trace_parent: None,
            labels: BTreeMap::new(),
        }
    }

    pub fn ensure_active(&self) -> EngineResult<()> {
        if self.deadline.is_some_and(|deadline| deadline <= Utc::now()) {
            return Err(EngineError::timeout(
                "request_deadline_exceeded",
                "engine request deadline exceeded",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineErrorKind {
    InvalidInput,
    NotFound,
    Conflict,
    Unavailable,
    Timeout,
    RateLimited,
    Capacity,
    Dependency,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{code}: {message}")]
pub struct EngineError {
    pub code: String,
    pub kind: EngineErrorKind,
    pub message: String,
    pub retryable: bool,
    pub details: BTreeMap<String, Value>,
}

impl EngineError {
    pub fn new(
        kind: EngineErrorKind,
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
    ) -> Self {
        Self {
            code: code.into(),
            kind,
            message: message.into(),
            retryable,
            details: BTreeMap::new(),
        }
    }

    pub fn invalid_input(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(EngineErrorKind::InvalidInput, code, message, false)
    }

    pub fn not_found(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(EngineErrorKind::NotFound, code, message, false)
    }

    pub fn conflict(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(EngineErrorKind::Conflict, code, message, false)
    }

    pub fn unavailable(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(EngineErrorKind::Unavailable, code, message, true)
    }

    pub fn timeout(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(EngineErrorKind::Timeout, code, message, true)
    }

    pub fn rate_limited(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(EngineErrorKind::RateLimited, code, message, true)
    }

    pub fn capacity(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(EngineErrorKind::Capacity, code, message, false)
    }

    pub fn dependency(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(EngineErrorKind::Dependency, code, message, true)
    }

    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }
}

#[async_trait]
pub trait Engine: Send + Sync {
    fn metadata(&self) -> EngineMetadata;
    async fn health(&self) -> EngineHealth;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_ids_are_stable_and_safe() {
        assert!(EngineId::new("math.sympy-v1").is_ok());
        assert!(EngineId::new(" ").is_err());
        assert!(EngineId::new("math engine").is_err());
    }

    #[test]
    fn expired_requests_fail_with_a_retryable_timeout() {
        let mut context = EngineRequestContext::new(Uuid::new_v4());
        context.deadline = Some(Utc::now() - chrono::Duration::seconds(1));
        let error = context.ensure_active().unwrap_err();
        assert_eq!(error.kind, EngineErrorKind::Timeout);
        assert!(error.retryable);
    }
}

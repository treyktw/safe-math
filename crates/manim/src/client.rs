use crate::protocol::{ManimRenderRequest, RenderSubmission};
use crate::validation::validate_request;
use async_trait::async_trait;
use engine_core::{
    CostClass, Engine, EngineError, EngineHealth, EngineId, EngineKind, EngineMetadata,
    EngineResult, EngineRuntime, HealthStatus,
};
use serde::Deserialize;
use std::time::{Duration, Instant};

const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;

#[async_trait]
pub trait ManimEngine: Engine {
    async fn submit(&self, request: ManimRenderRequest) -> EngineResult<RenderSubmission>;
}

#[derive(Debug, Clone)]
pub struct RemoteManimConfig {
    pub endpoint: String,
    pub timeout: Duration,
    pub bearer_token: Option<String>,
}

impl Default for RemoteManimConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:8083".into(),
            timeout: Duration::from_secs(10),
            bearer_token: None,
        }
    }
}

#[derive(Clone)]
pub struct RemoteManimEngine {
    client: reqwest::Client,
    config: RemoteManimConfig,
    metadata: EngineMetadata,
}

impl RemoteManimEngine {
    pub fn new(config: RemoteManimConfig) -> EngineResult<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(config.timeout)
            .user_agent("safe-math/0.1 manim-engine")
            .build()
            .map_err(|error| EngineError::dependency("manim_client_failed", error.to_string()))?;
        let mut metadata = EngineMetadata::new(
            EngineId::new("manim.remote-renderer")?,
            EngineKind::Manim,
            "Remote Manim renderer",
        )
        .with_capabilities([
            "technical_animation",
            "mathematical_animation",
            "structured_scene",
            "async_render",
        ]);
        metadata.runtime = EngineRuntime::RemoteApi;
        metadata.cost_class = CostClass::Medium;
        metadata
            .attributes
            .insert("acceptsPythonSource".into(), false.into());
        metadata
            .attributes
            .insert("acceptsRawLatex".into(), false.into());
        Ok(Self {
            client,
            config,
            metadata,
        })
    }

    fn endpoint(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.config.endpoint.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }
    fn authorize(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.config.bearer_token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }
}

#[async_trait]
impl Engine for RemoteManimEngine {
    fn metadata(&self) -> EngineMetadata {
        self.metadata.clone()
    }
    async fn health(&self) -> EngineHealth {
        let started = Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            self.authorize(self.client.get(self.endpoint("health")))
                .send(),
        )
        .await;
        let mut health = EngineHealth::healthy(started.elapsed().as_millis() as u64);
        match result {
            Ok(Ok(response)) if response.status().is_success() => {}
            Ok(Ok(response)) => {
                health.status = HealthStatus::Unhealthy;
                health.message = Some(format!("Manim renderer returned {}", response.status()));
            }
            Ok(Err(error)) => {
                health.status = HealthStatus::Unhealthy;
                health.message = Some(format!("Manim renderer unreachable: {error}"));
            }
            Err(_) => {
                health.status = HealthStatus::Unhealthy;
                health.message = Some("Manim renderer health check timed out".into());
            }
        }
        health
    }
}

#[async_trait]
impl ManimEngine for RemoteManimEngine {
    async fn submit(&self, request: ManimRenderRequest) -> EngineResult<RenderSubmission> {
        validate_request(&request)?;
        let response = self
            .authorize(self.client.post(self.endpoint("v1/renders")))
            .json(&request)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    EngineError::timeout(
                        "manim_submit_timeout",
                        "Manim render submission timed out",
                    )
                } else {
                    EngineError::unavailable("manim_renderer_unreachable", error.to_string())
                }
            })?;
        let (status, body) = read_response(response).await?;
        if !status.is_success() {
            let error = serde_json::from_slice::<RendererError>(&body).unwrap_or(RendererError {
                code: "manim_renderer_failed".into(),
                message: String::from_utf8_lossy(&body).chars().take(500).collect(),
                retryable: status.is_server_error(),
            });
            return Err(if status.as_u16() == 408 {
                EngineError::timeout(error.code, error.message)
            } else if status.as_u16() == 429 {
                EngineError::rate_limited(error.code, error.message)
            } else if status.is_client_error() {
                EngineError::invalid_input(error.code, error.message)
            } else if error.retryable {
                EngineError::unavailable(error.code, error.message)
            } else {
                EngineError::dependency(error.code, error.message)
            });
        }
        serde_json::from_slice(&body)
            .map_err(|error| EngineError::dependency("manim_response_invalid", error.to_string()))
    }
}

async fn read_response(
    mut response: reqwest::Response,
) -> EngineResult<(reqwest::StatusCode, Vec<u8>)> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES)
    {
        return Err(EngineError::dependency(
            "manim_response_too_large",
            "Manim renderer response exceeded 1 MiB",
        ));
    }
    let status = response.status();
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| EngineError::dependency("manim_response_broken", error.to_string()))?
    {
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES as usize {
            return Err(EngineError::dependency(
                "manim_response_too_large",
                "Manim renderer response exceeded 1 MiB",
            ));
        }
        body.extend_from_slice(&chunk);
    }
    Ok((status, body))
}

#[derive(Deserialize)]
struct RendererError {
    code: String,
    message: String,
    #[serde(default)]
    retryable: bool,
}

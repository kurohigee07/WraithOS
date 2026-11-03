use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Once;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum SdkError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("api error ({status}): {body}")]
    Api { status: StatusCode, body: String },
    #[error("invalid url: {0}")]
    InvalidUrl(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
    pub id: String,
    pub version: String,
    pub tenant: String,
    pub capabilities: serde_json::Value,
    pub quota: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResponse {
    pub count: usize,
    pub modules: Vec<ModuleInfo>,
}

pub struct WraithClient {
    http: Client,
    base_url: String,
    token: Option<String>,
}

impl WraithClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self, SdkError> {
        static PROVIDER: Once = Once::new();
        PROVIDER.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
        let url = base_url.into();
        if !url.starts_with("http") {
            return Err(SdkError::InvalidUrl(url));
        }
        let http = Client::builder().timeout(Duration::from_secs(30)).build()?;
        Ok(Self {
            http,
            base_url: url,
            token: None,
        })
    }

    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    fn req(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let mut rb = self.http.request(method, url);
        if let Some(t) = &self.token {
            rb = rb.bearer_auth(t);
        }
        rb
    }

    async fn handle_response(resp: reqwest::Response) -> Result<serde_json::Value, SdkError> {
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(SdkError::Api { status, body: text });
        }
        serde_json::from_str(&text).map_err(|e| SdkError::Api {
            status,
            body: format!("json parse: {e}: {text}"),
        })
    }

    pub async fn list(&self, tenant: Option<&str>) -> Result<ListResponse, SdkError> {
        let path = match tenant {
            Some(t) => format!("/t/{t}/modules"),
            None => "/modules".to_string(),
        };
        let resp = self.req(reqwest::Method::GET, &path).send().await?;
        let val = Self::handle_response(resp).await?;
        Ok(serde_json::from_value(val).unwrap_or(ListResponse {
            count: 0,
            modules: vec![],
        }))
    }

    pub async fn execute(
        &self,
        id: &str,
        tenant: Option<&str>,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, SdkError> {
        let path = match tenant {
            Some(t) => format!("/t/{t}/modules/{id}/execute"),
            None => format!("/modules/{id}/execute"),
        };
        let resp = self
            .req(reqwest::Method::POST, &path)
            .json(&payload)
            .send()
            .await?;
        Self::handle_response(resp).await
    }

    pub async fn delete(&self, id: &str, tenant: Option<&str>) -> Result<(), SdkError> {
        let path = match tenant {
            Some(t) => format!("/t/{t}/modules/{id}"),
            None => format!("/modules/{id}"),
        };
        let resp = self.req(reqwest::Method::DELETE, &path).send().await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(SdkError::Api {
                status,
                body: resp.text().await?,
            });
        }
        Ok(())
    }

    pub async fn upload(
        &self,
        id: &str,
        tenant: Option<&str>,
        wasm_bytes: Vec<u8>,
    ) -> Result<serde_json::Value, SdkError> {
        let path = match tenant {
            Some(t) => format!("/t/{t}/modules"),
            None => "/modules".to_string(),
        };
        let resp = self
            .req(reqwest::Method::POST, &path)
            .header("X-Module-Id", id)
            .header("Content-Type", "application/wasm")
            .body(wasm_bytes)
            .send()
            .await?;
        Self::handle_response(resp).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionLog {
    pub id: i64,
    pub tenant: String,
    pub module_id: String,
    pub status: String,
    pub duration_ms: i64,
    pub error_message: Option<String>,
    pub executed_at: String,
}

impl WraithClient {
    pub async fn get_logs(&self) -> Result<Vec<ExecutionLog>, SdkError> {
        let resp = self.req(reqwest::Method::GET, "/audit/logs").send().await?;
        let val = Self::handle_response(resp).await?;
        Ok(serde_json::from_value(val).unwrap_or_default())
    }
}

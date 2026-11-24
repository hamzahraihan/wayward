//! HTTP seam for official usage APIs.
//!
//! `UsageHttpClient` is the only network boundary in `wayward-ai`. Providers
//! depend on the trait so tests inject a fake client and never make requests.
//! No URL, header, or response body containing a credential is ever logged.

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use serde_json::Value;

/// A JSON response plus its HTTP status, so providers can handle a 403 body
/// (for example OpenCode's entitlement error) without losing the payload.
#[derive(Debug, Clone)]
pub struct HttpJsonResponse {
    pub status: u16,
    pub body: Value,
}

/// Owned request data; avoids lifetime-bearing futures in the trait.
#[derive(Debug, Clone)]
pub struct HttpGet {
    pub url: String,
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct HttpPost {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

#[async_trait]
pub trait UsageHttpClient: Send + Sync {
    async fn get_json(&self, request: HttpGet) -> anyhow::Result<HttpJsonResponse>;
    async fn post_json(&self, request: HttpPost) -> anyhow::Result<HttpJsonResponse>;
}

/// Production client. Uses rustls, so no OpenSSL dependency is introduced.
pub struct ReqwestClient {
    inner: reqwest::Client,
}

impl ReqwestClient {
    pub fn new() -> Self {
        Self {
            inner: reqwest::Client::builder()
                .user_agent(concat!("wayward/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("reqwest client with rustls should build"),
        }
    }
}

impl Default for ReqwestClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl UsageHttpClient for ReqwestClient {
    async fn get_json(&self, request: HttpGet) -> anyhow::Result<HttpJsonResponse> {
        let mut builder = self.inner.get(&request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        let response = builder.send().await.context("usage API request failed")?;
        let status = response.status().as_u16();
        let body: Value = response
            .json()
            .await
            .context("usage API response is not JSON")?;
        Ok(HttpJsonResponse { status, body })
    }

    async fn post_json(&self, request: HttpPost) -> anyhow::Result<HttpJsonResponse> {
        let mut builder = self.inner.post(&request.url).json(&request.body);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        let response = builder.send().await.context("usage API request failed")?;
        let status = response.status().as_u16();
        let body: Value = response
            .json()
            .await
            .context("usage API response is not JSON")?;
        Ok(HttpJsonResponse { status, body })
    }
}

/// Pure helper: require a 2xx status and map a non-2xx body to an error that
/// contains only the status and a bounded, non-secret error code if present.
pub fn require_success(response: HttpJsonResponse) -> anyhow::Result<Value> {
    if (200..300).contains(&response.status) {
        return Ok(response.body);
    }
    let code = response
        .body
        .get("error")
        .and_then(|error| error.get("type").or_else(|| error.get("code")))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    Err(anyhow!(
        "usage API returned HTTP {} ({code})",
        response.status
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_success_returns_body_for_200() {
        let body = serde_json::json!({ "ok": true });
        assert_eq!(
            require_success(HttpJsonResponse {
                status: 200,
                body: body.clone()
            })
            .unwrap(),
            body
        );
    }

    #[test]
    fn require_success_reports_status_and_error_code() {
        let error = require_success(HttpJsonResponse {
            status: 403,
            body: serde_json::json!({ "error": { "type": "EntitlementError" } }),
        })
        .expect_err("403 must fail");
        let message = error.to_string();
        assert!(message.contains("403"), "{message}");
        assert!(message.contains("EntitlementError"), "{message}");
    }

    #[test]
    fn require_success_without_error_body_still_reports_status() {
        let error = require_success(HttpJsonResponse {
            status: 503,
            body: serde_json::Value::Null,
        })
        .expect_err("503 must fail");
        assert!(error.to_string().contains("503"));
    }
}

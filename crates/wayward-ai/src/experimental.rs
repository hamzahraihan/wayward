//! Experimental provider for an undocumented internal endpoint.
//!
//! NEVER enabled by default. Requires both `experimental = true` in `[ai]`
//! and an explicit enabled provider entry. The endpoint and its field names
//! can change without notice; unknown response shapes fail closed. Endpoint
//! and response fields were taken from the reviewed reference implementation,
//! not invented. No credential is logged or written to TOML.

use std::sync::Arc;
use std::time::SystemTime;

use anyhow::anyhow;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::http::{require_success, HttpPost, UsageHttpClient};
use crate::{Meter, Usage, UsageProvider};

pub const MANUS_EXPERIMENTAL_ID: &str = "manus-experimental";

const MANUS_CREDITS_URL: &str = "https://api.manus.im/user.v1.UserService/GetAvailableCredits";

const CREDIT_FIELDS: &[&str] = &[
    "totalCredits",
    "freeCredits",
    "refreshCredits",
    "maxRefreshCredits",
    "proMonthlyCredits",
    "periodicCredits",
];

pub struct ExperimentalProvider {
    client: Arc<dyn UsageHttpClient>,
    secret: Option<String>,
}

impl ExperimentalProvider {
    pub fn new(client: Arc<dyn UsageHttpClient>, secret: Option<String>) -> Self {
        Self { client, secret }
    }
}

#[async_trait]
impl UsageProvider for ExperimentalProvider {
    fn id(&self) -> &'static str {
        MANUS_EXPERIMENTAL_ID
    }

    async fn fetch(&mut self) -> anyhow::Result<Usage> {
        let token = self
            .secret
            .as_deref()
            .filter(|token| !token.is_empty())
            .ok_or_else(|| anyhow!("no credential stored for {MANUS_EXPERIMENTAL_ID}"))?;

        let response = self
            .client
            .post_json(HttpPost {
                url: MANUS_CREDITS_URL.to_owned(),
                headers: vec![
                    ("Accept".to_owned(), "application/json".to_owned()),
                    ("Content-Type".to_owned(), "application/json".to_owned()),
                    ("Connect-Protocol-Version".to_owned(), "1".to_owned()),
                    ("Authorization".to_owned(), format!("Bearer {token}")),
                ],
                body: json!({}),
            })
            .await?;

        manus_usage(&require_success(response)?)
    }
}

fn count(value: &Value, field: &str) -> f64 {
    value
        .get(field)
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite() && number.is_sign_positive())
        .unwrap_or(0.0)
}

fn reset_at(value: &Value) -> Option<SystemTime> {
    value
        .get("nextRefreshTime")?
        .as_str()?
        .parse::<jiff::Timestamp>()
        .ok()
        .and_then(|stamp| SystemTime::try_from(stamp).ok())
}

/// Pure mapping from the documented response shape to `Usage`. Pure so tests
/// never touch the network or a credential.
pub fn manus_usage(body: &Value) -> anyhow::Result<Usage> {
    if !body.is_object()
        || !CREDIT_FIELDS
            .iter()
            .any(|field| body.get(*field).is_some_and(|v| !v.is_null()))
    {
        return Err(anyhow!("Manus credits response carries no credit fields"));
    }

    let max_refresh = count(body, "maxRefreshCredits");
    let refresh = count(body, "refreshCredits");
    let monthly_limit = count(body, "proMonthlyCredits");
    let monthly_remaining = count(body, "periodicCredits");

    let mut meters = Vec::new();
    if max_refresh > 0.0 {
        meters.push(Meter {
            label: "refresh".to_owned(),
            used: (max_refresh - refresh.clamp(0.0, max_refresh)).max(0.0),
            limit: max_refresh,
            reset_at: reset_at(body),
        });
    }
    if monthly_limit > 0.0 {
        meters.push(Meter {
            label: "monthly".to_owned(),
            used: (monthly_limit - monthly_remaining.clamp(0.0, monthly_limit)).max(0.0),
            limit: monthly_limit,
            reset_at: None,
        });
    }
    if meters.is_empty() {
        return Err(anyhow!("Manus credits response has no usable ceilings"));
    }

    Ok(Usage {
        plan: None,
        meters,
        spend: None,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::http::{HttpGet, HttpJsonResponse};

    struct FakeClient {
        status: u16,
        body: Value,
        seen: Mutex<Vec<HttpPost>>,
    }

    #[async_trait]
    impl UsageHttpClient for FakeClient {
        async fn get_json(&self, _request: HttpGet) -> anyhow::Result<HttpJsonResponse> {
            unreachable!("experimental provider only posts")
        }

        async fn post_json(&self, request: HttpPost) -> anyhow::Result<HttpJsonResponse> {
            self.seen.lock().expect("lock").push(request);
            Ok(HttpJsonResponse {
                status: self.status,
                body: self.body.clone(),
            })
        }
    }

    fn fixture() -> Value {
        json!({
            "totalCredits": 1180,
            "freeCredits": 1000,
            "refreshCredits": 180,
            "maxRefreshCredits": 300,
            "nextRefreshTime": "2026-05-02T16:00:00Z",
            "refreshInterval": "daily",
            "proMonthlyCredits": 900,
            "periodicCredits": 620
        })
    }

    #[test]
    fn maps_refresh_and_monthly_ceilings() {
        let usage = manus_usage(&fixture()).expect("valid credits response");
        assert_eq!(usage.meters.len(), 2);
        assert_eq!(usage.meters[0].label, "refresh");
        assert_eq!(usage.meters[0].used, 120.0);
        assert_eq!(usage.meters[0].limit, 300.0);
        assert!(usage.meters[0].reset_at.is_some());
        assert_eq!(usage.meters[1].label, "monthly");
        assert_eq!(usage.meters[1].used, 280.0);
        assert_eq!(usage.meters[1].reset_at, None);
    }

    #[test]
    fn unknown_shape_fails_closed() {
        assert!(manus_usage(&json!({ "unexpected": 1 })).is_err());
        assert!(manus_usage(&json!({ "totalCredits": 10 })).is_err());
    }

    #[test]
    fn above_ceiling_is_clamped() {
        let mut body = fixture();
        body["refreshCredits"] = json!(500.0);
        let usage = manus_usage(&body).expect("clamped");
        assert_eq!(usage.meters[0].used, 0.0);
    }

    #[test]
    fn request_uses_documented_endpoint_and_headers() {
        let client = Arc::new(FakeClient {
            status: 200,
            body: fixture(),
            seen: Mutex::new(Vec::new()),
        });
        let mut provider = ExperimentalProvider::new(client.clone(), Some("test-session".into()));
        crate::test_support::block_on(provider.fetch()).expect("fetch succeeds");

        let seen = client.seen.lock().expect("lock");
        assert_eq!(seen.len(), 1);
        assert!(seen[0]
            .url
            .contains("api.manus.im/user.v1.UserService/GetAvailableCredits"));
        assert!(seen[0]
            .headers
            .iter()
            .any(|(k, v)| k == "Connect-Protocol-Version" && v == "1"));
        assert!(seen[0]
            .headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer test-session"));
        assert_eq!(seen[0].body, json!({}));
    }

    #[test]
    fn missing_credential_fails_before_request() {
        let client = Arc::new(FakeClient {
            status: 200,
            body: fixture(),
            seen: Mutex::new(Vec::new()),
        });
        let mut provider = ExperimentalProvider::new(client.clone(), None);
        let error = crate::test_support::block_on(provider.fetch()).expect_err("no credential");
        assert!(error.to_string().contains("no credential"));
        assert!(client.seen.lock().expect("lock").is_empty());
    }
}

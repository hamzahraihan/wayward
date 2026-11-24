//! Official usage API providers, built only from endpoint and response shapes
//! observed in the reviewed reference implementation. No request is issued
//! unless the provider is explicitly enabled and a credential is stored.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::anyhow;
use async_trait::async_trait;
use serde_json::Value;

use crate::http::{require_success, HttpGet, UsageHttpClient};
use crate::{Meter, Spend, Usage, UsageProvider};

pub const CLAUDE_OFFICIAL_ID: &str = "claude-official";
pub const CODEX_OFFICIAL_ID: &str = "codex-official";
pub const OPENCODE_OFFICIAL_ID: &str = "opencode-official";

const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage?cedar_ember=1";
const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/api/codex/usage";
const OPENCODE_USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficialKind {
    Claude,
    Codex,
    OpenCode,
}

pub struct OfficialProvider {
    kind: OfficialKind,
    client: Arc<dyn UsageHttpClient>,
    secret: Option<String>,
    account: Option<String>,
}

impl OfficialProvider {
    pub fn new(
        kind: OfficialKind,
        client: Arc<dyn UsageHttpClient>,
        secret: Option<String>,
        account: Option<String>,
    ) -> Self {
        Self {
            kind,
            client,
            secret,
            account,
        }
    }

    fn id(&self) -> &'static str {
        match self.kind {
            OfficialKind::Claude => CLAUDE_OFFICIAL_ID,
            OfficialKind::Codex => CODEX_OFFICIAL_ID,
            OfficialKind::OpenCode => OPENCODE_OFFICIAL_ID,
        }
    }

    fn request(&self) -> anyhow::Result<HttpGet> {
        let token = self
            .secret
            .as_deref()
            .ok_or_else(|| anyhow!("no credential stored for {}", self.id()))?;
        if token.is_empty() {
            return Err(anyhow!("empty credential for {}", self.id()));
        }

        let (url, headers) = match self.kind {
            OfficialKind::Claude => (
                CLAUDE_USAGE_URL.to_owned(),
                vec![
                    ("authorization".to_owned(), format!("Bearer {token}")),
                    ("anthropic-beta".to_owned(), "oauth-2025-04-20".to_owned()),
                    (
                        "user-agent".to_owned(),
                        "claude-cli/2.1.280 (external, cli)".to_owned(),
                    ),
                ],
            ),
            OfficialKind::Codex => {
                let mut headers = vec![("authorization".to_owned(), format!("Bearer {token}"))];
                if let Some(account) = self.account.as_deref().filter(|s| !s.is_empty()) {
                    headers.push(("chatgpt-account-id".to_owned(), account.to_owned()));
                }
                (CODEX_USAGE_URL.to_owned(), headers)
            }
            OfficialKind::OpenCode => (
                OPENCODE_USAGE_URL.to_owned(),
                vec![("authorization".to_owned(), format!("Bearer {token}"))],
            ),
        };

        Ok(HttpGet { url, headers })
    }
}

#[async_trait]
impl UsageProvider for OfficialProvider {
    fn id(&self) -> &'static str {
        self.id()
    }

    async fn fetch(&mut self) -> anyhow::Result<Usage> {
        let response = self.client.get_json(self.request()?).await?;

        if self.kind == OfficialKind::OpenCode {
            if let Some(error) = response.body.get("error") {
                let code = error
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                if response.status == 403 && code == "EntitlementError" {
                    return Err(anyhow!("OpenCode Go subscription is not active"));
                }
            }
        }

        let body = require_success(response)?;
        match self.kind {
            OfficialKind::Claude => claude_usage(&body),
            OfficialKind::Codex => codex_usage(&body),
            OfficialKind::OpenCode => opencode_usage(&body),
        }
    }
}

fn member<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Value> {
    names
        .iter()
        .find_map(|name| value.get(*name))
        .filter(|found| !found.is_null())
}

fn number(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64).filter(|n| n.is_finite())
}

fn percent(value: Option<&Value>) -> Option<f64> {
    number(value).map(|n| n.clamp(0.0, 100.0))
}

/// Accept epoch seconds, epoch milliseconds, or an RFC 3339 string.
fn reset_at(value: Option<&Value>) -> Option<SystemTime> {
    match value? {
        Value::Number(number) => {
            let raw = number.as_f64()?;
            if raw <= 0.0 {
                return None;
            }
            // The reference treats sub-20-billion numbers as seconds.
            if raw < 20_000_000_000.0 {
                Some(SystemTime::UNIX_EPOCH + Duration::from_secs(raw as u64))
            } else {
                Some(SystemTime::UNIX_EPOCH + Duration::from_millis(raw as u64))
            }
        }
        Value::String(text) => text
            .parse::<jiff::Timestamp>()
            .ok()
            .and_then(|stamp| SystemTime::try_from(stamp).ok()),
        _ => None,
    }
}

fn money(value: Option<&Value>) -> Option<f64> {
    let object = value?;
    let minor = number(member(object, &["amount_minor", "amountMicroCents"]))?;
    let exponent = number(member(object, &["exponent"])).unwrap_or(2.0);
    if !(0.0..=9.0).contains(&exponent) {
        return None;
    }
    Some(minor / 10f64.powf(exponent))
}

fn currency(value: &Value) -> Option<String> {
    member(value, &["currency"])
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn meter(label: &str, used: f64, reset: Option<SystemTime>) -> Meter {
    Meter {
        label: label.to_owned(),
        used,
        limit: 100.0,
        reset_at: reset,
    }
}

fn claude_usage(body: &Value) -> anyhow::Result<Usage> {
    let session = member(body, &["five_hour", "fiveHour"])
        .ok_or_else(|| anyhow!("Claude response has no five_hour window"))?;
    let weekly = member(body, &["seven_day", "sevenDay"])
        .ok_or_else(|| anyhow!("Claude response has no seven_day window"))?;

    let used_percent = |window: &Value| {
        percent(member(
            window,
            &["utilization", "usedPercent", "used_percent", "percent"],
        ))
    };
    let session_used =
        used_percent(session).ok_or_else(|| anyhow!("Claude session window has no utilization"))?;
    let weekly_used =
        used_percent(weekly).ok_or_else(|| anyhow!("Claude weekly window has no utilization"))?;

    let spend = member(body, &["spend"]).and_then(|spend| {
        let used = money(member(spend, &["used"]))?;
        let limit = money(member(spend, &["limit"]))?;
        Some(Spend {
            used,
            limit,
            currency: currency(member(spend, &["used"]).unwrap_or(spend))
                .unwrap_or_else(|| "USD".to_owned()),
        })
    });

    Ok(Usage {
        plan: None,
        meters: vec![
            meter(
                "session",
                session_used,
                reset_at(member(session, &["resets_at", "resetsAt"])),
            ),
            meter(
                "weekly",
                weekly_used,
                reset_at(member(weekly, &["resets_at", "resetsAt"])),
            ),
        ],
        spend,
    })
}

fn codex_usage(body: &Value) -> anyhow::Result<Usage> {
    let plan = member(body, &["planType", "plan_type"])
        .and_then(Value::as_str)
        .map(str::to_owned);
    let rate = member(body, &["rateLimit", "rate_limit"])
        .ok_or_else(|| anyhow!("Codex response has no rateLimit"))?;

    let primary = member(rate, &["primaryWindow", "primary_window"])
        .ok_or_else(|| anyhow!("Codex response has no primary window"))?;
    let primary_used = percent(member(primary, &["usedPercent", "used_percent"]))
        .ok_or_else(|| anyhow!("Codex primary window has no usedPercent"))?;

    let mut meters = vec![meter(
        "session",
        primary_used,
        reset_at(member(primary, &["resetsAt", "resets_at"])),
    )];

    if let Some(secondary) = member(rate, &["secondaryWindow", "secondary_window"]) {
        if let Some(used) = percent(member(secondary, &["usedPercent", "used_percent"])) {
            meters.push(meter(
                "weekly",
                used,
                reset_at(member(secondary, &["resetsAt", "resets_at"])),
            ));
        }
    }

    Ok(Usage {
        plan,
        meters,
        spend: None,
    })
}

fn opencode_usage(body: &Value) -> anyhow::Result<Usage> {
    let usage =
        member(body, &["usage"]).ok_or_else(|| anyhow!("OpenCode response has no usage object"))?;

    let mut meters = Vec::new();
    for (label, names) in [
        ("rolling", &["rolling"][..]),
        ("weekly", &["weekly"][..]),
        ("monthly", &["monthly"][..]),
    ] {
        if let Some(window) = member(usage, names) {
            if let Some(used) = percent(member(window, &["percent", "usedPercent"])) {
                meters.push(meter(
                    label,
                    used,
                    reset_at(member(window, &["resetsAt", "resets_at"])),
                ));
            }
        }
    }
    if meters.is_empty() {
        return Err(anyhow!("OpenCode response has no usable windows"));
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
    use crate::http::{HttpGet, HttpJsonResponse, HttpPost};

    struct FakeClient {
        status: u16,
        body: Value,
        seen: Mutex<Vec<HttpGet>>,
    }

    impl FakeClient {
        fn new(status: u16, body: Value) -> Self {
            Self {
                status,
                body,
                seen: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl UsageHttpClient for FakeClient {
        async fn get_json(&self, request: HttpGet) -> anyhow::Result<HttpJsonResponse> {
            self.seen.lock().expect("lock").push(request);
            Ok(HttpJsonResponse {
                status: self.status,
                body: self.body.clone(),
            })
        }

        async fn post_json(&self, _request: HttpPost) -> anyhow::Result<HttpJsonResponse> {
            unreachable!("official providers only use GET")
        }
    }

    fn client(status: u16, body: Value) -> Arc<FakeClient> {
        Arc::new(FakeClient::new(status, body))
    }

    fn reset_epoch_secs() -> u64 {
        (SystemTime::now() + Duration::from_secs(600))
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("after epoch")
            .as_secs()
    }

    #[test]
    fn claude_maps_windows_and_spend() {
        let reset = reset_epoch_secs();
        let body = serde_json::json!({
            "five_hour": { "utilization": 62.0, "resets_at": reset },
            "seven_day": { "utilization": 36.0, "resets_at": reset },
            "spend": {
                "used": { "amount_minor": 1234, "exponent": 2, "currency": "USD" },
                "limit": { "amount_minor": 5000, "exponent": 2, "currency": "USD" }
            }
        });
        let usage = claude_usage(&body).expect("parse Claude response");

        assert_eq!(usage.meters.len(), 2);
        assert_eq!(usage.meters[0].label, "session");
        assert_eq!(usage.meters[0].used, 62.0);
        assert_eq!(usage.meters[0].limit, 100.0);
        assert!(usage.meters[0].reset_at.is_some());
        assert_eq!(usage.meters[1].label, "weekly");
        let spend = usage.spend.expect("spend mapped");
        assert_eq!(spend.used, 12.34);
        assert_eq!(spend.limit, 50.0);
        assert_eq!(spend.currency, "USD");
    }

    #[test]
    fn claude_missing_window_is_error() {
        let body = serde_json::json!({ "five_hour": { "utilization": 10.0 } });
        assert!(claude_usage(&body).is_err());
    }

    #[test]
    fn codex_maps_primary_and_secondary_windows() {
        let body = serde_json::json!({
            "planType": "pro",
            "rateLimit": {
                "primaryWindow": { "usedPercent": 88.0, "resetsAt": 1_800_000_000_000u64 },
                "secondaryWindow": { "usedPercent": 36.0, "resetsAt": 1_900_000_000_000u64 }
            }
        });
        let usage = codex_usage(&body).expect("parse Codex response");

        assert_eq!(usage.plan.as_deref(), Some("pro"));
        assert_eq!(usage.meters.len(), 2);
        assert_eq!(usage.meters[0].used, 88.0);
        assert!(usage.meters[0].reset_at.is_some());
        assert_eq!(usage.meters[1].label, "weekly");
    }

    #[test]
    fn opencode_maps_windows_and_iso_reset() {
        let body = serde_json::json!({
            "usage": {
                "rolling": { "percent": 42.0, "status": "healthy", "resetsAt": "2026-11-25T00:00:00Z" },
                "weekly": { "percent": 11.0, "status": "healthy", "resetsAt": "2026-12-01T00:00:00Z" }
            }
        });
        let usage = opencode_usage(&body).expect("parse OpenCode response");

        assert_eq!(usage.meters.len(), 2);
        assert_eq!(usage.meters[0].label, "rolling");
        assert_eq!(usage.meters[0].used, 42.0);
        assert!(usage.meters[0].reset_at.is_some());
    }

    #[test]
    fn opencode_empty_windows_is_error() {
        let body = serde_json::json!({ "usage": {} });
        assert!(opencode_usage(&body).is_err());
    }

    #[test]
    fn missing_credential_fails_before_request() {
        let fake = client(200, serde_json::json!({}));
        let mut provider = OfficialProvider::new(OfficialKind::Claude, fake.clone(), None, None);
        let error = crate::test_support::block_on(provider.fetch()).expect_err("no credential");
        assert!(error.to_string().contains("no credential"));
        assert!(fake.seen.lock().expect("lock").is_empty());
    }

    #[test]
    fn claude_request_uses_documented_endpoint_and_headers() {
        let body = serde_json::json!({
            "five_hour": { "utilization": 10.0 },
            "seven_day": { "utilization": 20.0 }
        });
        let fake = client(200, body);
        let mut provider = OfficialProvider::new(
            OfficialKind::Claude,
            fake.clone(),
            Some("test-token".to_owned()),
            None,
        );
        crate::test_support::block_on(provider.fetch()).expect("fetch succeeds");

        let seen = fake.seen.lock().expect("lock");
        assert_eq!(seen.len(), 1);
        assert!(seen[0].url.contains("api.anthropic.com/api/oauth/usage"));
        assert!(seen[0]
            .headers
            .iter()
            .any(|(k, v)| k == "authorization" && v == "Bearer test-token"));
        assert!(seen[0].headers.iter().any(|(k, _)| k == "anthropic-beta"));
    }

    #[test]
    fn opencode_entitlement_error_is_distinguishable() {
        let fake = client(
            403,
            serde_json::json!({ "error": { "type": "EntitlementError" } }),
        );
        let mut provider = OfficialProvider::new(
            OfficialKind::OpenCode,
            fake,
            Some("test-token".to_owned()),
            None,
        );
        let error = crate::test_support::block_on(provider.fetch()).expect_err("403 must fail");
        assert!(error.to_string().contains("subscription"));
    }

    #[test]
    fn codex_request_includes_account_id_when_configured() {
        let fake = client(
            200,
            serde_json::json!({
                "rateLimit": { "primaryWindow": { "usedPercent": 5.0 } }
            }),
        );
        let provider = OfficialProvider::new(
            OfficialKind::Codex,
            fake,
            Some("test-token".to_owned()),
            Some("acct-1".to_owned()),
        );
        let request = provider.request().expect("request built");
        assert!(request.url.contains("chatgpt.com/backend-api"));
        assert!(request
            .headers
            .iter()
            .any(|(k, v)| k == "chatgpt-account-id" && v == "acct-1"));
    }
}

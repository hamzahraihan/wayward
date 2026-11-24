use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use iced::futures::channel::mpsc;
use iced::futures::sink::SinkExt;
use iced::futures::{FutureExt, StreamExt};
use iced::{stream, Subscription};
use tokio::time::Instant as TokioInstant;
use wayward_ai::level::aggregate_level;
use wayward_ai::registry::{build_providers, ProviderSpec, SecretLookup};
use wayward_ai::Usage;
use wayward_core::config::{AiConfig, AiMode, AiProviderConfig};

/// Credential lookup backed by the Windows Credential Manager. Kept here at
/// the app boundary; `wayward-ai` only sees the `SecretLookup` trait.
struct KeyringSecrets;

impl SecretLookup for KeyringSecrets {
    fn get(&self, id: &str) -> Option<String> {
        match wayward_platform::load_credential(id) {
            Ok(secret) => secret,
            Err(error) => {
                tracing::warn!(provider = id, "credential lookup failed: {error}");
                None
            }
        }
    }
}

use crate::view::Message;

/// Maximum delay multiplier applied on repeated provider failures. A provider
/// failing 4 or more consecutive times is polled at most once every 16 of its
/// normal intervals.
pub const MAX_BACKOFF_MULTIPLIER: u32 = 16;

const MIN_INTERVAL: Duration = Duration::from_secs(1);
const IDLE_POLL: Duration = Duration::from_secs(3_600);

/// Delay before the next poll after `failures` consecutive failures.
///
/// The multiplier doubles with each consecutive failure (1x, 2x, 4x, ...) and
/// is capped at `cap_multiplier` to bound how stale a failing provider can get.
/// Arithmetic is checked so overflow can never panic the subscription task.
pub fn backoff_delay(interval: Duration, failures: u32, cap_multiplier: u32) -> Duration {
    let cap = cap_multiplier.max(1);
    let multiplier = match 2u32.checked_pow(failures) {
        Some(value) => value.min(cap),
        None => cap,
    };
    interval.checked_mul(multiplier).unwrap_or(Duration::MAX)
}

/// Hashable projection of one configured provider. `limit` is stored as bits
/// so the key stays `Eq + Hash` without requiring `f64` to implement them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AiProviderKey {
    pub id: String,
    pub enabled: bool,
    pub interval_ms: u64,
    pub root: Option<PathBuf>,
    pub account: Option<String>,
    pub limit_bits: u64,
    pub reset_after_secs: u64,
    pub reset_anchor_secs: Option<u64>,
}

/// Stable identity for the AI poll subscription. Changing any field recreates
/// the stream. `AiMode` does not implement `Hash`, so that field is hashed via
/// a local discriminant in the manual `Hash` impl below.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiStreamKey {
    pub mode: AiMode,
    pub enabled: bool,
    pub interval_ms: u64,
    pub experimental: bool,
    pub warn_below_bits: u32,
    pub urgent_below_bits: u32,
    pub providers: Vec<AiProviderKey>,
}

impl Hash for AiStreamKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.mode {
            AiMode::Mock => 0u8.hash(state),
            AiMode::Real => 1u8.hash(state),
            AiMode::Unsupported => 2u8.hash(state),
        }
        self.enabled.hash(state);
        self.interval_ms.hash(state);
        self.experimental.hash(state);
        self.warn_below_bits.hash(state);
        self.urgent_below_bits.hash(state);
        self.providers.hash(state);
    }
}

struct ProviderState {
    spec: ProviderSpec,
    next_due: TokioInstant,
    failures: u32,
}

fn poll_result_message(
    state: &mut ProviderState,
    now: TokioInstant,
    result: anyhow::Result<Usage>,
    warn_below: f32,
    urgent_below: f32,
) -> Message {
    let provider_id = state.spec.id.clone();
    match result {
        Ok(usage) => {
            state.failures = 0;
            state.next_due = now + state.spec.interval;
            let level = aggregate_level(&usage, warn_below, urgent_below);
            Message::AiFetched {
                provider_id,
                usage,
                level,
                fetched_at: Instant::now(),
            }
        }
        Err(error) => {
            state.failures = state.failures.saturating_add(1);
            let delay = backoff_delay(state.spec.interval, state.failures, MAX_BACKOFF_MULTIPLIER);
            state.next_due = now + delay;
            Message::AiFetchFailed {
                provider_id,
                error: error.to_string(),
            }
        }
    }
}

/// Poll every configured provider on its own interval and publish snapshots.
pub fn subscription(key: AiStreamKey) -> Subscription<Message> {
    Subscription::run_with(key, build_stream)
}

fn config_from_key(key: &AiStreamKey) -> AiConfig {
    AiConfig {
        enabled: key.enabled,
        mode: key.mode,
        interval_ms: key.interval_ms,
        warn_below: f32::from_bits(key.warn_below_bits),
        urgent_below: f32::from_bits(key.urgent_below_bits),
        experimental: key.experimental,
        providers: key
            .providers
            .iter()
            .map(|provider| AiProviderConfig {
                id: provider.id.clone(),
                enabled: provider.enabled,
                interval_ms: provider.interval_ms,
                root: provider.root.clone(),
                account: provider.account.clone(),
                limit: f64::from_bits(provider.limit_bits),
                reset_after_secs: provider.reset_after_secs,
                reset_anchor_secs: provider.reset_anchor_secs,
            })
            .collect(),
    }
}

fn build_stream(key: &AiStreamKey) -> impl iced::futures::Stream<Item = Message> {
    let warn_below_bits = key.warn_below_bits;
    let urgent_below_bits = key.urgent_below_bits;
    let config = config_from_key(key);
    let home = dirs::home_dir().unwrap_or_default();
    let mut states: Vec<ProviderState> = build_providers(&config, &home, &KeyringSecrets)
        .into_iter()
        .map(|mut spec| {
            spec.interval = spec.interval.max(MIN_INTERVAL);
            ProviderState {
                spec,
                next_due: TokioInstant::now(),
                failures: 0,
            }
        })
        .collect();

    stream::channel(8, move |mut output: mpsc::Sender<Message>| async move {
        let (refresh_tx, mut refresh_rx) = mpsc::unbounded::<()>();

        if output.send(Message::AiReady { refresh_tx }).await.is_err() {
            return;
        }

        loop {
            let earliest = states
                .iter()
                .map(|state| state.next_due)
                .min()
                .unwrap_or_else(|| TokioInstant::now() + IDLE_POLL);

            iced::futures::select! {
                _ = tokio::time::sleep_until(earliest).fuse() => {}
                refresh = refresh_rx.next().fuse() => {
                    if refresh.is_none() {
                        return;
                    }
                    let now = TokioInstant::now();
                    for state in &mut states {
                        state.next_due = now;
                    }
                }
            }

            let now = TokioInstant::now();
            for state in &mut states {
                if state.next_due > now {
                    continue;
                }

                let now = TokioInstant::now();
                let result = state.spec.provider.fetch().await;
                let message = poll_result_message(
                    state,
                    now,
                    result,
                    f32::from_bits(warn_below_bits),
                    f32::from_bits(urgent_below_bits),
                );
                if output.send(message).await.is_err() {
                    return;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interval() -> Duration {
        Duration::from_millis(500)
    }

    #[test]
    fn backoff_delay_zero_failures_is_one_interval() {
        assert_eq!(
            backoff_delay(interval(), 0, MAX_BACKOFF_MULTIPLIER),
            interval()
        );
    }

    #[test]
    fn backoff_delay_one_failure_is_two_intervals() {
        assert_eq!(
            backoff_delay(interval(), 1, MAX_BACKOFF_MULTIPLIER),
            Duration::from_millis(1_000)
        );
    }

    #[test]
    fn backoff_delay_two_failures_is_four_intervals() {
        assert_eq!(
            backoff_delay(interval(), 2, MAX_BACKOFF_MULTIPLIER),
            Duration::from_millis(2_000)
        );
    }

    #[test]
    fn backoff_delay_caps_at_sixteen_intervals() {
        assert_eq!(
            backoff_delay(interval(), 20, MAX_BACKOFF_MULTIPLIER),
            Duration::from_millis(8_000)
        );
    }

    #[test]
    fn backoff_delay_does_not_overflow_at_max_failures() {
        assert_eq!(
            backoff_delay(interval(), u32::MAX, MAX_BACKOFF_MULTIPLIER),
            Duration::from_millis(8_000)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn failing_provider_backoff_gaps_grow_and_cap() {
        let interval_ms = 60_000u64;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock is after epoch")
            .as_nanos();
        let key = AiStreamKey {
            mode: AiMode::Real,
            enabled: true,
            interval_ms,
            experimental: false,
            warn_below_bits: 0.2f32.to_bits(),
            urgent_below_bits: 0.0f32.to_bits(),
            providers: vec![AiProviderKey {
                id: "claude-code".to_owned(),
                enabled: true,
                interval_ms,
                root: Some(std::env::temp_dir().join(format!(
                    "wayward-backoff-missing-{}-{nanos}",
                    std::process::id()
                ))),
                account: None,
                limit_bits: 1_000_000.0f64.to_bits(),
                reset_after_secs: 3_600,
                reset_anchor_secs: None,
            }],
        };

        let mut stream = Box::pin(build_stream(&key));
        let refresh_tx = match stream.next().await {
            Some(Message::AiReady { refresh_tx }) => refresh_tx,
            other => panic!("expected AiReady, got {other:?}"),
        };

        let mut stamps = Vec::new();
        for _ in 0..5 {
            match stream.next().await {
                Some(Message::AiFetchFailed { .. }) => {}
                other => panic!("expected AiFetchFailed, got {other:?}"),
            }
            stamps.push(TokioInstant::now());
        }

        let gaps: Vec<_> = stamps.windows(2).map(|pair| pair[1] - pair[0]).collect();
        for (gap, multiplier) in gaps.iter().zip([2u64, 4, 8, 16, 16]) {
            assert_eq!(*gap, Duration::from_secs(interval_ms / 1_000 * multiplier));
        }
        drop(refresh_tx);
    }

    #[test]
    fn success_resets_the_backoff_gap() {
        let interval = Duration::from_secs(60);
        let config = AiConfig {
            mode: AiMode::Mock,
            interval_ms: 60_000,
            ..AiConfig::default()
        };
        let spec = build_providers(&config, std::path::Path::new("home"), &KeyringSecrets)
            .into_iter()
            .next()
            .expect("mock provider");
        let mut state = ProviderState {
            spec,
            next_due: TokioInstant::now(),
            failures: 0,
        };

        let first = TokioInstant::now();
        let message = poll_result_message(&mut state, first, Err(anyhow::anyhow!("x")), 0.2, 0.0);
        assert!(matches!(message, Message::AiFetchFailed { .. }));
        assert_eq!(state.next_due - first, interval * 2);

        let second = state.next_due;
        poll_result_message(&mut state, second, Err(anyhow::anyhow!("x")), 0.2, 0.0);
        assert_eq!(state.next_due - second, interval * 4);

        let third = state.next_due;
        let usage = Usage {
            plan: None,
            meters: Vec::new(),
            spend: None,
        };
        let message = poll_result_message(&mut state, third, Ok(usage), 0.2, 0.0);
        assert!(matches!(message, Message::AiFetched { .. }));
        assert_eq!(state.failures, 0);
        assert_eq!(state.next_due - third, interval);
    }
}

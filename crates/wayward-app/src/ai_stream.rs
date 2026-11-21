use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use iced::futures::channel::mpsc;
use iced::futures::sink::SinkExt;
use iced::futures::{FutureExt, StreamExt};
use iced::{stream, Subscription};
use wayward_ai::level::{aggregate_level, URGENT_BELOW, WARN_BELOW};
use wayward_ai::registry::{build_providers, ProviderSpec};
use wayward_core::config::{AiConfig, AiMode, AiProviderConfig};

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
    pub limit_bits: u64,
    pub reset_after_secs: u64,
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
        self.providers.hash(state);
    }
}

struct ProviderState {
    spec: ProviderSpec,
    next_due: Instant,
    failures: u32,
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
        warn_below: WARN_BELOW,
        urgent_below: URGENT_BELOW,
        experimental: key.experimental,
        providers: key
            .providers
            .iter()
            .map(|provider| AiProviderConfig {
                id: provider.id.clone(),
                enabled: provider.enabled,
                interval_ms: provider.interval_ms,
                root: provider.root.clone(),
                limit: f64::from_bits(provider.limit_bits),
                reset_after_secs: provider.reset_after_secs,
            })
            .collect(),
    }
}

fn build_stream(key: &AiStreamKey) -> impl iced::futures::Stream<Item = Message> {
    let config = config_from_key(key);
    let home = dirs::home_dir().unwrap_or_default();
    let mut states: Vec<ProviderState> = build_providers(&config, &home)
        .into_iter()
        .map(|mut spec| {
            spec.interval = spec.interval.max(MIN_INTERVAL);
            ProviderState {
                spec,
                next_due: Instant::now(),
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
                .unwrap_or_else(|| Instant::now() + IDLE_POLL);

            // `tokio::select!` needs tokio's `macros` feature, which this
            // workspace does not enable. The futures re-export from iced gives
            // the same wake-up semantics with the futures already in scope.
            iced::futures::select! {
                _ = tokio::time::sleep_until(tokio::time::Instant::from_std(earliest)).fuse() => {}
                refresh = refresh_rx.next().fuse() => {
                    if refresh.is_none() {
                        return;
                    }
                    let now = Instant::now();
                    for state in &mut states {
                        state.next_due = now;
                    }
                }
            }

            let now = Instant::now();
            for state in &mut states {
                if state.next_due > now {
                    continue;
                }

                let provider_id = state.spec.id.clone();
                match state.spec.provider.fetch().await {
                    Ok(usage) => {
                        state.failures = 0;
                        state.next_due = Instant::now() + state.spec.interval;
                        let level = aggregate_level(&usage, WARN_BELOW, URGENT_BELOW);
                        if output
                            .send(Message::AiFetched {
                                provider_id,
                                usage,
                                level,
                                fetched_at: Instant::now(),
                            })
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(error) => {
                        state.failures = state.failures.saturating_add(1);
                        let delay = backoff_delay(
                            state.spec.interval,
                            state.failures,
                            MAX_BACKOFF_MULTIPLIER,
                        );
                        state.next_due = Instant::now() + delay;
                        if output
                            .send(Message::AiFetchFailed {
                                provider_id,
                                error: error.to_string(),
                            })
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
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
}

use std::time::{Duration, Instant};

use iced::futures::channel::mpsc;
use iced::futures::sink::SinkExt;
use iced::futures::{FutureExt, StreamExt};
use iced::{stream, Subscription};
use wayward_ai::level::{aggregate_level, URGENT_BELOW, WARN_BELOW};
use wayward_ai::mock::MockProvider;
use wayward_ai::{Usage, UsageProvider};
use wayward_core::Level;

use crate::view::Message;

/// Stable identity for the AI poll subscription. Changing the interval
/// recreates the stream; everything else about it is fixed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AiStreamKey {
    pub interval_ms: u64,
}

/// Poll the AI provider on a fixed interval and publish usage snapshots.
pub fn subscription(key: AiStreamKey) -> Subscription<Message> {
    Subscription::run_with(key, build_stream)
}

fn build_stream(key: &AiStreamKey) -> impl iced::futures::Stream<Item = Message> {
    let interval_ms = key.interval_ms;
    stream::channel(8, move |mut output: mpsc::Sender<Message>| async move {
        let (refresh_tx, mut refresh_rx) = mpsc::unbounded::<()>();

        if output.send(Message::AiReady { refresh_tx }).await.is_err() {
            return;
        }

        let mut provider = MockProvider::new();

        let mut tick = tokio::time::interval(Duration::from_millis(interval_ms.max(30_000)));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // Number of upcoming wakes to skip after a failure, capped at 64.
        let mut backoff: u32 = 0;

        loop {
            // `tokio::select!` requires tokio's `macros` feature, which this
            // workspace does not enable. The futures re-export from iced gives
            // the same wake-up semantics with the futures already in scope.
            iced::futures::select! {
                _ = tick.tick().fuse() => {}
                _ = refresh_rx.next().fuse() => {}
            }

            if backoff > 0 {
                backoff -= 1;
                continue;
            }

            match provider.fetch().await {
                Ok(usage) => {
                    backoff = 0;
                    let level = aggregate_level(&usage, WARN_BELOW, URGENT_BELOW);
                    if output
                        .send(Message::AiFetched {
                            usage,
                            level,
                            fetched_at: Instant::now(),
                            error: None,
                        })
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                Err(error) => {
                    backoff = backoff.saturating_mul(2).saturating_add(1).min(64);
                    let usage = Usage {
                        plan: None,
                        meters: Vec::new(),
                        spend: None,
                    };
                    if output
                        .send(Message::AiFetched {
                            usage,
                            level: Level::Normal,
                            fetched_at: Instant::now(),
                            error: Some(error.to_string()),
                        })
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }
        }
    })
}

//! Subscription bridge for the Windows foreground hook.
//!
//! The hook runs on its own thread (see `wayward-platform/foreground.rs`) and
//! sends `ForegroundEvent`s over a std channel. This module drains that channel
//! as an iced stream, so the active-window cell updates immediately on every
//! foreground change without polling.

use std::hash::{Hash, Hasher};

use iced::futures::channel::mpsc;
use iced::futures::SinkExt;
use iced::{stream, Subscription};

use crate::view::Message;

/// Stable identity for the foreground subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForegroundKey;

impl Hash for ForegroundKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u8(0);
    }
}

pub fn subscription(key: ForegroundKey) -> Subscription<Message> {
    Subscription::run_with(key, build_stream)
}

fn build_stream(_key: &ForegroundKey) -> impl iced::futures::Stream<Item = Message> {
    stream::channel(8, move |mut output: mpsc::Sender<Message>| async move {
        let install_result = wayward_platform::foreground::install();
        let (_event_rx, _guard) = match install_result {
            Ok((event_rx, guard)) => (event_rx, guard),
            Err(error) => {
                tracing::warn!("foreground hook unavailable: {error}");
                let _ = output
                    .send(Message::PlatformFailed(format!(
                        "foreground hook unavailable: {error}"
                    )))
                    .await;
                std::future::pending::<()>().await;
                return;
            }
        };

        while let Ok(event) = _event_rx.recv() {
            if output
                .send(Message::ForegroundChanged(event.title))
                .await
                .is_err()
            {
                break;
            }
        }
    })
}

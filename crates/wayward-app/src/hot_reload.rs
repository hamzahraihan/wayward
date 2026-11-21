use std::path::{Path, PathBuf};
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use iced::futures::channel::mpsc;
use iced::futures::future::pending;
use iced::futures::SinkExt;
use iced::futures::StreamExt;
use iced::Subscription;
use notify::{RecursiveMode, Watcher};

const DEBOUNCE: Duration = Duration::from_millis(200);
const CHANNEL_SIZE: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WatchPaths {
    pub config_dir: PathBuf,
    pub themes_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReloadEvent {
    ConfigChanged,
    ThemeChanged,
    WatchFailed(String),
}

/// Classify a notify event by path alone. Windows emits coarse event kinds, so the
/// kind is deliberately ignored: only the affected path decides what reloaded.
pub fn classify_event(
    event: &notify::Event,
    config_path: &Path,
    themes_dir: &Path,
) -> Option<ReloadEvent> {
    let mut config = false;
    let mut theme = false;

    for path in &event.paths {
        if path == config_path {
            config = true;
        } else if path.starts_with(themes_dir) && path.extension().is_some_and(|ext| ext == "toml")
        {
            theme = true;
        }
    }

    if config {
        Some(ReloadEvent::ConfigChanged)
    } else if theme {
        Some(ReloadEvent::ThemeChanged)
    } else {
        None
    }
}

/// Collapse a burst into one event. Priority: config > watch failure > theme.
pub fn coalesce(events: impl IntoIterator<Item = ReloadEvent>) -> Option<ReloadEvent> {
    events.into_iter().max_by_key(|event| match event {
        ReloadEvent::ConfigChanged => 3,
        ReloadEvent::WatchFailed(_) => 2,
        ReloadEvent::ThemeChanged => 1,
    })
}

/// Stable identity comes from `WatchPaths: Hash` plus the `build_stream` function
/// pointer, so changing the watched paths restarts the stream and nothing else does.
pub fn subscription(paths: WatchPaths) -> Subscription<ReloadEvent> {
    Subscription::run_with(paths, build_stream)
}

fn build_stream(paths: &WatchPaths) -> impl iced::futures::Stream<Item = ReloadEvent> {
    let config_dir = paths.config_dir.clone();
    let themes_dir = paths.themes_dir.clone();

    iced::stream::channel(
        CHANNEL_SIZE,
        move |mut output: mpsc::Sender<ReloadEvent>| async move {
            let (mut futures_tx, mut futures_rx) = mpsc::channel::<ReloadEvent>(CHANNEL_SIZE);
            let (std_tx, std_rx) = std::sync::mpsc::channel::<ReloadEvent>();
            let config_path = wayward_core::config_file(&config_dir);

            let watch_tx = std_tx.clone();
            let watch_config_path = config_path.clone();
            let watch_themes_dir = themes_dir.clone();
            let mut watcher = match notify::recommended_watcher(
                move |result: Result<notify::Event, notify::Error>| {
                    let classified = match result {
                        Ok(event) => classify_event(&event, &watch_config_path, &watch_themes_dir),
                        Err(error) => Some(ReloadEvent::WatchFailed(error.to_string())),
                    };
                    if let Some(event) = classified {
                        let _ = watch_tx.send(event);
                    }
                },
            ) {
                Ok(watcher) => watcher,
                Err(error) => {
                    let _ = output
                        .send(ReloadEvent::WatchFailed(error.to_string()))
                        .await;
                    pending::<()>().await;
                    return;
                }
            };

            if let Err(error) = watcher.watch(&config_dir, RecursiveMode::NonRecursive) {
                let _ = std_tx.send(ReloadEvent::WatchFailed(format!(
                    "config dir {}: {error}",
                    config_dir.display()
                )));
            }
            if let Err(error) = watcher.watch(&themes_dir, RecursiveMode::NonRecursive) {
                let _ = std_tx.send(ReloadEvent::WatchFailed(format!(
                    "themes dir {}: {error}",
                    themes_dir.display()
                )));
            }

            std::thread::spawn(move || loop {
                match std_rx.recv_timeout(DEBOUNCE) {
                    Ok(first) => {
                        std::thread::sleep(DEBOUNCE);
                        let mut batch = vec![first];
                        while let Ok(next) = std_rx.try_recv() {
                            batch.push(next);
                        }
                        if let Some(event) = coalesce(batch) {
                            if futures_tx.try_send(event).is_err() {
                                break;
                            }
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            });

            while let Some(event) = futures_rx.next().await {
                if output.send(event).await.is_err() {
                    break;
                }
            }

            // Hold the watcher and the futures sender until the subscription is dropped.
            pending::<()>().await;
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};
    use notify::EventKind;

    fn event(kind: EventKind, paths: &[&str]) -> notify::Event {
        notify::Event {
            kind,
            paths: paths.iter().map(PathBuf::from).collect(),
            ..notify::Event::default()
        }
    }

    #[test]
    fn classify_config_path_returns_config_changed() {
        let config = Path::new("C:/cfg/wayward.toml");
        let themes = Path::new("C:/cfg/themes");
        let ev = event(EventKind::Modify(ModifyKind::Any), &["C:/cfg/wayward.toml"]);

        assert_eq!(
            classify_event(&ev, config, themes),
            Some(ReloadEvent::ConfigChanged)
        );
    }

    #[test]
    fn classify_theme_toml_under_themes_dir_returns_theme_changed() {
        let config = Path::new("C:/cfg/wayward.toml");
        let themes = Path::new("C:/cfg/themes");
        let ev = event(
            EventKind::Create(CreateKind::Any),
            &["C:/cfg/themes/tokyo-night.toml"],
        );

        assert_eq!(
            classify_event(&ev, config, themes),
            Some(ReloadEvent::ThemeChanged)
        );
    }

    #[test]
    fn classify_unrelated_path_is_ignored() {
        let config = Path::new("C:/cfg/wayward.toml");
        let themes = Path::new("C:/cfg/themes");
        let ev = event(EventKind::Modify(ModifyKind::Any), &["C:/cfg/other.txt"]);

        assert_eq!(classify_event(&ev, config, themes), None);
    }

    #[test]
    fn classify_accepts_any_event_kind() {
        let config = Path::new("C:/cfg/wayward.toml");
        let themes = Path::new("C:/cfg/themes");
        let kinds = [
            EventKind::Modify(ModifyKind::Any),
            EventKind::Create(CreateKind::Any),
            EventKind::Remove(RemoveKind::Any),
            EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
            EventKind::Any,
            EventKind::Other,
        ];

        for kind in kinds {
            let ev = event(kind, &["C:/cfg/wayward.toml"]);
            assert_eq!(
                classify_event(&ev, config, themes),
                Some(ReloadEvent::ConfigChanged),
                "kind {kind:?} must be classified by path alone"
            );
        }
    }

    #[test]
    fn coalesce_config_wins_over_theme() {
        let result = coalesce([ReloadEvent::ThemeChanged, ReloadEvent::ConfigChanged]);
        assert_eq!(result, Some(ReloadEvent::ConfigChanged));
    }

    #[test]
    fn coalesce_empty_is_none() {
        assert_eq!(coalesce([]), None);
    }

    #[test]
    fn coalesce_single_theme_is_theme() {
        assert_eq!(
            coalesce([ReloadEvent::ThemeChanged]),
            Some(ReloadEvent::ThemeChanged)
        );
    }
}

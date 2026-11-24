mod ai_stream;
mod cli;
mod config_io;
mod config_template;
mod foreground_stream;
mod hot_reload;
mod input;
mod popup_view;
mod style;
mod view;

use iced::window::{self, Id as WindowId, Level, Position, Settings};
use iced::{daemon, time, Point, Size, Subscription, Task};
use std::path::PathBuf;
use view::Message;
use wayward_core::{themes_dir, AiMode, BarPosition, Config, Module, ModuleState};
use wayward_platform::{PlatformOptions, WindowHandle};

use crate::ai_stream::{AiProviderKey, AiStreamKey};
use crate::config_io::{ReloadContext, ReloadOutcome};
use crate::hot_reload::{ReloadEvent, WatchPaths};
use crate::popup_view::{apply_failed, apply_fetched, retain_provider_snapshots, AiSnapshot};

struct ModuleCell {
    id: &'static str,
    module: Box<dyn Module>,
    state: ModuleState,
}

/// AI controller state — only present when the AI module is enabled.
struct AiController {
    snapshots: Vec<AiSnapshot>,
    refresh_tx: Option<iced::futures::channel::mpsc::UnboundedSender<()>>,
    show_left: bool,
}

pub struct AppState {
    config: Config,
    modules: Vec<ModuleCell>,
    colors: style::Colors,
    platform_error: Option<String>,
    config_error: Option<String>,
    config_dir: PathBuf,
    reload: ReloadContext,
    pub(crate) bar_id: Option<WindowId>,
    pub(crate) popup_id: Option<WindowId>,
    ai: Option<AiController>,
}

const POPUP_WIDTH: f32 = 420.0;
const POPUP_HEIGHT: f32 = 260.0;
const POPUP_GAP: f32 = 4.0;

/// Pure geometry helper for live config reloads.
fn geometry_for(config: &Config, width: f32, height: f32) -> (Size, Point) {
    let y = match config.bar.position {
        BarPosition::Top => 0.0,
        BarPosition::Bottom => height - config.bar.height as f32,
    };
    (
        Size::new(width, config.bar.height as f32),
        Point::new(0.0, y),
    )
}

fn ai_enabled(config: &Config) -> bool {
    config.ai.enabled && config.ai.mode != AiMode::Unsupported
}

fn build_modules(config: &Config) -> Vec<ModuleCell> {
    let mut modules = Vec::new();
    let ids = config
        .layout
        .left
        .iter()
        .chain(config.layout.center.iter())
        .chain(config.layout.right.iter());

    for id in ids {
        match wayward_modules::create(id, config) {
            Some(mut module) => {
                let cell = ModuleCell {
                    id: module.id(),
                    state: module.update(),
                    module,
                };
                modules.push(cell);
            }
            None => tracing::warn!(module = %id, "unknown module id in layout; skipping"),
        }
    }
    modules
}

fn boot() -> (AppState, Task<Message>) {
    let loaded = match config_io::load_config() {
        Ok(loaded) => loaded,
        Err(error) => {
            // Keep the bar alive with defaults if the config directory is
            // unresolvable or unreadable; log the failure instead of panicking.
            eprintln!("wayward: cannot load config: {error}; using defaults");
            let dir = std::env::temp_dir().join(".wayward-fallback");
            let path = dir.join("wayward.toml");
            config_io::LoadedConfig {
                dir,
                path,
                config: Config::default(),
                created: false,
                error: Some(error.to_string()),
            }
        }
    };
    config_io::validate_or_warn(&loaded.config);

    if loaded.created {
        tracing::info!("created default config at {}", loaded.path.display());
    }
    if let Some(error) = loaded.error.as_deref() {
        eprintln!("wayward: using default config: {error}");
    }

    let mut reload = ReloadContext::new(loaded.path.clone());
    reload.seed_from_disk();

    let ai = if ai_enabled(&loaded.config) {
        tracing::info!(mode = ?loaded.config.ai.mode, "AI module enabled");
        Some(AiController {
            snapshots: Vec::new(),
            refresh_tx: None,
            show_left: true,
        })
    } else {
        None
    };

    let (bar_id, open_task) = window::open(window_settings(&loaded.config));
    let acquire = open_task
        .then(|id| {
            iced::window::run(id, |window| {
                use iced::window::raw_window_handle::RawWindowHandle;
                match window.window_handle().map(|h| h.as_raw()) {
                    Ok(RawWindowHandle::Win32(handle)) => Ok(WindowHandle(handle.hwnd.get())),
                    Ok(other) => Err(format!("unsupported window handle: {other:?}")),
                    Err(error) => Err(format!("window handle unavailable: {error}")),
                }
            })
            .map(move |result| (id, result))
        })
        .map(|(id, result)| match result {
            Ok(handle) => {
                tracing::info!("HWND acquired for bar window");
                Message::Ready { id, handle }
            }
            Err(error) => {
                tracing::warn!("HWND acquire failed: {error}");
                Message::PlatformFailed(error)
            }
        });

    let state = AppState {
        modules: build_modules(&loaded.config),
        colors: style::theme_colors(&loaded.config),
        config: loaded.config,
        platform_error: None,
        config_error: None,
        config_dir: loaded.dir,
        reload,
        bar_id: Some(bar_id),
        popup_id: None,
        ai,
    };

    (state, acquire)
}

fn popup_settings(x: f32, y: f32) -> Settings {
    Settings {
        size: Size::new(POPUP_WIDTH, POPUP_HEIGHT),
        position: Position::Specific(Point::new(x, y)),
        resizable: false,
        decorations: false,
        level: Level::AlwaysOnTop,
        platform_specific: iced::window::settings::PlatformSpecific {
            skip_taskbar: true,
            undecorated_shadow: false,
            corner_preference: iced::window::settings::platform::CornerPreference::DoNotRound,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn popup_position(config: &Config) -> (f32, f32) {
    let area = wayward_platform::primary_work_area_logical();
    let x = area.right - POPUP_WIDTH - POPUP_GAP;
    let y = match config.bar.position {
        BarPosition::Top => area.top + config.bar.height as f32 + POPUP_GAP,
        BarPosition::Bottom => area.bottom - POPUP_HEIGHT - POPUP_GAP,
    };
    wayward_platform::clamp_work_area(area, POPUP_WIDTH, POPUP_HEIGHT, (x, y))
}

fn close_popup(state: &mut AppState) -> Task<Message> {
    match state.popup_id.take() {
        Some(id) => iced::window::close(id),
        None => Task::none(),
    }
}

fn refresh_ai(state: &AppState) -> Task<Message> {
    if let Some(sender) = state
        .ai
        .as_ref()
        .and_then(|controller| controller.refresh_tx.as_ref())
    {
        let sender = sender.clone();
        return Task::perform(async move { sender.unbounded_send(()).ok() }, |_| {
            Message::UiTick
        });
    }
    Task::none()
}

fn apply_reload(state: &mut AppState, event: ReloadEvent) -> Task<Message> {
    match event {
        ReloadEvent::WatchFailed(error) => {
            state.config_error = Some(error.clone());
            tracing::warn!("config watcher error: {error}");
            return Task::none();
        }
        ReloadEvent::ConfigChanged | ReloadEvent::ThemeChanged => {}
    }
    match state.reload.reload() {
        ReloadOutcome::Unchanged => Task::none(),
        ReloadOutcome::Invalid { error } => {
            state.config_error = Some(error.clone());
            tracing::warn!("keeping last-good config: {error}");
            Task::none()
        }
        ReloadOutcome::Loaded { config, warnings } => {
            for warning in &warnings {
                tracing::warn!("{warning}");
            }
            let old_height = state.config.bar.height;
            let old_position = state.config.bar.position;
            let old_ai = ai_enabled(&state.config);
            state.config = *config;
            state.modules = build_modules(&state.config);
            state.colors = style::theme_colors(&state.config);
            state.config_error = None;

            let new_ai = ai_enabled(&state.config);
            if old_ai != new_ai {
                let close_task = close_popup(state);
                state.ai = if new_ai {
                    tracing::info!("AI module enabled via hot reload");
                    Some(AiController {
                        snapshots: Vec::new(),
                        refresh_tx: None,
                        show_left: true,
                    })
                } else {
                    tracing::info!("AI module disabled via hot reload");
                    None
                };
                if !new_ai {
                    return close_task;
                }
            } else if new_ai {
                if let Some(controller) = state.ai.as_mut() {
                    let ids = if matches!(state.config.ai.mode, wayward_core::config::AiMode::Mock)
                    {
                        vec!["mock".to_owned()]
                    } else {
                        state
                            .config
                            .ai
                            .providers
                            .iter()
                            .filter(|provider| provider.enabled)
                            .map(|provider| provider.id.clone())
                            .collect()
                    };
                    retain_provider_snapshots(&mut controller.snapshots, &ids);
                }
            }

            if old_height != state.config.bar.height || old_position != state.config.bar.position {
                let options = PlatformOptions {
                    edge: match state.config.bar.position {
                        BarPosition::Top => wayward_platform::AppBarEdge::Top,
                        BarPosition::Bottom => wayward_platform::AppBarEdge::Bottom,
                    },
                    logical_height: state.config.bar.height,
                };
                if let Err(error) = wayward_platform::reconfigure(options) {
                    tracing::warn!("AppBar reconfigure failed: {error}");
                }
                if let Some(id) = state.bar_id {
                    let monitor = wayward_platform::primary_monitor_logical_size();
                    let (size, position) =
                        geometry_for(&state.config, monitor.width_logical, monitor.height_logical);
                    return Task::batch([
                        iced::window::resize(id, size),
                        iced::window::move_to(id, position),
                    ]);
                }
            }
            Task::none()
        }
    }
}

fn update(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        Message::Tick(id) => {
            if let Some(cell) = state.modules.iter_mut().find(|cell| cell.id == id) {
                cell.state = cell.module.update();
                tracing::info!(
                    module = cell.id,
                    text = %cell.state.text,
                    level = ?cell.state.level,
                    "module update"
                );
            }
            Task::none()
        }
        Message::ForegroundChanged(title) => {
            if let Some(cell) = state
                .modules
                .iter_mut()
                .find(|cell| cell.id == "active_window")
            {
                cell.module.push(&title);
                cell.state = cell.module.update();
            }
            Task::none()
        }
        Message::Ready { id, handle } => {
            tracing::info!("Ready message received");
            state.bar_id = Some(id);
            let edge = match state.config.bar.position {
                BarPosition::Top => wayward_platform::AppBarEdge::Top,
                BarPosition::Bottom => wayward_platform::AppBarEdge::Bottom,
            };
            if !state.config.bar.monitor.is_empty() {
                tracing::warn!("multi-monitor is not supported; using primary monitor");
            }
            let options = PlatformOptions {
                edge,
                logical_height: state.config.bar.height,
            };
            if let Err(error) = wayward_platform::attach(handle, options) {
                state.platform_error = Some(error.clone());
                eprintln!("wayward: AppBar setup failed: {error}");
            }
            // In daemon mode the window is created hidden; AppBar's MoveWindow runs
            // before winit shows it, so re-apply the geometry through iced now.
            let monitor = wayward_platform::primary_monitor_logical_size();
            let (size, position) =
                geometry_for(&state.config, monitor.width_logical, monitor.height_logical);
            Task::batch([
                iced::window::resize(id, size),
                iced::window::move_to(id, position),
            ])
        }
        Message::Rescaled(scale) => {
            tracing::info!("display scale changed: {scale}");
            Task::none()
        }
        Message::ConfigReloaded(event) => apply_reload(state, event),
        Message::PlatformFailed(error) => {
            state.platform_error = Some(error.clone());
            eprintln!("wayward: platform error: {error}");
            Task::none()
        }
        Message::AiReady { refresh_tx } => {
            if let Some(controller) = state.ai.as_mut() {
                controller.refresh_tx = Some(refresh_tx);
            }
            Task::none()
        }
        Message::AiFetched {
            provider_id,
            usage,
            level,
            fetched_at,
        } => {
            if let Some(controller) = state.ai.as_mut() {
                apply_fetched(
                    &mut controller.snapshots,
                    provider_id,
                    usage,
                    level,
                    fetched_at,
                    state.config.ai.warn_below,
                    state.config.ai.urgent_below,
                );
            }
            Task::none()
        }
        Message::AiFetchFailed { provider_id, error } => {
            if let Some(controller) = state.ai.as_mut() {
                tracing::warn!(provider = %provider_id, "AI provider refresh failed");
                apply_failed(
                    &mut controller.snapshots,
                    provider_id,
                    error,
                    state.config.ai.warn_below,
                    state.config.ai.urgent_below,
                );
            }
            Task::none()
        }
        Message::AiTogglePopup => {
            if state.popup_id.is_some() {
                close_popup(state)
            } else if state.ai.is_some() {
                let (x, y) = popup_position(&state.config);
                let (id, open_task) = iced::window::open(popup_settings(x, y));
                state.popup_id = Some(id);
                tracing::info!("AI popup opened");
                open_task.map(Message::PopupOpened)
            } else {
                Task::none()
            }
        }
        Message::AiRefreshPopup | Message::AiKeyboard(input::PopupKey::Refresh) => {
            refresh_ai(state)
        }
        Message::AiKeyboard(input::PopupKey::ToggleLeftUsed) => {
            if let Some(controller) = state.ai.as_mut() {
                controller.show_left = !controller.show_left;
            }
            Task::none()
        }
        Message::AiKeyboard(input::PopupKey::Close) => {
            if state.popup_id.is_some() {
                close_popup(state)
            } else {
                Task::none()
            }
        }
        Message::PopupOpened(id) => {
            tracing::info!("AI popup window ready: {id:?}");
            Task::none()
        }
        Message::PopupClosed(id) | Message::PopupUnfocused(id) => {
            if state.popup_id == Some(id) {
                tracing::info!("AI popup closed");
                close_popup(state)
            } else {
                Task::none()
            }
        }
        Message::UiTick => Task::none(),
        Message::WindowCloseRequested(id) => {
            if state.popup_id == Some(id) {
                close_popup(state)
            } else if state.bar_id == Some(id) {
                wayward_platform::shutdown();
                iced::exit()
            } else {
                Task::none()
            }
        }
    }
}

fn subscription(state: &AppState) -> Subscription<Message> {
    let mut subscriptions: Vec<Subscription<Message>> = Vec::new();

    for cell in &state.modules {
        if let Some(interval) = cell.module.interval() {
            subscriptions.push(
                time::every(interval)
                    .with(cell.id)
                    .map(|(id, _instant)| Message::Tick(id)),
            );
        }
    }

    subscriptions.push(iced::event::listen_with(
        |event, _status, _id| match event {
            iced::Event::Window(iced::window::Event::Rescaled(scale)) => {
                Some(Message::Rescaled(scale))
            }
            _ => None,
        },
    ));
    subscriptions.push(iced::window::close_requests().map(Message::WindowCloseRequested));
    subscriptions.push(iced::window::close_events().map(Message::PopupClosed));
    subscriptions.push(iced::event::listen_with(input::popup_key).map(Message::AiKeyboard));

    if state.modules.iter().any(|cell| cell.id == "active_window") {
        subscriptions.push(foreground_stream::subscription(
            foreground_stream::ForegroundKey,
        ));
    }

    if let Some(popup_id) = state.popup_id {
        subscriptions.push(iced::window::events().with(popup_id).filter_map(
            |(tracked, (id, event))| {
                if id == tracked && matches!(event, iced::window::Event::Unfocused) {
                    Some(Message::PopupUnfocused(id))
                } else {
                    None
                }
            },
        ));
        subscriptions.push(time::every(std::time::Duration::from_secs(1)).map(|_| Message::UiTick));
    }

    let watch_dir = state.config_dir.clone();
    subscriptions.push(
        hot_reload::subscription(WatchPaths {
            config_dir: watch_dir.clone(),
            themes_dir: themes_dir(&watch_dir),
        })
        .map(Message::ConfigReloaded),
    );

    if ai_enabled(&state.config) {
        let ai = &state.config.ai;
        subscriptions.push(ai_stream::subscription(AiStreamKey {
            mode: ai.mode,
            enabled: ai.enabled,
            interval_ms: ai.interval_ms,
            experimental: ai.experimental,
            warn_below_bits: ai.warn_below.to_bits(),
            urgent_below_bits: ai.urgent_below.to_bits(),
            providers: ai
                .providers
                .iter()
                .map(|provider| AiProviderKey {
                    id: provider.id.clone(),
                    enabled: provider.enabled,
                    interval_ms: provider.interval_ms,
                    root: provider.root.clone(),
                    account: provider.account.clone(),
                    limit_bits: provider.limit.to_bits(),
                    reset_after_secs: provider.reset_after_secs,
                    reset_anchor_secs: provider.reset_anchor_secs,
                })
                .collect(),
        }));
    }

    Subscription::batch(subscriptions)
}

fn window_settings(config: &Config) -> Settings {
    let monitor = wayward_platform::primary_monitor_logical_size();
    let (size, position) = geometry_for(config, monitor.width_logical, monitor.height_logical);

    Settings {
        size,
        position: Position::Specific(position),
        resizable: false,
        decorations: false,
        level: Level::AlwaysOnTop,
        exit_on_close_request: false,
        platform_specific: iced::window::settings::PlatformSpecific {
            skip_taskbar: true,
            undecorated_shadow: false,
            corner_preference: iced::window::settings::platform::CornerPreference::DoNotRound,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    match cli::parse_args() {
        cli::CliAction::PrintConfigPath => {
            let loaded = config_io::load_config()?;
            println!("{}", loaded.dir.display());
            return Ok(());
        }
        cli::CliAction::OpenConfig => {
            let loaded = config_io::load_config()?;
            wayward_platform::open_in_explorer(&loaded.path)?;
            return Ok(());
        }
        cli::CliAction::SetCredential { provider } => {
            let account = wayward_platform::account_for(&provider).map_err(anyhow::Error::msg)?;
            eprint!("Credential for {account}: ");
            let secret = wayward_platform::read_secret().map_err(anyhow::Error::msg)?;
            if secret.is_empty() {
                anyhow::bail!("secret must not be empty");
            }
            wayward_platform::store_credential(&provider, &secret).map_err(anyhow::Error::msg)?;
            println!("Stored credential for {account}");
            return Ok(());
        }
        cli::CliAction::DeleteCredential { provider } => {
            let removed =
                wayward_platform::delete_credential(&provider).map_err(anyhow::Error::msg)?;
            if removed {
                println!("Deleted credential for {provider}");
            } else {
                println!("No credential stored for {provider}");
            }
            return Ok(());
        }
        cli::CliAction::Run => {}
    }

    daemon(boot, update, view::window_view)
        .subscription(subscription)
        .run()?;
    wayward_platform::shutdown();
    Ok(())
}

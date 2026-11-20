mod cli;
mod config_io;
mod config_template;
mod input;
mod style;
mod view;

use iced::window::{Level, Position, Settings};
use iced::{application, time, Point, Size, Subscription, Task};
use view::Message;
use wayward_core::{BarPosition, Config, Module, ModuleState};
use wayward_platform::{PlatformOptions, WindowHandle};

struct ModuleCell {
    id: &'static str,
    module: Box<dyn Module>,
    state: ModuleState,
}

struct State {
    config: Config,
    modules: Vec<ModuleCell>,
    colors: style::Colors,
    platform_error: Option<String>,
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

fn boot() -> (State, Task<Message>) {
    let loaded = config_io::load_config().expect("load config");
    config_io::validate_or_warn(&loaded.config);

    if loaded.created {
        tracing::info!("created default config at {}", loaded.path.display());
    }
    if let Some(error) = loaded.error.as_deref() {
        eprintln!("wayward: using default config: {error}");
    }

    let state = State {
        modules: build_modules(&loaded.config),
        colors: style::theme_colors(&loaded.config),
        config: loaded.config,
        platform_error: None,
    };

    let acquire = iced::window::latest()
        .and_then(|id| {
            iced::window::run(id, |window| {
                use iced::window::raw_window_handle::RawWindowHandle;
                match window.window_handle().map(|h| h.as_raw()) {
                    Ok(RawWindowHandle::Win32(handle)) => Ok(WindowHandle(handle.hwnd.get())),
                    Ok(other) => Err(format!("unsupported window handle: {other:?}")),
                    Err(error) => Err(format!("window handle unavailable: {error}")),
                }
            })
        })
        .map(|result| match result {
            Ok(handle) => Message::Ready(handle),
            Err(error) => Message::PlatformFailed(error),
        });

    (state, acquire)
}

fn update(state: &mut State, message: Message) -> Task<Message> {
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
        }
        Message::Ready(handle) => {
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
        }
        Message::Rescaled(scale) => {
            tracing::info!("display scale changed: {scale}");
        }
        Message::CloseRequested => {
            wayward_platform::shutdown();
            return iced::exit();
        }
        Message::PlatformFailed(error) => {
            state.platform_error = Some(error.clone());
            eprintln!("wayward: platform error: {error}");
        }
    }
    Task::none()
}

fn subscription(state: &State) -> Subscription<Message> {
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
    subscriptions.push(iced::window::close_requests().map(|_| Message::CloseRequested));

    Subscription::batch(subscriptions)
}

fn view(state: &State) -> iced::Element<'_, Message> {
    view::view(
        state.modules.iter().map(|cell| &cell.state),
        &state.colors,
        &state.config,
    )
}

fn window_settings(config: &Config) -> Settings {
    let monitor = wayward_platform::primary_monitor_logical_size();
    let height = config.bar.height as f32;
    let y = match config.bar.position {
        BarPosition::Top => 0.0,
        BarPosition::Bottom => monitor.height_logical - height,
    };

    Settings {
        size: Size::new(monitor.width_logical, height),
        position: Position::Specific(Point::new(0.0, y)),
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
        cli::CliAction::Run => {}
    }

    let loaded = config_io::load_config()?;
    let settings = window_settings(&loaded.config);
    application(boot, update, view)
        .title("wayward")
        .window(settings)
        .subscription(subscription)
        .exit_on_close_request(false)
        .run()?;
    wayward_platform::shutdown();
    Ok(())
}

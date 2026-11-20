mod cli;
mod clock;
mod config_io;
mod config_template;
mod style;
mod view;

use std::time::Duration;

use iced::window::{Level, Position, Settings};
use iced::{application, time, Point, Size, Subscription};
use view::Message;
use wayward_core::{BarPosition, Config};

struct State {
    config: Config,
    clock_text: String,
    colors: (iced::Color, iced::Color),
}

fn boot() -> State {
    let loaded = config_io::load_config().expect("load config");
    config_io::validate_or_warn(&loaded.config);

    if loaded.created {
        tracing::info!("created default config at {}", loaded.path.display());
    }

    if let Some(error) = loaded.error.as_deref() {
        eprintln!("wayward: using default config: {error}");
    }

    State {
        clock_text: clock::format_clock(&clock::now_zoned(), &loaded.config.modules.clock.format),
        colors: style::theme_colors(&loaded.config),
        config: loaded.config,
    }
}

fn update(state: &mut State, _message: Message) {
    state.clock_text = clock::format_clock(&clock::now_zoned(), &state.config.modules.clock.format);
}

fn subscription(state: &State) -> Subscription<Message> {
    let interval = state.config.modules.clock.interval_ms.max(1000);
    time::every(Duration::from_millis(interval)).map(|_| Message::Tick)
}

fn view(state: &State) -> iced::Element<'_, Message> {
    view::view(
        &state.clock_text,
        state.colors,
        state.config.font.size,
        f32::from(state.config.bar.padding),
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
        .run()?;
    Ok(())
}

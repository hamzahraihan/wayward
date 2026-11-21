use std::time::Instant;

use iced::{Color, Element};
use wayward_core::{Config, Level, ModuleState};

use crate::style::{level_color, Colors};
use crate::{input, popup_view};

#[derive(Debug, Clone)]
pub enum Message {
    Tick(&'static str),
    Ready {
        id: iced::window::Id,
        handle: wayward_platform::WindowHandle,
    },
    Rescaled(f32),
    ConfigReloaded(crate::hot_reload::ReloadEvent),
    PlatformFailed(String),
    AiReady {
        refresh_tx: iced::futures::channel::mpsc::UnboundedSender<()>,
    },
    AiFetched {
        usage: wayward_ai::Usage,
        level: Level,
        fetched_at: Instant,
        error: Option<String>,
    },
    AiTogglePopup,
    AiRefreshPopup,
    AiKeyboard(input::PopupKey),
    PopupOpened(iced::window::Id),
    PopupClosed(iced::window::Id),
    PopupUnfocused(iced::window::Id),
    UiTick,
    WindowCloseRequested(iced::window::Id),
}

/// Render the bar window. The AI icon (when enabled) is wrapped in a
/// `mouse_area` so left-click toggles the popup and right-click refreshes it.
pub fn view<'a>(
    cells: impl IntoIterator<Item = &'a ModuleState>,
    colors: &'a Colors,
    config: &'a Config,
    config_error: Option<&str>,
    ai_level: Option<Level>,
) -> Element<'a, Message> {
    let mut row = iced::widget::Row::new().spacing(f32::from(config.bar.padding));

    for cell in cells {
        row = row.push(
            iced::widget::text(format!("{}{}", cell.icon, cell.text))
                .font(iced::Font {
                    family: iced::font::Family::Monospace,
                    ..Default::default()
                })
                .size(config.font.size)
                .color(level_color(cell.level, colors)),
        );
    }

    if let Some(level) = ai_level {
        let icon = iced::widget::text("AI")
            .font(iced::Font {
                family: iced::font::Family::Monospace,
                ..Default::default()
            })
            .size(config.font.size)
            .color(level_color(level, colors));
        row = row.push(
            iced::widget::mouse_area(icon)
                .on_press(Message::AiTogglePopup)
                .on_right_press(Message::AiRefreshPopup),
        );
    }

    if config_error.is_some() {
        row = row.push(
            iced::widget::text("!")
                .font(iced::Font {
                    family: iced::font::Family::Monospace,
                    ..Default::default()
                })
                .size(config.font.size)
                .color(colors.urgent),
        );
    }

    let background: Color = colors.background;
    iced::widget::container(row)
        .width(iced::Length::Fill)
        .padding(f32::from(config.bar.padding))
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(background)),
            text_color: Some(colors.foreground),
            ..Default::default()
        })
        .into()
}

/// Render a window by id: bar, popup, or an empty fallback.
pub fn window_view<'a>(
    state: &'a crate::AppState,
    window_id: iced::window::Id,
) -> Element<'a, Message> {
    if Some(window_id) == state.popup_id {
        let ai = state.ai.as_ref();
        return popup_view::popup(
            ai.and_then(|controller| controller.snapshot.as_ref()),
            &state.colors,
            ai.is_some_and(|controller| controller.show_left),
            Instant::now(),
        );
    }

    if Some(window_id) == state.bar_id {
        return view(
            state.modules.iter().map(|cell| &cell.state),
            &state.colors,
            &state.config,
            state.config_error.as_deref(),
            state
                .ai
                .as_ref()
                .and_then(|controller| controller.snapshot.as_ref().map(|snapshot| snapshot.level)),
        );
    }

    iced::widget::container(iced::widget::text("")).into()
}

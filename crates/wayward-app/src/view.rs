use iced::{Color, Element};
use wayward_core::Config;

use crate::style::{level_color, Colors};

#[derive(Debug, Clone)]
pub enum Message {
    Tick(&'static str),
    Ready(wayward_platform::WindowHandle),
    Rescaled(f32),
    CloseRequested,
    PlatformFailed(String),
}

/// Render one flat cell per module. Plain data in; no I/O here (AGENTS.md §3).
pub fn view<'a>(
    cells: impl IntoIterator<Item = &'a wayward_core::ModuleState>,
    colors: &'a Colors,
    config: &'a Config,
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

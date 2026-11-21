//! Pure renderer for the AI usage popup (AGENTS.md §7).
//!
//! No I/O, no network, no hardcoded colors. Every color comes from
//! [`Colors`] (the active theme) or from [`level_color`]. The progress bar is
//! built from discrete fixed-size rectangles, never a gradient.

use std::time::{Instant, SystemTime};

use iced::widget::{container, row, text, Column, Container, Row};
use iced::Element;

use wayward_ai::level::{format_remaining, meter_level, percent_used, URGENT_BELOW, WARN_BELOW};
use wayward_ai::Usage;
use wayward_core::Level;

use crate::style::{level_color, Colors};
use crate::view::Message;

/// Plain data snapshot handed to the renderer by the app layer.
#[derive(Debug, Clone)]
pub struct AiSnapshot {
    pub usage: Usage,
    pub level: Level,
    pub fetched_at: Instant,
}

const CELL_COUNT: usize = 10;
const CELL_WIDTH: f32 = 8.0;
const CELL_HEIGHT: f32 = 14.0;
const FONT_SIZE: f32 = 14.0;
const SPACING: f32 = 4.0;
const PADDING: f32 = 8.0;

fn mono() -> iced::Font {
    iced::Font {
        family: iced::font::Family::Monospace,
        ..Default::default()
    }
}

/// One discrete progress cell. `color` is always a theme-derived value.
fn cell<'a>(color: iced::Color) -> Element<'a, Message> {
    container(text(""))
        .width(CELL_WIDTH)
        .height(CELL_HEIGHT)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(color)),
            ..Default::default()
        })
        .into()
}

/// Render the popup body. `None` renders a `--` placeholder instead of panicking.
pub fn popup<'a>(
    snapshot: Option<&'a AiSnapshot>,
    colors: &'a Colors,
    show_left: bool,
    now: Instant,
) -> Element<'a, Message> {
    let mut body = Column::new().spacing(SPACING);

    body = body.push(
        text("-- AI USAGE --")
            .font(mono())
            .size(FONT_SIZE)
            .color(colors.accent),
    );

    match snapshot {
        Some(snapshot) => {
            let plan = snapshot.usage.plan.as_deref().unwrap_or("--");
            let system_now = SystemTime::now();

            for meter in &snapshot.usage.meters {
                body = body.push(
                    row([
                        text(meter.label.as_str())
                            .font(mono())
                            .size(FONT_SIZE)
                            .color(colors.foreground)
                            .into(),
                        text(plan)
                            .font(mono())
                            .size(FONT_SIZE)
                            .color(colors.foreground)
                            .into(),
                    ])
                    .spacing(SPACING),
                );

                let used = percent_used(meter.used, meter.limit);
                let percent = if show_left { 1.0 - used } else { used };
                let filled = ((percent * CELL_COUNT as f32).round() as usize).clamp(0, CELL_COUNT);
                let meter_level = meter_level(meter, WARN_BELOW, URGENT_BELOW);
                let filled_color = level_color(meter_level, colors);

                let mut bar = Row::new().spacing(1.0);
                for index in 0..CELL_COUNT {
                    let color = if index < filled {
                        filled_color
                    } else {
                        colors.surface
                    };
                    bar = bar.push(cell(color));
                }

                let reset = format_remaining(meter.reset_at, system_now);
                body = body.push(
                    row([
                        text(meter.label.as_str())
                            .font(mono())
                            .size(FONT_SIZE)
                            .color(colors.foreground)
                            .into(),
                        bar.into(),
                        text(format!("{}%", (percent * 100.0).round() as u32))
                            .font(mono())
                            .size(FONT_SIZE)
                            .color(colors.foreground)
                            .into(),
                        text(reset)
                            .font(mono())
                            .size(FONT_SIZE)
                            .color(colors.muted)
                            .into(),
                    ])
                    .spacing(SPACING)
                    .align_y(iced::alignment::Vertical::Center),
                );
            }
        }
        None => {
            body = body.push(text("--").font(mono()).size(FONT_SIZE).color(colors.muted));
        }
    }

    let updated = match snapshot {
        Some(snapshot) => format!(
            "updated {}s ago",
            now.saturating_duration_since(snapshot.fetched_at).as_secs()
        ),
        None => String::new(),
    };
    body = body.push(
        row([
            text(updated)
                .font(mono())
                .size(FONT_SIZE)
                .color(colors.muted)
                .into(),
            text("r refresh  l/u toggle  Esc close")
                .font(mono())
                .size(FONT_SIZE)
                .color(colors.muted)
                .align_x(iced::alignment::Horizontal::Right)
                .width(iced::Length::Fill)
                .into(),
        ])
        .spacing(SPACING),
    );

    Container::new(body)
        .padding(PADDING)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(colors.background)),
            text_color: Some(colors.foreground),
            border: iced::Border {
                color: colors.surface,
                width: 1.0,
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

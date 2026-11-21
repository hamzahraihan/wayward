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
    pub provider_id: String,
    pub usage: Option<Usage>,
    pub level: Level,
    pub fetched_at: Instant,
    pub error: Option<String>,
}

/// Record a successful refresh, replacing the matching provider in place.
pub fn apply_fetched(
    snapshots: &mut Vec<AiSnapshot>,
    provider_id: String,
    usage: Usage,
    level: Level,
    fetched_at: Instant,
) {
    if let Some(snapshot) = snapshots
        .iter_mut()
        .find(|snapshot| snapshot.provider_id == provider_id)
    {
        snapshot.usage = Some(usage);
        snapshot.level = level;
        snapshot.fetched_at = fetched_at;
        snapshot.error = None;
    } else {
        snapshots.push(AiSnapshot {
            provider_id,
            usage: Some(usage),
            level,
            fetched_at,
            error: None,
        });
    }
}

/// Record a failed refresh without disturbing the last successful data.
pub fn apply_failed(snapshots: &mut Vec<AiSnapshot>, provider_id: String, error: String) {
    if let Some(snapshot) = snapshots
        .iter_mut()
        .find(|snapshot| snapshot.provider_id == provider_id)
    {
        snapshot.error = Some(error);
    } else {
        snapshots.push(AiSnapshot {
            provider_id,
            usage: None,
            level: Level::Normal,
            fetched_at: Instant::now(),
            error: Some(error),
        });
    }
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

fn divider<'a>(colors: &Colors) -> Element<'a, Message> {
    let color = colors.surface;
    container(text(""))
        .width(iced::Length::Fill)
        .height(1.0)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(color)),
            ..Default::default()
        })
        .into()
}

pub fn retain_provider_snapshots(snapshots: &mut Vec<AiSnapshot>, ids: &[String]) {
    snapshots.retain(|snapshot| ids.iter().any(|id| id == &snapshot.provider_id));
}

/// Render one flat block per provider snapshot, in order.
pub fn popup<'a>(
    snapshots: &'a [AiSnapshot],
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

    if snapshots.is_empty() {
        body = body.push(text("--").font(mono()).size(FONT_SIZE).color(colors.muted));
    }

    let system_now = SystemTime::now();

    for (index, snapshot) in snapshots.iter().enumerate() {
        if index > 0 {
            body = body.push(divider(colors));
        }

        let plan = snapshot
            .usage
            .as_ref()
            .and_then(|usage| usage.plan.as_deref())
            .unwrap_or("--");
        let mut header = Row::new().spacing(SPACING).push(
            text(snapshot.provider_id.to_uppercase())
                .font(mono())
                .size(FONT_SIZE)
                .color(colors.foreground),
        );
        header = header.push(text(plan).font(mono()).size(FONT_SIZE).color(colors.muted));
        if snapshot.error.is_some() {
            header = header.push(text("!").font(mono()).size(FONT_SIZE).color(colors.urgent));
        }
        body = body.push(header);

        if snapshot.usage.is_none() {
            body = body.push(text("--").font(mono()).size(FONT_SIZE).color(colors.muted));
        } else if let Some(usage) = snapshot.usage.as_ref() {
            for meter in &usage.meters {
                let used = percent_used(meter.used, meter.limit);
                let percent = if show_left { 1.0 - used } else { used };
                let filled = ((percent * CELL_COUNT as f32).round() as usize).clamp(0, CELL_COUNT);
                let meter_level = meter_level(meter, WARN_BELOW, URGENT_BELOW);
                let filled_color = level_color(meter_level, colors);

                let mut bar = Row::new().spacing(1.0);
                for cell_index in 0..CELL_COUNT {
                    let color = if cell_index < filled {
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

        let updated = format!(
            "updated {}s ago",
            now.saturating_duration_since(snapshot.fetched_at).as_secs()
        );
        body = body.push(
            text(updated)
                .font(mono())
                .size(FONT_SIZE)
                .color(colors.muted),
        );
    }
    body = body.push(
        text("r refresh  l/u toggle  Esc close")
            .font(mono())
            .size(FONT_SIZE)
            .color(colors.muted),
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use wayward_ai::Meter;

    use super::*;

    fn usage(label: &str, used: f64) -> Usage {
        Usage {
            plan: Some("MAX".to_owned()),
            meters: vec![Meter {
                label: label.to_owned(),
                used,
                limit: 100.0,
                reset_at: None,
            }],
            spend: None,
        }
    }

    fn ago(secs: u64) -> Instant {
        Instant::now() - Duration::from_secs(secs)
    }

    #[test]
    fn reload_prunes_snapshots_for_removed_providers() {
        let mut snapshots = Vec::new();
        apply_fetched(
            &mut snapshots,
            "claude-code".to_owned(),
            usage("session", 10.0),
            Level::Normal,
            ago(5),
        );
        apply_fetched(
            &mut snapshots,
            "opencode".to_owned(),
            usage("session", 20.0),
            Level::Normal,
            ago(5),
        );

        retain_provider_snapshots(&mut snapshots, &["opencode".to_owned()]);

        assert_eq!(
            snapshots
                .iter()
                .map(|snapshot| snapshot.provider_id.as_str())
                .collect::<Vec<_>>(),
            vec!["opencode"]
        );
    }

    #[test]
    fn failed_refresh_preserves_previous_snapshot() {
        let mut snapshots = Vec::new();
        let fetched_at = ago(30);
        apply_fetched(
            &mut snapshots,
            "mock".to_owned(),
            usage("session", 62.0),
            Level::Warn,
            fetched_at,
        );
        let before = snapshots[0].clone();

        apply_failed(&mut snapshots, "mock".to_owned(), "boom".to_owned());

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].provider_id, "mock");
        assert_eq!(snapshots[0].usage, before.usage);
        assert_eq!(snapshots[0].level, before.level);
        assert_eq!(snapshots[0].fetched_at, before.fetched_at);
        assert_eq!(snapshots[0].error.as_deref(), Some("boom"));
    }

    #[test]
    fn failed_refresh_for_unknown_id_creates_placeholder() {
        let mut snapshots = Vec::new();
        let before = Instant::now();

        apply_failed(&mut snapshots, "claude-code".to_owned(), "boom".to_owned());
        let after = Instant::now();

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].provider_id, "claude-code");
        assert!(snapshots[0].usage.is_none());
        assert_eq!(snapshots[0].level, Level::Normal);
        assert_eq!(snapshots[0].error.as_deref(), Some("boom"));
        assert!(snapshots[0].fetched_at >= before && snapshots[0].fetched_at <= after);
    }

    #[test]
    fn fetched_replaces_snapshot_and_clears_error() {
        let mut snapshots = Vec::new();
        apply_failed(&mut snapshots, "claude-code".to_owned(), "boom".to_owned());
        let fetched_at = ago(5);

        apply_fetched(
            &mut snapshots,
            "claude-code".to_owned(),
            usage("weekly", 100.0),
            Level::Urgent,
            fetched_at,
        );

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].usage, Some(usage("weekly", 100.0)));
        assert_eq!(snapshots[0].level, Level::Urgent);
        assert_eq!(snapshots[0].fetched_at, fetched_at);
        assert!(snapshots[0].error.is_none());
    }

    #[test]
    fn snapshots_keep_first_seen_order() {
        let mut snapshots = Vec::new();
        apply_fetched(
            &mut snapshots,
            "b".to_owned(),
            usage("b", 1.0),
            Level::Normal,
            ago(3),
        );
        apply_fetched(
            &mut snapshots,
            "a".to_owned(),
            usage("a", 2.0),
            Level::Normal,
            ago(2),
        );
        apply_fetched(
            &mut snapshots,
            "c".to_owned(),
            usage("c", 3.0),
            Level::Normal,
            ago(1),
        );

        assert_eq!(
            snapshots
                .iter()
                .map(|s| s.provider_id.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "a", "c"]
        );

        apply_fetched(
            &mut snapshots,
            "a".to_owned(),
            usage("a", 9.0),
            Level::Warn,
            ago(0),
        );

        assert_eq!(
            snapshots
                .iter()
                .map(|s| s.provider_id.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "a", "c"]
        );
        assert_eq!(snapshots[1].usage, Some(usage("a", 9.0)));
    }
}

use iced::{Color, Element};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    Tick,
}

pub fn view<'a>(
    clock_text: &'a str,
    colors: (Color, Color),
    font_size: f32,
    padding: f32,
) -> Element<'a, Message> {
    let clock = iced::widget::text(clock_text)
        .font(iced::Font {
            family: iced::font::Family::Monospace,
            ..Default::default()
        })
        .size(font_size)
        .color(colors.1);

    iced::widget::container(iced::widget::row![clock].spacing(padding))
        .width(iced::Length::Fill)
        .padding(padding)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(colors.0)),
            text_color: Some(colors.1),
            ..Default::default()
        })
        .into()
}

use crate::state::Message;
use crate::theme::Palette;
use iced::widget::{column, container, row, text};
use iced::{Element, Length};

pub fn view(badge: &str, title: &str, subtitle: &str, palette: &Palette) -> Element<'static, Message> {
    let badge_bg = palette.accent_soft;
    let badge_fg = palette.accent_fg;
    let badge_circle = container(text(badge.to_string()).size(13).color(badge_fg))
        .width(Length::Fixed(24.0))
        .height(Length::Fixed(24.0))
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
        .style(move |_theme| container::Style {
            background: Some(badge_bg.into()),
            border: iced::Border { radius: 999.0.into(), ..Default::default() },
            ..Default::default()
        });

    row![
        badge_circle,
        column![
            text(title.to_string()).size(15).color(palette.fg),
            text(subtitle.to_string()).size(12).color(palette.dim),
        ]
        .spacing(1),
    ]
    .spacing(11)
    .into()
}

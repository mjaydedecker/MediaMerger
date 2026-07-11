use crate::state::Message;
use iced::widget::svg;
use iced::{Color, Element, Length};

fn icon(bytes: &'static [u8], color: Color) -> Element<'static, Message> {
    let handle = svg::Handle::from_memory(bytes);
    svg(handle)
        .width(Length::Fixed(18.0))
        .height(Length::Fixed(18.0))
        .style(move |_theme, _status| svg::Style { color: Some(color) })
        .into()
}

pub fn video(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/video.svg"), color)
}

pub fn audio(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/audio.svg"), color)
}

pub fn subtitle(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/subtitle.svg"), color)
}

pub fn folder(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/folder.svg"), color)
}

pub fn check(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/check.svg"), color)
}

pub fn warning(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/warning.svg"), color)
}

pub fn sparkle(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/sparkle.svg"), color)
}

pub fn layers(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/layers.svg"), color)
}

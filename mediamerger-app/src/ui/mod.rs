mod extras;
mod file_pickers;
mod icons;
mod offset_panel;
mod output_log;
mod section_header;
mod track_table;

use crate::state::{AppState, Message};
use crate::theme::{self, Palette};
use iced::widget::{column, container, scrollable};
use iced::{Element, Length};

pub fn view(state: &AppState) -> Element<Message> {
    let palette: Palette = theme::build(state.is_dark, &state.accent_hex);

    let sections = column![
        file_pickers::view(state, &palette),
        track_table::view(state, &palette),
        offset_panel::view(state, &palette),
        extras::view(state, &palette),
        output_log::view(state, &palette),
    ]
    .spacing(20);

    // The content column is left at its natural (Shrink) height rather than
    // Fill, so it can grow taller than the window and give `scrollable`
    // something to scroll to - sections were getting silently clipped with
    // no way to reach them before this. The outer container still fills the
    // window so the body background covers the full area even when the
    // content is shorter than the viewport.
    let scroll_area = scrollable(container(sections).width(Length::Fill).padding(24))
        .width(Length::Fill)
        .height(Length::Fill);

    container(scroll_area)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(palette.body_bg.into()),
            ..Default::default()
        })
        .into()
}

mod extras;
mod file_pickers;
mod icons;
mod offset_panel;
mod output_log;
mod track_table;

use crate::state::{AppState, Message};
use crate::theme::{self, Palette};
use iced::widget::{column, container, text};
use iced::{Element, Length};

pub fn view(state: &AppState) -> Element<Message> {
    let palette: Palette = theme::build(state.is_dark, &state.accent_hex);

    let mut sections = column![
        file_pickers::view(state, &palette),
        track_table::view(state, &palette),
        offset_panel::view(state, &palette),
        extras::view(state, &palette),
        output_log::view(state, &palette),
    ]
    .spacing(20);

    if let Some(err) = &state.framerate_error {
        sections = sections.push(text(err.to_string()).color(palette.danger_fg));
    }

    container(sections)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(palette.body_bg.into()),
            ..Default::default()
        })
        .padding(24)
        .into()
}

mod extras;
mod file_pickers;
mod offset_panel;
mod track_table;

use crate::state::{AppState, Message};
use iced::widget::{column, text};
use iced::Element;

pub fn view(state: &AppState) -> Element<Message> {
    let mut sections = column![
        file_pickers::view(state),
        track_table::view(state),
        offset_panel::view(state),
        extras::view(state),
    ]
    .spacing(20);

    if let Some(err) = &state.framerate_error {
        sections = sections.push(text(err.to_string()));
    }

    sections.into()
}

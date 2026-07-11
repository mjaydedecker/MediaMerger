mod file_pickers;

use crate::state::{AppState, Message};
use iced::widget::{column, text};
use iced::Element;

pub fn view(state: &AppState) -> Element<Message> {
    let mut sections = column![file_pickers::view(state)].spacing(20);

    if let Some(err) = &state.framerate_error {
        sections = sections.push(text(err.to_string()));
    }

    sections.into()
}

use crate::state::{AppState, Message};
use iced::widget::{button, column, row, text};
use iced::Element;

pub fn view(state: &AppState) -> Element<Message> {
    column![
        row![
            text(match &state.file_a {
                Some(f) => f.path.display().to_string(),
                None => "No file selected".to_string(),
            }),
            button("Browse (File A)").on_press(Message::PickFileA),
        ]
        .spacing(10),
        row![
            text(match &state.file_b {
                Some(f) => f.path.display().to_string(),
                None => "No file selected".to_string(),
            }),
            button("Browse (File B)").on_press(Message::PickFileB),
        ]
        .spacing(10),
    ]
    .spacing(10)
    .into()
}

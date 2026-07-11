use crate::state::{AppState, Message};
use iced::widget::{button, column, row, text};
use iced::Element;

pub fn view(state: &AppState) -> Element<Message> {
    let pick_a_press = if state.picking_file_a { None } else { Some(Message::PickFileA) };
    let pick_b_press = if state.picking_file_b { None } else { Some(Message::PickFileB) };

    column![
        row![
            text(match &state.file_a {
                Some(f) => f.path.display().to_string(),
                None => "No file selected".to_string(),
            }),
            button("Browse (File A)").on_press_maybe(pick_a_press),
        ]
        .spacing(10),
        row![
            text(match &state.file_b {
                Some(f) => f.path.display().to_string(),
                None => "No file selected".to_string(),
            }),
            button("Browse (File B)").on_press_maybe(pick_b_press),
        ]
        .spacing(10),
    ]
    .spacing(10)
    .into()
}

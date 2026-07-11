use crate::state::{AppState, ChaptersChoice, Message};
use iced::widget::{checkbox, column, radio, row, text};
use iced::Element;

pub fn view(state: &AppState) -> Element<Message> {
    column![
        row![
            text("Chapters:"),
            radio("File A", ChaptersChoice::FileA, Some(state.chapters_choice), Message::ChaptersChoiceChanged),
            radio("File B", ChaptersChoice::FileB, Some(state.chapters_choice), Message::ChaptersChoiceChanged),
            radio("None", ChaptersChoice::None, Some(state.chapters_choice), Message::ChaptersChoiceChanged),
        ]
        .spacing(10),
        row![
            checkbox(state.attachments_a).label("Attachments from A").on_toggle(Message::ToggleAttachmentsA),
            checkbox(state.attachments_b).label("Attachments from B").on_toggle(Message::ToggleAttachmentsB),
        ]
        .spacing(10),
        row![
            checkbox(state.tags_a).label("Tags from A").on_toggle(Message::ToggleTagsA),
            checkbox(state.tags_b).label("Tags from B").on_toggle(Message::ToggleTagsB),
        ]
        .spacing(10),
    ]
    .spacing(10)
    .into()
}

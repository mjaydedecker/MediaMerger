use crate::state::{AppState, Message};
use iced::widget::{button, column, row, text};
use iced::Element;

pub fn view(state: &AppState) -> Element<Message> {
    let output_label = match &state.output_path {
        Some(p) => p.display().to_string(),
        None => "No output selected".to_string(),
    };

    let blocking_reason = state.blocking_reason();
    let merge_press = if blocking_reason.is_some() {
        None
    } else {
        Some(Message::StartMerge)
    };

    let mut col = column![
        row![
            text(output_label),
            button("Browse (Output)").on_press(Message::PickOutput),
            button("Merge").on_press_maybe(merge_press),
        ]
        .spacing(10),
    ]
    .spacing(10);

    if let Some(reason) = &blocking_reason {
        col = col.push(text(format!("Merge blocked: {reason}")));
    }

    if !state.missing_binaries.is_empty() {
        col = col.push(text(format!("Missing required tools: {}", state.missing_binaries.join(", "))));
    }
    if let Some(p) = state.merge_progress {
        col = col.push(text(format!("Progress: {:.0}%", p * 100.0)));
    }
    if let Some(err) = &state.merge_error {
        col = col.push(text(format!("Merge failed: {err}")));
    }
    for line in &state.log {
        col = col.push(text(line));
    }

    col.into()
}

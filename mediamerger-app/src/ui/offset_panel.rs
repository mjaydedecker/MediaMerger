use crate::state::{AppState, Message, OffsetState};
use iced::widget::{button, column, row, text, text_input};
use iced::Element;
use mediamerger_core::offset::Consistency;

pub fn view(state: &AppState) -> Element<Message> {
    let status: Element<Message> = match &state.offset {
        OffsetState::NotDetected => text("Offset not yet detected").into(),
        OffsetState::Detecting => text("Detecting offset…").into(),
        OffsetState::Detected(r) => {
            let consistency_label = match r.consistency {
                Consistency::Consistent if r.confidence < 3.0 => "consistent (low confidence)",
                Consistency::Consistent => "consistent",
                Consistency::Inconsistent => "INCONSISTENT — resolve manually before merging",
                Consistency::Unverified => "unverified (file too short for a second check)",
            };
            text(format!(
                "early: {:.3}s, late: {:.3}s ({consistency_label}), confidence: {:.2}",
                r.early_offset, r.late_offset, r.confidence
            ))
            .into()
        }
        OffsetState::ManualOverride(v) => text(format!("manual override: {v:.3}s")).into(),
    };

    let mut col = column![
        row![
            button("Detect Offset").on_press(Message::DetectOffset),
            text_input("offset seconds", &state.manual_offset_input)
                .on_input(Message::ManualOffsetChanged),
        ]
        .spacing(10),
        status,
    ]
    .spacing(10);

    if let Some(err) = &state.detect_offset_error {
        col = col.push(text(format!("Could not detect offset: {err}")));
    }

    col.into()
}

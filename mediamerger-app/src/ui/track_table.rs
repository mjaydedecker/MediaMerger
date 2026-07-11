use crate::state::{AppState, Message, TrackUiState};
use iced::widget::{checkbox, column, row, text};
use iced::Element;
use mediamerger_core::probe::{MediaFile, Track};

fn track_label(track: &Track) -> String {
    let lang = track.language.as_deref().unwrap_or("und");
    format!("{:?}: {} ({lang})", track.kind, track.codec)
}

fn track_row<'a>(
    idx: usize,
    track: &'a Track,
    ui: &TrackUiState,
    on_toggle: impl Fn(usize) -> Message + 'a,
    on_default: impl Fn(usize, bool) -> Message + 'a,
    on_forced: impl Fn(usize, bool) -> Message + 'a,
) -> Element<'a, Message> {
    row![
        checkbox(ui.selected)
            .label(track_label(track))
            .on_toggle(move |_| on_toggle(idx)),
        checkbox(ui.default_flag)
            .label("Default")
            .on_toggle(move |v| on_default(idx, v)),
        checkbox(ui.forced_flag)
            .label("Forced")
            .on_toggle(move |v| on_forced(idx, v)),
    ]
    .spacing(10)
    .into()
}

fn file_column<'a>(
    file: &'a Option<MediaFile>,
    ui: &'a [TrackUiState],
    on_toggle: impl Fn(usize) -> Message + Copy + 'a,
    on_default: impl Fn(usize, bool) -> Message + Copy + 'a,
    on_forced: impl Fn(usize, bool) -> Message + Copy + 'a,
) -> Element<'a, Message> {
    match file {
        None => text("No file loaded").into(),
        Some(f) => {
            let mut col = column![].spacing(5);
            for (idx, track) in f.tracks.iter().enumerate() {
                let row_ui = ui.get(idx).cloned().unwrap_or_default();
                col = col.push(track_row(
                    idx, track, &row_ui, on_toggle, on_default, on_forced,
                ));
            }
            col.into()
        }
    }
}

pub fn view(state: &AppState) -> Element<Message> {
    row![
        file_column(
            &state.file_a,
            &state.tracks_a_ui,
            Message::ToggleTrackA,
            Message::SetDefaultFlagA,
            Message::SetForcedFlagA,
        ),
        file_column(
            &state.file_b,
            &state.tracks_b_ui,
            Message::ToggleTrackB,
            Message::SetDefaultFlagB,
            Message::SetForcedFlagB,
        ),
    ]
    .spacing(30)
    .into()
}

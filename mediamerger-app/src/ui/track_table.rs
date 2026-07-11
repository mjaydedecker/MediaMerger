use crate::state::{AppState, Message};
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
    selected: bool,
    on_toggle: impl Fn(usize) -> Message + 'a,
) -> Element<'a, Message> {
    row![
        checkbox(selected).label(track_label(track)).on_toggle(move |_| on_toggle(idx)),
    ]
    .into()
}

fn file_column<'a>(
    file: &'a Option<MediaFile>,
    ui: &'a [crate::state::TrackUiState],
    on_toggle: impl Fn(usize) -> Message + Copy + 'a,
) -> Element<'a, Message> {
    match file {
        None => text("No file loaded").into(),
        Some(f) => {
            let mut col = column![].spacing(5);
            for (idx, track) in f.tracks.iter().enumerate() {
                let selected = ui.get(idx).map(|u| u.selected).unwrap_or(false);
                col = col.push(track_row(idx, track, selected, on_toggle));
            }
            col.into()
        }
    }
}

pub fn view(state: &AppState) -> Element<Message> {
    row![
        file_column(&state.file_a, &state.tracks_a_ui, Message::ToggleTrackA),
        file_column(&state.file_b, &state.tracks_b_ui, Message::ToggleTrackB),
    ]
    .spacing(30)
    .into()
}

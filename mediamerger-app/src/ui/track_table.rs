use crate::state::{AppState, Message, TrackUiState};
use crate::theme::Palette;
use crate::ui::icons;
use iced::widget::{button, checkbox, column, container, row, rule, text};
use iced::{Element, Length};
use mediamerger_core::probe::{channel_layout_label, MediaFile, Track, TrackKind};

fn track_detail_line(track: &Track) -> String {
    match track.kind {
        TrackKind::Video => {
            let res = match (track.width, track.height) {
                (Some(_), Some(h)) if h >= 2000 => format!("{}p", h),
                (Some(_), Some(h)) => format!("{h}p"),
                _ => String::new(),
            };
            let dynamic_range = match (track.is_hdr10, track.is_dolby_vision) {
                (true, true) => "HDR10 + Dolby Vision",
                (true, false) => "HDR10",
                (false, true) => "Dolby Vision",
                (false, false) => "SDR",
            };
            [res, dynamic_range.to_string()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")
        }
        TrackKind::Audio => {
            let mut parts = Vec::new();
            if let Some(ch) = track.channels {
                parts.push(channel_layout_label(ch));
            }
            if let Some(rate) = track.sampling_rate {
                parts.push(format!("{} kHz", rate / 1000));
            }
            if let Some(bps) = track.bitrate_bps {
                parts.push(format!("{:.1} kbps", bps as f64 / 1000.0));
            }
            parts.join(" · ")
        }
        TrackKind::Subtitle => String::new(),
    }
}

fn track_row<'a>(
    idx: usize,
    track: &'a Track,
    ui: &TrackUiState,
    palette: &Palette,
    on_toggle: impl Fn(usize) -> Message + 'a,
    on_default: impl Fn(usize, bool) -> Message + 'a,
    on_forced: impl Fn(usize, bool) -> Message + 'a,
) -> Element<'a, Message> {
    let kind_icon = match track.kind {
        TrackKind::Video => icons::video(palette.dim),
        TrackKind::Audio => icons::audio(palette.dim),
        TrackKind::Subtitle => icons::subtitle(palette.dim),
    };

    let lang = track.language.clone().unwrap_or_else(|| "und".to_string());
    let detail = track_detail_line(track);

    let mut info = column![
        row![text(track.codec.clone()).size(13).color(palette.fg), text(lang.to_uppercase()).size(9).color(palette.dim)].spacing(7),
    ]
    .spacing(1);
    if !detail.is_empty() {
        info = info.push(text(detail).size(12).color(palette.faint));
    }

    let def_style = if ui.default_flag { palette.accent_soft } else { palette.chip_bg };
    let forced_style = if ui.forced_flag { palette.accent_soft } else { palette.chip_bg };

    row![
        checkbox(ui.selected).on_toggle(move |_| on_toggle(idx)),
        kind_icon,
        info.width(Length::Fill),
        button(text("Default").size(10))
            .style(move |_theme, _status| button::Style { background: Some(def_style.into()), ..Default::default() })
            .on_press(on_default(idx, !ui.default_flag)),
        button(text("Forced").size(10))
            .style(move |_theme, _status| button::Style { background: Some(forced_style.into()), ..Default::default() })
            .on_press(on_forced(idx, !ui.forced_flag)),
    ]
    .spacing(11)
    .padding(11)
    .into()
}

fn file_column<'a>(
    file: &'a Option<MediaFile>,
    ui: &'a [TrackUiState],
    palette: &Palette,
    on_toggle: impl Fn(usize) -> Message + Copy + 'a,
    on_default: impl Fn(usize, bool) -> Message + Copy + 'a,
    on_forced: impl Fn(usize, bool) -> Message + Copy + 'a,
) -> Element<'a, Message> {
    // Both branches get an explicit FillPortion(1) width so File A/B split
    // the row evenly regardless of content - without this, whichever file's
    // codec/detail text happened to be wider (or an empty "No file loaded"
    // side) could unbalance the two columns on resize, the same class of
    // bug fixed for the file-picker cards above.
    match file {
        None => {
            let card_bg = palette.card;
            let border_color = palette.border;
            container(text("No file loaded").size(12).color(palette.faint))
                .width(Length::FillPortion(1))
                .padding(16)
                .align_x(iced::alignment::Horizontal::Center)
                .style(move |_theme| container::Style {
                    background: Some(card_bg.into()),
                    border: iced::Border { color: border_color, width: 1.0, radius: 12.0.into() },
                    ..Default::default()
                })
                .into()
        }
        Some(f) => {
            let mut col = column![].spacing(0);
            let separator_color = palette.separator;
            for (idx, track) in f.tracks.iter().enumerate() {
                if idx > 0 {
                    col = col.push(rule::horizontal(1).style(move |_theme| rule::Style {
                        color: separator_color,
                        radius: 0.0.into(),
                        fill_mode: rule::FillMode::Full,
                        snap: false,
                    }));
                }
                let row_ui = ui.get(idx).cloned().unwrap_or_default();
                col = col.push(track_row(idx, track, &row_ui, palette, on_toggle, on_default, on_forced));
            }
            let card_bg = palette.card;
            let border_color = palette.border;
            container(col)
                .width(Length::FillPortion(1))
                .style(move |_theme| container::Style {
                    background: Some(card_bg.into()),
                    border: iced::Border { color: border_color, width: 1.0, radius: 12.0.into() },
                    ..Default::default()
                })
                .into()
        }
    }
}

pub fn view<'a>(state: &'a AppState, palette: &Palette) -> Element<'a, Message> {
    row![
        file_column(&state.file_a, &state.tracks_a_ui, palette, Message::ToggleTrackA, Message::SetDefaultFlagA, Message::SetForcedFlagA),
        file_column(&state.file_b, &state.tracks_b_ui, palette, Message::ToggleTrackB, Message::SetDefaultFlagB, Message::SetForcedFlagB),
    ]
    .spacing(16)
    .into()
}

use crate::state::{AppState, Message};
use crate::theme::Palette;
use crate::ui::icons;
use iced::widget::{button, column, container, row, text};
use iced::Element;
use mediamerger_core::probe::{MediaFile, TrackKind};

fn chip(label: String, palette: &Palette) -> Element<'static, Message> {
    // Copy the Color fields out before the `move` closure: closing over
    // `palette` itself (a `&Palette`) ties the closure's lifetime to the
    // caller's borrow, which conflicts with this fn's `'static` return.
    let text_color = palette.dim;
    let bg = palette.chip_bg;
    let border_color = palette.chip_border;
    container(text(label).size(11).color(text_color))
        .padding([3, 8])
        .style(move |_theme| container::Style {
            background: Some(bg.into()),
            border: iced::Border { color: border_color, width: 1.0, radius: 6.0.into() },
            ..Default::default()
        })
        .into()
}

fn file_chips(file: &MediaFile, palette: &Palette) -> Element<'static, Message> {
    let video_track = file.tracks.iter().find(|t| t.kind == TrackKind::Video);
    let mut chips = row![chip(file.container.clone(), palette)].spacing(6);

    if let Some(v) = video_track {
        if let (Some(w), Some(h)) = (v.width, v.height) {
            chips = chips.push(chip(format!("{w}x{h}"), palette));
        }
        if let Some(fps) = v.fps {
            chips = chips.push(chip(format!("{fps:.3} fps"), palette));
        }
    }
    chips = chips.push(chip(format!("{} tracks", file.tracks.len()), palette));

    let size_gb = file.file_size_bytes as f64 / 1_073_741_824.0;
    chips = chips.push(chip(format!("{size_gb:.1} GB"), palette));

    chips.into()
}

fn file_card<'a>(
    label: &'static str,
    file: &'a Option<MediaFile>,
    picking: bool,
    on_browse: Message,
    palette: &Palette,
) -> Element<'a, Message> {
    let path_text = match file {
        Some(f) => f.path.display().to_string(),
        None => "No file selected".to_string(),
    };

    let browse_press = if picking { None } else { Some(on_browse) };

    let mut card = column![
        row![
            text(label).size(13).color(palette.fg),
            button(row![icons::folder(palette.fg), text("Browse")].spacing(6))
                .on_press_maybe(browse_press),
        ]
        .spacing(10),
        row![icons::video(palette.dim), text(path_text).size(12).color(palette.fg)].spacing(8),
    ]
    .spacing(10);

    if let Some(f) = file {
        card = card.push(file_chips(f, palette));
    }

    // Same reasoning as `chip`: copy the Colors out so the closure doesn't
    // capture `palette`'s reference and drag its lifetime into `Element<'a>`.
    let card_bg = palette.card;
    let border_color = palette.border;
    container(card)
        .padding(15)
        .style(move |_theme| container::Style {
            background: Some(card_bg.into()),
            border: iced::Border { color: border_color, width: 1.0, radius: 12.0.into() },
            ..Default::default()
        })
        .into()
}

fn framerate_banner<'a>(state: &'a AppState, palette: &Palette) -> Option<Element<'a, Message>> {
    if let Some(err) = &state.framerate_error {
        return Some(
            row![icons::warning(palette.danger_fg), text(err.to_string()).color(palette.danger_fg)]
                .spacing(8)
                .into(),
        );
    }
    if state.file_a.is_some() && state.file_b.is_some() {
        // Both files present and framerate_error is None means
        // check_framerate already confirmed a match - file_b's own fps
        // isn't needed here, just file_a's, to display as representative.
        let fps_a = state
            .file_a
            .as_ref()
            .and_then(|f| f.tracks.iter().find(|t| t.kind == TrackKind::Video))
            .and_then(|t| t.fps);
        if let Some(fps) = fps_a {
            return Some(
                row![
                    icons::check(palette.success_fg),
                    text(format!("Framerates match — {fps:.3} fps. Safe to align and merge.")).color(palette.success_fg),
                ]
                .spacing(8)
                .into(),
            );
        }
    }
    None
}

pub fn view<'a>(state: &'a AppState, palette: &Palette) -> Element<'a, Message> {
    let mut col = column![
        row![
            file_card("File A · Base", &state.file_a, state.picking_file_a, Message::PickFileA, palette),
            file_card("File B · Donor", &state.file_b, state.picking_file_b, Message::PickFileB, palette),
        ]
        .spacing(14),
    ]
    .spacing(12);

    if let Some(banner) = framerate_banner(state, palette) {
        col = col.push(banner);
    }

    col.into()
}

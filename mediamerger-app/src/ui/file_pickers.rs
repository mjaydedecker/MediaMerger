use crate::state::{AppState, Message};
use crate::theme::Palette;
use crate::ui::icons;
use iced::widget::{button, checkbox, column, container, row, text};
use iced::{Element, Length};
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
    }

    if let Some(secs) = file.duration_secs {
        chips = chips.push(chip(crate::state::format_duration(secs), palette));
    }

    chips = chips.push(chip(format!("{} tracks", file.tracks.len()), palette));

    if let Some(v) = video_track {
        if let Some(fps) = v.fps {
            chips = chips.push(chip(format!("{fps:.3} fps"), palette));
        }
    }

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
    let browse_press = if picking { None } else { Some(on_browse) };

    let btn_bg = palette.btn_bg;
    let btn_hover = palette.btn_hover;
    let fg = palette.fg;
    let mut card = column![
        row![
            text(label).size(13).color(palette.fg),
            button(row![icons::folder(palette.fg), text("Browse")].spacing(6))
                .style(move |_theme, status| {
                    let base = button::Style { background: Some(btn_bg.into()), text_color: fg, ..Default::default() };
                    match status {
                        button::Status::Hovered => {
                            button::Style { background: Some(btn_hover.into()), ..base }
                        }
                        // Mirror iced_widget::button's own `disabled()` helper: scale
                        // both background and text alpha by 0.5 rather than rendering
                        // disabled buttons identically to enabled ones.
                        button::Status::Disabled => button::Style {
                            background: base.background.map(|b| b.scale_alpha(0.5)),
                            text_color: base.text_color.scale_alpha(0.5),
                            ..base
                        },
                        _ => base,
                    }
                })
                .on_press_maybe(browse_press),
        ]
        .spacing(10),
    ]
    .spacing(10);

    match file {
        Some(f) => {
            card = card.push(
                row![
                    icons::video(palette.dim),
                    // Fill + wrap rather than letting the text force the row (and
                    // therefore this whole card) wider to fit a long path on one
                    // line - without this, a long File A path could grow File A's
                    // card at File B's expense when the window is resized, since
                    // neither card had an explicit width to split the row evenly.
                    // WordOrGlyph is required, not just Fill width: iced's default
                    // Word wrapping treats an unbroken path (no spaces) as one
                    // indivisible unit and lets it overflow the row rather than
                    // breaking it - WordOrGlyph falls back to a mid-word break when
                    // a "word" can't fit on a line by itself.
                    text(f.path.display().to_string())
                        .size(12)
                        .color(palette.fg)
                        .width(Length::Fill)
                        .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                ]
                .spacing(8),
            );
            card = card.push(file_chips(f, palette));
        }
        None => {
            // iced's container Border has no dash-pattern field (solid only)
            // - approximated with a solid border, consistent with this
            // project's established practice of documenting approximations
            // over reaching for a Canvas overlay for a minor cosmetic detail.
            let view_bg = palette.view;
            let border_color = palette.border;
            card = card.push(
                container(text("No file selected — click Browse to load").size(12).color(palette.faint))
                    .padding(16)
                    .width(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Center)
                    .style(move |_theme| container::Style {
                        background: Some(view_bg.into()),
                        border: iced::Border { color: border_color, width: 1.0, radius: 8.0.into() },
                        ..Default::default()
                    }),
            );
        }
    }

    // Same reasoning as `chip`: copy the Colors out so the closure doesn't
    // capture `palette`'s reference and drag its lifetime into `Element<'a>`.
    let card_bg = palette.card;
    let border_color = palette.border;
    container(card)
        // Split the row evenly between File A/B regardless of content
        // length - each card previously defaulted to Length::Shrink,
        // sizing to its own content, so whichever file had the longer path
        // text could dominate the row's width on resize.
        .width(Length::FillPortion(1))
        .padding(15)
        // Defense-in-depth: even with WordOrGlyph wrapping above, clip
        // anything that still doesn't fit rather than letting it render
        // outside the card's rounded border.
        .clip(true)
        .style(move |_theme| container::Style {
            background: Some(card_bg.into()),
            border: iced::Border { color: border_color, width: 1.0, radius: 12.0.into() },
            ..Default::default()
        })
        .into()
}

fn framerate_banner<'a>(state: &'a AppState, palette: &Palette) -> Option<Element<'a, Message>> {
    if let Some(err) = &state.framerate_error {
        let warning_row = row![icons::warning(palette.danger_fg), text(err.to_string()).color(palette.danger_fg)].spacing(8);
        let override_checkbox = checkbox(state.framerate_override)
            .label("I know the audio speed matches — continue anyway")
            .on_toggle(Message::FramerateOverrideToggled);
        return Some(column![warning_row, override_checkbox].spacing(6).into());
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

use crate::state::{AppState, Message, OffsetState};
use crate::theme::Palette;
use crate::ui::icons;
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Element, Length};
use mediamerger_core::offset::{Consistency, WaveformEnvelope};

fn status_banner<'a>(state: &'a AppState, palette: &Palette) -> Element<'a, Message> {
    match &state.offset {
        OffsetState::NotDetected => text("Offset not detected yet").color(palette.dim).into(),
        OffsetState::Detecting => text("Detecting offset…").color(palette.dim).into(),
        OffsetState::Detected(r) => {
            let (icon, _color, bg, headline) = match r.consistency {
                Consistency::Consistent if r.confidence < 3.0 => {
                    (icons::check(palette.success_fg), palette.success_fg, palette.success_soft, "Aligned (low confidence) — verify before merging")
                }
                Consistency::Consistent => (icons::check(palette.success_fg), palette.success_fg, palette.success_soft, "Aligned — ready to merge"),
                Consistency::Inconsistent => (icons::warning(palette.danger_fg), palette.danger_fg, palette.danger_soft, "Measurements disagree — verify manually"),
                Consistency::Unverified => (icons::warning(palette.warn_fg), palette.warn_fg, palette.warn_soft, "Unverified (file too short for a second check)"),
            };
            let detail = format!(
                "early {:.3}s · late {:.3}s · confidence {:.1}",
                r.early_offset, r.late_offset, r.confidence
            );
            container(
                row![icon, column![text(headline).color(palette.fg), text(detail).size(12).color(palette.dim)]].spacing(12),
            )
            .padding(12)
            .style(move |_theme| container::Style { background: Some(bg.into()), ..Default::default() })
            .into()
        }
        OffsetState::ManualOverride(v) => text(format!("Manual override: {v:.3}s")).color(palette.fg).into(),
    }
}

fn waveform_bars(envelope: &WaveformEnvelope, offset_secs: f64, palette: &Palette) -> Element<'static, Message> {
    let bar_row = |bars: &[f32], color: iced::Color| -> Element<'static, Message> {
        let mut r = row![].spacing(2);
        for &b in bars {
            let height = (b * 40.0).max(2.0);
            r = r.push(
                container(text(""))
                    .width(Length::Fixed(4.0))
                    .height(Length::Fixed(height))
                    .style(move |_theme| container::Style { background: Some(color.into()), ..Default::default() }),
            );
        }
        r.into()
    };

    let offset_fraction = (offset_secs / envelope.window_duration_secs).clamp(0.0, 1.0);
    let marker_label = text(format!("+{offset_secs:.3}s")).size(11).color(palette.accent_fg);

    column![
        row![text("A").size(12).color(palette.accent_fg), bar_row(&envelope.bars_a, palette.accent)].spacing(8),
        row![text("B").size(12).color(palette.dim), bar_row(&envelope.bars_b, palette.wave)].spacing(8),
        row![text(format!("offset marker at {:.0}% of window", offset_fraction * 100.0)).size(10).color(palette.faint), marker_label].spacing(8),
    ]
    .spacing(6)
    .into()
}

pub fn view<'a>(state: &'a AppState, palette: &Palette) -> Element<'a, Message> {
    let detect_offset_press = if state.framerate_error.is_some() { None } else { Some(Message::DetectOffset) };

    let mut col = column![
        status_banner(state, palette),
    ]
    .spacing(15);

    if let (Some(envelope), Some(offset)) = (&state.waveform, state.resolved_offset_secs()) {
        col = col.push(waveform_bars(envelope, offset, palette));
    }

    col = col.push(
        row![
            text("Offset").size(12).color(palette.dim),
            text_input("0.000", &state.manual_offset_input).on_input(Message::ManualOffsetChanged).width(Length::Fixed(78.0)),
            button(row![icons::sparkle(palette.accent_fg), text("Detect offset")].spacing(7)).on_press_maybe(detect_offset_press),
        ]
        .spacing(12),
    );

    if let Some(err) = &state.detect_offset_error {
        col = col.push(row![icons::warning(palette.danger_fg), text(format!("Could not detect offset: {err}")).color(palette.danger_fg)].spacing(8));
    }

    let card_bg = palette.card;
    let border_color = palette.border;

    container(col)
        .padding(16)
        .style(move |_theme| container::Style {
            background: Some(card_bg.into()),
            border: iced::Border { color: border_color, width: 1.0, radius: 12.0.into() },
            ..Default::default()
        })
        .into()
}

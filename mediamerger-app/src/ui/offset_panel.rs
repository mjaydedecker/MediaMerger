use crate::state::{confidence_quality_label, AppState, Message, OffsetState};
use crate::theme::Palette;
use crate::ui::icons;
use iced::widget::canvas;
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Element, Length};
use mediamerger_core::offset::{Consistency, WaveformEnvelope};

/// Draws two dashed vertical guide lines over the waveform bar rows: one at
/// the zero/aligned position, one at the detected offset's position.
struct WaveformGuides {
    offset_fraction: f32,
    dim_color: iced::Color,
    accent_color: iced::Color,
}

impl canvas::Program<Message> for WaveformGuides {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let dash_pattern = canvas::LineDash { segments: &[4.0, 4.0], offset: 0 };

        let stroke_with = |color: iced::Color| {
            let mut stroke = canvas::Stroke::default().with_color(color).with_width(2.0);
            stroke.line_dash = dash_pattern;
            stroke
        };

        let zero_line = canvas::Path::line(iced::Point::new(0.0, 0.0), iced::Point::new(0.0, bounds.height));
        frame.stroke(&zero_line, stroke_with(self.dim_color));

        let offset_x = bounds.width * self.offset_fraction.clamp(0.0, 1.0);
        let offset_line = canvas::Path::line(iced::Point::new(offset_x, 0.0), iced::Point::new(offset_x, bounds.height));
        frame.stroke(&offset_line, stroke_with(self.accent_color));

        vec![frame.into_geometry()]
    }
}

fn status_banner<'a>(state: &'a AppState, palette: &Palette) -> Element<'a, Message> {
    if let Some(r) = &state.last_detected {
        let (icon, color, bg, headline, detail, pill_label) = match r.consistency {
            Consistency::Consistent if r.confidence < 3.0 => (
                icons::check(palette.success_fg), palette.success_fg, palette.success_soft,
                "Aligned (low confidence) — verify before merging".to_string(),
                format!("File B's audio starts {:.3}s after File A. Its tracks will be delayed to match.", r.offset),
                "Consistent",
            ),
            Consistency::Consistent => (
                icons::check(palette.success_fg), palette.success_fg, palette.success_soft,
                "Aligned — ready to merge".to_string(),
                format!("File B's audio starts {:.3}s after File A. Its tracks will be delayed to match.", r.offset),
                "Consistent",
            ),
            Consistency::Inconsistent => (
                icons::warning(palette.danger_fg), palette.danger_fg, palette.danger_soft,
                "Measurements disagree — verify manually".to_string(),
                format!(
                    "Early and late probes differ by {:.2}s. Enter a known offset or re-run detection before merging.",
                    (r.early_offset - r.late_offset).abs()
                ),
                "Inconsistent",
            ),
            Consistency::Unverified => (
                icons::warning(palette.warn_fg), palette.warn_fg, palette.warn_soft,
                "Unverified (file too short for a second check)".to_string(),
                format!("Measured a single offset of {:.3}s - not independently confirmed.", r.offset),
                "Unverified",
            ),
        };

        let pill = container(text(pill_label).size(11).color(color))
            .padding([4, 11])
            .style(move |_theme| container::Style {
                background: None,
                border: iced::Border { color, width: 1.0, radius: 999.0.into() },
                ..Default::default()
            });

        return container(
            row![
                icon,
                column![text(headline).color(palette.fg), text(detail).size(12).color(palette.dim)].width(Length::Fill),
                pill,
            ]
            .spacing(12)
            .align_y(iced::alignment::Vertical::Center),
        )
        .padding(12)
        .style(move |_theme| container::Style { background: Some(bg.into()), ..Default::default() })
        .into();
    }

    let neutral_bg = palette.chip_bg;
    let fg = palette.fg;
    let dim = palette.dim;

    match &state.offset {
        OffsetState::ManualOverride(v) => container(
            row![
                icons::edit(dim),
                column![
                    text("Manual offset entered").color(fg),
                    text(format!(
                        "No detection has been run to verify this {v:.3}s value - merge with caution or run Detect offset first."
                    ))
                    .size(12)
                    .color(dim),
                ]
                .width(Length::Fill),
            ]
            .spacing(12)
            .align_y(iced::alignment::Vertical::Center),
        )
        .padding(12)
        .style(move |_theme| container::Style { background: Some(neutral_bg.into()), ..Default::default() })
        .into(),
        OffsetState::Detecting => text("Detecting offset…").color(palette.dim).into(),
        // NotDetected and Detected(_) share this arm: Detected(_) cannot
        // actually reach here, since last_detected is always set in
        // lockstep with every transition into OffsetState::Detected (see
        // main.rs's OffsetDetected and UseDetectedOffset handlers) - the
        // `if let Some(r) = &state.last_detected` branch above always wins
        // first whenever state.offset is genuinely Detected.
        OffsetState::NotDetected | OffsetState::Detected(_) => container(
            row![
                icons::info(dim),
                column![
                    text("Offset not detected yet").color(fg),
                    text("Run detection to measure how far File B is shifted, or type a known offset below.").size(12).color(dim),
                ]
                .width(Length::Fill),
            ]
            .spacing(12)
            .align_y(iced::alignment::Vertical::Center),
        )
        .padding(12)
        .style(move |_theme| container::Style { background: Some(neutral_bg.into()), ..Default::default() })
        .into(),
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

    let offset_fraction = (offset_secs / envelope.window_duration_secs).clamp(0.0, 1.0) as f32;
    let guides = canvas(WaveformGuides { offset_fraction, dim_color: palette.dim, accent_color: palette.accent })
        .width(Length::Fill)
        .height(Length::Fixed(88.0));

    let bars_layer = column![
        row![text("A").size(12).color(palette.accent_fg), bar_row(&envelope.bars_a, palette.accent)].spacing(8),
        row![text("B").size(12).color(palette.dim), bar_row(&envelope.bars_b, palette.wave)].spacing(8),
    ]
    .spacing(8);

    iced::widget::stack![bars_layer, guides].into()
}

pub fn view<'a>(state: &'a AppState, palette: &Palette) -> Element<'a, Message> {
    let detect_offset_press = if state.framerate_error.is_some() && !state.framerate_override { None } else { Some(Message::DetectOffset) };

    let mut col = column![
        status_banner(state, palette),
    ]
    .spacing(15);

    if let (Some(envelope), Some(offset)) = (&state.waveform, state.resolved_offset_secs()) {
        col = col.push(waveform_bars(envelope, offset, palette));
    }

    let manual_active = matches!(state.offset, OffsetState::ManualOverride(_));

    let right_of_offset_row: Option<Element<Message>> = if manual_active {
        let accent = palette.accent;
        let accent_soft = palette.accent_soft;
        let accent_fg = palette.accent_fg;
        let dim = palette.dim;
        let fg = palette.fg;

        let pill = container(row![icons::edit(accent_fg), text("Manual override").size(11).color(accent_fg)].spacing(6))
            .padding([4, 10])
            .style(move |_theme| container::Style {
                background: Some(accent_soft.into()),
                border: iced::Border { color: accent, width: 1.0, radius: 999.0.into() },
                ..Default::default()
            });

        let mut controls = row![pill].spacing(10).align_y(iced::alignment::Vertical::Center);

        if state.last_detected.is_some() {
            controls = controls.push(
                button(row![icons::undo(dim), text("Use detected").size(12).color(dim)].spacing(5))
                    .style(move |_theme, status| {
                        let base = button::Style { background: None, text_color: dim, ..Default::default() };
                        match status {
                            button::Status::Hovered => button::Style { text_color: fg, ..base },
                            _ => base,
                        }
                    })
                    .on_press(Message::UseDetectedOffset),
            );
        }

        Some(controls.into())
    } else {
        match &state.offset {
            OffsetState::Detected(r) if r.consistency != Consistency::Unverified => {
                let quality = confidence_quality_label(r.confidence);
                let color = if r.consistency == Consistency::Inconsistent { palette.danger_fg } else { palette.faint };
                Some(
                    text(format!(
                        "Measured {:.3}s early · {:.3}s late · confidence {:.1} ({quality})",
                        r.early_offset, r.late_offset, r.confidence
                    ))
                    .size(12)
                    .color(color)
                    .into(),
                )
            }
            _ => None,
        }
    };

    let view_bg = palette.view;
    let value_color = palette.fg;
    let placeholder_color = palette.faint;
    let selection_color = palette.accent_soft;
    let input_border_color = if manual_active { palette.accent } else { palette.border };

    let offset_input = text_input("0.000", &state.manual_offset_input)
        .on_input(Message::ManualOffsetChanged)
        .width(Length::Fixed(78.0))
        .style(move |_theme, _status| text_input::Style {
            background: iced::Background::Color(view_bg),
            border: iced::Border { color: input_border_color, width: 1.0, radius: 8.0.into() },
            icon: value_color,
            placeholder: placeholder_color,
            value: value_color,
            selection: selection_color,
        });

    let detect_bg = palette.accent_soft;
    let detect_fg = palette.accent_fg;
    let detect_accent = palette.accent;
    let detect_style = move |_theme: &_, status: button::Status| {
        let base = button::Style {
            background: Some(detect_bg.into()),
            text_color: detect_fg,
            border: iced::Border { color: iced::Color::TRANSPARENT, width: 1.0, radius: 8.0.into() },
            ..Default::default()
        };
        match status {
            button::Status::Hovered => button::Style { border: iced::Border { color: detect_accent, ..base.border }, ..base },
            button::Status::Disabled => button::Style {
                background: base.background.map(|b| b.scale_alpha(0.5)),
                text_color: base.text_color.scale_alpha(0.5),
                ..base
            },
            _ => base,
        }
    };

    let mut offset_row = row![
        text("Offset").size(12).color(palette.dim),
        offset_input,
        button(row![icons::sparkle(detect_fg), text("Detect offset")].spacing(7))
            .padding([8, 14])
            .style(detect_style)
            .on_press_maybe(detect_offset_press),
    ]
    .spacing(12)
    .align_y(iced::alignment::Vertical::Center);

    offset_row = offset_row.push(iced::widget::space::horizontal());
    if let Some(right_side) = right_of_offset_row {
        offset_row = offset_row.push(right_side);
    }

    col = col.push(offset_row);

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

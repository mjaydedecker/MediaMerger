use crate::state::{confidence_quality_label, AppState, Message, OffsetState};
use crate::theme::Palette;
use crate::ui::icons;
use iced::widget::canvas;
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Element, Length};
use mediamerger_core::offset::{Consistency, WaveformEnvelope};

/// Height of one waveform lane, and the gap between the A and B lanes.
const LANE_HEIGHT: f32 = 40.0;
const LANE_GAP: f32 = 8.0;
/// Height of the waveform block: two lanes plus the gap between them.
const WAVEFORM_HEIGHT: f32 = LANE_HEIGHT * 2.0 + LANE_GAP;

/// Draws both waveform lanes stretched to the available width, plus two
/// dashed vertical guide lines: one at the zero/aligned position, one at
/// the detected offset's position. Drawing bars and guides on the same
/// canvas keeps the offset line aligned with the bars at any window width.
struct Waveform<'a> {
    bars_a: &'a [f32],
    bars_b: &'a [f32],
    offset_fraction: f32,
    color_a: iced::Color,
    color_b: iced::Color,
    dim_color: iced::Color,
    accent_color: iced::Color,
}

impl canvas::Program<Message> for Waveform<'_> {
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

        // Each bar gets an equal slot across the full width; the bar fills
        // two thirds of it (the design's 4px bar + 2px gap ratio), centred
        // vertically in its lane.
        let draw_lane = |frame: &mut canvas::Frame, bars: &[f32], lane_top: f32, color: iced::Color| {
            if bars.is_empty() {
                return;
            }
            let slot = bounds.width / bars.len() as f32;
            let bar_width = (slot * 2.0 / 3.0).max(1.0);
            for (i, &b) in bars.iter().enumerate() {
                let height = (b.clamp(0.0, 1.0) * LANE_HEIGHT).max(2.0);
                let top_left = iced::Point::new(i as f32 * slot, lane_top + (LANE_HEIGHT - height) / 2.0);
                let size = iced::Size::new(bar_width, height);
                let radius = (bar_width / 2.0).min(2.0);
                frame.fill(&canvas::Path::rounded_rectangle(top_left, size, radius.into()), color);
            }
        };
        draw_lane(&mut frame, self.bars_a, 0.0, self.color_a);
        draw_lane(&mut frame, self.bars_b, LANE_HEIGHT + LANE_GAP, self.color_b);

        let dash_pattern = canvas::LineDash { segments: &[4.0, 4.0], offset: 0 };
        let stroke_with = |color: iced::Color| {
            let mut stroke = canvas::Stroke::default().with_color(color).with_width(2.0);
            stroke.line_dash = dash_pattern;
            stroke
        };

        // Inset by half the stroke width so the zero line isn't clipped.
        let zero_line = canvas::Path::line(iced::Point::new(1.0, 0.0), iced::Point::new(1.0, bounds.height));
        frame.stroke(&zero_line, stroke_with(self.dim_color));

        let offset_x = (bounds.width * self.offset_fraction.clamp(0.0, 1.0)).clamp(1.0, (bounds.width - 1.0).max(1.0));
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

fn waveform_bars<'a>(envelope: &'a WaveformEnvelope, offset_secs: f64, palette: &Palette) -> Element<'a, Message> {
    let lane_label = |label: &'static str, color: iced::Color| {
        container(text(label).size(12).color(color)).width(Length::Fixed(20.0)).height(Length::Fixed(LANE_HEIGHT)).center_x(Length::Fixed(20.0)).center_y(Length::Fixed(LANE_HEIGHT))
    };
    let labels = column![lane_label("A", palette.accent_fg), lane_label("B", palette.dim)].spacing(LANE_GAP);

    let offset_fraction = (offset_secs / envelope.window_duration_secs).clamp(0.0, 1.0) as f32;
    let waveform = canvas(Waveform {
        bars_a: &envelope.bars_a,
        bars_b: &envelope.bars_b,
        offset_fraction,
        color_a: palette.accent,
        color_b: palette.wave,
        dim_color: palette.dim,
        accent_color: palette.accent,
    })
    .width(Length::Fill)
    .height(Length::Fixed(WAVEFORM_HEIGHT));

    row![labels, waveform].spacing(12).into()
}

pub fn view<'a>(state: &'a AppState, palette: &Palette) -> Element<'a, Message> {
    let detect_offset_press = if state.framerate_error.is_some() && !state.framerate_override { None } else { Some(Message::DetectOffset) };

    let mut col = column![
        status_banner(state, palette),
    ]
    .spacing(15);

    // The waveform arrives in a second background step after the offset
    // result, so its slot is reserved from the moment detection starts.
    // Otherwise the panel would grow mid-way through the user selecting
    // tracks below it, shifting the rows under their cursor.
    let detecting = matches!(state.offset, OffsetState::Detecting);
    if let (Some(envelope), Some(offset)) = (&state.waveform, state.resolved_offset_secs()) {
        col = col.push(container(waveform_bars(envelope, offset, palette)).height(Length::Fixed(WAVEFORM_HEIGHT)));
    } else if detecting || state.waveform_pending || state.last_detected.is_some() {
        let hint = if detecting || state.waveform_pending { "Loading waveform…" } else { "Waveform preview unavailable" };
        col = col.push(
            container(text(hint).size(12).color(palette.faint))
                .width(Length::Fill)
                .height(Length::Fixed(WAVEFORM_HEIGHT))
                .center_x(Length::Fill)
                .center_y(Length::Fixed(WAVEFORM_HEIGHT)),
        );
    }

    let manual_active = matches!(state.offset, OffsetState::ManualOverride(_));

    // Borderless text button used for the "Use midpoint" / "Use detected"
    // shortcuts next to the offset row.
    let link_button = |icon: Element<'a, Message>, label: String, msg: Message| -> Element<'a, Message> {
        let dim = palette.dim;
        let fg = palette.fg;
        button(row![icon, text(label).size(12).color(dim)].spacing(5))
            .style(move |_theme, status| {
                let base = button::Style { background: None, text_color: dim, ..Default::default() };
                match status {
                    button::Status::Hovered => button::Style { text_color: fg, ..base },
                    _ => base,
                }
            })
            .on_press(msg)
            .into()
    };

    // Offered whenever it would change the current value - in practice an
    // Inconsistent result (whose detected offset is the early measurement)
    // or a manual value that differs from the midpoint.
    let midpoint_button = state
        .midpoint_offset_secs()
        .filter(|_| state.midpoint_differs_from_current())
        .map(|mid| link_button(icons::layers(palette.dim), format!("Use midpoint ({mid:.3}s)"), Message::UseMidpointOffset));

    let right_of_offset_row: Option<Element<Message>> = if manual_active {
        let accent = palette.accent;
        let accent_soft = palette.accent_soft;
        let accent_fg = palette.accent_fg;

        let pill = container(row![icons::edit(accent_fg), text("Manual override").size(11).color(accent_fg)].spacing(6))
            .padding([4, 10])
            .style(move |_theme| container::Style {
                background: Some(accent_soft.into()),
                border: iced::Border { color: accent, width: 1.0, radius: 999.0.into() },
                ..Default::default()
            });

        let mut controls = row![pill].spacing(10).align_y(iced::alignment::Vertical::Center);

        if let Some(b) = midpoint_button {
            controls = controls.push(b);
        }
        if state.last_detected.is_some() {
            controls = controls.push(link_button(icons::undo(palette.dim), "Use detected".to_string(), Message::UseDetectedOffset));
        }

        Some(controls.into())
    } else {
        match &state.offset {
            OffsetState::Detected(r) if r.consistency != Consistency::Unverified => {
                let quality = confidence_quality_label(r.confidence);
                let color = if r.consistency == Consistency::Inconsistent { palette.danger_fg } else { palette.faint };
                let mut controls = row![text(format!(
                    "Measured {:.3}s early · {:.3}s late · confidence {:.1} ({quality})",
                    r.early_offset, r.late_offset, r.confidence
                ))
                .size(12)
                .color(color)]
                .spacing(10)
                .align_y(iced::alignment::Vertical::Center);
                if let Some(b) = midpoint_button {
                    controls = controls.push(b);
                }
                Some(controls.into())
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

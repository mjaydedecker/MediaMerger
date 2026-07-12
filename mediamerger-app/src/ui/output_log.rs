use crate::state::{AppState, Message};
use crate::theme::Palette;
use crate::ui::icons;
use iced::widget::{button, column, container, row, rule, text};
use iced::{Element, Length};

pub fn view<'a>(state: &'a AppState, palette: &Palette) -> Element<'a, Message> {
    let output_label = match &state.output_path {
        Some(p) => p.display().to_string(),
        None => "No output selected".to_string(),
    };

    let blocking_reason = state.blocking_reason();
    let selected_count = state.tracks_a_ui.iter().filter(|t| t.selected).count() + state.tracks_b_ui.iter().filter(|t| t.selected).count();
    // Must mirror every condition AppState::to_merge_plan actually requires,
    // including offset_resolved and both files being loaded - omitting any
    // of these would let this show "ready to merge" (button enabled) in
    // states where to_merge_plan still returns None (e.g. no audio track
    // found, detection failed, files swapped after a prior detection, or
    // only one of file_a/file_b loaded), making Merge silently no-op.
    let offset_resolved = state.resolved_offset_secs().is_some();
    let merge_enabled = blocking_reason.is_none()
        && selected_count > 0
        && state.output_path.is_some()
        && offset_resolved
        && state.file_a.is_some()
        && state.file_b.is_some();
    let merge_press = if merge_enabled { Some(Message::StartMerge) } else { None };

    let (ready_text, ready_color) = if merge_enabled {
        (format!("{selected_count} tracks selected · ready to merge"), palette.success_fg)
    } else if let Some(reason) = &blocking_reason {
        (format!("Merge blocked: {reason}"), palette.danger_fg)
    } else if selected_count == 0 {
        ("Select at least one track".to_string(), palette.warn_fg)
    } else if !offset_resolved {
        ("Detect or enter a sync offset before merging".to_string(), palette.warn_fg)
    } else {
        ("Choose an output file".to_string(), palette.warn_fg)
    };

    let btn_bg = palette.btn_bg;
    let btn_hover = palette.btn_hover;
    let btn_text = palette.fg;
    let btn_style = move |_theme: &_, status: button::Status| {
        let base = button::Style { background: Some(btn_bg.into()), text_color: btn_text, ..Default::default() };
        match status {
            button::Status::Hovered => button::Style { background: Some(btn_hover.into()), ..base },
            // Mirror iced_widget::button's own `disabled()` helper: scale both
            // background and text alpha by 0.5 rather than rendering disabled
            // buttons (e.g. Merge, before all four merge_enabled conditions are
            // met) identically to enabled ones.
            button::Status::Disabled => button::Style {
                background: base.background.map(|b| b.scale_alpha(0.5)),
                text_color: base.text_color.scale_alpha(0.5),
                ..base
            },
            _ => base,
        }
    };

    let accent = palette.accent;
    let accent_text = palette.accent_text;
    let chip_bg = palette.chip_bg;
    let faint = palette.faint;
    let merge_btn_style = move |_theme: &_, status: button::Status| {
        let (bg, fg) = if merge_enabled { (accent, accent_text) } else { (chip_bg, faint) };
        let base = button::Style { background: Some(bg.into()), text_color: fg, border: iced::Border { radius: 999.0.into(), ..Default::default() }, ..Default::default() };
        match status {
            button::Status::Disabled => button::Style {
                background: base.background.map(|b| b.scale_alpha(0.5)),
                text_color: base.text_color.scale_alpha(0.5),
                ..base
            },
            _ => base,
        }
    };

    let output_row = column![
        text("OUTPUT FILE").size(10).color(palette.faint),
        row![
            // WordOrGlyph: an unbroken output path (no spaces) would
            // otherwise overflow the row under iced's default Word
            // wrapping, the same issue fixed for the File A/B path text in
            // file_pickers.rs.
            text(output_label)
                .size(12)
                .color(palette.fg)
                .width(Length::Fill)
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            button(row![icons::folder(palette.fg), text("Browse")].spacing(6)).style(btn_style).on_press(Message::PickOutput),
        ]
        .spacing(10)
        .align_y(iced::alignment::Vertical::Center),
    ]
    .spacing(5)
    .width(Length::Fill);

    let merge_running = state.merge_receiver.is_some();
    let new_merge_press = if merge_running { None } else { Some(Message::NewMerge) };

    let border_color = palette.border;
    let fg = palette.fg;
    let new_merge_style = move |_theme: &_, status: button::Status| {
        let base = button::Style {
            background: None,
            text_color: fg,
            border: iced::Border { color: border_color, width: 1.0, radius: 999.0.into() },
            ..Default::default()
        };
        match status {
            button::Status::Hovered => button::Style { background: Some(btn_bg.into()), ..base },
            button::Status::Disabled => button::Style {
                text_color: base.text_color.scale_alpha(0.5),
                border: iced::Border { color: base.border.color.scale_alpha(0.5), ..base.border },
                ..base
            },
            _ => base,
        }
    };

    let merge_column = column![
        text(ready_text).size(12).color(ready_color),
        row![
            button(row![icons::refresh(fg), text("New merge")].spacing(7))
                .padding([11, 20])
                .style(new_merge_style)
                .on_press_maybe(new_merge_press),
            button(row![icons::layers(if merge_enabled { accent_text } else { faint }), text("Merge")].spacing(9))
                .padding([12, 30])
                .style(merge_btn_style)
                .on_press_maybe(merge_press),
        ]
        .spacing(10)
        .align_y(iced::alignment::Vertical::Center),
    ]
    .spacing(7)
    .align_x(iced::alignment::Horizontal::Right);

    let footer_bg = palette.headerbar;
    let separator_color = palette.separator;
    let mut footer = column![row![output_row, merge_column].spacing(16)].spacing(10);

    if !state.missing_binaries.is_empty() {
        footer = footer.push(row![icons::warning(palette.danger_fg), text(format!("Missing required tools: {}", state.missing_binaries.join(", "))).color(palette.danger_fg)].spacing(8));
    }
    if let Some(p) = state.merge_progress {
        footer = footer.push(text(format!("Progress: {:.0}%", p * 100.0)).color(palette.dim));
    }
    if let Some(err) = &state.merge_error {
        footer = footer.push(text(format!("Merge failed: {err}")).color(palette.danger_fg));
    }

    let log_toggle_label = if state.log_expanded { "Hide details ▲" } else { "Show details ▼" };
    footer = footer.push(button(text(log_toggle_label).size(11).color(palette.dim)).on_press(Message::ToggleLogExpanded));

    if state.log_expanded {
        let mut log_col = column![].spacing(2);
        for line in &state.log {
            log_col = log_col.push(text(line).size(11).color(palette.faint));
        }
        let view_bg = palette.view;
        footer = footer.push(container(log_col).padding(8).style(move |_theme| container::Style { background: Some(view_bg.into()), ..Default::default() }));
    }

    // iced_core::Border has a single scalar `width` applied to all four
    // sides (see iced_core::border::Border), so it can't express the
    // mockup's border-top-only look directly. Follow the same pattern
    // track_table.rs already uses for a single dividing line: a dedicated
    // `rule::horizontal` styled with the separator color, stacked above the
    // footer content instead of a border on the outer container.
    let top_rule = rule::horizontal(1).style(move |_theme| rule::Style {
        color: separator_color,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: false,
    });

    container(column![top_rule, container(footer).padding([14, 20])])
        .style(move |_theme| container::Style { background: Some(footer_bg.into()), ..Default::default() })
        .into()
}

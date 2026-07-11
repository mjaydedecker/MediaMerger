use crate::state::{AppState, Message};
use crate::theme::Palette;
use crate::ui::icons;
use iced::widget::{button, column, container, row, text};
use iced::{Element, Length};

pub fn view<'a>(state: &'a AppState, palette: &Palette) -> Element<'a, Message> {
    let output_label = match &state.output_path {
        Some(p) => p.display().to_string(),
        None => "No output selected".to_string(),
    };

    let blocking_reason = state.blocking_reason();
    let selected_count = state.tracks_a_ui.iter().filter(|t| t.selected).count() + state.tracks_b_ui.iter().filter(|t| t.selected).count();
    // Must mirror every condition AppState::to_merge_plan actually requires,
    // including offset_resolved - omitting it would let this show "ready to
    // merge" (button enabled) in states where to_merge_plan still returns
    // None (e.g. no audio track found, detection failed, files swapped
    // after a prior detection), making Merge silently no-op.
    let offset_resolved = state.resolved_offset_secs().is_some();
    let merge_enabled = blocking_reason.is_none() && selected_count > 0 && state.output_path.is_some() && offset_resolved;
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
    let btn_style = move |_theme: &_, status: button::Status| {
        let base = button::Style { background: Some(btn_bg.into()), ..Default::default() };
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

    let mut col = column![
        row![
            text(output_label).size(12).color(palette.fg).width(Length::Fill),
            button(row![icons::folder(palette.fg), text("Browse")].spacing(6)).style(btn_style).on_press(Message::PickOutput),
        ]
        .spacing(10),
        row![
            text(ready_text).size(12).color(ready_color).width(Length::Fill),
            button("Merge").style(btn_style).on_press_maybe(merge_press),
        ]
        .spacing(10),
    ]
    .spacing(10);

    if !state.missing_binaries.is_empty() {
        col = col.push(row![icons::warning(palette.danger_fg), text(format!("Missing required tools: {}", state.missing_binaries.join(", "))).color(palette.danger_fg)].spacing(8));
    }
    if let Some(p) = state.merge_progress {
        col = col.push(text(format!("Progress: {:.0}%", p * 100.0)).color(palette.dim));
    }
    if let Some(err) = &state.merge_error {
        col = col.push(text(format!("Merge failed: {err}")).color(palette.danger_fg));
    }

    let log_toggle_label = if state.log_expanded { "Hide details ▲" } else { "Show details ▼" };
    col = col.push(button(text(log_toggle_label).size(11).color(palette.dim)).on_press(Message::ToggleLogExpanded));

    if state.log_expanded {
        let mut log_col = column![].spacing(2);
        for line in &state.log {
            log_col = log_col.push(text(line).size(11).color(palette.faint));
        }
        let view_bg = palette.view;
        col = col.push(container(log_col).padding(8).style(move |_theme| container::Style { background: Some(view_bg.into()), ..Default::default() }));
    }

    col.into()
}

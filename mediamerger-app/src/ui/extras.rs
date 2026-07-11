use crate::state::{AppState, ChaptersChoice, Message};
use crate::theme::Palette;
use iced::widget::{button, checkbox, column, container, row, text};
use iced::Element;

fn segment(label: &'static str, active: bool, on_press: Message, palette: &Palette) -> Element<'static, Message> {
    let (bg, fg) = if active { (palette.accent, palette.accent_text) } else { (iced::Color::TRANSPARENT, palette.dim) };
    button(text(label).size(12).color(fg))
        .padding([7, 16])
        .style(move |_theme, _status| iced::widget::button::Style { background: Some(bg.into()), ..Default::default() })
        .on_press(on_press)
        .into()
}

fn toggle_row<'a>(label: &'static str, sublabel: &'static str, a: bool, b: bool, on_a: impl Fn(bool) -> Message + 'a, on_b: impl Fn(bool) -> Message + 'a, palette: &Palette) -> Element<'a, Message> {
    row![
        column![text(label).size(13).color(palette.fg), text(sublabel).size(12).color(palette.faint)].width(iced::Length::Fill),
        row![text("A").size(12).color(palette.dim), checkbox(a).on_toggle(on_a)].spacing(8),
        row![text("B").size(12).color(palette.dim), checkbox(b).on_toggle(on_b)].spacing(8),
    ]
    .spacing(18)
    .padding([13, 16])
    .into()
}

pub fn view<'a>(state: &'a AppState, palette: &Palette) -> Element<'a, Message> {
    let chapters_row = row![
        text("Chapters").size(13).color(palette.fg),
        segment("File A", state.chapters_choice == ChaptersChoice::FileA, Message::ChaptersChoiceChanged(ChaptersChoice::FileA), palette),
        segment("File B", state.chapters_choice == ChaptersChoice::FileB, Message::ChaptersChoiceChanged(ChaptersChoice::FileB), palette),
        segment("None", state.chapters_choice == ChaptersChoice::None, Message::ChaptersChoiceChanged(ChaptersChoice::None), palette),
    ]
    .spacing(10)
    .padding([13, 16]);

    let attachments_row = toggle_row(
        "Attachments", "Embedded fonts and cover art",
        state.attachments_a, state.attachments_b,
        Message::ToggleAttachmentsA, Message::ToggleAttachmentsB,
        palette,
    );
    let tags_row = toggle_row(
        "Tags", "Metadata tags (title, cast, ratings)",
        state.tags_a, state.tags_b,
        Message::ToggleTagsA, Message::ToggleTagsB,
        palette,
    );

    let card_bg = palette.card;
    let border_color = palette.border;

    container(column![chapters_row, attachments_row, tags_row].spacing(0))
        .style(move |_theme| iced::widget::container::Style {
            background: Some(card_bg.into()),
            border: iced::Border { color: border_color, width: 1.0, radius: 12.0.into() },
            ..Default::default()
        })
        .into()
}

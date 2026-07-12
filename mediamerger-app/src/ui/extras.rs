use crate::state::{AppState, ChaptersChoice, Message};
use crate::theme::Palette;
use iced::widget::{button, checkbox, column, container, row, rule, text};
use iced::{Element, Length};

fn segment(label: &'static str, active: bool, on_press: Message, palette: &Palette) -> Element<'static, Message> {
    let (bg, fg) = if active { (palette.accent, palette.accent_text) } else { (iced::Color::TRANSPARENT, palette.dim) };
    button(text(label).size(12).color(fg))
        .padding([7, 16])
        .style(move |_theme, _status| iced::widget::button::Style { background: Some(bg.into()), text_color: fg, ..Default::default() })
        .on_press(on_press)
        .into()
}

fn segment_divider(color: iced::Color) -> Element<'static, Message> {
    rule::vertical(1)
        .style(move |_theme| rule::Style { color, radius: 0.0.into(), fill_mode: rule::FillMode::Full, snap: false })
        .into()
}

fn row_divider(color: iced::Color) -> Element<'static, Message> {
    rule::horizontal(1)
        .style(move |_theme| rule::Style { color, radius: 0.0.into(), fill_mode: rule::FillMode::Full, snap: false })
        .into()
}

fn toggle_row<'a>(label: &'static str, sublabel: &'static str, a: bool, b: bool, on_a: impl Fn(bool) -> Message + 'a, on_b: impl Fn(bool) -> Message + 'a, palette: &Palette) -> Element<'a, Message> {
    row![
        column![text(label).size(13).color(palette.fg), text(sublabel).size(12).color(palette.faint)].width(Length::Fill),
        row![text("A").size(12).color(palette.dim), checkbox(a).on_toggle(on_a)].spacing(8),
        row![text("B").size(12).color(palette.dim), checkbox(b).on_toggle(on_b)].spacing(8),
    ]
    .spacing(18)
    .padding([13, 16])
    .into()
}

pub fn view<'a>(state: &'a AppState, palette: &Palette) -> Element<'a, Message> {
    let border_color = palette.border;
    let separator_color = palette.separator;

    let chapter_group = container(
        row![
            segment("File A", state.chapters_choice == ChaptersChoice::FileA, Message::ChaptersChoiceChanged(ChaptersChoice::FileA), palette),
            segment_divider(border_color),
            segment("File B", state.chapters_choice == ChaptersChoice::FileB, Message::ChaptersChoiceChanged(ChaptersChoice::FileB), palette),
            segment_divider(border_color),
            segment("None", state.chapters_choice == ChaptersChoice::None, Message::ChaptersChoiceChanged(ChaptersChoice::None), palette),
        ],
    )
    .style(move |_theme| container::Style {
        border: iced::Border { color: border_color, width: 1.0, radius: 8.0.into() },
        ..Default::default()
    });

    let chapters_row = row![
        column![
            text("Chapters").size(13).color(palette.fg),
            text("Which file's chapter markers to keep").size(12).color(palette.faint),
        ]
        .width(Length::Fill),
        chapter_group,
    ]
    .spacing(14)
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
    let card_border = palette.border;

    container(
        column![chapters_row, row_divider(separator_color), attachments_row, row_divider(separator_color), tags_row].spacing(0),
    )
    .style(move |_theme| iced::widget::container::Style {
        background: Some(card_bg.into()),
        border: iced::Border { color: card_border, width: 1.0, radius: 12.0.into() },
        ..Default::default()
    })
    .into()
}

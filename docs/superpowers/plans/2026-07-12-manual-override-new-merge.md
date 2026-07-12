# Manual Override & New Merge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a "Manual override" indicator (pill + "Use detected" reset) to the offset panel, a "New merge" session-reset button next to Merge, and fix two pre-existing fidelity gaps (the "not detected yet" banner, the track table's empty-state placeholder) found while re-reading the mockup for this work.

**Architecture:** Add `AppState.last_detected: Option<OffsetResult>` (retains the last successful detection even after a manual override) and two new `Message` variants (`UseDetectedOffset`, `NewMerge`), wired in `main.rs`. The offset panel's banner is restructured to key off `last_detected` rather than `state.offset` directly, so it survives a manual override. `NewMerge` resets `AppState` to its default, preserving only environment-derived fields, and is disabled while a merge is running.

**Tech Stack:** Rust, `iced` 0.14 (`mediamerger-app`), `mediamerger-core` (unchanged by this plan).

## Global Constraints

- `AppState::blocking_reason`/`to_merge_plan`/`resolved_offset_secs` are unaffected by this plan — `last_detected` is additional display/restore context only, never consulted by merge-readiness gating.
- Any new `Message` enum variant added to `state.rs` immediately breaks `main.rs`'s exhaustive `match message { ... }` — a prior plan in this project (`2026-07-11-framerate-override.md`) discovered this the hard way when state and wiring were split across separate tasks, producing a non-compiling intermediate commit. This plan avoids that by putting the `state.rs` enum/field additions and their `main.rs` handling in the **same task** (Task 2).
- Closures capturing `Color`/palette-derived values in `move |...|` styling callbacks must capture local `let`-bound copies, not read `palette.field` directly inside the closure (this project's established lifetime pattern, needed because `Element<'a, Message>`-returning functions can't have a closure borrow through a `&Palette` parameter).
- Verify any `iced` widget API against the actually-installed crate source (`~/.cargo/registry/src/*/iced_widget-0.14.2/`, `~/.cargo/registry/src/*/iced_core-0.14.0/`) before relying on its exact signature where this plan says a sketch needs verification; where this plan gives exact code without that caveat, it has already been verified against the installed source during planning.
- `iced_core::Border` (`background`/`border` fields on `container::Style`) supports only a solid line (`color`, `width`, `radius` — no dash pattern); a mockup detail calling for a *dashed* border is approximated with a solid one, consistent with this project's established practice of documenting approximations rather than reaching for `Canvas` over minor cosmetic gaps.

---

### Task 1: New icons

**Files:**
- Create: `mediamerger-app/assets/icons/info.svg`
- Create: `mediamerger-app/assets/icons/edit.svg`
- Create: `mediamerger-app/assets/icons/undo.svg`
- Create: `mediamerger-app/assets/icons/refresh.svg`
- Modify: `mediamerger-app/src/ui/icons.rs`

**Interfaces:**
- Produces: `icons::info(color)`, `icons::edit(color)`, `icons::undo(color)`, `icons::refresh(color)` — each `fn(Color) -> Element<'static, Message>`, consumed by Tasks 3 and 4.

- [ ] **Step 1: Add the four SVG assets**

Exact path data copied verbatim from the mockup's inline SVGs
(`design_files/MediaMerger Design Help-v1.0.2-handoff.zip`), following this
project's existing icon-asset convention (`fill="none" stroke="black"`,
recolored at runtime via `svg::Style`):

`mediamerger-app/assets/icons/info.svg`:
```xml
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="M12 11v5"/><path d="M12 7.6v.2"/></svg>
```

`mediamerger-app/assets/icons/edit.svg`:
```xml
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M4 20l4.5-1L19 8.5 15.5 5 5 15.5z"/><path d="M13.5 7l3.5 3.5"/></svg>
```

`mediamerger-app/assets/icons/undo.svg`:
```xml
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M3.5 8a9 9 0 1 1-1 5"/><path d="M3.5 3.5V8H8"/></svg>
```

`mediamerger-app/assets/icons/refresh.svg`:
```xml
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M20 11a8 8 0 1 0-2.4 5.7"/><path d="M20 4v5h-5"/></svg>
```

- [ ] **Step 2: Add the four icon functions**

Add to `mediamerger-app/src/ui/icons.rs`, following the exact existing
pattern (e.g. `pub fn layers`):

```rust
pub fn info(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/info.svg"), color)
}

pub fn edit(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/edit.svg"), color)
}

pub fn undo(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/undo.svg"), color)
}

pub fn refresh(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/refresh.svg"), color)
}
```

- [ ] **Step 3: Build**

Run: `cargo build --workspace`
Expected: zero errors (no new testable logic — these are static asset
functions, consistent with this project's existing icon functions, none of
which are unit tested).

- [ ] **Step 4: Commit**

```bash
git add mediamerger-app/assets/icons/info.svg mediamerger-app/assets/icons/edit.svg mediamerger-app/assets/icons/undo.svg mediamerger-app/assets/icons/refresh.svg mediamerger-app/src/ui/icons.rs
git commit -m "Add info, edit, undo, and refresh icons"
```

---

### Task 2: State additions and `main.rs` wiring

**Files:**
- Modify: `mediamerger-app/src/state.rs`
- Modify: `mediamerger-app/src/main.rs`

**Interfaces:**
- Produces: `AppState.last_detected: Option<OffsetResult>`, `Message::UseDetectedOffset`, `Message::NewMerge` — all consumed by Tasks 3 and 4.
- Consumes: nothing new from other tasks.

This task combines the state shape and its `main.rs` handling in one task
specifically to avoid a non-compiling intermediate commit (see this plan's
Global Constraints) — do not split it further.

- [ ] **Step 1: Add `last_detected` to `AppState`**

In `mediamerger-app/src/state.rs`, add the field right after `pub offset: OffsetState,` (line 65):

```rust
    pub offset: OffsetState,
    pub last_detected: Option<OffsetResult>,
```

And in `Default for AppState`, right after `offset: OffsetState::NotDetected,` (line 96):

```rust
            offset: OffsetState::NotDetected,
            last_detected: None,
```

- [ ] **Step 2: Add the two new `Message` variants**

Add `UseDetectedOffset` right after `ManualOffsetChanged(String),` (line 136):

```rust
    ManualOffsetChanged(String),
    UseDetectedOffset,
```

Add `NewMerge` right after `ToggleLogExpanded,` at the end of the enum (line 147):

```rust
    ToggleLogExpanded,
    NewMerge,
```

- [ ] **Step 3: Wire `OffsetDetected` to populate `last_detected`**

In `mediamerger-app/src/main.rs`'s `Message::OffsetDetected(result)` handler,
in the `Ok(r) => { ... }` arm (line 210), add the assignment right after the
existing `manual_offset_input` line so it's set exactly once regardless of
which of the arm's several early-return paths fires next:

```rust
            Ok(r) => {
                state.manual_offset_input = format!("{:.3}", r.offset);
                state.last_detected = Some(r.clone());
                let (file_a, file_b) = (state.file_a.clone(), state.file_b.clone());
```

- [ ] **Step 4: Add the `UseDetectedOffset` handler**

Add a new match arm in `main.rs`'s `update` function, right after the
`Message::ManualOffsetChanged(text) => { ... }` arm (after line 257):

```rust
        Message::UseDetectedOffset => {
            if let Some(r) = state.last_detected.clone() {
                state.manual_offset_input = format!("{:.3}", r.offset);
                state.offset = state::OffsetState::Detected(r);
            }
            Task::none()
        }
```

- [ ] **Step 5: Add the `NewMerge` handler**

Add a new match arm right after the `Message::ToggleLogExpanded => { ... }`
arm (main.rs's last arm before this plan, around line 374-377):

```rust
        Message::NewMerge => {
            // Disabled while a merge is running (mirrors the merge_receiver
            // guard StartMerge already uses) - resetting file_a/file_b/log
            // etc. while the background worker thread is still sending
            // MergeEventReceived events would let those events repopulate
            // the just-reset UI mid-reset.
            if state.merge_receiver.is_some() {
                return Task::none();
            }
            let is_dark = state.is_dark;
            let accent_hex = state.accent_hex.clone();
            let missing_binaries = state.missing_binaries.clone();
            *state = AppState::default();
            state.is_dark = is_dark;
            state.accent_hex = accent_hex;
            state.missing_binaries = missing_binaries;
            Task::none()
        }
```

- [ ] **Step 6: Write the failing tests**

Add to `main.rs`'s existing `#[cfg(test)] mod tests` block (uses the
existing `track`/`media_file` helpers already defined there):

```rust
    #[test]
    fn use_detected_offset_restores_last_detected_after_manual_override() {
        let mut state = AppState::default();
        let detected = mediamerger_core::offset::OffsetResult {
            early_offset: 2.34,
            late_offset: 2.36,
            consistency: mediamerger_core::offset::Consistency::Consistent,
            confidence: 8.0,
            offset: 2.35,
            early_window_start: 0.0,
            window_duration: 180.0,
        };
        state.last_detected = Some(detected);
        state.offset = state::OffsetState::ManualOverride(9.999);
        state.manual_offset_input = "9.999".to_string();

        update(&mut state, Message::UseDetectedOffset);

        match state.offset {
            state::OffsetState::Detected(r) => assert_eq!(r.offset, 2.35),
            _ => panic!("expected offset to be restored to Detected"),
        }
        assert_eq!(state.manual_offset_input, "2.350");
    }

    #[test]
    fn use_detected_offset_is_noop_when_nothing_was_ever_detected() {
        let mut state = AppState::default();
        state.offset = state::OffsetState::ManualOverride(1.0);
        state.manual_offset_input = "1.000".to_string();

        update(&mut state, Message::UseDetectedOffset);

        match state.offset {
            state::OffsetState::ManualOverride(v) => assert_eq!(v, 1.0),
            _ => panic!("expected offset to remain ManualOverride when last_detected is None"),
        }
    }

    #[test]
    fn new_merge_resets_state_but_preserves_environment_fields() {
        let mut state = AppState::default();
        state.is_dark = true;
        state.accent_hex = "#123456".to_string();
        state.missing_binaries = vec!["ffmpeg"];
        state.file_a = Some(media_file("a.mkv"));
        state.output_path = Some(PathBuf::from("out.mkv"));
        state.attachments_a = false;
        state.offset = state::OffsetState::ManualOverride(5.0);

        update(&mut state, Message::NewMerge);

        assert!(state.file_a.is_none());
        assert!(state.output_path.is_none());
        assert!(state.attachments_a, "attachments_a should reset to its default (true)");
        assert!(matches!(state.offset, state::OffsetState::NotDetected));
        assert!(state.is_dark, "is_dark must survive the reset");
        assert_eq!(state.accent_hex, "#123456");
        assert_eq!(state.missing_binaries, vec!["ffmpeg"]);
    }

    #[test]
    fn new_merge_is_noop_while_a_merge_is_running() {
        let mut state = AppState::default();
        state.file_a = Some(media_file("a.mkv"));
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel::<state::MuxUiEvent>();
        state.merge_receiver = Some(std::sync::Arc::new(tokio::sync::Mutex::new(rx)));

        update(&mut state, Message::NewMerge);

        assert!(state.file_a.is_some(), "state must not reset while a merge is in flight");
    }
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test -p mediamerger-app`
Expected: all passing, including the 4 new tests above.

Run: `cargo test --workspace`
Expected: all passing, no regressions.

- [ ] **Step 8: Commit**

```bash
git add mediamerger-app/src/state.rs mediamerger-app/src/main.rs
git commit -m "Add last_detected state and UseDetectedOffset/NewMerge messages"
```

---

### Task 3: Offset panel — banner decoupling, manual-override pill, accent-bordered input

**Files:**
- Modify: `mediamerger-app/src/ui/offset_panel.rs`

**Interfaces:**
- Consumes: `AppState.last_detected` (Task 2), `Message::UseDetectedOffset` (Task 2), `icons::edit`/`icons::undo`/`icons::info` (Task 1).
- No signature change to `pub fn view`.

- [ ] **Step 1: Rewrite `status_banner` to key off `last_detected`**

Replace the whole `status_banner` function:

```rust
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
```

- [ ] **Step 2: Replace the measured-text logic with the manual-override pill / Use Detected button, and give the offset input an accent border while overriding**

Replace everything from `let measured_text: Option<Element<Message>> = ...`
through `col = col.push(offset_row);` (the current `pub fn view`'s middle
section) with:

```rust
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

    let mut offset_row = row![
        text("Offset").size(12).color(palette.dim),
        offset_input,
        button(row![icons::sparkle(palette.accent_fg), text("Detect offset")].spacing(7)).on_press_maybe(detect_offset_press),
    ]
    .spacing(12);

    offset_row = offset_row.push(iced::widget::space::horizontal());
    if let Some(right_side) = right_of_offset_row {
        offset_row = offset_row.push(right_side);
    }

    col = col.push(offset_row);
```

`text_input::Style`'s exact field set (`background: Background`, `border: Border`, `icon: Color`, `placeholder: Color`, `value: Color`, `selection: Color`) and its `.style(impl Fn(&Theme, Status) -> Style)` signature were confirmed against the installed `iced_widget-0.14.2/src/text_input.rs` during planning — this is new API surface for this codebase (no existing `text_input` in this project has a `.style()` override), but the sketch above is exact, not a guess. No import change is needed: `iced_widget::text_input` is both a module (`pub struct Style`, `pub enum Status`) and a free function (`pub fn text_input(...)`) re-exported under the same name — confirmed against `iced_widget-0.14.2/src/lib.rs:41` and `helpers.rs:1444` — exactly the same duality this codebase already relies on for `button::Style`/`button::Status` alongside the `button(...)` function in `file_pickers.rs`, `output_log.rs`, and `track_table.rs`. The existing `use iced::widget::{..., text_input};` line already brings in both.

- [ ] **Step 3: Build and test**

Run: `cargo build --workspace`
Expected: zero errors.

Run: `cargo test --workspace`
Expected: all passing (no new testable logic in this task — pure view code).

- [ ] **Step 4: Commit**

```bash
git add mediamerger-app/src/ui/offset_panel.rs
git commit -m "Decouple offset banner from manual override; add override pill and Use Detected"
```

---

### Task 4: New Merge button

**Files:**
- Modify: `mediamerger-app/src/ui/output_log.rs`

**Interfaces:**
- Consumes: `Message::NewMerge` (Task 2), `icons::refresh` (Task 1), `state.merge_receiver` (existing).
- No signature change to `pub fn view`.

- [ ] **Step 1: Add the New Merge button next to Merge**

Replace the `merge_column` construction in `pub fn view`:

```rust
    let merge_running = state.merge_receiver.is_some();
    let new_merge_press = if merge_running { None } else { Some(Message::NewMerge) };

    let border_color = palette.border;
    let fg = palette.fg;
    let btn_bg = palette.btn_bg;
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
        .spacing(10),
    ]
    .spacing(7)
    .align_x(iced::alignment::Horizontal::Right);
```

Note `border_color` and `btn_bg` shadow no existing bindings in this function
(verify against the current file — `btn_bg`/`btn_hover` are already bound
earlier in `view` for the Browse button's style; reuse the existing `btn_bg`
binding rather than re-declaring it if it's already in scope at this point
in the function, to avoid an "unused variable" or shadow warning).

- [ ] **Step 2: Build and test**

Run: `cargo build --workspace`
Expected: zero errors.

Run: `cargo test --workspace`
Expected: all passing (no new testable logic — pure view code; `NewMerge`'s
actual reset behavior is already tested in Task 2).

- [ ] **Step 3: Commit**

```bash
git add mediamerger-app/src/ui/output_log.rs
git commit -m "Add New Merge button next to Merge in the footer"
```

---

### Task 5: Empty-state placeholders

**Files:**
- Modify: `mediamerger-app/src/ui/file_pickers.rs`
- Modify: `mediamerger-app/src/ui/track_table.rs`

**Interfaces:** none new — pure styling changes to existing `None`-file branches.

- [ ] **Step 1: File A/B card placeholder**

In `mediamerger-app/src/ui/file_pickers.rs`'s `file_card`, replace the
`path_text` construction and the row that renders it, so the `None` case
gets a distinct placeholder box instead of a plain "No file selected" path
row. Replace from `let path_text = match file { ... };` through the closing
of the `row![icons::video(...), text(path_text)...]` row (currently ending
with `.spacing(8),` before the outer `column!`'s closing `]`):

```rust
    let browse_press = if picking { None } else { Some(on_browse) };

    let btn_bg = palette.btn_bg;
    let btn_hover = palette.btn_hover;
    let mut card = column![
        row![
            text(label).size(13).color(palette.fg),
            button(row![icons::folder(palette.fg), text("Browse")].spacing(6))
                .style(move |_theme, status| {
                    let base = button::Style { background: Some(btn_bg.into()), ..Default::default() };
                    match status {
                        button::Status::Hovered => {
                            button::Style { background: Some(btn_hover.into()), ..base }
                        }
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
```

This removes the prior unconditional `if let Some(f) = file { card = card.push(file_chips(f, palette)); }` block later in the function (now folded into the `Some(f)` arm above) — delete that now-duplicate block.

- [ ] **Step 2: Track table empty-state placeholder**

In `mediamerger-app/src/ui/track_table.rs`'s `file_column`, replace the
`None` arm:

```rust
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
```

This gives the empty state the same card background/border the loaded
state already has (previously missing entirely), matching the mockup's
model of an unconditional card wrapper with only the inner content varying.

- [ ] **Step 3: Build and test**

Run: `cargo build --workspace`
Expected: zero errors.

Run: `cargo test --workspace`
Expected: all passing.

- [ ] **Step 4: Commit**

```bash
git add mediamerger-app/src/ui/file_pickers.rs mediamerger-app/src/ui/track_table.rs
git commit -m "Add distinct empty-state placeholders to file cards and track table"
```

---

### Task 6: Final workspace verification

**Files:** none (verification only)

- [ ] **Step 1: Full workspace build, test, and lint**

Run: `cargo build --workspace`
Expected: clean build.

Run: `cargo test --workspace`
Expected: all tests pass, including the 4 new tests added in Task 2.

Run: `cargo clippy --workspace --all-targets`
Expected: no new warnings beyond this project's known pre-existing
categories (elided-lifetime warnings on `ui/*.rs`/`main.rs` view function
signatures, `field_reassign_with_default` in `state.rs`/`main.rs` test
code).

- [ ] **Step 2: Commit if any fixup was needed**

```bash
git add -A
git commit -m "Fix workspace build/lint issues found in manual-override/new-merge verification"
```

---

## Manual verification (requires a real GNOME desktop; not available in this sandbox)

1. Detect an offset, then type a different manual value: confirm the top
   banner keeps showing the detected result's aligned/inconsistent status,
   while the measured-text line is replaced by the "Manual override" pill
   and "Use detected" button, and the offset input's border turns accent
   color.
2. Click "Use detected": confirm the offset input reverts to the detected
   value, the border returns to normal, and the manual-override pill
   disappears.
3. Type a manual offset with no prior detection run: confirm a simpler
   "Manual offset entered" message appears instead of a consistency-pill
   banner, and no "Use detected" button appears.
4. Click "New merge": confirm files, tracks, offset, output path, and
   extras all reset to their defaults, and the File A/B cards + track
   tables show the new empty-state placeholders.
5. Start a merge, then attempt "New merge" mid-merge: confirm the button is
   disabled until the merge finishes or fails.
6. Confirm the "Offset not detected yet" box (icon/headline/detail) renders
   correctly on a freshly-loaded file pair before any detection has run.

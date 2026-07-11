# MediaMerger Redesign Fidelity Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the gap between the shipped visual redesign and the Claude Design mockup: missing section headers, wrong section order, missing file duration, offset-panel content/layout mismatches, and a non-distinct footer.

**Architecture:** No new crates. One new small module (`mediamerger-app/src/ui/section_header.rs`) for the shared numbered-badge/title/subtitle header used by every section; one corrected mistake from the prior round (`Palette.headerbar`, removed too eagerly, needed back for the footer); everything else is targeted changes to existing files.

**Tech Stack:** Same as the existing app. One new `iced` widget usage this round: `iced::widget::canvas` (for the waveform's dashed guide-line overlay) and `iced::widget::stack` (to layer that overlay on top of the existing bar rows) — both already part of `iced` 0.14, no new dependency.

## Global Constraints

- No custom window chrome, no in-app accent picker, no estimated bitrate — unchanged from the original redesign. (spec: Non-goals)
- `MediaFile.duration_secs` comes from mkvmerge's own `-J` output (`container.properties.duration`, nanoseconds) — no second `ffprobe` call. (spec: gap 3)
- The confidence "(high)"/"(low)" quality label reuses the exact same `3.0` threshold already established in `offset_panel.rs`'s low-confidence banner check — do not introduce a second, different threshold. (spec: Testing strategy)
- Section order must be File Pickers → Offset Panel → Track Table → Extras → Output. (spec: gap 2)

---

## Task 1: `probe` module — file duration

**Files:**
- Modify: `mediamerger-core/src/probe.rs`
- Modify: `mediamerger-app/src/state.rs` (the `media_file()` test helper constructs a `MediaFile` literal directly and must be updated to keep compiling)

**Interfaces:**
- Consumes: existing `MkvmergeJson`/`MkvmergeContainer`/`parse_mkvmerge_json` (already in `probe.rs`)
- Produces: `MediaFile` gains `duration_secs: Option<f64>`.

- [ ] **Step 1: Write the failing tests**

Update the existing `parses_video_audio_subtitle_tracks` fixture and add a new test in `mediamerger-core/src/probe.rs`'s `tests` module:

```rust
    #[test]
    fn parses_video_audio_subtitle_tracks() {
        let json = br#"{
            "container": {"type": "Matroska", "properties": {"duration": 5072000000000}},
            "tracks": [
                {"id":0,"type":"video","codec":"MPEG-4p10/AVC/h.264","properties":{"default_track":true,"forced_track":false,"default_duration":41708333,"pixel_dimensions":"3840x2160","color_transfer_characteristics":16,"block_addition_mappings":[{"id_type":4}]}},
                {"id":1,"type":"audio","codec":"AC-3","properties":{"default_track":true,"forced_track":false,"language":"eng","audio_channels":6,"audio_sampling_frequency":48000,"audio_bits_per_sample":16,"tag_bps":"640000"}},
                {"id":2,"type":"subtitles","codec":"SubRip/SRT","properties":{"default_track":false,"forced_track":false,"language":"fre","track_name":"Forced"}}
            ]
        }"#;

        let media = parse_mkvmerge_json(json, Path::new("test.mkv")).unwrap();

        assert_eq!(media.container, "Matroska");
        assert!((media.duration_secs.unwrap() - 5072.0).abs() < 0.001, "got {:?}", media.duration_secs);
        assert_eq!(media.tracks.len(), 3);

        assert_eq!(media.tracks[0].kind, TrackKind::Video);
        assert!((media.tracks[0].fps.unwrap() - 23.976).abs() < 0.01);
        assert_eq!(media.tracks[0].width, Some(3840));
        assert_eq!(media.tracks[0].height, Some(2160));
        assert!(media.tracks[0].is_hdr10, "transfer characteristic 16 (PQ) should be detected as HDR10");
        assert!(media.tracks[0].is_dolby_vision, "block addition id_type 4 should be detected as Dolby Vision");

        assert_eq!(media.tracks[1].kind, TrackKind::Audio);
        assert_eq!(media.tracks[1].channels, Some(6));
        assert_eq!(media.tracks[1].language.as_deref(), Some("eng"));
        assert_eq!(media.tracks[1].sampling_rate, Some(48000));
        assert_eq!(media.tracks[1].bits_per_sample, Some(16));
        assert_eq!(media.tracks[1].bitrate_bps, Some(640000));

        assert_eq!(media.tracks[2].kind, TrackKind::Subtitle);
        assert_eq!(media.tracks[2].language.as_deref(), Some("fre"));
        assert_eq!(media.tracks[2].name.as_deref(), Some("Forced"));
        assert_eq!(media.tracks[2].width, None);
        assert!(!media.tracks[2].is_hdr10);
        assert!(!media.tracks[2].is_dolby_vision);
    }

    #[test]
    fn missing_container_duration_yields_none() {
        let json = br#"{
            "container": {"type": "Matroska"},
            "tracks": [
                {"id":0,"type":"video","codec":"AV1","properties":{"default_track":false,"forced_track":false}}
            ]
        }"#;

        let media = parse_mkvmerge_json(json, Path::new("test.mkv")).unwrap();

        assert_eq!(media.duration_secs, None);
    }
```

Note: `missing_optional_properties_yield_none_not_a_parse_error` (the existing test with a bare `{"container": {"type": "Matroska"}, ...}` fixture with no `properties` key on the container at all) already exercises the "container has no properties object" path once `MkvmergeContainer.properties` becomes `Option`-typed below — no changes needed to that test itself.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mediamerger-core probe::tests`
Expected: FAIL — compile error, `duration_secs` doesn't exist on `MediaFile` yet, and the fixture JSON's new `container.properties` key isn't handled.

- [ ] **Step 3: Implement**

Add `duration_secs: Option<f64>` to `MediaFile` in `mediamerger-core/src/probe.rs`:

```rust
#[derive(Debug, Clone)]
pub struct MediaFile {
    pub path: PathBuf,
    pub container: String,
    pub tracks: Vec<Track>,
    pub file_size_bytes: u64,
    pub duration_secs: Option<f64>,
}
```

Update `MkvmergeContainer` to optionally carry a `properties.duration` (nanoseconds):

```rust
#[derive(Deserialize)]
struct MkvmergeContainer {
    #[serde(rename = "type")]
    kind: String,
    properties: Option<MkvmergeContainerProperties>,
}

#[derive(Deserialize, Default)]
struct MkvmergeContainerProperties {
    duration: Option<u64>,
}
```

Update `parse_mkvmerge_json`'s final `Ok(MediaFile { ... })` construction:

```rust
    let file_size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let duration_secs = parsed
        .container
        .properties
        .and_then(|p| p.duration)
        .map(|ns| ns as f64 / 1_000_000_000.0);

    Ok(MediaFile {
        path: path.to_path_buf(),
        container: parsed.container.kind,
        tracks,
        file_size_bytes,
        duration_secs,
    })
```

(`parsed.container.kind` must be read before `parsed.container.properties` is moved out via `and_then` — order the two lines so `kind` isn't borrowed-after-move; the snippet above already does this correctly by capturing `duration_secs` before constructing the final `MediaFile`, which reads `parsed.container.kind` afterward. If you reorder this, make sure `parsed.container.properties` is taken by value exactly once.)

- [ ] **Step 4: Fix the companion `MediaFile` literal in `mediamerger-app/src/state.rs`**

Add `duration_secs: None,` to the `media_file()` test helper's `MediaFile { ... }` literal in `mediamerger-app/src/state.rs`'s `tests` module.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p mediamerger-core probe::tests` then `cargo test --workspace`
Expected: PASS (all core and app tests, no regressions).

- [ ] **Step 6: Commit**

```bash
git add mediamerger-core/src/probe.rs mediamerger-app/src/state.rs
git commit -m "Add file duration, parsed from mkvmerge's own container properties"
```

---

## Task 2: App-layer pure formatting helpers

**Files:**
- Modify: `mediamerger-app/src/state.rs`

**Interfaces:**
- Produces: `pub fn format_duration(secs: f64) -> String` (e.g. `5072.0` → `"1:24:32"`), `pub fn confidence_quality_label(confidence: f32) -> &'static str` (`"high"` at/above the existing `3.0` threshold, `"low"` below it).

These are small, pure, UI-facing utilities used by later tasks (`file_pickers.rs`'s duration chip, `offset_panel.rs`'s measured-text line). Placed in `state.rs` alongside the existing `parse_accent_name` — the one other pure formatting/parsing helper already living there — rather than in a view file, consistent with this project's practice of keeping `ui/*.rs` files test-free and putting testable logic in `state.rs`.

- [ ] **Step 1: Write the failing tests**

Add to `mediamerger-app/src/state.rs`'s `tests` module:

```rust
    #[test]
    fn format_duration_formats_hours_minutes_seconds() {
        assert_eq!(format_duration(5072.0), "1:24:32");
    }

    #[test]
    fn format_duration_pads_minutes_and_seconds() {
        assert_eq!(format_duration(65.0), "0:01:05");
    }

    #[test]
    fn format_duration_handles_over_ten_hours() {
        assert_eq!(format_duration(37800.0), "10:30:00");
    }

    #[test]
    fn confidence_quality_label_matches_existing_low_confidence_threshold() {
        assert_eq!(confidence_quality_label(8.4), "high");
        assert_eq!(confidence_quality_label(3.0), "high");
        assert_eq!(confidence_quality_label(2.99), "low");
        assert_eq!(confidence_quality_label(2.1), "low");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mediamerger-app format_duration`
Expected: FAIL — functions don't exist yet.

- [ ] **Step 3: Implement**

Add to `mediamerger-app/src/state.rs` (near `parse_accent_name`):

```rust
pub fn format_duration(secs: f64) -> String {
    let total_secs = secs.round() as u64;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    format!("{hours}:{minutes:02}:{seconds:02}")
}

/// Mirrors the `< 3.0` threshold `offset_panel.rs` already uses to flag a
/// "consistent but low confidence" result - keep both in sync if this ever
/// changes; it must not become a second, different threshold.
pub fn confidence_quality_label(confidence: f32) -> &'static str {
    if confidence >= 3.0 { "high" } else { "low" }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mediamerger-app format_duration confidence_quality_label`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add mediamerger-app/src/state.rs
git commit -m "Add format_duration and confidence_quality_label helpers"
```

---

## Task 3: `theme.rs` — restore `Palette.headerbar`

**Files:**
- Modify: `mediamerger-app/src/theme.rs`

**Interfaces:**
- Produces: `Palette` gains back `headerbar: Color`.

The prior redesign round removed this field, reasoning it only mattered for
the custom window-chrome titlebar this redesign still doesn't build. That
reasoning was incomplete: the mockup's *footer* also uses `c.headerbar` as
its background, which this plan's Task 9 needs. Re-add it with its original
mockup values (`#2e2e2e` dark, `#ffffff` light — identical to `card`/`view`
in light mode, which matches the mockup exactly).

- [ ] **Step 1: Write the failing test**

Add to `mediamerger-app/src/theme.rs`'s `tests` module:

```rust
    #[test]
    fn dark_palette_headerbar_matches_mockup() {
        let p = build(true, "#3584e4");
        assert_eq!(p.headerbar, Color::from_rgb8(0x2e, 0x2e, 0x2e));
    }

    #[test]
    fn light_palette_headerbar_matches_mockup() {
        let p = build(false, "#3584e4");
        assert_eq!(p.headerbar, Color::from_rgb8(0xff, 0xff, 0xff));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mediamerger-app theme::tests`
Expected: FAIL — compile error, `headerbar` doesn't exist on `Palette` yet.

- [ ] **Step 3: Add the field back**

In `mediamerger-app/src/theme.rs`, add `pub headerbar: Color,` to the `Palette` struct, and `headerbar: rgba("#2e2e2e", 1.0),` to the dark branch / `headerbar: rgba("#ffffff", 1.0),` to the light branch of `build`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mediamerger-app theme::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add mediamerger-app/src/theme.rs
git commit -m "Restore Palette.headerbar, needed for the footer background"
```

---

## Task 4: New icon — merge/layers glyph

**Files:**
- Create: `mediamerger-app/assets/icons/layers.svg`
- Modify: `mediamerger-app/src/ui/icons.rs`

**Interfaces:**
- Produces: `pub fn layers(color: Color) -> Element<'static, Message>` in `ui::icons`.

The mockup uses this glyph twice: once on the custom titlebar's app icon
(which this redesign doesn't build) and once on the Merge button (which
Task 9 needs). Same extraction/tinting approach as the existing 7 icons.

- [ ] **Step 1: Create the SVG asset**

`mediamerger-app/assets/icons/layers.svg`:
```xml
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="1.8" stroke-linejoin="round"><path d="M12 3 21 7.5 12 12 3 7.5z"/><path d="M3 12.5 12 17l9-4.5"/></svg>
```

- [ ] **Step 2: Add the icon function**

Add to `mediamerger-app/src/ui/icons.rs`:

```rust
pub fn layers(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/layers.svg"), color)
}
```

- [ ] **Step 3: Build to confirm it compiles**

Run: `cargo build -p mediamerger-app`
Expected: clean build (an `unused function` warning is expected until Task 9 calls it).

- [ ] **Step 4: Commit**

```bash
git add mediamerger-app/assets/icons/layers.svg mediamerger-app/src/ui/icons.rs
git commit -m "Add layers icon for the Merge button"
```

---

## Task 5: Shared section-header component

**Files:**
- Create: `mediamerger-app/src/ui/section_header.rs`
- Modify: `mediamerger-app/src/ui/mod.rs` (register the module only — wiring it into the layout is Task 6)

**Interfaces:**
- Produces: `pub fn view(badge: &str, title: &str, subtitle: &str, palette: &Palette) -> Element<'static, Message>` in `ui::section_header`.

- [ ] **Step 1: Implement**

Create `mediamerger-app/src/ui/section_header.rs`:

```rust
use crate::state::Message;
use crate::theme::Palette;
use iced::widget::{column, container, row, text};
use iced::{Element, Length};

pub fn view(badge: &str, title: &str, subtitle: &str, palette: &Palette) -> Element<'static, Message> {
    let badge_bg = palette.accent_soft;
    let badge_fg = palette.accent_fg;
    let badge_circle = container(text(badge.to_string()).size(13).color(badge_fg))
        .width(Length::Fixed(24.0))
        .height(Length::Fixed(24.0))
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
        .style(move |_theme| container::Style {
            background: Some(badge_bg.into()),
            border: iced::Border { radius: 999.0.into(), ..Default::default() },
            ..Default::default()
        });

    row![
        badge_circle,
        column![
            text(title.to_string()).size(15).color(palette.fg),
            text(subtitle.to_string()).size(12).color(palette.dim),
        ]
        .spacing(1),
    ]
    .spacing(11)
    .into()
}
```

Verify `container`'s `.align_x`/`.align_y` method names against the actually-installed `iced` crate before relying on this exact call (check `~/.cargo/registry/src/*/iced_widget-*/src/container.rs` or docs.rs for the resolved version) — this codebase has repeatedly found small API differences in container/button builder methods across tasks; adapt if these specific names have drifted (e.g. some `iced` versions use `Alignment` values passed directly rather than an `iced::alignment::Horizontal`/`Vertical` enum).

- [ ] **Step 2: Register the module**

Add `mod section_header;` to `mediamerger-app/src/ui/mod.rs`'s module list (alphabetical order, after `output_log`).

- [ ] **Step 3: Build to confirm it compiles**

Run: `cargo build -p mediamerger-app`
Expected: clean build (an `unused function` warning is expected until Task 6 wires it in).

- [ ] **Step 4: Commit**

```bash
git add mediamerger-app/src/ui/section_header.rs mediamerger-app/src/ui/mod.rs
git commit -m "Add shared section-header component"
```

---

## Task 6: `ui/mod.rs` — section reorder + wire in headers

**Files:**
- Modify: `mediamerger-app/src/ui/mod.rs`

**Interfaces:**
- No signature changes to any section's `view()` — this task only changes composition order and inserts `section_header::view(...)` calls between sections.

- [ ] **Step 1: Reorder and add headers**

Replace `mediamerger-app/src/ui/mod.rs`'s `view` function body:

```rust
pub fn view(state: &AppState) -> Element<Message> {
    let palette: Palette = theme::build(state.is_dark, &state.accent_hex);

    let sections = column![
        section_header::view("1", "Source files", "Two encodes of the same movie to combine.", &palette),
        file_pickers::view(state, &palette),
        section_header::view("2", "Sync offset", "Aligns File B's timing to File A by cross-correlating their audio.", &palette),
        offset_panel::view(state, &palette),
        section_header::view("3", "Tracks to include", "Pick which tracks go into the merged file. Set default and forced flags per track.", &palette),
        track_table::view(state, &palette),
        section_header::view("+", "Extras", "Optional metadata to carry over.", &palette),
        extras::view(state, &palette),
        output_log::view(state, &palette),
    ]
    .spacing(13);

    let scroll_area = scrollable(container(sections).width(Length::Fill).padding(24))
        .width(Length::Fill)
        .height(Length::Fill);

    container(scroll_area)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(palette.body_bg.into()),
            ..Default::default()
        })
        .into()
}
```

Note the `.spacing(13)` change from the prior `.spacing(20)`: the mockup's section-to-section gap is 24px with a tighter 13px gap between each header and its own content (matching `margin-bottom:13px` on the header, `gap:24px` on the outer content flex column) — using a single uniform `13` here between every element in the flattened column is a reasonable approximation (headers now sit close to their content, content-to-next-header sits slightly tighter than the mockup's 24px). If this reads as too cramped once visually verified on a real desktop, increase spacing selectively (e.g. wrap each `[header, content]` pair in its own `column![...].spacing(13)`, then join those pairs in an outer `column![...].spacing(24)`) rather than changing the single flat value — call this out in the manual verification pass.

- [ ] **Step 2: Build and test**

Run: `cargo build --workspace` (expect zero errors) and `cargo test --workspace` (expect all passing, no regressions — this task changes only composition/ordering, no state or logic).

- [ ] **Step 3: Commit**

```bash
git add mediamerger-app/src/ui/mod.rs
git commit -m "Reorder sections to match the mockup and wire in section headers"
```

---

## Task 7: `file_pickers.rs` — duration chip

**Files:**
- Modify: `mediamerger-app/src/ui/file_pickers.rs`

**Interfaces:**
- Consumes: `MediaFile.duration_secs` (Task 1), `state::format_duration` (Task 2)

- [ ] **Step 1: Add the duration chip**

In `mediamerger-app/src/ui/file_pickers.rs`'s `file_chips` function, insert the duration chip after the resolution/fps chips (matching the mockup's chip order: container, resolution, duration, track count, fps, size — the mockup actually orders it container → resolution → **duration** → track count → fps → size; match that exact order):

```rust
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
```

(This restructures the existing fps chip to be pushed after track-count rather than immediately after resolution, to match the mockup's exact chip order — re-checking `video_track` twice is harmless since it's a cheap `Option` reference, not a re-probe.)

- [ ] **Step 2: Build and test**

Run: `cargo build --workspace` (expect zero errors) and `cargo test --workspace` (expect all passing).

- [ ] **Step 3: Commit**

```bash
git add mediamerger-app/src/ui/file_pickers.rs
git commit -m "Add file duration chip, omitted gracefully when not reported"
```

---

## Task 8: `offset_panel.rs` — banner copy, pill badge, measured-text row

**Files:**
- Modify: `mediamerger-app/src/ui/offset_panel.rs`

**Interfaces:**
- Consumes: `state::confidence_quality_label` (Task 2), existing `OffsetResult`/`Consistency` fields (unchanged)
- No signature change to `pub fn view` beyond its existing `(state, palette)` form.

This task does NOT touch the waveform bar rendering itself — that's Task 9. This task only changes the status banner's content and adds the measured-text line to the offset-input row.

- [ ] **Step 1: Rewrite the status banner with friendly copy and a pill badge**

Replace `status_banner` in `mediamerger-app/src/ui/offset_panel.rs`:

```rust
fn status_banner<'a>(state: &'a AppState, palette: &Palette) -> Element<'a, Message> {
    match &state.offset {
        OffsetState::NotDetected => text("Offset not detected yet").color(palette.dim).into(),
        OffsetState::Detecting => text("Detecting offset…").color(palette.dim).into(),
        OffsetState::Detected(r) => {
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

            container(
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
            .into()
        }
        OffsetState::ManualOverride(v) => text(format!("Manual override: {v:.3}s")).color(palette.fg).into(),
    }
}
```

Verify `row!`'s `.align_y` method (for vertically centering the icon/text/pill) against the actually-installed `iced` crate before relying on it, per this codebase's established practice of checking real APIs rather than assuming.

- [ ] **Step 2: Add the measured-text line to the offset-input row**

Replace the bottom row construction in `pub fn view`:

```rust
    let measured_text: Option<Element<Message>> = match &state.offset {
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
    };

    let mut offset_row = row![
        text("Offset").size(12).color(palette.dim),
        text_input("0.000", &state.manual_offset_input).on_input(Message::ManualOffsetChanged).width(Length::Fixed(78.0)),
        button(row![icons::sparkle(palette.accent_fg), text("Detect offset")].spacing(7)).on_press_maybe(detect_offset_press),
    ]
    .spacing(12);

    offset_row = offset_row.push(iced::widget::horizontal_space());
    if let Some(measured) = measured_text {
        offset_row = offset_row.push(measured);
    }

    col = col.push(offset_row);
```

Update the existing `use crate::state::{AppState, Message, OffsetState};` line at the top of `mediamerger-app/src/ui/offset_panel.rs` to also import `confidence_quality_label`:

```rust
use crate::state::{confidence_quality_label, AppState, Message, OffsetState};
```

`iced::widget::horizontal_space()` is the equivalent of the mockup's `<div style="flex:1;"></div>` spacer — verify this function's exact name/signature (some `iced` versions take an explicit width argument) against the installed crate.

- [ ] **Step 3: Run the workspace build and test suite**

Run: `cargo build --workspace` (expect zero errors) and `cargo test --workspace` (expect all passing — this task adds no new testable logic beyond what Task 2 already covers).

- [ ] **Step 4: Commit**

```bash
git add mediamerger-app/src/ui/offset_panel.rs
git commit -m "Rewrite offset banner with friendly copy/pill badge, reposition measured text"
```

---

## Task 9: `offset_panel.rs` — waveform dashed guide lines

**Files:**
- Modify: `mediamerger-app/src/ui/offset_panel.rs`

**Interfaces:**
- Produces: a new private `WaveformGuides` type implementing `iced::widget::canvas::Program<Message>`, used only within `offset_panel.rs`'s existing `waveform_bars` function.

This is the highest API-risk task in this plan — `iced::widget::canvas` is used nowhere else in this codebase, unlike `checkbox`/`button`/`svg`, which had several prior tasks establish working patterns. Budget time to actually read the installed crate's source rather than transcribing the sketch below verbatim.

- [ ] **Step 1: Check the real `iced::widget::canvas` API before writing this**

Read the actually-installed `iced_widget::canvas` module's source (e.g. `~/.cargo/registry/src/*/iced_widget-*/src/canvas.rs` and its `program.rs`/`stroke.rs`/`path.rs` siblings, or docs.rs for the resolved `iced` version) for:
- The exact `Program` trait signature (associated `State` type, `draw` method's parameters and return type).
- How to construct a `Frame`, a `Path` (specifically a simple straight line between two points), and a dashed `Stroke` (look for a `line_dash`-equivalent field/method — the exact name may differ from the sketch below).
- The exact `iced::widget::canvas::Canvas::new(...)` entry point and its `.width()`/`.height()` builder methods.
- Whether `iced::widget::stack!` (or a `stack(...)` function) exists in this version for layering the canvas over the existing bar-row `Element`, and its exact usage.

Do not guess any of these — this task's whole risk is here.

- [ ] **Step 2: Implement the dashed-guide-line overlay**

Add to `mediamerger-app/src/ui/offset_panel.rs` (adapt names/signatures per Step 1's findings):

```rust
struct WaveformGuides {
    offset_fraction: f32,
    dim_color: iced::Color,
    accent_color: iced::Color,
}

impl iced::widget::canvas::Program<Message> for WaveformGuides {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<iced::widget::canvas::Geometry> {
        let mut frame = iced::widget::canvas::Frame::new(renderer, bounds.size());

        let offset_x = bounds.width * self.offset_fraction.clamp(0.0, 1.0);

        let dash_pattern = iced::widget::canvas::LineDash { segments: &[4.0, 4.0], offset: 0 };

        let zero_line = iced::widget::canvas::Path::line(
            iced::Point::new(0.0, 0.0),
            iced::Point::new(0.0, bounds.height),
        );
        frame.stroke(
            &zero_line,
            iced::widget::canvas::Stroke::default().with_color(self.dim_color).with_width(2.0).with_line_dash(dash_pattern),
        );

        let offset_line = iced::widget::canvas::Path::line(
            iced::Point::new(offset_x, 0.0),
            iced::Point::new(offset_x, bounds.height),
        );
        frame.stroke(
            &offset_line,
            iced::widget::canvas::Stroke::default().with_color(self.accent_color).with_width(2.0).with_line_dash(dash_pattern),
        );

        vec![frame.into_geometry()]
    }
}
```

- [ ] **Step 3: Layer the overlay onto the existing bar rows**

In `waveform_bars` (the existing function rendering `bars_a`/`bars_b` as two rows of thin rectangles), wrap the existing bar-rows column together with the new canvas overlay using `iced`'s stacking primitive, sized to match the bars' combined height (two 40px-tall bar rows plus the 8px gap between them the existing code already uses — 88px total, matching the existing layout constants):

```rust
    let offset_fraction = (offset_secs / envelope.window_duration_secs).clamp(0.0, 1.0) as f32;
    let guides = iced::widget::canvas(WaveformGuides {
        offset_fraction,
        dim_color: palette.dim,
        accent_color: palette.accent,
    })
    .width(Length::Fill)
    .height(Length::Fixed(88.0));

    let bars_layer = column![
        row![text("A").size(12).color(palette.accent_fg), bar_row(&envelope.bars_a, palette.accent)].spacing(8),
        row![text("B").size(12).color(palette.dim), bar_row(&envelope.bars_b, palette.wave)].spacing(8),
    ]
    .spacing(8);

    iced::widget::stack![bars_layer, guides].into()
```

Replace the existing `waveform_bars` function's final `column![...]` construction (the one currently returning the two bar rows plus the old text-based offset-marker line) with this stacked version — the offset value is now conveyed visually by the guide line's position rather than needing a separate text label overlaid on the bars (the numeric offset already appears in the banner and the measured-text line from Task 8).

- [ ] **Step 4: Build and manually reason through the geometry**

Run: `cargo build --workspace`. If `canvas`/`stack` don't exist under the exact names used above in the resolved `iced` version, adapt per Step 1's findings — do not abandon the dashed-line requirement, find the real equivalent API.

Run: `cargo test --workspace` (expect all passing — no new testable logic in this task).

- [ ] **Step 5: Commit**

```bash
git add mediamerger-app/src/ui/offset_panel.rs
git commit -m "Add dashed vertical guide lines to the waveform via a Canvas overlay"
```

---

## Task 10: `output_log.rs` — distinct footer bar

**Files:**
- Modify: `mediamerger-app/src/ui/output_log.rs`

**Interfaces:**
- Consumes: `Palette.headerbar` (Task 3), `icons::layers` (Task 4)
- No signature change to `pub fn view`.

- [ ] **Step 1: Restructure the footer's outer container and layout**

Replace `mediamerger-app/src/ui/output_log.rs`'s `pub fn view` to wrap the whole footer in a distinctly-styled bar and adopt the mockup's two-column layout:

```rust
pub fn view<'a>(state: &'a AppState, palette: &Palette) -> Element<'a, Message> {
    let output_label = match &state.output_path {
        Some(p) => p.display().to_string(),
        None => "No output selected".to_string(),
    };

    let blocking_reason = state.blocking_reason();
    let selected_count = state.tracks_a_ui.iter().filter(|t| t.selected).count() + state.tracks_b_ui.iter().filter(|t| t.selected).count();
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
    let btn_style = move |_theme: &_, status: button::Status| {
        let base = button::Style { background: Some(btn_bg.into()), ..Default::default() };
        match status {
            button::Status::Hovered => button::Style { background: Some(btn_hover.into()), ..base },
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
            text(output_label).size(12).color(palette.fg).width(Length::Fill),
            button(row![icons::folder(palette.fg), text("Browse")].spacing(6)).style(btn_style).on_press(Message::PickOutput),
        ]
        .spacing(10),
    ]
    .spacing(5)
    .width(Length::Fill);

    let merge_column = column![
        text(ready_text).size(12).color(ready_color),
        button(row![icons::layers(if merge_enabled { accent_text } else { faint }), text("Merge")].spacing(9))
            .padding([12, 30])
            .style(merge_btn_style)
            .on_press_maybe(merge_press),
    ]
    .spacing(7)
    .align_x(iced::alignment::Horizontal::End);

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

    container(footer)
        .padding([14, 20])
        .style(move |_theme| container::Style {
            background: Some(footer_bg.into()),
            border: iced::Border { color: separator_color, width: 1.0, radius: 0.0.into() },
            ..Default::default()
        })
        .into()
}
```

Verify `column!`'s `.align_x` method (for right-aligning the ready-text/Merge-button stack) against the actually-installed `iced` crate before relying on it. Note the border approach above (`iced::Border { color: separator_color, width: 1.0, ... }`) draws a border on *all four* sides, not just the top like the mockup's `border-top`; if the installed `iced::Border`/`container::Style` doesn't support a single-side border directly, either accept the all-around border as a close approximation or add a separate 1px-tall `container` above the footer content styled with `separator_color` as its background, matching Task 9's `rule::horizontal` pattern already used in `track_table.rs` — check both options against what's actually available before picking one.

- [ ] **Step 2: Run the workspace build and test suite**

Run: `cargo build --workspace` (expect zero errors) and `cargo test --workspace` (expect all passing).

- [ ] **Step 3: Commit**

```bash
git add mediamerger-app/src/ui/output_log.rs
git commit -m "Restyle footer as a distinct bar with uppercase label and pill Merge button"
```

---

## Task 11: Final workspace verification

**Files:** none (verification only)

- [ ] **Step 1: Full workspace build, test, and lint**

Run: `cargo build --workspace`
Expected: clean build.

Run: `cargo test --workspace`
Expected: all tests pass.

Run: `cargo clippy --workspace --all-targets`
Expected: no new warnings beyond the project's known pre-existing categories (elided-lifetime warnings on several `ui/*.rs` view function signatures, `field_reassign_with_default` in `state.rs` test code). Pay particular attention to any `field is never read` warning on `theme::Palette` — confirm `headerbar` (re-added in Task 3) is actually consumed by Task 10's footer, and that no other field regressed to unused.

- [ ] **Step 2: Commit if any fixup was needed**

If Step 1 required any fixes, commit them:

```bash
git add -A
git commit -m "Fix workspace build/lint issues found in fidelity-fixes verification"
```

---

## Manual verification (after all tasks)

In addition to the original redesign's manual-verification checklist, on a real GNOME desktop:

1. Section order and headers (badge/title/subtitle) match the mockup exactly, including copy, for all four numbered sections.
2. Duration chip shows a correctly-formatted `H:MM:SS` value in the mockup's chip position (after resolution, before track count), and is simply absent (not blank) when mkvmerge doesn't report a container duration.
3. Offset panel: banner shows the friendly copy and a bordered pill badge; the technical "Measured X early · Y late · confidence Z (quality)" text appears to the right of the Offset input/Detect button row via the spacer, not inside the banner; the waveform's dashed guide lines render at the correct proportional positions (one at the start, one at the detected offset) and don't visually clash with the bars underneath.
4. Footer renders as a visually distinct bar (background, border) with the uppercase "OUTPUT FILE" label, output path + Browse on the left, ready-text + a large pill-shaped icon'd Merge button stacked on the right.
5. Section-to-header-to-content spacing (Task 6's `.spacing(13)` approximation) reads correctly, not too cramped or too loose — adjust per that task's note if needed.

# MediaMerger Visual Redesign — Design

## Purpose

Restyle MediaMerger's `iced` GUI to match a GNOME/libadwaita-inspired mockup
produced in Claude Design (handoff bundle: `design_files/MediaMerger Design
Help-handoff.zip`, primary file `MediaMerger Redesign.dc.html`), while adding
two genuinely new capabilities the mockup calls for: a real audio waveform
visualization for the sync-offset step, and richer per-file/per-track
metadata display (resolution, file size, dynamic range, channel layout).

## Background

The mockup is a full-window GNOME-style redesign: custom card-based
sections, chip badges for file metadata, a waveform visualizer for the sync
offset, pill-style Default/Forced track buttons, a segmented chapters
control, animated toggle switches for attachments/tags, and a light/dark
theme with a configurable accent color (mirroring GNOME's Adwaita
accent-color system). A screenshot of the *current* app was pasted into the
design tool as a "before" reference — that screenshot happens to show the
exact `bad denominator in r_frame_rate: 24/1,` bug fixed earlier in this
project's history, confirming it predates today's app state.

This redesign was scoped down from the mockup's full scope during
brainstorming: the mockup's fully custom window chrome (hand-drawn
headerbar with a working hamburger menu and custom minimize/maximize/close
buttons, no native title bar) is explicitly out of scope for this round —
see Non-goals.

## Non-goals

- **No custom window chrome.** The native OS title bar/decorations stay
  exactly as they are today. Only the content below the title bar is
  restyled.
- **No in-app accent-color picker or settings UI.** Accent color is detected
  automatically from the GNOME system setting; there is no picker, no
  preferences screen, and no new persisted configuration file.
- **No estimated/approximated bitrate.** Per-track bitrate is shown only
  when the source container reports it directly; it is never derived by
  estimation (e.g. from file size ÷ duration), since an estimate could
  silently mislead the user about the actual encode.

## Visual system

**Theme/accent detection**: a new `detect_accent_color()` function, sibling
to the existing `detect_is_dark()` in `mediamerger-app/src/main.rs`, reads
GNOME's `org.gnome.desktop.interface accent-color` setting (GNOME 47+) via
`gsettings`, mapping the returned name (`blue`/`green`/`purple`/`orange`/
etc.) to a hex value matching the mockup's palette. Falls back to Adwaita
blue (`#3584e4`) if the key is missing, `gsettings` fails, or the value is
unrecognized — the same defensive fallback pattern already used for
dark/light detection. Both checks are re-evaluated on the existing
10-second `RefreshSystemTheme` poll, so live accent changes are picked up
the same way live theme changes already are.

The `gsettings`-output-parsing logic is split into a pure function,
`parse_accent_name(output: &str) -> Option<&'static str>` (returns a hex
string), separate from the `gsettings`-shelling wrapper — mirroring how
`detect_is_dark` could be tested if it were split the same way, and making
this one actually unit-testable.

**Typography**: the mockup uses Cantarell (GNOME's default UI font) via a
Google Fonts import. The real app tries the system-installed Cantarell
(present on essentially all GNOME systems) by family name through `iced`'s
font loading, falling back to `iced`'s default font if unavailable — no
bundled font files.

**Iconography**: the mockup's icons (video/audio/subtitle glyphs,
folder/browse, checkmark, warning triangle, sparkle "detect" icon, etc.)
are simple inline SVG paths. These are extracted into small embedded
`.svg` assets and rendered via `iced::widget::svg`, tinted to match
surrounding text color using `iced`'s SVG color-filter support.

**Color/spacing system**: a new `mediamerger-app/src/theme.rs` module ports
the mockup's `buildColors()` function into a `Palette` struct, computed
once per `view()` call from `(is_dark, accent_hex)`. Every restyled widget
reads colors from this one `Palette` rather than hardcoding values, so
light/dark/accent all stay centrally controlled — matching the mockup's own
single-source-of-truth color function.

## New backend capability 1: real waveform envelope

**Location**: `mediamerger-core/src/offset.rs` (audio-domain logic stays
alongside the offset-detection code it reuses).

```rust
pub struct WaveformEnvelope {
    pub bars_a: Vec<f32>,           // normalized 0.0..=1.0 RMS amplitude per bucket
    pub bars_b: Vec<f32>,
    pub window_start_secs: f64,
    pub window_duration_secs: f64,
}

pub fn extract_waveform(
    file_a: &Path, track_a: u64,
    file_b: &Path, track_b: u64,
    start_secs: f64, duration_secs: f64,
    bucket_count: usize,
) -> Result<WaveformEnvelope, MergerError>
```

Reuses the existing `extract_window` (already used by offset detection) to
get PCM for both files over the same window, then downsamples each into
`bucket_count` RMS-amplitude buckets via a pure helper:

```rust
fn downsample_rms(samples: &[f32], bucket_count: usize) -> Vec<f32>
```

**Normalization is joint, not per-track**: both tracks' bars are scaled
against the shared peak across both, so genuine relative loudness
differences between File A and File B stay visible rather than each track
independently stretching to fill 100% of its own row.

**Which window gets visualized**: the same "early" window `detect_offset`
already used for its primary offset measurement, so the waveform shows
exactly the audio the displayed offset was computed from. To avoid
re-deriving `pick_windows`' private window-selection logic in the app
layer, `OffsetResult` gains two new fields:

```rust
pub struct OffsetResult {
    // ...existing fields...
    pub early_window_start: f64,
    pub window_duration: f64,
}
```

After a successful `DetectOffset`, the app fires a second background call
to `extract_waveform` using these exact values — one extra pair of
`ffmpeg` extractions per "Detect Offset" click, kept as a separate function
and a separate async step rather than merged into `detect_offset` itself,
so offset-math and waveform-rendering stay independently testable
concerns.

**Rendering**: no custom canvas — `bars_a`/`bars_b` become two rows of thin
rectangle widgets (`iced::widget::container`s sized per-bar to the
normalized amplitude), the same technique the mockup itself uses. The
offset marker's horizontal position is computed as a fraction
(`offset_secs / window_duration_secs`) of the row's rendered width, so it
stays proportionally correct for any offset/window-duration combination,
rather than the mockup's fixed 64px.

## New backend capability 2: richer file/track metadata

Extends `mediamerger-core/src/probe.rs`'s existing types, all populated
from mkvmerge's `-J` output already being parsed, or a plain filesystem
stat — no new external tools:

```rust
pub struct Track {
    // ...existing fields...
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub sampling_rate: Option<u32>,
    pub bits_per_sample: Option<u32>,
    pub bitrate_bps: Option<u64>,               // only ever a source-reported value
    pub dynamic_range: Option<DynamicRange>,    // best-effort; None when not confidently detectable
}

pub enum DynamicRange { Sdr, Hdr10, DolbyVision }

pub struct MediaFile {
    // ...existing fields...
    pub file_size_bytes: u64,
}
```

Plus a pure helper for the audio detail line:

```rust
pub fn channel_layout_label(channels: u32) -> String
```

mapping common counts to conventional labels (`2` → `"2.0"`, `6` →
`"5.1"`, `8` → `"7.1"`), falling back to `"{n}ch"` for anything else.

Per the "best effort with available data" scope: `bitrate_bps` is `None`
whenever mkvmerge doesn't report a value directly; `dynamic_range` is
`None` whenever the color-property heuristic isn't confident. The UI
omits that piece of the detail line in either case rather than guessing or
estimating. `file_size_bytes` comes from `std::fs::metadata(path)?.len()`
in `identify()`.

## Screen-by-screen layout mapping

Each existing `ui/*.rs` file is restyled in place; none need further
splitting, and existing state/message plumbing carries over almost
entirely unchanged.

- **`file_pickers.rs`** → "Source files" card pair. The chip row
  (container, resolution, track count, fps, file size) is computed purely
  from `MediaFile`'s existing + new fields — no new state. The framerate
  banner gains a success case: today it only renders when `framerate_error`
  is `Some`; it now also renders "Framerates match — {fps} fps. Safe to
  align and merge." whenever both files are loaded, `framerate_error` is
  `None`, and both have a video track — purely derived from existing data,
  no new field.
- **`track_table.rs`** → same three checkboxes per row (selected/Default/
  Forced) restyled as a colored checkbox + pill buttons; codec/language/
  detail line pulls from the enriched `Track` fields. No new messages —
  `ToggleTrackA/B`, `SetDefaultFlagA/B`, `SetForcedFlagA/B` already exist
  and already do exactly what the pill buttons need.
- **`offset_panel.rs`** → restyled three-state banner (Detected-consistent
  / Detected-inconsistent / NotDetected — an exact match to the existing
  `Consistency` enum, just needing the mockup's visual treatment) plus the
  new waveform. The one section needing new state/message wiring (below).
- **`extras.rs`** → chapters `radio` becomes a 3-segment control over the
  same `ChaptersChoice`; attachments/tags `checkbox`es become toggle
  switches over the same four booleans. Pure restyle, zero state changes.
- **`output_log.rs`** → footer restyle (output path row + status text +
  Merge button), plus the log panel becomes collapsible: new
  `AppState.log_expanded: bool` (default `false`) and
  `Message::ToggleLogExpanded`, auto-set to `true` when `StartMerge` fires
  so users don't have to remember to open it.

## New state/message wiring for the waveform

- `AppState` gains `waveform: Option<WaveformEnvelope>` (default `None`).
- `Message::OffsetDetected(Ok(result))` currently sets `state.offset` and
  returns `Task::none()`; it now additionally returns
  `Task::perform(extract_waveform_async(...), Message::WaveformExtracted)`
  using the new `result.early_window_start`/`result.window_duration`
  fields.
- New `Message::WaveformExtracted(Result<WaveformEnvelope, MergerError>)`:
  on `Ok`, sets `state.waveform = Some(envelope)`; on `Err`, leaves it
  `None` with no user-facing error — offset detection itself already
  succeeded and is fully usable without the visualization, so a
  waveform-fetch failure is silently skipped rather than surfaced as an
  alarming error for what is a supplementary visual.
- The waveform section only renders when `state.waveform.is_some()`,
  keyed off real data readiness rather than a separate visibility flag.

## Testing strategy

**New pure functions get unit tests, following the existing core-crate
pattern:**
- `downsample_rms`: synthetic sample arrays with known RMS values,
  verifying bucket count, correct averaging, and the joint-normalization
  behavior (peak across *both* tracks) using a two-track case where one is
  louder than the other.
- `channel_layout_label`: table-driven over common counts (1, 2, 6, 8) plus
  an uncommon count confirming the `"{n}ch"` fallback.
- `parse_mkvmerge_json` (already tested): extended with fixture JSON
  including `pixel_dimensions`, `audio_sampling_frequency`,
  `audio_bits_per_sample`, and a color-property block, asserting the new
  fields populate correctly — plus a fixture *without* those properties,
  asserting graceful `None`s rather than a parse failure.
- `parse_accent_name`: tested directly against sample `gsettings` output
  strings, separate from the `gsettings`-shelling wrapper.

**Not independently unit-testable** (consistent with the rest of the app
crate): `extract_waveform` itself (needs real `ffmpeg`), and all restyled
`view()` functions — this project has never unit-tested `iced` view code,
relying on build-checking plus manual verification, since there is no
display server in the development sandbox.

**Manual verification** (additions to the project's existing checklist):
1. Light/dark and all four accent colors render correctly and match the
   mockup's palettes.
2. The waveform renders proportionally correct bars for a real file pair,
   and the offset marker lines up with the actual detected offset value.
3. The framerate-match success banner appears when expected (matching fps,
   both files loaded) and correctly switches to the mismatch banner when
   they don't.
4. The collapsible log expands automatically on merge start and can be
   manually toggled afterward.
5. Metadata chips show real values (resolution, size, HDR/DV where
   applicable) and gracefully omit fields the source file doesn't report
   rather than showing a wrong or placeholder value.

# MediaMerger Visual Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restyle MediaMerger's `iced` GUI to match the GNOME/libadwaita-inspired Claude Design mockup, plus two new backend capabilities the mockup requires: a real audio waveform visualization and richer file/track metadata.

**Architecture:** No new crates or module splits beyond one new `mediamerger-app/src/theme.rs` (a `Palette` struct computed from `is_dark`/`accent_hex`) and one new `mediamerger-app/src/ui/icons.rs` (SVG icon helpers). Every existing `mediamerger-core` and `mediamerger-app` file is extended in place; state/message plumbing already established (offset detection, track selection, extras) carries over almost entirely unchanged.

**Tech Stack:** Same as the existing app — Rust, `iced` 0.14, `tokio`, `rfd`, `dark-light`. No new crate dependencies required (accent detection reuses `gsettings` the same way dark/light detection already does; SVG rendering uses `iced::widget::svg`, already part of `iced`).

## Global Constraints

- No custom window chrome — the native OS title bar/decorations are unchanged; only content below it is restyled. (spec: Non-goals)
- No in-app accent-color picker or settings UI/persistence — accent is detected automatically from the GNOME system setting, falling back to Adwaita blue (`#3584e4`) when unavailable. (spec: Non-goals, Visual system)
- Per-track bitrate is shown only when the source container reports it directly (via mkvmerge's `tag_bps` property) — never estimated from file size ÷ duration. (spec: Non-goals)
- Waveform bar normalization is joint across both tracks (shared peak), not per-track independently — so relative loudness differences between File A and File B stay visible. (spec: New backend capability 1)
- The waveform visualizes the same "early" window `detect_offset` already used for its primary measurement, not an arbitrary different slice. (spec: New backend capability 1)
- A waveform-extraction failure must never surface as a user-facing error — offset detection already succeeded and is fully usable without the visualization. (spec: New state/message wiring)
- `dynamic_range`/HDR detection and per-track bitrate must default to absent/omitted when not confidently derivable from the source data, rather than guessing. (spec: New backend capability 2)

---

## Task 1: `probe` module — richer file/track metadata

**Files:**
- Modify: `mediamerger-core/src/probe.rs`
- Modify: `mediamerger-app/src/state.rs` (the `track()` test helper constructs `Track` literals directly and must be updated to keep compiling with the new required fields)

**Interfaces:**
- Consumes: existing `Track`/`MediaFile`/`parse_mkvmerge_json`/`identify` (already in `probe.rs`)
- Produces: `Track` gains `width: Option<u32>`, `height: Option<u32>`, `sampling_rate: Option<u32>`, `bits_per_sample: Option<u32>`, `bitrate_bps: Option<u64>`, `is_hdr10: bool`, `is_dolby_vision: bool`. `MediaFile` gains `file_size_bytes: u64`. New pure function `pub fn channel_layout_label(channels: u32) -> String`.

- [ ] **Step 1: Write the failing tests for the new fields and the channel-layout helper**

Add to the `tests` module in `mediamerger-core/src/probe.rs` (replacing the existing `parses_video_audio_subtitle_tracks` test's fixture JSON with an enriched one, and adding new assertions/tests):

```rust
    #[test]
    fn parses_video_audio_subtitle_tracks() {
        let json = br#"{
            "container": {"type": "Matroska"},
            "tracks": [
                {"id":0,"type":"video","codec":"MPEG-4p10/AVC/h.264","properties":{"default_track":true,"forced_track":false,"default_duration":41708333,"pixel_dimensions":"3840x2160","color_transfer_characteristics":16,"block_addition_mappings":[{"id_type":4}]}},
                {"id":1,"type":"audio","codec":"AC-3","properties":{"default_track":true,"forced_track":false,"language":"eng","audio_channels":6,"audio_sampling_frequency":48000,"audio_bits_per_sample":16,"tag_bps":"640000"}},
                {"id":2,"type":"subtitles","codec":"SubRip/SRT","properties":{"default_track":false,"forced_track":false,"language":"fre","track_name":"Forced"}}
            ]
        }"#;

        let media = parse_mkvmerge_json(json, Path::new("test.mkv")).unwrap();

        assert_eq!(media.container, "Matroska");
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
    fn missing_optional_properties_yield_none_not_a_parse_error() {
        let json = br#"{
            "container": {"type": "Matroska"},
            "tracks": [
                {"id":0,"type":"video","codec":"AV1","properties":{"default_track":false,"forced_track":false}}
            ]
        }"#;

        let media = parse_mkvmerge_json(json, Path::new("test.mkv")).unwrap();

        assert_eq!(media.tracks[0].width, None);
        assert_eq!(media.tracks[0].height, None);
        assert_eq!(media.tracks[0].bitrate_bps, None);
        assert!(!media.tracks[0].is_hdr10);
        assert!(!media.tracks[0].is_dolby_vision);
    }

    #[test]
    fn channel_layout_label_maps_common_counts() {
        assert_eq!(channel_layout_label(1), "1.0");
        assert_eq!(channel_layout_label(2), "2.0");
        assert_eq!(channel_layout_label(6), "5.1");
        assert_eq!(channel_layout_label(8), "7.1");
    }

    #[test]
    fn channel_layout_label_falls_back_for_uncommon_counts() {
        assert_eq!(channel_layout_label(3), "3ch");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mediamerger-core probe::tests`
Expected: FAIL — compile error, since `width`/`height`/`is_hdr10`/etc. don't exist on `Track` yet, and `channel_layout_label` doesn't exist.

- [ ] **Step 3: Implement the new fields, JSON parsing, and helper**

Replace the `Track`/`MkvmergeTrackProperties` definitions and `parse_mkvmerge_json` in `mediamerger-core/src/probe.rs`:

```rust
#[derive(Debug, Clone)]
pub struct Track {
    pub id: u64,
    pub kind: TrackKind,
    pub codec: String,
    pub language: Option<String>,
    pub name: Option<String>,
    pub default_flag: bool,
    pub forced_flag: bool,
    pub fps: Option<f64>,
    pub channels: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub sampling_rate: Option<u32>,
    pub bits_per_sample: Option<u32>,
    /// Only ever a value the source container reports directly (mkvmerge's
    /// `tag_bps` property) - never estimated from file size / duration.
    pub bitrate_bps: Option<u64>,
    /// Best-effort from color/block-addition properties; false when not
    /// confidently detectable, never a guess.
    pub is_hdr10: bool,
    pub is_dolby_vision: bool,
}

#[derive(Debug, Clone)]
pub struct MediaFile {
    pub path: PathBuf,
    pub container: String,
    pub tracks: Vec<Track>,
    pub file_size_bytes: u64,
}
```

```rust
#[derive(Deserialize, Default)]
struct MkvmergeTrackProperties {
    #[serde(default)]
    default_track: bool,
    #[serde(default)]
    forced_track: bool,
    language: Option<String>,
    track_name: Option<String>,
    audio_channels: Option<u32>,
    default_duration: Option<u64>,
    pixel_dimensions: Option<String>,
    audio_sampling_frequency: Option<u32>,
    audio_bits_per_sample: Option<u32>,
    tag_bps: Option<String>,
    color_transfer_characteristics: Option<u32>,
    #[serde(default)]
    block_addition_mappings: Vec<MkvmergeBlockAdditionMapping>,
}

#[derive(Deserialize)]
struct MkvmergeBlockAdditionMapping {
    id_type: Option<u32>,
}
```

Replace the body of `parse_mkvmerge_json`:

```rust
fn parse_mkvmerge_json(bytes: &[u8], path: &Path) -> Result<MediaFile, MergerError> {
    let parsed: MkvmergeJson =
        serde_json::from_slice(bytes).map_err(|e| MergerError::Probe(e.to_string()))?;

    let tracks = parsed
        .tracks
        .into_iter()
        .filter_map(|t| {
            let kind = match t.kind.as_str() {
                "video" => TrackKind::Video,
                "audio" => TrackKind::Audio,
                "subtitles" => TrackKind::Subtitle,
                _ => return None,
            };
            let fps = t
                .properties
                .default_duration
                .filter(|&ns| ns > 0)
                .map(|ns| 1_000_000_000.0 / ns as f64);
            let (width, height) = t
                .properties
                .pixel_dimensions
                .as_deref()
                .and_then(|s| s.split_once('x'))
                .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))
                .map_or((None, None), |(w, h)| (Some(w), Some(h)));
            // Transfer characteristic 16 = SMPTE ST 2084 (PQ), 18 = ARIB
            // STD-B67 (HLG) - both are HDR transfer functions per the
            // ISO/IEC 23001-8 registry mkvmerge reports numerically.
            let is_hdr10 = matches!(t.properties.color_transfer_characteristics, Some(16) | Some(18));
            // Dolby Vision-in-MKV is conventionally signaled via a block
            // addition mapping with id_type 4. Best-effort: absent/
            // unrecognized data means `false`, never a guessed `true`.
            let is_dolby_vision = t
                .properties
                .block_addition_mappings
                .iter()
                .any(|m| m.id_type == Some(4));
            let bitrate_bps = t.properties.tag_bps.as_deref().and_then(|s| s.parse::<u64>().ok());
            Some(Track {
                id: t.id,
                kind,
                codec: t.codec,
                language: t.properties.language,
                name: t.properties.track_name,
                default_flag: t.properties.default_track,
                forced_flag: t.properties.forced_track,
                fps,
                channels: t.properties.audio_channels,
                width,
                height,
                sampling_rate: t.properties.audio_sampling_frequency,
                bits_per_sample: t.properties.audio_bits_per_sample,
                bitrate_bps,
                is_hdr10,
                is_dolby_vision,
            })
        })
        .collect();

    let file_size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    Ok(MediaFile { path: path.to_path_buf(), container: parsed.container.kind, tracks, file_size_bytes })
}
```

Add `channel_layout_label` near the bottom of `mediamerger-core/src/probe.rs` (above the `#[cfg(test)]` module):

```rust
pub fn channel_layout_label(channels: u32) -> String {
    match channels {
        1 => "1.0".to_string(),
        2 => "2.0".to_string(),
        6 => "5.1".to_string(),
        8 => "7.1".to_string(),
        n => format!("{n}ch"),
    }
}
```

- [ ] **Step 4: Fix the now-broken `Track` literal in `mediamerger-app/src/state.rs`**

Edit the `track()` test helper in `mediamerger-app/src/state.rs`'s `tests` module:

```rust
    fn track(id: u64, kind: TrackKind) -> Track {
        Track {
            id,
            kind,
            codec: "test".to_string(),
            language: None,
            name: None,
            default_flag: false,
            forced_flag: false,
            fps: None,
            channels: None,
            width: None,
            height: None,
            sampling_rate: None,
            bits_per_sample: None,
            bitrate_bps: None,
            is_hdr10: false,
            is_dolby_vision: false,
        }
    }
```

Also add `file_size_bytes: 0,` to the `media_file()` test helper's `MediaFile { ... }` literal in the same file.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p mediamerger-core probe::tests` then `cargo test --workspace`
Expected: PASS (all core tests, all app tests, no regressions).

- [ ] **Step 6: Commit**

```bash
git add mediamerger-core/src/probe.rs mediamerger-app/src/state.rs
git commit -m "Add richer file/track metadata (resolution, HDR/DV, bitrate, file size)"
```

---

## Task 2: `offset` module — real waveform envelope + `OffsetResult` window fields

**Files:**
- Modify: `mediamerger-core/src/offset.rs`
- Modify: `mediamerger-app/src/state.rs` (the `OffsetResult { ... }` literals in tests must be updated to keep compiling with the two new required fields)

**Interfaces:**
- Consumes: existing `extract_window`, `SAMPLE_RATE_HZ` (already in `offset.rs`)
- Produces: `OffsetResult` gains `early_window_start: f64`, `window_duration: f64`. New `WaveformEnvelope { bars_a: Vec<f32>, bars_b: Vec<f32>, window_start_secs: f64, window_duration_secs: f64 }` (`#[derive(Debug, Clone)]`). New `pub fn extract_waveform(file_a: &Path, track_a: u64, file_b: &Path, track_b: u64, start_secs: f64, duration_secs: f64, bucket_count: usize) -> Result<WaveformEnvelope, MergerError>`.

- [ ] **Step 1: Write the failing test for the pure downsampling helper**

Add to `mediamerger-core/src/offset.rs`, in a new test module (or alongside existing tests):

```rust
#[derive(Debug, Clone)]
pub struct WaveformEnvelope {
    pub bars_a: Vec<f32>,
    pub bars_b: Vec<f32>,
    pub window_start_secs: f64,
    pub window_duration_secs: f64,
}

fn downsample_rms(samples: &[f32], bucket_count: usize) -> Vec<f32> {
    if bucket_count == 0 {
        return Vec::new();
    }
    if samples.is_empty() {
        return vec![0.0; bucket_count];
    }
    let chunk_size = (samples.len() / bucket_count).max(1);
    let mut bars: Vec<f32> = samples
        .chunks(chunk_size)
        .map(|chunk| {
            let sum_sq: f32 = chunk.iter().map(|s| s * s).sum();
            (sum_sq / chunk.len() as f32).sqrt()
        })
        .collect();
    bars.truncate(bucket_count);
    bars.resize(bucket_count, 0.0);
    bars
}

fn normalize_joint(bars_a: &mut [f32], bars_b: &mut [f32]) {
    let peak = bars_a
        .iter()
        .chain(bars_b.iter())
        .cloned()
        .fold(0.0f32, f32::max);
    if peak > 1e-6 {
        for b in bars_a.iter_mut().chain(bars_b.iter_mut()) {
            *b /= peak;
        }
    }
}

#[cfg(test)]
mod waveform_tests {
    use super::*;

    #[test]
    fn downsample_rms_produces_requested_bucket_count() {
        let samples: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.01).sin()).collect();
        let bars = downsample_rms(&samples, 20);
        assert_eq!(bars.len(), 20);
    }

    #[test]
    fn downsample_rms_of_silence_is_zero() {
        let samples = vec![0.0f32; 500];
        let bars = downsample_rms(&samples, 10);
        assert!(bars.iter().all(|&b| b == 0.0));
    }

    #[test]
    fn downsample_rms_handles_empty_input() {
        let bars = downsample_rms(&[], 10);
        assert_eq!(bars, vec![0.0; 10]);
    }

    #[test]
    fn normalize_joint_scales_against_shared_peak_not_per_track() {
        let mut bars_a = vec![1.0, 0.5]; // louder track
        let mut bars_b = vec![0.25, 0.1]; // quieter track
        normalize_joint(&mut bars_a, &mut bars_b);

        // Peak (1.0) came from bars_a, so bars_a's max normalizes to 1.0...
        assert!((bars_a[0] - 1.0).abs() < 1e-6);
        // ...but bars_b, being quieter, must NOT also reach 1.0 - it stays
        // proportionally smaller, preserving the real loudness difference.
        assert!(bars_b[0] < 0.5, "bars_b[0] = {}, should stay well below 1.0", bars_b[0]);
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p mediamerger-core offset::waveform_tests`
Expected: PASS (pure functions, no ffmpeg needed).

- [ ] **Step 3: Add `early_window_start`/`window_duration` to `OffsetResult` and populate them in `detect_offset`**

Edit `OffsetResult` in `mediamerger-core/src/offset.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OffsetResult {
    pub early_offset: f64,
    pub late_offset: f64,
    pub consistency: Consistency,
    pub confidence: f32,
    pub offset: f64,
    pub early_window_start: f64,
    pub window_duration: f64,
}
```

Update both `Ok(OffsetResult { ... })` construction sites inside `detect_offset` (the `shorter < 120.0` short-file branch and the main branch) to populate the two new fields with the actual window parameters used for the *early* measurement in each branch:

```rust
    if shorter < 120.0 {
        let window = (shorter * 0.5).max(1.0);
        let start = shorter * 0.25;
        let (offset, confidence) = measure_at(file_a, audio_track_a, file_b, audio_track_b, start, window)?;
        return Ok(OffsetResult {
            early_offset: offset,
            late_offset: offset,
            consistency: Consistency::Unverified,
            confidence,
            offset,
            early_window_start: start,
            window_duration: window,
        });
    }

    let (early_start, late_start, window) = pick_windows(shorter);
    let (early_offset, early_conf) =
        measure_at(file_a, audio_track_a, file_b, audio_track_b, early_start, window)?;
    let (late_offset, late_conf) =
        measure_at(file_a, audio_track_a, file_b, audio_track_b, late_start, window)?;

    let consistency = if (early_offset - late_offset).abs() <= CONSISTENCY_TOLERANCE_SECS {
        Consistency::Consistent
    } else {
        Consistency::Inconsistent
    };
    let offset = if consistency == Consistency::Consistent {
        (early_offset + late_offset) / 2.0
    } else {
        early_offset
    };

    Ok(OffsetResult {
        early_offset,
        late_offset,
        consistency,
        confidence: early_conf.min(late_conf),
        offset,
        early_window_start: early_start,
        window_duration: window,
    })
```

- [ ] **Step 4: Fix the now-broken `OffsetResult` literals in `mediamerger-app/src/state.rs`**

Add `early_window_start: 0.0, window_duration: 180.0,` to every `OffsetResult { ... }` test literal in `mediamerger-app/src/state.rs` (there are three: in `resolved_offset_uses_detected_value`, `blocking_reason_some_when_offset_detected_inconsistent`, and any other constructed in that file's `tests` module).

- [ ] **Step 5: Implement `extract_waveform`**

Add to `mediamerger-core/src/offset.rs`:

```rust
pub fn extract_waveform(
    file_a: &Path,
    track_a: u64,
    file_b: &Path,
    track_b: u64,
    start_secs: f64,
    duration_secs: f64,
    bucket_count: usize,
) -> Result<WaveformEnvelope, MergerError> {
    let pcm_a = extract_window(file_a, track_a, start_secs, duration_secs)?;
    let pcm_b = extract_window(file_b, track_b, start_secs, duration_secs)?;

    let mut bars_a = downsample_rms(&pcm_a, bucket_count);
    let mut bars_b = downsample_rms(&pcm_b, bucket_count);
    normalize_joint(&mut bars_a, &mut bars_b);

    Ok(WaveformEnvelope {
        bars_a,
        bars_b,
        window_start_secs: start_secs,
        window_duration_secs: duration_secs,
    })
}
```

- [ ] **Step 6: Run the full core test suite to verify nothing regressed**

Run: `cargo test -p mediamerger-core` then `cargo test --workspace`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add mediamerger-core/src/offset.rs mediamerger-app/src/state.rs
git commit -m "Add waveform envelope extraction and OffsetResult window fields"
```

---

## Task 3: Accent-color detection

**Files:**
- Modify: `mediamerger-app/src/main.rs`
- Modify: `mediamerger-app/src/state.rs`

**Interfaces:**
- Consumes: existing `detect_is_dark()` pattern in `main.rs`
- Produces: `AppState.accent_hex: String` (default `"#3584e4"`), `pub fn parse_accent_name(output: &str) -> Option<&'static str>` (pure), `fn detect_accent_color() -> String` in `main.rs`, wired into the existing `RefreshSystemTheme`/`SystemThemeDetected` poll cycle.

- [ ] **Step 1: Write the failing test for the pure accent-name parser**

Add to `mediamerger-app/src/state.rs` (a new pure function, plus a test in the existing `tests` module):

```rust
pub fn parse_accent_name(output: &str) -> Option<&'static str> {
    let name = output.trim().trim_matches('\'');
    match name {
        "blue" => Some("#3584e4"),
        "green" => Some("#3a944a"),
        "purple" => Some("#9141ac"),
        "orange" => Some("#ed5b00"),
        _ => None,
    }
}
```

```rust
    #[test]
    fn parse_accent_name_maps_known_gnome_names() {
        assert_eq!(parse_accent_name("'blue'\n"), Some("#3584e4"));
        assert_eq!(parse_accent_name("purple"), Some("#9141ac"));
    }

    #[test]
    fn parse_accent_name_returns_none_for_unrecognized_value() {
        assert_eq!(parse_accent_name("'teal'\n"), None);
    }
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p mediamerger-app parse_accent_name`
Expected: PASS.

- [ ] **Step 3: Add `accent_hex` to `AppState`**

Add `pub accent_hex: String,` to the `AppState` struct and `accent_hex: detect_accent_color(),` to its `Default` impl in `mediamerger-app/src/state.rs`.

- [ ] **Step 4: Implement `detect_accent_color` and wire it into the refresh cycle**

Add to `mediamerger-app/src/main.rs`, next to `detect_is_dark`:

```rust
// GNOME 47+ exposes a system accent color the same way it exposes
// light/dark; mirror the existing detect_is_dark defensive pattern (falls
// back to Adwaita blue on any failure, non-GNOME desktop, or older GNOME
// without this key).
fn detect_accent_color() -> String {
    std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "accent-color"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .as_deref()
        .and_then(state::parse_accent_name)
        .unwrap_or("#3584e4")
        .to_string()
}
```

Update `Message::RefreshSystemTheme`'s handler and add a new arm to also refresh the accent color on the same 10-second tick. Change the message and handler:

```rust
        Message::RefreshSystemTheme => Task::perform(
            async {
                tokio::task::spawn_blocking(|| (detect_is_dark(), detect_accent_color())).await.unwrap_or((false, "#3584e4".to_string()))
            },
            |(is_dark, accent)| Message::SystemThemeDetected(is_dark, accent),
        ),
        Message::SystemThemeDetected(is_dark, accent) => {
            if state.is_dark != is_dark {
                state.is_dark = is_dark;
            }
            if state.accent_hex != accent {
                state.accent_hex = accent;
            }
            Task::none()
        }
```

Update the `Message::SystemThemeDetected(bool)` variant in `mediamerger-app/src/state.rs` to `SystemThemeDetected(bool, String)`.

- [ ] **Step 5: Run the full workspace build and test suite**

Run: `cargo build --workspace` then `cargo test --workspace`
Expected: builds cleanly, all tests pass.

- [ ] **Step 6: Commit**

```bash
git add mediamerger-app/src/main.rs mediamerger-app/src/state.rs
git commit -m "Add system accent-color detection alongside existing dark/light detection"
```

---

## Task 4: `theme.rs` — shared `Palette` module

**Files:**
- Create: `mediamerger-app/src/theme.rs`
- Modify: `mediamerger-app/src/main.rs` (add `mod theme;`)

**Interfaces:**
- Consumes: `AppState.is_dark`, `AppState.accent_hex` (Task 3)
- Produces: `pub struct Palette { card: Color, view: Color, body_bg: Color, border: Color, separator: Color, fg: Color, dim: Color, faint: Color, chip_bg: Color, chip_border: Color, btn_bg: Color, btn_hover: Color, accent: Color, accent_text: Color, accent_fg: Color, accent_soft: Color, success_fg: Color, success_soft: Color, warn_fg: Color, warn_soft: Color, danger_fg: Color, danger_soft: Color, wave: Color }`, `pub fn build(is_dark: bool, accent_hex: &str) -> Palette`.

This task ports the mockup's `buildColors()`/`hexToRgb`/`shade`/`rgba` functions, using the exact color values from `MediaMerger Redesign.dc.html`, **except** the mockup's `window` and `headerbar` fields — those color the custom CSD titlebar/rounded-window-frame the redesign explicitly does not build (native OS decorations stay); porting them would leave two `Palette` fields nothing ever reads, which triggers a real "field is never read" warning in a binary crate (unlike `mediamerger-core`'s library types, `mediamerger-app`'s `pub` items get no dead-code exemption, since nothing outside the binary can ever consume them).

- [ ] **Step 1: Write the failing test pinning the exact mockup color values**

Create `mediamerger-app/src/theme.rs`:

```rust
use iced::Color;

pub struct Palette {
    pub card: Color,
    pub view: Color,
    pub body_bg: Color,
    pub border: Color,
    pub separator: Color,
    pub fg: Color,
    pub dim: Color,
    pub faint: Color,
    pub chip_bg: Color,
    pub chip_border: Color,
    pub btn_bg: Color,
    pub btn_hover: Color,
    pub accent: Color,
    pub accent_text: Color,
    pub accent_fg: Color,
    pub accent_soft: Color,
    pub success_fg: Color,
    pub success_soft: Color,
    pub warn_fg: Color,
    pub warn_soft: Color,
    pub danger_fg: Color,
    pub danger_soft: Color,
    pub wave: Color,
}

fn hex_to_rgb(hex: &str) -> (u8, u8, u8) {
    let h = hex.trim_start_matches('#');
    let h: String = if h.len() == 3 {
        h.chars().flat_map(|c| [c, c]).collect()
    } else {
        h.to_string()
    };
    let n = u32::from_str_radix(&h, 16).unwrap_or(0x3584e4);
    (((n >> 16) & 255) as u8, ((n >> 8) & 255) as u8, (n & 255) as u8)
}

fn rgba(hex: &str, a: f32) -> Color {
    let (r, g, b) = hex_to_rgb(hex);
    Color::from_rgba8(r, g, b, a)
}

/// Ports the mockup's `shade(h, amt)`: amt < 0 darkens toward black by
/// |amt|; amt >= 0 lightens toward white by amt. Used to derive readable
/// accent-colored text against the theme's own background.
fn shade(hex: &str, amt: f32) -> Color {
    let (r, g, b) = hex_to_rgb(hex);
    let target: f32 = if amt < 0.0 { 0.0 } else { 255.0 };
    let p = amt.abs();
    let mix = |c: u8| -> u8 { (c as f32 + (target - c as f32) * p).round() as u8 };
    Color::from_rgb8(mix(r), mix(g), mix(b))
}

pub fn build(is_dark: bool, accent_hex: &str) -> Palette {
    if is_dark {
        Palette {
            card: rgba("#323232", 1.0),
            view: rgba("#1c1c1c", 1.0),
            body_bg: rgba("#242424", 1.0),
            border: rgba("#ffffff", 0.11),
            separator: rgba("#ffffff", 0.09),
            fg: rgba("#ffffff", 0.95),
            dim: rgba("#ffffff", 0.66),
            faint: rgba("#ffffff", 0.46),
            chip_bg: rgba("#ffffff", 0.09),
            chip_border: rgba("#ffffff", 0.11),
            btn_bg: rgba("#ffffff", 0.10),
            btn_hover: rgba("#ffffff", 0.17),
            accent: rgba(accent_hex, 1.0),
            accent_text: rgba("#ffffff", 1.0),
            accent_fg: shade(accent_hex, 0.42),
            accent_soft: rgba(accent_hex, 0.26),
            success_fg: rgba("#8ff0a4", 1.0),
            success_soft: rgba("#2ec27e", 0.20),
            warn_fg: rgba("#f8e45c", 1.0),
            warn_soft: rgba("#e5a50a", 0.20),
            danger_fg: rgba("#ff7b63", 1.0),
            danger_soft: rgba("#e01b24", 0.22),
            wave: rgba("#ffffff", 0.24),
        }
    } else {
        Palette {
            card: rgba("#ffffff", 1.0),
            view: rgba("#ffffff", 1.0),
            body_bg: rgba("#fafafb", 1.0),
            border: rgba("#000000", 0.09),
            separator: rgba("#000000", 0.07),
            fg: rgba("#000000", 0.87),
            dim: rgba("#000000", 0.55),
            faint: rgba("#000000", 0.40),
            chip_bg: rgba("#000000", 0.055),
            chip_border: rgba("#000000", 0.08),
            btn_bg: rgba("#000000", 0.06),
            btn_hover: rgba("#000000", 0.11),
            accent: rgba(accent_hex, 1.0),
            accent_text: rgba("#ffffff", 1.0),
            accent_fg: shade(accent_hex, -0.22),
            accent_soft: rgba(accent_hex, 0.13),
            success_fg: rgba("#1a7f4b", 1.0),
            success_soft: rgba("#2ec27e", 0.16),
            warn_fg: rgba("#9a5b00", 1.0),
            warn_soft: rgba("#e5a50a", 0.16),
            danger_fg: rgba("#c01c28", 1.0),
            danger_soft: rgba("#e01b24", 0.11),
            wave: rgba("#000000", 0.20),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_palette_matches_mockup_body_background() {
        let p = build(true, "#3584e4");
        assert_eq!(p.body_bg, Color::from_rgb8(0x24, 0x24, 0x24));
    }

    #[test]
    fn light_palette_matches_mockup_body_background() {
        let p = build(false, "#3584e4");
        assert_eq!(p.body_bg, Color::from_rgb8(0xfa, 0xfa, 0xfb));
    }

    #[test]
    fn accent_color_is_used_directly_for_the_accent_field() {
        let p = build(true, "#9141ac");
        assert_eq!(p.accent, Color::from_rgb8(0x91, 0x41, 0xac));
    }

    #[test]
    fn shade_lightens_toward_white_for_positive_amount() {
        let lightened = shade("#3584e4", 1.0);
        assert_eq!(lightened, Color::from_rgb8(0xff, 0xff, 0xff), "amt=1.0 should fully reach white");
    }

    #[test]
    fn shade_darkens_toward_black_for_negative_amount() {
        let darkened = shade("#3584e4", -1.0);
        assert_eq!(darkened, Color::from_rgb8(0x00, 0x00, 0x00), "amt=-1.0 should fully reach black");
    }
}
```

- [ ] **Step 2: Register the module**

Edit `mediamerger-app/src/main.rs`, add `mod theme;` alongside the existing `mod state;`/`mod ui;`.

- [ ] **Step 3: Try the system Cantarell font, falling back to `iced`'s default**

The mockup specifies Cantarell (GNOME's default UI font) via a Google Fonts import; the real app has no bundled font file and instead asks the system font backend to resolve the already-installed system Cantarell by family name, exactly as the design spec requires ("try system Cantarell... fall back to iced's default font if unavailable — no bundled font files").

Edit the `application(...)` builder chain in `mediamerger-app/src/main.rs`'s `main()`:

```rust
        .default_font(iced::Font::with_name("Cantarell"))
```

Add this call in the builder chain (alongside `.title(...)`, `.window(...)`, `.theme(theme)`, `.subscription(subscription)`). Verify `iced::Font::with_name` is the correct constructor against the actually-installed `iced` crate (check `~/.cargo/registry/src/*/iced_core-*/src/font.rs` or docs.rs for the resolved version) before relying on this exact call — if the system doesn't have Cantarell installed, `iced`'s normal font-fallback behavior applies (whatever it substitutes for an unresolvable family name), which is the intended graceful degradation, not an error to handle explicitly.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mediamerger-app theme::`
Expected: PASS.

- [ ] **Step 5: Build to confirm the font call compiles**

Run: `cargo build -p mediamerger-app`
Expected: clean build.

- [ ] **Step 6: Commit**

```bash
git add mediamerger-app/src/theme.rs mediamerger-app/src/main.rs
git commit -m "Add Palette module and Cantarell font loading with system fallback"
```

---

## Task 5: Icon assets + `icons.rs` helper module

**Files:**
- Create: `mediamerger-app/assets/icons/video.svg`
- Create: `mediamerger-app/assets/icons/audio.svg`
- Create: `mediamerger-app/assets/icons/subtitle.svg`
- Create: `mediamerger-app/assets/icons/folder.svg`
- Create: `mediamerger-app/assets/icons/check.svg`
- Create: `mediamerger-app/assets/icons/warning.svg`
- Create: `mediamerger-app/assets/icons/sparkle.svg`
- Create: `mediamerger-app/src/ui/icons.rs`
- Modify: `mediamerger-app/src/ui/mod.rs` (add `mod icons;`)

**Interfaces:**
- Produces: `pub fn video(color: Color) -> Element<'static, Message>`, `pub fn audio(color: Color) -> Element<'static, Message>`, `pub fn subtitle(color: Color) -> Element<'static, Message>`, `pub fn folder(color: Color) -> Element<'static, Message>`, `pub fn check(color: Color) -> Element<'static, Message>`, `pub fn warning(color: Color) -> Element<'static, Message>`, `pub fn sparkle(color: Color) -> Element<'static, Message>` in `ui::icons`.

Icon path data is extracted directly from `design_files/MediaMerger Design Help-handoff.zip`'s `MediaMerger Redesign.dc.html`. The mockup's SVGs use `stroke="currentColor"`/`fill="currentColor"`, which has no meaning outside a browser's CSS context — real files use an explicit opaque color (`black`) instead, and get recolored at render time via `iced`'s SVG color-filter styling.

- [ ] **Step 1: Create the SVG assets**

`mediamerger-app/assets/icons/video.svg`:
```xml
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="1.5"><rect x="3" y="5" width="18" height="14" rx="2"/><path d="M3 9.5h18M3 14.5h18M8 5v14M16 5v14"/></svg>
```

`mediamerger-app/assets/icons/audio.svg`:
```xml
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round"><path d="M5 10v4M9 7.5v9M13 5.5v13M17 8v8M20 10.5v3"/></svg>
```

`mediamerger-app/assets/icons/subtitle.svg`:
```xml
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="1.5" stroke-linecap="round"><rect x="3" y="6" width="18" height="12" rx="2"/><path d="M6.5 11h4M13.5 11h4M6.5 14.5h7.5"/></svg>
```

`mediamerger-app/assets/icons/folder.svg`:
```xml
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="1.7" stroke-linejoin="round"><path d="M3 7a2 2 0 0 1 2-2h3.6l1.6 2H19a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/></svg>
```

`mediamerger-app/assets/icons/check.svg`:
```xml
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="M8.5 12l2.4 2.4L15.5 9"/></svg>
```

`mediamerger-app/assets/icons/warning.svg`:
```xml
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3 21.5 20 2.5 20z"/><path d="M12 10v4.4"/><path d="M12 17.4v.2"/></svg>
```

`mediamerger-app/assets/icons/sparkle.svg`:
```xml
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="black"><path d="M12 2l1.5 5L18 8.5 13.5 10 12 15l-1.5-5L6 8.5 10.5 7z"/><path d="M18.5 14l.8 2.4 2.2.8-2.2.8-.8 2.4-.8-2.4-2.2-.8 2.2-.8z"/></svg>
```

- [ ] **Step 2: Verify `iced`'s SVG color-filter API against the installed crate**

Before writing `icons.rs`, check the actually-installed `iced_widget::svg` module's `Style`/`.style()` API (e.g. `~/.cargo/registry/src/*/iced_widget-*/src/svg.rs` or docs.rs for the resolved `iced` version) for how to apply a solid-color tint/filter to a rendered SVG regardless of its embedded stroke/fill color — this codebase has repeatedly found real API drift between assumed and actual `iced` widget signatures (checkbox, radio, button's `on_press_maybe`), so confirm this one directly rather than guessing the exact method/field names below.

- [ ] **Step 3: Implement `icons.rs`**

Create `mediamerger-app/src/ui/icons.rs`:

```rust
use crate::state::Message;
use iced::widget::svg;
use iced::{Color, Element, Length};

fn icon(bytes: &'static [u8], color: Color) -> Element<'static, Message> {
    let handle = svg::Handle::from_memory(bytes);
    svg(handle)
        .width(Length::Fixed(18.0))
        .height(Length::Fixed(18.0))
        .style(move |_theme, _status| svg::Style { color: Some(color) })
        .into()
}

pub fn video(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/video.svg"), color)
}

pub fn audio(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/audio.svg"), color)
}

pub fn subtitle(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/subtitle.svg"), color)
}

pub fn folder(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/folder.svg"), color)
}

pub fn check(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/check.svg"), color)
}

pub fn warning(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/warning.svg"), color)
}

pub fn sparkle(color: Color) -> Element<'static, Message> {
    icon(include_bytes!("../../assets/icons/sparkle.svg"), color)
}
```

Adapt the `svg::Style { color: Some(color) }` line to whatever the real, installed `iced_widget::svg::Style` struct actually looks like per Step 2's findings — the field name and whether it's a direct struct literal or a builder method may differ from this sketch.

- [ ] **Step 4: Register the module and verify the build**

Add `mod icons;` to `mediamerger-app/src/ui/mod.rs`.

Run: `cargo build -p mediamerger-app`
Expected: builds cleanly (icons aren't consumed by any view yet, so expect an `unused` warning for now — that's resolved as later tasks start calling these functions).

- [ ] **Step 5: Commit**

```bash
git add mediamerger-app/assets/icons mediamerger-app/src/ui/icons.rs mediamerger-app/src/ui/mod.rs
git commit -m "Add SVG icon assets and icons helper module"
```

---

## Task 6: `file_pickers.rs` restyle — source file cards

**Files:**
- Modify: `mediamerger-app/src/ui/file_pickers.rs`
- Modify: `mediamerger-app/src/ui/mod.rs` (pass the `Palette` through, see below)

**Interfaces:**
- Consumes: `theme::Palette` (Task 4), `ui::icons::folder`/`video` (Task 5), enriched `MediaFile`/`Track` fields (Task 1)
- Produces: `pub fn view(state: &AppState, palette: &Palette) -> Element<Message>` (signature changes from `view(state: &AppState)` — every `ui/*.rs` view function in this and subsequent tasks gains a `palette: &Palette` parameter)

Since every restyled section now needs the palette, `ui::mod::view` computes it once and passes it down instead of each section recomputing it. This step also updates `ui/mod.rs`'s signature.

- [ ] **Step 1: Thread `Palette` through `ui::mod::view`**

Edit `mediamerger-app/src/ui/mod.rs`:

```rust
mod extras;
mod file_pickers;
mod icons;
mod offset_panel;
mod output_log;
mod track_table;

use crate::state::{AppState, Message};
use crate::theme::{self, Palette};
use iced::widget::{column, container, text};
use iced::{Element, Length};

pub fn view(state: &AppState) -> Element<Message> {
    let palette = theme::build(state.is_dark, &state.accent_hex);

    let mut sections = column![
        file_pickers::view(state, &palette),
        track_table::view(state, &palette),
        offset_panel::view(state, &palette),
        extras::view(state, &palette),
        output_log::view(state, &palette),
    ]
    .spacing(20);

    if let Some(err) = &state.framerate_error {
        sections = sections.push(text(err.to_string()).color(palette.danger_fg));
    }

    container(sections)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(palette.body_bg.into()),
            ..Default::default()
        })
        .padding(24)
        .into()
}
```

Adapt `container::Style`'s exact field names to whatever the actually-installed `iced_widget::container` module provides (check the real crate rather than assuming, per this project's established practice).

- [ ] **Step 2: Restyle `file_pickers.rs`**

Replace `mediamerger-app/src/ui/file_pickers.rs`:

```rust
use crate::state::{AppState, Message};
use crate::theme::Palette;
use crate::ui::icons;
use iced::widget::{button, column, container, row, text};
use iced::{Element, Length};
use mediamerger_core::probe::{MediaFile, TrackKind};

fn chip(label: String, palette: &Palette) -> Element<'static, Message> {
    container(text(label).size(11).color(palette.dim))
        .padding([3, 8])
        .style(move |_theme| container::Style {
            background: Some(palette.chip_bg.into()),
            border: iced::Border { color: palette.chip_border, width: 1.0, radius: 6.0.into() },
            ..Default::default()
        })
        .into()
}

fn file_chips(file: &MediaFile, palette: &Palette) -> Element<'static, Message> {
    let video_track = file.tracks.iter().find(|t| t.kind == TrackKind::Video);
    let mut chips = row![chip(file.container.clone(), palette)].spacing(6);

    if let Some(v) = video_track {
        if let (Some(w), Some(h)) = (v.width, v.height) {
            chips = chips.push(chip(format!("{w}x{h}"), palette));
        }
        if let Some(fps) = v.fps {
            chips = chips.push(chip(format!("{fps:.3} fps"), palette));
        }
    }
    chips = chips.push(chip(format!("{} tracks", file.tracks.len()), palette));

    let size_gb = file.file_size_bytes as f64 / 1_073_741_824.0;
    chips = chips.push(chip(format!("{size_gb:.1} GB"), palette));

    chips.into()
}

fn file_card<'a>(
    label: &'static str,
    file: &'a Option<MediaFile>,
    picking: bool,
    on_browse: Message,
    palette: &Palette,
) -> Element<'a, Message> {
    let path_text = match file {
        Some(f) => f.path.display().to_string(),
        None => "No file selected".to_string(),
    };

    let browse_press = if picking { None } else { Some(on_browse) };

    let mut card = column![
        row![
            text(label).size(13).color(palette.fg),
            button(row![icons::folder(palette.fg), text("Browse")].spacing(6))
                .on_press_maybe(browse_press),
        ]
        .spacing(10),
        row![icons::video(palette.dim), text(path_text).size(12).color(palette.fg)].spacing(8),
    ]
    .spacing(10);

    if let Some(f) = file {
        card = card.push(file_chips(f, palette));
    }

    container(card)
        .padding(15)
        .style(move |_theme| container::Style {
            background: Some(palette.card.into()),
            border: iced::Border { color: palette.border, width: 1.0, radius: 12.0.into() },
            ..Default::default()
        })
        .into()
}

fn framerate_banner<'a>(state: &'a AppState, palette: &Palette) -> Option<Element<'a, Message>> {
    if let Some(err) = &state.framerate_error {
        return Some(
            row![icons::warning(palette.danger_fg), text(err.to_string()).color(palette.danger_fg)]
                .spacing(8)
                .into(),
        );
    }
    if state.file_a.is_some() && state.file_b.is_some() {
        // Both files present and framerate_error is None means
        // check_framerate already confirmed a match - file_b's own fps
        // isn't needed here, just file_a's, to display as representative.
        let fps_a = state
            .file_a
            .as_ref()
            .and_then(|f| f.tracks.iter().find(|t| t.kind == TrackKind::Video))
            .and_then(|t| t.fps);
        if let Some(fps) = fps_a {
            return Some(
                row![
                    icons::check(palette.success_fg),
                    text(format!("Framerates match — {fps:.3} fps. Safe to align and merge.")).color(palette.success_fg),
                ]
                .spacing(8)
                .into(),
            );
        }
    }
    None
}

pub fn view(state: &AppState, palette: &Palette) -> Element<Message> {
    let mut col = column![
        row![
            file_card("File A · Base", &state.file_a, state.picking_file_a, Message::PickFileA, palette),
            file_card("File B · Donor", &state.file_b, state.picking_file_b, Message::PickFileB, palette),
        ]
        .spacing(14),
    ]
    .spacing(12);

    if let Some(banner) = framerate_banner(state, palette) {
        col = col.push(banner);
    }

    col.into()
}
```

- [ ] **Step 3: Build and fix any remaining `view(state)` call sites**

At this point `track_table::view`, `offset_panel::view`, `extras::view`, and `output_log::view` still have the old one-argument signature — `ui/mod.rs` now calls them with two arguments. This will fail to compile until Tasks 7–10 update those signatures; that's expected and resolved by those tasks, which follow immediately. Run `cargo build -p mediamerger-app` now anyway to confirm `file_pickers.rs` itself compiles correctly in isolation (the *other* call sites' errors are expected and will be Task 7–10's job, not a sign this task is wrong) — check that the only errors reported are about `track_table::view`/`offset_panel::view`/`extras::view`/`output_log::view` argument count, not about anything inside `file_pickers.rs` itself.

- [ ] **Step 4: Commit**

```bash
git add mediamerger-app/src/ui/mod.rs mediamerger-app/src/ui/file_pickers.rs
git commit -m "Restyle source file cards with chips, icons, and framerate success banner"
```

---

## Task 7: `track_table.rs` restyle — track rows with metadata detail lines

**Files:**
- Modify: `mediamerger-app/src/ui/track_table.rs`

**Interfaces:**
- Consumes: `theme::Palette` (Task 4), `ui::icons` (Task 5), enriched `Track` fields + `channel_layout_label` (Task 1)
- Produces: `pub fn view(state: &AppState, palette: &Palette) -> Element<Message>` (signature gains `palette`)

- [ ] **Step 1: Build the detail-line composer and restyled row**

Replace `mediamerger-app/src/ui/track_table.rs`:

```rust
use crate::state::{AppState, Message, TrackUiState};
use crate::theme::Palette;
use crate::ui::icons;
use iced::widget::{button, checkbox, column, container, row, text};
use iced::{Element, Length};
use mediamerger_core::probe::{channel_layout_label, MediaFile, Track, TrackKind};

fn track_detail_line(track: &Track) -> String {
    match track.kind {
        TrackKind::Video => {
            let res = match (track.width, track.height) {
                (Some(w), Some(h)) if h >= 2000 => format!("{}p", h),
                (Some(_), Some(h)) => format!("{h}p"),
                _ => String::new(),
            };
            let dynamic_range = match (track.is_hdr10, track.is_dolby_vision) {
                (true, true) => "HDR10 + Dolby Vision",
                (true, false) => "HDR10",
                (false, true) => "Dolby Vision",
                (false, false) => "SDR",
            };
            [res, dynamic_range.to_string()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")
        }
        TrackKind::Audio => {
            let mut parts = Vec::new();
            if let Some(ch) = track.channels {
                parts.push(channel_layout_label(ch));
            }
            if let Some(rate) = track.sampling_rate {
                parts.push(format!("{} kHz", rate / 1000));
            }
            if let Some(bps) = track.bitrate_bps {
                parts.push(format!("{:.1} kbps", bps as f64 / 1000.0));
            }
            parts.join(" · ")
        }
        TrackKind::Subtitle => String::new(),
    }
}

fn track_row<'a>(
    idx: usize,
    track: &'a Track,
    ui: &TrackUiState,
    palette: &Palette,
    on_toggle: impl Fn(usize) -> Message + 'a,
    on_default: impl Fn(usize, bool) -> Message + 'a,
    on_forced: impl Fn(usize, bool) -> Message + 'a,
) -> Element<'a, Message> {
    let kind_icon = match track.kind {
        TrackKind::Video => icons::video(palette.dim),
        TrackKind::Audio => icons::audio(palette.dim),
        TrackKind::Subtitle => icons::subtitle(palette.dim),
    };

    let lang = track.language.clone().unwrap_or_else(|| "und".to_string());
    let detail = track_detail_line(track);

    let mut info = column![
        row![text(track.codec.clone()).size(13).color(palette.fg), text(lang.to_uppercase()).size(9).color(palette.dim)].spacing(7),
    ]
    .spacing(1);
    if !detail.is_empty() {
        info = info.push(text(detail).size(12).color(palette.faint));
    }

    let def_style = if ui.default_flag { palette.accent_soft } else { palette.chip_bg };
    let forced_style = if ui.forced_flag { palette.accent_soft } else { palette.chip_bg };

    row![
        checkbox("", ui.selected).on_toggle(move |_| on_toggle(idx)),
        kind_icon,
        info.width(Length::Fill),
        button(text("Default").size(10))
            .style(move |_theme, _status| button::Style { background: Some(def_style.into()), ..Default::default() })
            .on_press(on_default(idx, !ui.default_flag)),
        button(text("Forced").size(10))
            .style(move |_theme, _status| button::Style { background: Some(forced_style.into()), ..Default::default() })
            .on_press(on_forced(idx, !ui.forced_flag)),
    ]
    .spacing(11)
    .padding(11)
    .into()
}

fn file_column<'a>(
    file: &'a Option<MediaFile>,
    ui: &'a [TrackUiState],
    palette: &Palette,
    on_toggle: impl Fn(usize) -> Message + Copy + 'a,
    on_default: impl Fn(usize, bool) -> Message + Copy + 'a,
    on_forced: impl Fn(usize, bool) -> Message + Copy + 'a,
) -> Element<'a, Message> {
    match file {
        None => text("No file loaded").color(palette.faint).into(),
        Some(f) => {
            let mut col = column![].spacing(0);
            for (idx, track) in f.tracks.iter().enumerate() {
                let row_ui = ui.get(idx).cloned().unwrap_or_default();
                col = col.push(track_row(idx, track, &row_ui, palette, on_toggle, on_default, on_forced));
            }
            container(col)
                .style(move |_theme| container::Style {
                    background: Some(palette.card.into()),
                    border: iced::Border { color: palette.border, width: 1.0, radius: 12.0.into() },
                    ..Default::default()
                })
                .into()
        }
    }
}

pub fn view(state: &AppState, palette: &Palette) -> Element<Message> {
    row![
        file_column(&state.file_a, &state.tracks_a_ui, palette, Message::ToggleTrackA, Message::SetDefaultFlagA, Message::SetForcedFlagA),
        file_column(&state.file_b, &state.tracks_b_ui, palette, Message::ToggleTrackB, Message::SetDefaultFlagB, Message::SetForcedFlagB),
    ]
    .spacing(16)
    .into()
}
```

Note the checkbox label changed from the previous `track_label(track)` text to an empty string (`checkbox("", ui.selected)`) since the codec/language/detail info now renders as its own styled block next to the checkbox rather than as the checkbox's built-in label — verify this reads acceptably against the real `iced` checkbox widget (an empty-label checkbox should render as just the box), adjusting if the real widget handles an empty label oddly.

- [ ] **Step 2: Build and verify only the expected remaining call-site errors exist**

Run: `cargo build -p mediamerger-app`
Expected: `offset_panel::view`/`extras::view`/`output_log::view` argument-count errors only (resolved by Tasks 8–10); nothing wrong inside `track_table.rs`/`file_pickers.rs` themselves.

- [ ] **Step 3: Commit**

```bash
git add mediamerger-app/src/ui/track_table.rs
git commit -m "Restyle track rows with icons, detail lines, and pill-style flag buttons"
```

---

## Task 8: `offset_panel.rs` restyle — status banner + real waveform + wiring

**Files:**
- Modify: `mediamerger-app/src/ui/offset_panel.rs`
- Modify: `mediamerger-app/src/state.rs` (new `waveform` field + `WaveformExtracted` message)
- Modify: `mediamerger-app/src/main.rs` (wire `OffsetDetected` success to also fetch the waveform)

**Interfaces:**
- Consumes: `theme::Palette` (Task 4), `ui::icons` (Task 5), `mediamerger_core::offset::{extract_waveform, WaveformEnvelope}` + `OffsetResult.early_window_start`/`window_duration` (Task 2)
- Produces: `AppState.waveform: Option<WaveformEnvelope>`, `Message::WaveformExtracted(Result<WaveformEnvelope, MergerError>)`, `pub fn view(state: &AppState, palette: &Palette) -> Element<Message>` (signature gains `palette`)

- [ ] **Step 1: Add the new state field and message variant**

Add to `mediamerger-app/src/state.rs`:

```rust
use mediamerger_core::offset::WaveformEnvelope;
```

Add `pub waveform: Option<WaveformEnvelope>,` to `AppState` and `waveform: None,` to its `Default` impl.

Add `WaveformExtracted(Result<WaveformEnvelope, MergerError>),` to the `Message` enum.

- [ ] **Step 2: Wire `OffsetDetected(Ok(...))` to also fetch the waveform**

Edit the `Message::OffsetDetected` arm in `mediamerger-app/src/main.rs`:

```rust
        Message::OffsetDetected(result) => {
            match result {
                Ok(r) => {
                    state.manual_offset_input = format!("{:.3}", r.offset);
                    let (file_a, file_b) = (state.file_a.clone(), state.file_b.clone());
                    let (Some(file_a), Some(file_b)) = (file_a, file_b) else {
                        state.offset = state::OffsetState::Detected(r);
                        return Task::none();
                    };
                    let Some(track_a) = first_audio_track_id(&file_a) else {
                        state.offset = state::OffsetState::Detected(r);
                        return Task::none();
                    };
                    let Some(track_b) = first_audio_track_id(&file_b) else {
                        state.offset = state::OffsetState::Detected(r);
                        return Task::none();
                    };
                    let (start, duration) = (r.early_window_start, r.window_duration);
                    state.offset = state::OffsetState::Detected(r);
                    Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                mediamerger_core::offset::extract_waveform(&file_a.path, track_a, &file_b.path, track_b, start, duration, 120)
                            })
                            .await
                            .unwrap_or_else(|e| Err(mediamerger_core::error::MergerError::Probe(e.to_string())))
                        },
                        Message::WaveformExtracted,
                    )
                }
                Err(e) => {
                    state.detect_offset_error = Some(e.to_string());
                    state.offset = state::OffsetState::NotDetected;
                    Task::none()
                }
            }
        }
        Message::WaveformExtracted(result) => {
            // A waveform-fetch failure is never surfaced as a user-facing
            // error - the offset itself already succeeded and is fully
            // usable without this supplementary visualization.
            state.waveform = result.ok();
            Task::none()
        }
```

- [ ] **Step 3: Run the workspace test suite to verify nothing regressed**

Run: `cargo test --workspace`
Expected: PASS (this task doesn't change any tested pure logic — `to_merge_plan`/`resolved_offset_secs`/`blocking_reason` tests should be unaffected).

- [ ] **Step 4: Restyle `offset_panel.rs` with the waveform**

Replace `mediamerger-app/src/ui/offset_panel.rs`:

```rust
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
            let (icon, color, bg, headline) = match r.consistency {
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

pub fn view(state: &AppState, palette: &Palette) -> Element<Message> {
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

    container(col)
        .padding(16)
        .style(move |_theme| container::Style {
            background: Some(palette.card.into()),
            border: iced::Border { color: palette.border, width: 1.0, radius: 12.0.into() },
            ..Default::default()
        })
        .into()
}
```

- [ ] **Step 5: Build and verify only the expected remaining call-site errors exist**

Run: `cargo build -p mediamerger-app`
Expected: `extras::view`/`output_log::view` argument-count errors only (Tasks 9–10).

- [ ] **Step 6: Run the workspace test suite**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add mediamerger-app/src/ui/offset_panel.rs mediamerger-app/src/state.rs mediamerger-app/src/main.rs
git commit -m "Restyle sync-offset panel with status banner and real waveform visualization"
```

---

## Task 9: `extras.rs` restyle — segmented chapters control + toggle switches

**Files:**
- Modify: `mediamerger-app/src/ui/extras.rs`

**Interfaces:**
- Consumes: `theme::Palette` (Task 4)
- Produces: `pub fn view(state: &AppState, palette: &Palette) -> Element<Message>` (signature gains `palette`)

No new state or messages — `ChaptersChoiceChanged`/`ToggleAttachmentsA/B`/`ToggleTagsA/B` already exist and do exactly what the restyled controls need.

- [ ] **Step 1: Restyle as a segmented control + toggle-switch-styled checkboxes**

Replace `mediamerger-app/src/ui/extras.rs`:

```rust
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
        row![text("A").size(12).color(palette.dim), checkbox("", a).on_toggle(on_a)].spacing(8),
        row![text("B").size(12).color(palette.dim), checkbox("", b).on_toggle(on_b)].spacing(8),
    ]
    .spacing(18)
    .padding([13, 16])
    .into()
}

pub fn view(state: &AppState, palette: &Palette) -> Element<Message> {
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

    container(column![chapters_row, attachments_row, tags_row].spacing(0))
        .style(move |_theme| iced::widget::container::Style {
            background: Some(palette.card.into()),
            border: iced::Border { color: palette.border, width: 1.0, radius: 12.0.into() },
            ..Default::default()
        })
        .into()
}
```

- [ ] **Step 2: Build and verify only the expected remaining call-site error exists**

Run: `cargo build -p mediamerger-app`
Expected: `output_log::view` argument-count error only (Task 10).

- [ ] **Step 3: Commit**

```bash
git add mediamerger-app/src/ui/extras.rs
git commit -m "Restyle extras section with segmented chapters control and toggle switches"
```

---

## Task 10: `output_log.rs` restyle — footer + collapsible log

**Files:**
- Modify: `mediamerger-app/src/ui/output_log.rs`
- Modify: `mediamerger-app/src/state.rs` (new `log_expanded` field + `ToggleLogExpanded` message)
- Modify: `mediamerger-app/src/main.rs` (`StartMerge` sets `log_expanded = true`; handle `ToggleLogExpanded`)

**Interfaces:**
- Consumes: `theme::Palette` (Task 4), `ui::icons::folder` (Task 5)
- Produces: `AppState.log_expanded: bool` (default `false`), `Message::ToggleLogExpanded`, `pub fn view(state: &AppState, palette: &Palette) -> Element<Message>` (signature gains `palette`)

- [ ] **Step 1: Add the new state field and message variant**

Add `pub log_expanded: bool,` to `AppState` and `log_expanded: false,` to its `Default` impl in `mediamerger-app/src/state.rs`. Add `ToggleLogExpanded,` to `Message`.

- [ ] **Step 2: Wire the new message and auto-expand on merge start**

In `mediamerger-app/src/main.rs`, add near the top of the `Message::StartMerge` arm's body (after the existing guards, before spawning the worker thread): `state.log_expanded = true;`

Add a new match arm:

```rust
        Message::ToggleLogExpanded => {
            state.log_expanded = !state.log_expanded;
            Task::none()
        }
```

- [ ] **Step 3: Restyle `output_log.rs` with a collapsible log**

Replace `mediamerger-app/src/ui/output_log.rs`:

```rust
use crate::state::{AppState, Message};
use crate::theme::Palette;
use crate::ui::icons;
use iced::widget::{button, column, container, row, text};
use iced::{Element, Length};

pub fn view(state: &AppState, palette: &Palette) -> Element<Message> {
    let output_label = match &state.output_path {
        Some(p) => p.display().to_string(),
        None => "No output selected".to_string(),
    };

    let blocking_reason = state.blocking_reason();
    let selected_count = state.tracks_a_ui.iter().filter(|t| t.selected).count() + state.tracks_b_ui.iter().filter(|t| t.selected).count();
    let merge_enabled = blocking_reason.is_none() && selected_count > 0 && state.output_path.is_some();
    let merge_press = if merge_enabled { Some(Message::StartMerge) } else { None };

    let (ready_text, ready_color) = if merge_enabled {
        (format!("{selected_count} tracks selected · ready to merge"), palette.success_fg)
    } else if let Some(reason) = &blocking_reason {
        (format!("Merge blocked: {reason}"), palette.danger_fg)
    } else if selected_count == 0 {
        ("Select at least one track".to_string(), palette.warn_fg)
    } else {
        ("Choose an output file".to_string(), palette.warn_fg)
    };

    let mut col = column![
        row![
            text(output_label).size(12).color(palette.fg).width(Length::Fill),
            button(row![icons::folder(palette.fg), text("Browse")].spacing(6)).on_press(Message::PickOutput),
        ]
        .spacing(10),
        row![
            text(ready_text).size(12).color(ready_color).width(Length::Fill),
            button("Merge").on_press_maybe(merge_press),
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
        col = col.push(container(log_col).padding(8).style(move |_theme| container::Style { background: Some(palette.view.into()), ..Default::default() }));
    }

    col.into()
}
```

- [ ] **Step 4: Build the full workspace — all `view()` signatures should now agree**

Run: `cargo build --workspace`
Expected: builds cleanly, no remaining argument-count errors anywhere.

- [ ] **Step 5: Run the full test suite**

Run: `cargo test --workspace`
Expected: PASS (all core + app tests).

- [ ] **Step 6: Commit**

```bash
git add mediamerger-app/src/ui/output_log.rs mediamerger-app/src/state.rs mediamerger-app/src/main.rs
git commit -m "Restyle footer with ready-to-merge status text and collapsible log"
```

---

## Task 11: Final workspace verification

**Files:** none (verification only)

- [ ] **Step 1: Full workspace build, test, and lint**

Run: `cargo build --workspace`
Expected: clean build.

Run: `cargo test --workspace`
Expected: all tests pass (core probe/offset/mux/error tests, app state tests, end-to-end integration test skip-or-pass).

Run: `cargo clippy --workspace --all-targets`
Expected: no new warnings beyond the project's pre-existing lifetime-elision/field-reassign lint categories. Pay particular attention to any `field is never read` warning on `theme::Palette` — `mediamerger-app` is a binary crate, so unlike `mediamerger-core`'s library types, nothing about a field being `pub` exempts it from dead-code analysis here. `btn_bg`/`btn_hover`/`separator` in particular aren't consumed by every task's minimal restyle in this plan (e.g. no row divider between track rows, no explicit hover styling on the plain "Browse"/"Merge" buttons beyond `iced`'s own default). If any of these warn, either wire them into the relevant view code now (a divider via `iced::widget::horizontal_rule` between track rows for `separator`; explicit background/hover styling on the plain buttons for `btn_bg`/`btn_hover`) or leave a `#[allow(dead_code)]` with a one-line comment if intentionally reserved for a later polish pass — don't leave an unexplained warning.

- [ ] **Step 2: Commit if any fixup was needed**

If Step 1 required any fixes, commit them:

```bash
git add -A
git commit -m "Fix workspace build/lint issues found in final redesign verification"
```

---

## Manual verification (after all tasks)

This redesign is fundamentally visual; automated tests cover the new pure logic (metadata parsing, waveform downsampling, palette/accent color math) but not what the app actually looks like. Before considering this done, run the real app on a real GNOME desktop (this sandbox has no display server) and confirm:

1. Light and dark mode both render with colors matching the mockup, and switching the OS theme live-updates the app within ~10 seconds.
2. All four accent colors (blue/green/purple/orange) are picked up correctly from the GNOME accent-color setting and recolor the accent-dependent elements (Detect Offset button, selected track highlighting, waveform's "A" track bars).
3. The waveform renders proportionally correct bars for a real file pair, and the offset marker's position visually corresponds to the actual detected offset relative to the window duration.
4. Metadata chips and track detail lines show real values (resolution, file size, channel layout, HDR/DV where applicable) and cleanly omit fields a given file doesn't report (e.g. no bitrate shown when mkvmerge didn't report `tag_bps`) rather than showing a blank or wrong value.
5. The framerate-match success banner appears when both files are loaded with compatible framerates, and correctly switches to the mismatch banner (still blocking Detect Offset/Merge, per the existing `blocking_reason` guard) when they aren't.
6. The collapsible log auto-expands when a merge starts and can be manually toggled afterward.
7. All icons render at a reasonable size/color and don't appear as solid black boxes or blank space (which would indicate the SVG color-filter approach from Task 5 didn't work as assumed against the real `iced` version — see that task's note to verify the real API before relying on the sketch).

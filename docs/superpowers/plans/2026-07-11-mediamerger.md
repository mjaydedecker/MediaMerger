# MediaMerger Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Linux GUI app that lets a user pick tracks from two encodes of the same movie, detects the time offset between them via audio cross-correlation, and muxes the selection into one synced `.mkv` with `mkvmerge`.

**Architecture:** A two-crate Rust workspace — `mediamerger-core` (probing, offset detection, mkvmerge command building/execution; no GUI deps, fully unit-testable) and `mediamerger-app` (an `iced` GUI binary named `mediamerger`), mirroring the shape of the sibling project [MediaNamer](https://github.com/mjaydedecker/MediaNamer).

**Tech Stack:** Rust, `iced` 0.14 (`tokio` feature), `tokio`, `rfd` (`xdg-portal` feature), `dark-light`, `rustfft`, `serde`/`serde_json`. Runtime dependencies (not bundled): `ffmpeg`/`ffprobe`, `mkvtoolnix` (`mkvmerge`).

## Global Constraints

- File A is always the sync reference (timeline zero); any track pulled from File B gets the computed delay. (spec: "Reference file")
- If File A and File B's video framerates differ beyond a small tolerance, the app detects this and blocks the whole workflow before offset detection — no speed/tempo correction is implemented. (spec: "FPS mismatch", "Non-goals")
- Offset consistency (measured near the 25–35% mark vs. the 65–75% mark) must be within 50ms to auto-apply; otherwise the app warns and requires manual confirmation — it never silently proceeds on disagreeing data. (spec: "Drift handling")
- Track selection is fully flexible: any combination of video/audio/subtitle tracks from either file can be chosen for the output. (spec: "Track scope")
- Chapters/attachments/tags are explicit per-file checkboxes/radio, not automatic. (spec: "Extras")
- No Python runtime dependency anywhere — the cross-correlation engine is native Rust (`rustfft`), not a shelled-out Python tool. (spec: "Offset engine")
- Packaging follows MediaNamer's exact `.deb`/`.rpm` metadata pattern, declaring `mkvtoolnix` and `ffmpeg` as package dependencies rather than bundling them.

---

## Task 1: Workspace scaffolding + `MergerError`

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `mediamerger-core/Cargo.toml`
- Create: `mediamerger-core/src/lib.rs`
- Create: `mediamerger-core/src/error.rs`
- Test: inline in `mediamerger-core/src/error.rs`

**Interfaces:**
- Produces: `mediamerger_core::error::MergerError` — `#[derive(Debug, Clone)]` enum with variants `Probe(String)`, `FramerateMismatch { file_a_fps: f64, file_b_fps: f64 }`, `FfmpegNotFound`, `MkvmergeNotFound`, `MuxFailed(String)`. Implements `std::fmt::Display` and `std::error::Error`.

- [ ] **Step 1: Create the workspace root `Cargo.toml`**

`mediamerger-app` isn't created until Task 9 — listing it as a member now would break `cargo build`/`cargo test` for anyone checking out this commit alone, since Cargo resolves the whole workspace manifest before scoping to `-p mediamerger-core`. List only `mediamerger-core` for now; Task 9 adds `mediamerger-app` to this list when it creates that crate.

```toml
[workspace]
members = ["mediamerger-core"]
resolver = "2"
```

- [ ] **Step 2: Create `mediamerger-core/Cargo.toml`**

```toml
[package]
name = "mediamerger-core"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rustfft = "6"
```

- [ ] **Step 3: Create `mediamerger-core/src/lib.rs`**

```rust
pub mod error;
```

- [ ] **Step 4: Write the failing test for `MergerError`'s Display output**

Create `mediamerger-core/src/error.rs`:

```rust
use std::fmt;

#[derive(Debug, Clone)]
pub enum MergerError {
    Probe(String),
    FramerateMismatch { file_a_fps: f64, file_b_fps: f64 },
    FfmpegNotFound,
    MkvmergeNotFound,
    MuxFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framerate_mismatch_message_names_both_values() {
        let err = MergerError::FramerateMismatch { file_a_fps: 23.976, file_b_fps: 25.0 };
        let msg = err.to_string();
        assert!(msg.contains("23.976"), "message was: {msg}");
        assert!(msg.contains("25.000") || msg.contains("25"), "message was: {msg}");
    }
}
```

- [ ] **Step 5: Run test to verify it fails**

Run: `cargo test -p mediamerger-core framerate_mismatch_message_names_both_values`
Expected: FAIL — `MergerError` does not implement `Display`/`to_string`.

- [ ] **Step 6: Implement `Display` and `Error`**

Add above the `#[cfg(test)]` module in `mediamerger-core/src/error.rs`:

```rust
impl fmt::Display for MergerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MergerError::Probe(msg) => write!(f, "failed to probe media file: {msg}"),
            MergerError::FramerateMismatch { file_a_fps, file_b_fps } => write!(
                f,
                "video framerates differ (File A: {file_a_fps:.3} fps, File B: {file_b_fps:.3} fps); a single fixed offset cannot hold"
            ),
            MergerError::FfmpegNotFound => write!(f, "ffmpeg/ffprobe not found on PATH"),
            MergerError::MkvmergeNotFound => write!(f, "mkvmerge not found on PATH"),
            MergerError::MuxFailed(msg) => write!(f, "mkvmerge failed: {msg}"),
        }
    }
}

impl std::error::Error for MergerError {}
```

- [ ] **Step 7: Run test to verify it passes**

Run: `cargo test -p mediamerger-core framerate_mismatch_message_names_both_values`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml mediamerger-core/Cargo.toml mediamerger-core/src/lib.rs mediamerger-core/src/error.rs
git commit -m "Scaffold workspace and add MergerError"
```

---

## Task 2: `probe` module — mkvmerge JSON identification

**Files:**
- Create: `mediamerger-core/src/probe.rs`
- Modify: `mediamerger-core/src/lib.rs` (add `pub mod probe;`)
- Test: inline in `mediamerger-core/src/probe.rs`

**Interfaces:**
- Consumes: `mediamerger_core::error::MergerError` (Task 1)
- Produces: `TrackKind` (`#[derive(Debug, Clone, Copy, PartialEq, Eq)]`, variants `Video`, `Audio`, `Subtitle`), `Track { id: u64, kind: TrackKind, codec: String, language: Option<String>, name: Option<String>, default_flag: bool, forced_flag: bool, fps: Option<f64>, channels: Option<u32> }` (`#[derive(Debug, Clone)]`), `MediaFile { path: PathBuf, container: String, tracks: Vec<Track> }` (`#[derive(Debug, Clone)]`), `pub fn identify(path: &Path) -> Result<MediaFile, MergerError>`.

- [ ] **Step 1: Write the failing test for JSON parsing**

Create `mediamerger-core/src/probe.rs`:

```rust
use crate::error::MergerError;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Video,
    Audio,
    Subtitle,
}

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
}

#[derive(Debug, Clone)]
pub struct MediaFile {
    pub path: PathBuf,
    pub container: String,
    pub tracks: Vec<Track>,
}

#[derive(Deserialize)]
struct MkvmergeJson {
    container: MkvmergeContainer,
    tracks: Vec<MkvmergeTrack>,
}

#[derive(Deserialize)]
struct MkvmergeContainer {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
struct MkvmergeTrack {
    id: u64,
    #[serde(rename = "type")]
    kind: String,
    codec: String,
    properties: MkvmergeTrackProperties,
}

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
}

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
            })
        })
        .collect();

    Ok(MediaFile { path: path.to_path_buf(), container: parsed.container.kind, tracks })
}

pub fn identify(path: &Path) -> Result<MediaFile, MergerError> {
    let output = Command::new("mkvmerge")
        .arg("-J")
        .arg(path)
        .output()
        .map_err(|_| MergerError::MkvmergeNotFound)?;

    if !output.status.success() {
        return Err(MergerError::Probe(String::from_utf8_lossy(&output.stderr).to_string()));
    }

    parse_mkvmerge_json(&output.stdout, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_video_audio_subtitle_tracks() {
        let json = br#"{
            "container": {"type": "Matroska"},
            "tracks": [
                {"id":0,"type":"video","codec":"MPEG-4p10/AVC/h.264","properties":{"default_track":true,"forced_track":false,"default_duration":41708333}},
                {"id":1,"type":"audio","codec":"AC-3","properties":{"default_track":true,"forced_track":false,"language":"eng","audio_channels":6}},
                {"id":2,"type":"subtitles","codec":"SubRip/SRT","properties":{"default_track":false,"forced_track":false,"language":"fre","track_name":"Forced"}}
            ]
        }"#;

        let media = parse_mkvmerge_json(json, Path::new("test.mkv")).unwrap();

        assert_eq!(media.container, "Matroska");
        assert_eq!(media.tracks.len(), 3);

        assert_eq!(media.tracks[0].kind, TrackKind::Video);
        assert!((media.tracks[0].fps.unwrap() - 23.976).abs() < 0.01);

        assert_eq!(media.tracks[1].kind, TrackKind::Audio);
        assert_eq!(media.tracks[1].channels, Some(6));
        assert_eq!(media.tracks[1].language.as_deref(), Some("eng"));

        assert_eq!(media.tracks[2].kind, TrackKind::Subtitle);
        assert_eq!(media.tracks[2].language.as_deref(), Some("fre"));
        assert_eq!(media.tracks[2].name.as_deref(), Some("Forced"));
    }
}
```

- [ ] **Step 2: Add the module to `lib.rs`**

Edit `mediamerger-core/src/lib.rs`:

```rust
pub mod error;
pub mod probe;
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p mediamerger-core parses_video_audio_subtitle_tracks`
Expected: FAIL (compile error, `serde` not yet usable, or logic wrong) — since the code above already includes the implementation, run this first to confirm the test harness itself is wired up; if it compiles and passes immediately, skip to Step 5.

- [ ] **Step 4: Fix until it passes**

If Step 3 fails on a compile error, double check `serde`'s `derive` feature is enabled in `mediamerger-core/Cargo.toml` (added in Task 1, Step 2).

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p mediamerger-core parses_video_audio_subtitle_tracks`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add mediamerger-core/src/lib.rs mediamerger-core/src/probe.rs
git commit -m "Add mkvmerge JSON identification to probe module"
```

---

## Task 3: `probe` module — framerate check and duration

**Files:**
- Modify: `mediamerger-core/src/probe.rs`

**Interfaces:**
- Consumes: `MergerError` (Task 1)
- Produces: `pub fn check_framerate(file_a: &Path, file_b: &Path) -> Result<(), MergerError>`, `pub fn duration_secs(path: &Path) -> Result<f64, MergerError>`

- [ ] **Step 1: Write the failing tests for the pure parsing helpers**

Add to the `tests` module in `mediamerger-core/src/probe.rs`:

```rust
    #[test]
    fn parses_ntsc_frame_rate_fraction() {
        let fps = parse_r_frame_rate(b"24000/1001\n").unwrap();
        assert!((fps - 23.976).abs() < 0.001, "got {fps}");
    }

    #[test]
    fn parses_integer_frame_rate_fraction() {
        let fps = parse_r_frame_rate(b"25/1\n").unwrap();
        assert!((fps - 25.0).abs() < 0.001, "got {fps}");
    }

    #[test]
    fn frame_rates_within_tolerance_match() {
        assert!(fps_within_tolerance(23.976, 23.98));
        assert!(!fps_within_tolerance(23.976, 25.0));
    }

    #[test]
    fn parses_duration_seconds() {
        let secs = parse_duration_output(b"7261.234000\n").unwrap();
        assert!((secs - 7261.234).abs() < 0.001, "got {secs}");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mediamerger-core probe::tests`
Expected: FAIL — `parse_r_frame_rate`, `fps_within_tolerance`, `parse_duration_output` don't exist yet.

- [ ] **Step 3: Implement the parsing helpers and the public functions**

Add to `mediamerger-core/src/probe.rs` (above the `#[cfg(test)]` module):

```rust
fn parse_r_frame_rate(bytes: &[u8]) -> Result<f64, MergerError> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim();
    let (num, den) = text
        .split_once('/')
        .ok_or_else(|| MergerError::Probe(format!("unexpected r_frame_rate output: {text}")))?;
    let num: f64 = num
        .parse()
        .map_err(|_| MergerError::Probe(format!("bad numerator in r_frame_rate: {text}")))?;
    let den: f64 = den
        .parse()
        .map_err(|_| MergerError::Probe(format!("bad denominator in r_frame_rate: {text}")))?;
    if den == 0.0 {
        return Err(MergerError::Probe(format!("zero denominator in r_frame_rate: {text}")));
    }
    Ok(num / den)
}

fn fps_within_tolerance(a: f64, b: f64) -> bool {
    (a - b).abs() <= 0.05
}

fn parse_duration_output(bytes: &[u8]) -> Result<f64, MergerError> {
    let text = String::from_utf8_lossy(bytes);
    text.trim()
        .parse()
        .map_err(|_| MergerError::Probe(format!("unexpected duration output: {}", text.trim())))
}

fn ffprobe_video_fps(path: &Path) -> Result<f64, MergerError> {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=r_frame_rate", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .map_err(|_| MergerError::FfmpegNotFound)?;
    if !output.status.success() {
        return Err(MergerError::Probe(String::from_utf8_lossy(&output.stderr).to_string()));
    }
    parse_r_frame_rate(&output.stdout)
}

pub fn check_framerate(file_a: &Path, file_b: &Path) -> Result<(), MergerError> {
    let fps_a = ffprobe_video_fps(file_a)?;
    let fps_b = ffprobe_video_fps(file_b)?;
    if !fps_within_tolerance(fps_a, fps_b) {
        return Err(MergerError::FramerateMismatch { file_a_fps: fps_a, file_b_fps: fps_b });
    }
    Ok(())
}

pub fn duration_secs(path: &Path) -> Result<f64, MergerError> {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .map_err(|_| MergerError::FfmpegNotFound)?;
    if !output.status.success() {
        return Err(MergerError::Probe(String::from_utf8_lossy(&output.stderr).to_string()));
    }
    parse_duration_output(&output.stdout)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mediamerger-core probe::tests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add mediamerger-core/src/probe.rs
git commit -m "Add framerate mismatch check and duration probing"
```

---

## Task 4: `offset` module — PCM window extraction

**Files:**
- Create: `mediamerger-core/src/offset.rs`
- Modify: `mediamerger-core/src/lib.rs` (add `pub mod offset;`)

**Interfaces:**
- Consumes: `MergerError` (Task 1)
- Produces: `pub const SAMPLE_RATE_HZ: u32 = 16000;`, `pub fn extract_window(path: &Path, track_id: u64, start_secs: f64, duration_secs: f64) -> Result<Vec<f32>, MergerError>`

- [ ] **Step 1: Write the failing test for the pure byte-decoding helper**

Create `mediamerger-core/src/offset.rs`:

```rust
use crate::error::MergerError;
use std::path::Path;
use std::process::Command;

pub const SAMPLE_RATE_HZ: u32 = 16000;

fn bytes_to_f32_samples(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

pub fn extract_window(
    path: &Path,
    track_id: u64,
    start_secs: f64,
    duration_secs: f64,
) -> Result<Vec<f32>, MergerError> {
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-ss"])
        .arg(start_secs.to_string())
        .arg("-t")
        .arg(duration_secs.to_string())
        .arg("-i")
        .arg(path)
        .args(["-map", &format!("0:{track_id}"), "-vn", "-ac", "1", "-ar", &SAMPLE_RATE_HZ.to_string(), "-f", "f32le", "-"])
        .output()
        .map_err(|_| MergerError::FfmpegNotFound)?;

    if !output.status.success() {
        return Err(MergerError::Probe(String::from_utf8_lossy(&output.stderr).to_string()));
    }

    Ok(bytes_to_f32_samples(&output.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_little_endian_f32_samples() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.extend_from_slice(&(-0.5f32).to_le_bytes());
        bytes.extend_from_slice(&0.25f32.to_le_bytes());

        let samples = bytes_to_f32_samples(&bytes);

        assert_eq!(samples, vec![1.0, -0.5, 0.25]);
    }

    #[test]
    fn drops_trailing_partial_sample() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.push(0); // 1 stray byte, not a full f32

        let samples = bytes_to_f32_samples(&bytes);

        assert_eq!(samples, vec![1.0]);
    }
}
```

- [ ] **Step 2: Add the module to `lib.rs`**

Edit `mediamerger-core/src/lib.rs`:

```rust
pub mod error;
pub mod offset;
pub mod probe;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p mediamerger-core offset::tests`
Expected: FAIL if the module isn't wired up yet; since the implementation is included above, this mainly confirms compilation.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mediamerger-core offset::tests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add mediamerger-core/src/lib.rs mediamerger-core/src/offset.rs
git commit -m "Add PCM window extraction to offset module"
```

---

## Task 5: `offset` module — FFT cross-correlation

**Files:**
- Modify: `mediamerger-core/src/offset.rs`

**Interfaces:**
- Produces: `pub fn cross_correlate(a: &[f32], b: &[f32], sample_rate: f64) -> (f64, f32)` — returns `(offset_secs, confidence)`. **Contract:** a positive `offset_secs` means the shared content occurs *later* in `b`'s own timeline than in `a`'s (i.e. `b` lags `a`; to align, `b` needs a *negative* delay applied when muxed — this inversion happens in `mux::build_command`, Task 7).

- [ ] **Step 1: Write the failing tests that pin the offset sign and magnitude**

Add to `mediamerger-core/src/offset.rs`, above the existing `#[cfg(test)]` module (or add these functions to it — either location, as long as both test functions below are added):

```rust
use rustfft::{num_complex::Complex32, FftPlanner};

pub fn cross_correlate(a: &[f32], b: &[f32], sample_rate: f64) -> (f64, f32) {
    let n = (a.len() + b.len()).next_power_of_two();

    let mut buf_a: Vec<Complex32> = a.iter().map(|&x| Complex32::new(x, 0.0)).collect();
    buf_a.resize(n, Complex32::new(0.0, 0.0));
    let mut buf_b: Vec<Complex32> = b.iter().map(|&x| Complex32::new(x, 0.0)).collect();
    buf_b.resize(n, Complex32::new(0.0, 0.0));

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(n);
    fft.process(&mut buf_a);
    fft.process(&mut buf_b);

    let mut cross: Vec<Complex32> = buf_a
        .iter()
        .zip(buf_b.iter())
        .map(|(fa, fb)| {
            let prod = fa * fb.conj();
            let mag = prod.norm();
            if mag > 1e-12 { prod / mag } else { Complex32::new(0.0, 0.0) }
        })
        .collect();

    let ifft = planner.plan_fft_inverse(n);
    ifft.process(&mut cross);

    let mags: Vec<f32> = cross.iter().map(|c| c.norm()).collect();
    let (peak_idx, &peak_val) = mags
        .iter()
        .enumerate()
        .max_by(|(_, x), (_, y)| x.total_cmp(y))
        .expect("mags is non-empty");

    let sum: f32 = mags.iter().sum();
    let mean_other = (sum - peak_val) / (mags.len() as f32 - 1.0).max(1.0);
    let confidence = if mean_other > 1e-9 { peak_val / mean_other } else { peak_val };

    // NOTE ON SIGN: this lag convention is verified by the tests below, not by
    // derivation. If `positive_offset_means_b_lags_a` fails with the correct
    // magnitude but flipped sign, negate `lag` here — the test is the source
    // of truth for the convention documented on this function, not this comment.
    let lag = if peak_idx > n / 2 { peak_idx as i64 - n as i64 } else { peak_idx as i64 };
    let offset_secs = lag as f64 / sample_rate;

    (offset_secs, confidence)
}

#[cfg(test)]
mod cross_correlate_tests {
    use super::*;

    fn synthetic_signal(len: usize) -> Vec<f32> {
        (0..len).map(|i| ((i as f32) * 0.1).sin() + ((i as f32) * 0.031).sin() * 0.5).collect()
    }

    #[test]
    fn positive_offset_means_b_lags_a() {
        let sample_rate = 1000.0;
        let base = synthetic_signal(1000);
        let shift = 137usize;

        let a = base.clone();
        let mut b = vec![0.0f32; shift];
        b.extend_from_slice(&base);

        let (offset_secs, confidence) = cross_correlate(&a, &b, sample_rate);
        let expected = shift as f64 / sample_rate;

        assert!((offset_secs - expected).abs() < 0.01, "offset {offset_secs} expected {expected}");
        assert!(confidence > 3.0, "confidence too low: {confidence}");
    }

    #[test]
    fn low_confidence_for_uncorrelated_noise() {
        let mut state = 12345u32;
        let mut next = move || {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            (state as f32 / u32::MAX as f32) * 2.0 - 1.0
        };
        let noise_a: Vec<f32> = (0..2000).map(|_| next()).collect();
        let noise_b: Vec<f32> = (0..2000).map(|_| next()).collect();
        let (_, noise_confidence) = cross_correlate(&noise_a, &noise_b, 1000.0);

        let signal = synthetic_signal(2000);
        let (_, signal_confidence) = cross_correlate(&signal, &signal, 1000.0);

        assert!(
            noise_confidence < signal_confidence,
            "noise confidence {noise_confidence} should be less than matched-signal confidence {signal_confidence}"
        );
    }
}
```

- [ ] **Step 2: Add `rustfft` import check and run tests to verify they fail or pass**

Run: `cargo test -p mediamerger-core cross_correlate_tests`
Expected: If `positive_offset_means_b_lags_a` fails only on sign (magnitude correct, sign flipped), negate the `lag` computation per the comment in Step 1 and re-run. If it fails on magnitude, double-check `shift`/`sample_rate` arithmetic matches the test.

- [ ] **Step 3: Iterate until both tests pass**

Run: `cargo test -p mediamerger-core cross_correlate_tests`
Expected: PASS for both `positive_offset_means_b_lags_a` and `low_confidence_for_uncorrelated_noise`.

- [ ] **Step 4: Commit**

```bash
git add mediamerger-core/src/offset.rs
git commit -m "Add FFT-based GCC-PHAT cross-correlation for offset detection"
```

---

## Task 6: `offset` module — windowed detection with consistency check

**Files:**
- Modify: `mediamerger-core/src/offset.rs`

**Interfaces:**
- Consumes: `extract_window`, `cross_correlate`, `SAMPLE_RATE_HZ` (Tasks 4–5), `probe::duration_secs` (Task 3)
- Produces: `Consistency` (`#[derive(Debug, Clone, Copy, PartialEq)]`, variants `Consistent`, `Inconsistent`, `Unverified`), `OffsetResult { early_offset: f64, late_offset: f64, consistency: Consistency, confidence: f32, offset: f64 }` (`#[derive(Debug, Clone, Copy, PartialEq)]`), `pub fn detect_offset(file_a: &Path, audio_track_a: u64, file_b: &Path, audio_track_b: u64) -> Result<OffsetResult, MergerError>`

- [ ] **Step 1: Write the failing test for the pure window-picking helper**

Add to `mediamerger-core/src/offset.rs`:

```rust
const CONSISTENCY_TOLERANCE_SECS: f64 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Consistency {
    Consistent,
    Inconsistent,
    Unverified,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OffsetResult {
    pub early_offset: f64,
    pub late_offset: f64,
    pub consistency: Consistency,
    pub confidence: f32,
    pub offset: f64,
}

fn pick_windows(shorter_duration: f64) -> (f64, f64, f64) {
    let window = 180.0_f64.min(shorter_duration * 0.1).max(5.0);
    if shorter_duration >= 1200.0 {
        (shorter_duration * 0.30, shorter_duration * 0.70, window)
    } else {
        (shorter_duration * 0.20, shorter_duration * 0.80, window)
    }
}

#[cfg(test)]
mod detect_offset_tests {
    use super::*;

    #[test]
    fn long_file_uses_30_70_split_with_full_window() {
        let (early, late, window) = pick_windows(3600.0);
        assert_eq!(early, 1080.0);
        assert_eq!(late, 2520.0);
        assert_eq!(window, 180.0);
    }

    #[test]
    fn short_file_uses_20_80_split_with_smaller_window() {
        let (early, late, window) = pick_windows(300.0);
        assert_eq!(early, 60.0);
        assert_eq!(late, 240.0);
        assert!(window < 180.0, "window {window} should be smaller than the default cap");
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p mediamerger-core detect_offset_tests`
Expected: PASS (these are pure arithmetic, should pass immediately given the implementation above — if not, fix `pick_windows`).

- [ ] **Step 3: Implement `detect_offset`**

Add to `mediamerger-core/src/offset.rs`:

```rust
use crate::probe;

fn measure_at(
    file_a: &Path,
    track_a: u64,
    file_b: &Path,
    track_b: u64,
    start: f64,
    window: f64,
) -> Result<(f64, f32), MergerError> {
    let a = extract_window(file_a, track_a, start, window)?;
    let b = extract_window(file_b, track_b, start, window)?;
    Ok(cross_correlate(&a, &b, SAMPLE_RATE_HZ as f64))
}

pub fn detect_offset(
    file_a: &Path,
    audio_track_a: u64,
    file_b: &Path,
    audio_track_b: u64,
) -> Result<OffsetResult, MergerError> {
    let duration_a = probe::duration_secs(file_a)?;
    let duration_b = probe::duration_secs(file_b)?;
    let shorter = duration_a.min(duration_b);

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
    })
}
```

- [ ] **Step 4: Run the full core test suite to verify nothing regressed**

Run: `cargo test -p mediamerger-core`
Expected: PASS (all tests from Tasks 1–6).

- [ ] **Step 5: Commit**

```bash
git add mediamerger-core/src/offset.rs
git commit -m "Add windowed offset detection with drift consistency check"
```

---

## Task 7: `mux` module — pure command builder

**Files:**
- Create: `mediamerger-core/src/mux.rs`
- Modify: `mediamerger-core/src/lib.rs` (add `pub mod mux;`)

**Interfaces:**
- Consumes: `probe::TrackKind` (Task 2)
- Produces: `TrackSelection { track_id: u64, kind: TrackKind, set_default: bool, set_forced: bool }` (`#[derive(Debug, Clone)]`), `ChapterSource` (`#[derive(Debug, Clone, Copy, PartialEq)]`, variants `FileA`, `FileB`, `None`), `MergePlan { file_a: PathBuf, file_b: PathBuf, tracks_from_a: Vec<TrackSelection>, tracks_from_b: Vec<TrackSelection>, offset_secs: f64, chapters: ChapterSource, attachments_from_a: bool, attachments_from_b: bool, tags_from_a: bool, tags_from_b: bool, output_path: PathBuf }` (`#[derive(Debug, Clone)]`), `pub fn build_command(plan: &MergePlan) -> Vec<String>`

- [ ] **Step 1: Write the failing test for the simple video-A/audio-B case**

Create `mediamerger-core/src/mux.rs`:

```rust
use crate::probe::TrackKind;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct TrackSelection {
    pub track_id: u64,
    pub kind: TrackKind,
    pub set_default: bool,
    pub set_forced: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChapterSource {
    FileA,
    FileB,
    None,
}

#[derive(Debug, Clone)]
pub struct MergePlan {
    pub file_a: PathBuf,
    pub file_b: PathBuf,
    pub tracks_from_a: Vec<TrackSelection>,
    pub tracks_from_b: Vec<TrackSelection>,
    pub offset_secs: f64,
    pub chapters: ChapterSource,
    pub attachments_from_a: bool,
    pub attachments_from_b: bool,
    pub tags_from_a: bool,
    pub tags_from_b: bool,
    pub output_path: PathBuf,
}

fn push_track_selection_args(args: &mut Vec<String>, selections: &[TrackSelection]) {
    for kind in [TrackKind::Video, TrackKind::Audio, TrackKind::Subtitle] {
        let ids: Vec<String> = selections
            .iter()
            .filter(|s| s.kind == kind)
            .map(|s| s.track_id.to_string())
            .collect();
        let (keep_flag, exclude_flag) = match kind {
            TrackKind::Video => ("--video-tracks", "--no-video"),
            TrackKind::Audio => ("--audio-tracks", "--no-audio"),
            TrackKind::Subtitle => ("--subtitle-tracks", "--no-subtitles"),
        };
        if ids.is_empty() {
            args.push(exclude_flag.to_string());
        } else {
            args.push(keep_flag.to_string());
            args.push(ids.join(","));
        }
    }
}

pub fn build_command(plan: &MergePlan) -> Vec<String> {
    let mut args = Vec::new();

    push_track_selection_args(&mut args, &plan.tracks_from_a);
    for sel in &plan.tracks_from_a {
        if sel.set_default {
            args.push("--default-track-flag".into());
            args.push(format!("{}:yes", sel.track_id));
        }
        if sel.set_forced {
            args.push("--forced-display-flag".into());
            args.push(format!("{}:yes", sel.track_id));
        }
    }
    if plan.chapters != ChapterSource::FileA {
        args.push("--no-chapters".into());
    }
    if !plan.attachments_from_a {
        args.push("--no-attachments".into());
    }
    if !plan.tags_from_a {
        args.push("--no-global-tags".into());
        args.push("--no-track-tags".into());
    }
    args.push(plan.file_a.to_string_lossy().into_owned());

    push_track_selection_args(&mut args, &plan.tracks_from_b);
    for sel in &plan.tracks_from_b {
        if sel.set_default {
            args.push("--default-track-flag".into());
            args.push(format!("{}:yes", sel.track_id));
        }
        if sel.set_forced {
            args.push("--forced-display-flag".into());
            args.push(format!("{}:yes", sel.track_id));
        }
        // File B's shared content occurs `offset_secs` later than File A's
        // (per cross_correlate's contract, Task 5). To align it, apply the
        // *negative* of that offset as this track's mkvmerge delay.
        let delay_ms = (-plan.offset_secs * 1000.0).round() as i64;
        args.push("--sync".into());
        args.push(format!("{}:{}", sel.track_id, delay_ms));
    }
    if plan.chapters != ChapterSource::FileB {
        args.push("--no-chapters".into());
    }
    if !plan.attachments_from_b {
        args.push("--no-attachments".into());
    }
    if !plan.tags_from_b {
        args.push("--no-global-tags".into());
        args.push("--no-track-tags".into());
    }
    args.push(plan.file_b.to_string_lossy().into_owned());

    args.push("-o".into());
    args.push(plan.output_path.to_string_lossy().into_owned());

    let mut order_parts = Vec::new();
    for sel in &plan.tracks_from_a {
        order_parts.push(format!("0:{}", sel.track_id));
    }
    for sel in &plan.tracks_from_b {
        order_parts.push(format!("1:{}", sel.track_id));
    }
    args.push("--track-order".into());
    args.push(order_parts.join(","));

    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_of(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn builds_command_for_simple_video_a_audio_b_case() {
        let plan = MergePlan {
            file_a: PathBuf::from("video_source.mkv"),
            file_b: PathBuf::from("audio_source.mkv"),
            tracks_from_a: vec![TrackSelection {
                track_id: 0,
                kind: TrackKind::Video,
                set_default: true,
                set_forced: false,
            }],
            tracks_from_b: vec![TrackSelection {
                track_id: 1,
                kind: TrackKind::Audio,
                set_default: true,
                set_forced: false,
            }],
            offset_secs: 2.348,
            chapters: ChapterSource::FileA,
            attachments_from_a: false,
            attachments_from_b: false,
            tags_from_a: false,
            tags_from_b: false,
            output_path: PathBuf::from("output.mkv"),
        };

        let args = build_command(&plan);

        assert_eq!(
            args,
            args_of(&[
                "--video-tracks", "0",
                "--no-audio",
                "--no-subtitles",
                "--default-track-flag", "0:yes",
                "--no-attachments",
                "--no-global-tags", "--no-track-tags",
                "video_source.mkv",
                "--no-video",
                "--audio-tracks", "1",
                "--no-subtitles",
                "--default-track-flag", "1:yes",
                "--sync", "1:-2348",
                "--no-chapters",
                "--no-attachments",
                "--no-global-tags", "--no-track-tags",
                "audio_source.mkv",
                "-o", "output.mkv",
                "--track-order", "0:0,1:1",
            ])
        );
    }

    #[test]
    fn no_chapters_for_both_and_attachments_kept_for_b_with_negative_offset() {
        let plan = MergePlan {
            file_a: PathBuf::from("a.mkv"),
            file_b: PathBuf::from("b.mkv"),
            tracks_from_a: vec![TrackSelection {
                track_id: 0,
                kind: TrackKind::Video,
                set_default: false,
                set_forced: false,
            }],
            tracks_from_b: vec![TrackSelection {
                track_id: 2,
                kind: TrackKind::Subtitle,
                set_default: false,
                set_forced: true,
            }],
            offset_secs: -0.5,
            chapters: ChapterSource::None,
            attachments_from_a: false,
            attachments_from_b: true,
            tags_from_a: false,
            tags_from_b: false,
            output_path: PathBuf::from("out.mkv"),
        };

        let args = build_command(&plan);

        assert_eq!(args.iter().filter(|a| a.as_str() == "--no-chapters").count(), 2);
        assert_eq!(args.iter().filter(|a| a.as_str() == "--no-attachments").count(), 1);
        assert_eq!(args.iter().filter(|a| a.as_str() == "--forced-display-flag").count(), 1);

        let sync_idx = args.iter().position(|a| a == "--sync").expect("--sync present");
        assert_eq!(args[sync_idx + 1], "2:500");
    }
}
```

- [ ] **Step 2: Add the module to `lib.rs`**

Edit `mediamerger-core/src/lib.rs`:

```rust
pub mod error;
pub mod mux;
pub mod offset;
pub mod probe;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p mediamerger-core mux::tests`
Expected: FAIL until the module is wired in; since the implementation is included above, this run mainly confirms compilation and exact argument ordering.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mediamerger-core mux::tests`
Expected: PASS for both `builds_command_for_simple_video_a_audio_b_case` and `no_chapters_for_both_and_attachments_kept_for_b_with_negative_offset`.

- [ ] **Step 5: Commit**

```bash
git add mediamerger-core/src/lib.rs mediamerger-core/src/mux.rs
git commit -m "Add pure mkvmerge command builder"
```

---

## Task 8: `mux` module — process execution with progress parsing

**Files:**
- Modify: `mediamerger-core/src/mux.rs`

**Interfaces:**
- Consumes: `MergerError` (Task 1), `build_command`'s output (Task 7)
- Produces: `MuxEvent` (`#[derive(Debug, Clone, PartialEq)]`, variants `Progress(f32)`, `Log(String)`), `pub fn run_mux(args: &[String], on_event: impl FnMut(MuxEvent)) -> Result<(), MergerError>`

- [ ] **Step 1: Write the failing test for the pure line parser**

Add to `mediamerger-core/src/mux.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum MuxEvent {
    Progress(f32),
    Log(String),
}

fn parse_line(line: &str) -> MuxEvent {
    if let Some(rest) = line.strip_prefix("#GUI#progress ") {
        if let Ok(pct) = rest.trim().trim_end_matches('%').parse::<f32>() {
            return MuxEvent::Progress(pct / 100.0);
        }
    }
    MuxEvent::Log(line.to_string())
}
```

Add to the `tests` module:

```rust
    #[test]
    fn parses_gui_progress_line() {
        assert_eq!(parse_line("#GUI#progress 42%"), MuxEvent::Progress(0.42));
    }

    #[test]
    fn treats_other_lines_as_log() {
        assert_eq!(
            parse_line("Warning: some warning text"),
            MuxEvent::Log("Warning: some warning text".to_string())
        );
    }
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p mediamerger-core mux::tests`
Expected: PASS (pure parsing, no subprocess involved).

- [ ] **Step 3: Implement `run_mux`**

Add to `mediamerger-core/src/mux.rs`:

```rust
use crate::error::MergerError;
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};

pub fn run_mux(args: &[String], mut on_event: impl FnMut(MuxEvent)) -> Result<(), MergerError> {
    let mut full_args = vec!["--gui-mode".to_string()];
    full_args.extend_from_slice(args);

    let mut child = Command::new("mkvmerge")
        .args(&full_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| MergerError::MkvmergeNotFound)?;

    let stdout = child.stdout.take().expect("stdout was piped at spawn");
    let reader = BufReader::new(stdout);
    for line in reader.lines() {
        let line = line.map_err(|e| MergerError::MuxFailed(e.to_string()))?;
        on_event(parse_line(&line));
    }

    let status = child.wait().map_err(|e| MergerError::MuxFailed(e.to_string()))?;
    match status.code() {
        Some(0) | Some(1) => Ok(()),
        _ => {
            let mut stderr_text = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_string(&mut stderr_text);
            }
            Err(MergerError::MuxFailed(stderr_text))
        }
    }
}
```

- [ ] **Step 4: Run the full core test suite to verify nothing regressed**

Run: `cargo test -p mediamerger-core`
Expected: PASS (all tests from Tasks 1–8).

- [ ] **Step 5: Commit**

```bash
git add mediamerger-core/src/mux.rs
git commit -m "Add mkvmerge process execution with progress/log streaming"
```

---

## Task 9: App scaffolding + file pickers & probing

**Files:**
- Modify: `Cargo.toml` (workspace root — add `mediamerger-app` to `members`)
- Create: `mediamerger-app/Cargo.toml`
- Create: `mediamerger-app/src/main.rs`
- Create: `mediamerger-app/src/state.rs`
- Create: `mediamerger-app/src/ui/mod.rs`
- Create: `mediamerger-app/src/ui/file_pickers.rs`

**Interfaces:**
- Consumes: `mediamerger_core::probe::{identify, check_framerate, MediaFile}` (Tasks 2–3), `mediamerger_core::error::MergerError` (Task 1)
- Produces: `AppState` (`#[derive(Debug, Clone)]`) with fields `file_a: Option<MediaFile>`, `file_b: Option<MediaFile>`, `framerate_error: Option<MergerError>`, `is_dark: bool`; `Message` (`#[derive(Debug, Clone)]`) with initial variants `PickFileA`, `PickFileB`, `FileAProbed(Result<MediaFile, MergerError>)`, `FileBProbed(Result<MediaFile, MergerError>)`, `RefreshSystemTheme`, `SystemThemeDetected(bool)`; `pub fn view(state: &AppState) -> Element<Message>` in `ui::mod`.

- [ ] **Step 0: Add `mediamerger-app` to the workspace members**

Edit the workspace root `Cargo.toml` (Task 1 created it with only `mediamerger-core`):

```toml
[workspace]
members = ["mediamerger-core", "mediamerger-app"]
resolver = "2"
```

- [ ] **Step 1: Create `mediamerger-app/Cargo.toml`**

```toml
[package]
name = "mediamerger-app"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "mediamerger"
path = "src/main.rs"

[dependencies]
mediamerger-core = { path = "../mediamerger-core" }
iced = { version = "0.14", features = ["tokio"] }
tokio = { version = "1", features = ["full"] }
rfd = { version = "0.14", features = ["xdg-portal"] }
dark-light = "1"
```

- [ ] **Step 2: Create `mediamerger-app/src/state.rs`**

```rust
use mediamerger_core::error::MergerError;
use mediamerger_core::probe::MediaFile;

#[derive(Debug, Clone)]
pub struct AppState {
    pub file_a: Option<MediaFile>,
    pub file_b: Option<MediaFile>,
    pub framerate_error: Option<MergerError>,
    pub is_dark: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            file_a: None,
            file_b: None,
            framerate_error: None,
            is_dark: crate::detect_is_dark(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    PickFileA,
    PickFileB,
    FileAProbed(Result<MediaFile, MergerError>),
    FileBProbed(Result<MediaFile, MergerError>),
    RefreshSystemTheme,
    SystemThemeDetected(bool),
}
```

- [ ] **Step 3: Create `mediamerger-app/src/ui/file_pickers.rs`**

```rust
use crate::state::{AppState, Message};
use iced::widget::{button, column, row, text};
use iced::Element;

pub fn view(state: &AppState) -> Element<Message> {
    column![
        row![
            text(match &state.file_a {
                Some(f) => f.path.display().to_string(),
                None => "No file selected".to_string(),
            }),
            button("Browse (File A)").on_press(Message::PickFileA),
        ]
        .spacing(10),
        row![
            text(match &state.file_b {
                Some(f) => f.path.display().to_string(),
                None => "No file selected".to_string(),
            }),
            button("Browse (File B)").on_press(Message::PickFileB),
        ]
        .spacing(10),
    ]
    .spacing(10)
    .into()
}
```

- [ ] **Step 4: Create `mediamerger-app/src/ui/mod.rs`**

```rust
mod file_pickers;

use crate::state::{AppState, Message};
use iced::widget::{column, text};
use iced::Element;

pub fn view(state: &AppState) -> Element<Message> {
    let mut sections = column![file_pickers::view(state)].spacing(20);

    if let Some(err) = &state.framerate_error {
        sections = sections.push(text(err.to_string()));
    }

    sections.into()
}
```

- [ ] **Step 5: Create `mediamerger-app/src/main.rs`**

```rust
use iced::{application, time, window, Element, Subscription, Task, Theme};
use state::{AppState, Message};
use std::time::Duration;

mod state;
mod ui;

fn main() -> iced::Result {
    application(|| (AppState::default(), Task::none()), update, view)
        .title("MediaMerger")
        .window(window::Settings {
            platform_specific: window::settings::PlatformSpecific {
                application_id: "mediamerger".to_string(),
                ..Default::default()
            },
            ..Default::default()
        })
        .theme(theme)
        .subscription(subscription)
        .run()
}

fn view(state: &AppState) -> Element<Message> {
    ui::view(state)
}

fn theme(state: &AppState) -> Theme {
    if state.is_dark { Theme::Dark } else { Theme::Light }
}

// dark-light falls back to a theme *name* lookup on some GNOME versions that
// doesn't reflect the color-scheme GSettings key modern Ubuntu uses. Read the
// key directly; fall back to dark-light for non-GNOME desktops.
fn detect_is_dark() -> bool {
    std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.contains("prefer-dark"))
        .unwrap_or_else(|| dark_light::detect() == dark_light::Mode::Dark)
}

fn subscription(_state: &AppState) -> Subscription<Message> {
    time::every(Duration::from_secs(10)).map(|_| Message::RefreshSystemTheme)
}

fn update(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        Message::PickFileA => Task::perform(pick_and_probe(), Message::FileAProbed),
        Message::PickFileB => Task::perform(pick_and_probe(), Message::FileBProbed),

        Message::FileAProbed(result) => {
            apply_probe_result(state, result, true);
            Task::none()
        }
        Message::FileBProbed(result) => {
            apply_probe_result(state, result, false);
            Task::none()
        }

        Message::RefreshSystemTheme => Task::perform(
            async { tokio::task::spawn_blocking(detect_is_dark).await.unwrap_or(false) },
            Message::SystemThemeDetected,
        ),
        Message::SystemThemeDetected(is_dark) => {
            if state.is_dark != is_dark {
                state.is_dark = is_dark;
            }
            Task::none()
        }
    }
}

async fn pick_and_probe() -> Result<mediamerger_core::probe::MediaFile, mediamerger_core::error::MergerError> {
    let handle = rfd::AsyncFileDialog::new()
        .add_filter("Video files", &["mkv", "mp4", "avi", "mov", "m4v", "webm"])
        .pick_file()
        .await;

    let path = match handle {
        Some(h) => h.path().to_path_buf(),
        None => return Err(mediamerger_core::error::MergerError::Probe("no file selected".to_string())),
    };

    tokio::task::spawn_blocking(move || mediamerger_core::probe::identify(&path))
        .await
        .unwrap_or_else(|e| Err(mediamerger_core::error::MergerError::Probe(e.to_string())))
}

fn apply_probe_result(
    state: &mut AppState,
    result: Result<mediamerger_core::probe::MediaFile, mediamerger_core::error::MergerError>,
    is_file_a: bool,
) {
    match result {
        Ok(media_file) => {
            if is_file_a {
                state.file_a = Some(media_file);
            } else {
                state.file_b = Some(media_file);
            }
            state.framerate_error = None;
            if let (Some(a), Some(b)) = (&state.file_a, &state.file_b) {
                if let Err(e) = mediamerger_core::probe::check_framerate(&a.path, &b.path) {
                    state.framerate_error = Some(e);
                }
            }
        }
        Err(e) => state.framerate_error = Some(e),
    }
}
```

- [ ] **Step 6: Build the app to verify it compiles and runs**

Run: `cargo build -p mediamerger-app`
Expected: builds successfully (a real `.mkv` file and installed `mkvmerge`/`ffmpeg` are not required to compile).

Run: `cargo run -p mediamerger-app`
Expected: a window titled "MediaMerger" opens with two "Browse" buttons; clicking one opens a native file picker and, after choosing an `.mkv`, the path text updates.

- [ ] **Step 7: Commit**

```bash
git add mediamerger-app
git commit -m "Scaffold iced app with file pickers and probing"
```

---

## Task 10: Track table UI and selection state

**Files:**
- Modify: `mediamerger-app/src/state.rs`
- Create: `mediamerger-app/src/ui/track_table.rs`
- Modify: `mediamerger-app/src/ui/mod.rs`
- Modify: `mediamerger-app/src/main.rs`

**Interfaces:**
- Consumes: `AppState.file_a`/`file_b` (Task 9), `mediamerger_core::probe::Track` (Task 2)
- Produces: `TrackUiState { selected: bool, default_flag: bool, forced_flag: bool }` (`#[derive(Debug, Clone, Default)]`), `AppState` fields `tracks_a_ui: Vec<TrackUiState>`, `tracks_b_ui: Vec<TrackUiState>`, new `Message` variants `ToggleTrackA(usize)`, `ToggleTrackB(usize)`, `SetDefaultFlagA(usize, bool)`, `SetDefaultFlagB(usize, bool)`, `SetForcedFlagA(usize, bool)`, `SetForcedFlagB(usize, bool)`

- [ ] **Step 1: Write the failing test for track-row sync on probe**

Add to `mediamerger-app/src/state.rs` (below the existing structs):

```rust
#[derive(Debug, Clone, Default)]
pub struct TrackUiState {
    pub selected: bool,
    pub default_flag: bool,
    pub forced_flag: bool,
}
```

Add fields to `AppState`:

```rust
pub struct AppState {
    pub file_a: Option<MediaFile>,
    pub file_b: Option<MediaFile>,
    pub tracks_a_ui: Vec<TrackUiState>,
    pub tracks_b_ui: Vec<TrackUiState>,
    pub framerate_error: Option<MergerError>,
    pub is_dark: bool,
}
```

Update `Default for AppState` to add `tracks_a_ui: Vec::new(), tracks_b_ui: Vec::new(),`.

Add new `Message` variants:

```rust
    ToggleTrackA(usize),
    ToggleTrackB(usize),
    SetDefaultFlagA(usize, bool),
    SetDefaultFlagB(usize, bool),
    SetForcedFlagA(usize, bool),
    SetForcedFlagB(usize, bool),
```

Add at the bottom of `mediamerger-app/src/state.rs`:

```rust
impl AppState {
    pub fn sync_track_ui_len(tracks: &[mediamerger_core::probe::Track], ui: &mut Vec<TrackUiState>) {
        ui.resize_with(tracks.len(), TrackUiState::default);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mediamerger_core::probe::{Track, TrackKind};

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
        }
    }

    #[test]
    fn sync_track_ui_len_grows_and_shrinks_to_match_tracks() {
        let mut ui = vec![TrackUiState { selected: true, ..Default::default() }];
        let tracks = vec![track(0, TrackKind::Video), track(1, TrackKind::Audio)];

        AppState::sync_track_ui_len(&tracks, &mut ui);

        assert_eq!(ui.len(), 2);
        assert!(ui[0].selected, "existing row state must be preserved");
        assert!(!ui[1].selected, "newly appended row defaults to unselected");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mediamerger-app sync_track_ui_len_grows_and_shrinks_to_match_tracks`
Expected: FAIL — `sync_track_ui_len` doesn't exist yet (if written incrementally) or PASS immediately since the implementation is included above; if it fails, implement `sync_track_ui_len` exactly as shown.

- [ ] **Step 3: Wire `sync_track_ui_len` into `apply_probe_result`**

Edit `mediamerger-app/src/main.rs`, inside `apply_probe_result`, right after `state.file_a = Some(media_file);` / `state.file_b = Some(media_file);`:

```rust
        Ok(media_file) => {
            if is_file_a {
                AppState::sync_track_ui_len(&media_file.tracks, &mut state.tracks_a_ui);
                state.file_a = Some(media_file);
            } else {
                AppState::sync_track_ui_len(&media_file.tracks, &mut state.tracks_b_ui);
                state.file_b = Some(media_file);
            }
```

Add `use state::AppState;` alongside the existing `use state::{AppState, Message};` import at the top of `main.rs` if not already present (it already is, from Task 9).

- [ ] **Step 4: Handle the new toggle messages in `update`**

Add to the `match message` block in `mediamerger-app/src/main.rs`:

```rust
        Message::ToggleTrackA(idx) => {
            if let Some(row) = state.tracks_a_ui.get_mut(idx) {
                row.selected = !row.selected;
            }
            Task::none()
        }
        Message::ToggleTrackB(idx) => {
            if let Some(row) = state.tracks_b_ui.get_mut(idx) {
                row.selected = !row.selected;
            }
            Task::none()
        }
        Message::SetDefaultFlagA(idx, value) => {
            if let Some(row) = state.tracks_a_ui.get_mut(idx) {
                row.default_flag = value;
            }
            Task::none()
        }
        Message::SetDefaultFlagB(idx, value) => {
            if let Some(row) = state.tracks_b_ui.get_mut(idx) {
                row.default_flag = value;
            }
            Task::none()
        }
        Message::SetForcedFlagA(idx, value) => {
            if let Some(row) = state.tracks_a_ui.get_mut(idx) {
                row.forced_flag = value;
            }
            Task::none()
        }
        Message::SetForcedFlagB(idx, value) => {
            if let Some(row) = state.tracks_b_ui.get_mut(idx) {
                row.forced_flag = value;
            }
            Task::none()
        }
```

- [ ] **Step 5: Create `mediamerger-app/src/ui/track_table.rs`**

```rust
use crate::state::{AppState, Message};
use iced::widget::{checkbox, column, row, text};
use iced::Element;
use mediamerger_core::probe::{MediaFile, Track};

fn track_label(track: &Track) -> String {
    let lang = track.language.as_deref().unwrap_or("und");
    format!("{:?}: {} ({lang})", track.kind, track.codec)
}

fn track_row<'a>(
    idx: usize,
    track: &'a Track,
    selected: bool,
    on_toggle: impl Fn(usize) -> Message + 'a,
) -> Element<'a, Message> {
    row![
        checkbox(track_label(track), selected).on_toggle(move |_| on_toggle(idx)),
    ]
    .into()
}

fn file_column<'a>(
    file: &'a Option<MediaFile>,
    ui: &'a [crate::state::TrackUiState],
    on_toggle: impl Fn(usize) -> Message + Copy + 'a,
) -> Element<'a, Message> {
    match file {
        None => text("No file loaded").into(),
        Some(f) => {
            let mut col = column![].spacing(5);
            for (idx, track) in f.tracks.iter().enumerate() {
                let selected = ui.get(idx).map(|u| u.selected).unwrap_or(false);
                col = col.push(track_row(idx, track, selected, on_toggle));
            }
            col.into()
        }
    }
}

pub fn view(state: &AppState) -> Element<Message> {
    row![
        file_column(&state.file_a, &state.tracks_a_ui, Message::ToggleTrackA),
        file_column(&state.file_b, &state.tracks_b_ui, Message::ToggleTrackB),
    ]
    .spacing(30)
    .into()
}
```

- [ ] **Step 6: Wire the track table into `ui::view`**

Edit `mediamerger-app/src/ui/mod.rs`:

```rust
mod file_pickers;
mod track_table;

use crate::state::{AppState, Message};
use iced::widget::{column, text};
use iced::Element;

pub fn view(state: &AppState) -> Element<Message> {
    let mut sections = column![file_pickers::view(state), track_table::view(state)].spacing(20);

    if let Some(err) = &state.framerate_error {
        sections = sections.push(text(err.to_string()));
    }

    sections.into()
}
```

- [ ] **Step 7: Run tests and build**

Run: `cargo test -p mediamerger-app`
Expected: PASS

Run: `cargo run -p mediamerger-app`
Expected: after picking File A and File B, each shows a checkbox list of its tracks; toggling a checkbox flips its checked state.

- [ ] **Step 8: Commit**

```bash
git add mediamerger-app/src/state.rs mediamerger-app/src/main.rs mediamerger-app/src/ui
git commit -m "Add track selection table and per-track flag state"
```

---

## Task 11: Offset detection UI

**Files:**
- Modify: `mediamerger-app/src/state.rs`
- Create: `mediamerger-app/src/ui/offset_panel.rs`
- Modify: `mediamerger-app/src/ui/mod.rs`
- Modify: `mediamerger-app/src/main.rs`

**Interfaces:**
- Consumes: `mediamerger_core::offset::{detect_offset, OffsetResult, Consistency}` (Task 6)
- Produces: `OffsetState` (`#[derive(Debug, Clone)]`, variants `NotDetected`, `Detecting`, `Detected(OffsetResult)`, `ManualOverride(f64)`), `AppState` fields `offset: OffsetState`, `manual_offset_input: String`; new `Message` variants `DetectOffset`, `OffsetDetected(Result<OffsetResult, MergerError>)`, `ManualOffsetChanged(String)`; `AppState::resolved_offset_secs(&self) -> Option<f64>`

- [ ] **Step 1: Write the failing test for `resolved_offset_secs`**

Add to `mediamerger-app/src/state.rs`:

```rust
use mediamerger_core::offset::{Consistency, OffsetResult};

#[derive(Debug, Clone)]
pub enum OffsetState {
    NotDetected,
    Detecting,
    Detected(OffsetResult),
    ManualOverride(f64),
}
```

Add fields to `AppState`: `pub offset: OffsetState, pub manual_offset_input: String,` and to its `Default` impl: `offset: OffsetState::NotDetected, manual_offset_input: String::new(),`.

Add new `Message` variants:

```rust
    DetectOffset,
    OffsetDetected(Result<OffsetResult, MergerError>),
    ManualOffsetChanged(String),
```

Add to the `impl AppState` block:

```rust
    pub fn resolved_offset_secs(&self) -> Option<f64> {
        match &self.offset {
            OffsetState::Detected(r) => Some(r.offset),
            OffsetState::ManualOverride(v) => Some(*v),
            OffsetState::NotDetected | OffsetState::Detecting => None,
        }
    }
```

Add to the `tests` module:

```rust
    #[test]
    fn resolved_offset_prefers_manual_override_when_set() {
        let mut state = AppState::default();
        state.offset = OffsetState::ManualOverride(1.5);
        assert_eq!(state.resolved_offset_secs(), Some(1.5));
    }

    #[test]
    fn resolved_offset_none_while_detecting() {
        let mut state = AppState::default();
        state.offset = OffsetState::Detecting;
        assert_eq!(state.resolved_offset_secs(), None);
    }

    #[test]
    fn resolved_offset_uses_detected_value() {
        let mut state = AppState::default();
        state.offset = OffsetState::Detected(OffsetResult {
            early_offset: 2.34,
            late_offset: 2.36,
            consistency: Consistency::Consistent,
            confidence: 8.0,
            offset: 2.35,
        });
        assert_eq!(state.resolved_offset_secs(), Some(2.35));
    }
```

- [ ] **Step 2: Run tests to verify they fail, then pass**

Run: `cargo test -p mediamerger-app resolved_offset`
Expected: PASS once `resolved_offset_secs` is added exactly as above (write it first if following strict red-green, but the method is small enough to include directly).

- [ ] **Step 3: Wire `DetectOffset`/`OffsetDetected`/`ManualOffsetChanged` into `update`**

Add to `mediamerger-app/src/main.rs`'s `match message` block:

```rust
        Message::DetectOffset => {
            state.offset = state::OffsetState::Detecting;
            let (Some(file_a), Some(file_b)) = (state.file_a.clone(), state.file_b.clone()) else {
                state.offset = state::OffsetState::NotDetected;
                return Task::none();
            };
            let Some(track_a) = first_audio_track_id(&file_a) else {
                state.offset = state::OffsetState::NotDetected;
                return Task::none();
            };
            let Some(track_b) = first_audio_track_id(&file_b) else {
                state.offset = state::OffsetState::NotDetected;
                return Task::none();
            };
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        mediamerger_core::offset::detect_offset(&file_a.path, track_a, &file_b.path, track_b)
                    })
                    .await
                    .unwrap_or_else(|e| Err(mediamerger_core::error::MergerError::Probe(e.to_string())))
                },
                Message::OffsetDetected,
            )
        }
        Message::OffsetDetected(result) => {
            state.offset = match result {
                Ok(r) => {
                    state.manual_offset_input = format!("{:.3}", r.offset);
                    state::OffsetState::Detected(r)
                }
                Err(_) => state::OffsetState::NotDetected,
            };
            Task::none()
        }
        Message::ManualOffsetChanged(text) => {
            if let Ok(value) = text.parse::<f64>() {
                state.offset = state::OffsetState::ManualOverride(value);
            }
            state.manual_offset_input = text;
            Task::none()
        }
```

Add this helper function near the bottom of `mediamerger-app/src/main.rs`:

```rust
fn first_audio_track_id(file: &mediamerger_core::probe::MediaFile) -> Option<u64> {
    file.tracks
        .iter()
        .find(|t| t.kind == mediamerger_core::probe::TrackKind::Audio)
        .map(|t| t.id)
}
```

- [ ] **Step 4: Create `mediamerger-app/src/ui/offset_panel.rs`**

```rust
use crate::state::{AppState, Message, OffsetState};
use iced::widget::{button, column, row, text, text_input};
use iced::Element;
use mediamerger_core::offset::Consistency;

pub fn view(state: &AppState) -> Element<Message> {
    let status: Element<Message> = match &state.offset {
        OffsetState::NotDetected => text("Offset not yet detected").into(),
        OffsetState::Detecting => text("Detecting offset…").into(),
        OffsetState::Detected(r) => {
            let consistency_label = match r.consistency {
                Consistency::Consistent if r.confidence < 3.0 => "consistent (low confidence)",
                Consistency::Consistent => "consistent",
                Consistency::Inconsistent => "INCONSISTENT — resolve manually before merging",
                Consistency::Unverified => "unverified (file too short for a second check)",
            };
            text(format!(
                "early: {:.3}s, late: {:.3}s ({consistency_label}), confidence: {:.2}",
                r.early_offset, r.late_offset, r.confidence
            ))
            .into()
        }
        OffsetState::ManualOverride(v) => text(format!("manual override: {v:.3}s")).into(),
    };

    column![
        row![
            button("Detect Offset").on_press(Message::DetectOffset),
            text_input("offset seconds", &state.manual_offset_input)
                .on_input(Message::ManualOffsetChanged),
        ]
        .spacing(10),
        status,
    ]
    .spacing(10)
    .into()
}
```

- [ ] **Step 5: Wire the offset panel into `ui::view`**

Edit `mediamerger-app/src/ui/mod.rs`:

```rust
mod file_pickers;
mod offset_panel;
mod track_table;

use crate::state::{AppState, Message};
use iced::widget::{column, text};
use iced::Element;

pub fn view(state: &AppState) -> Element<Message> {
    let mut sections = column![
        file_pickers::view(state),
        track_table::view(state),
        offset_panel::view(state),
    ]
    .spacing(20);

    if let Some(err) = &state.framerate_error {
        sections = sections.push(text(err.to_string()));
    }

    sections.into()
}
```

- [ ] **Step 6: Run tests and build**

Run: `cargo test -p mediamerger-app`
Expected: PASS

Run: `cargo run -p mediamerger-app`
Expected: with both files loaded, clicking "Detect Offset" shows "Detecting offset…" then the early/late/consistency/confidence line; typing in the text field overrides the offset.

- [ ] **Step 7: Commit**

```bash
git add mediamerger-app/src/state.rs mediamerger-app/src/main.rs mediamerger-app/src/ui
git commit -m "Add offset detection UI with consistency display and manual override"
```

---

## Task 12: Extras UI + `MergePlan` construction

**Files:**
- Modify: `mediamerger-app/src/state.rs`
- Create: `mediamerger-app/src/ui/extras.rs`
- Modify: `mediamerger-app/src/ui/mod.rs`
- Modify: `mediamerger-app/src/main.rs`

**Interfaces:**
- Consumes: `mediamerger_core::mux::{MergePlan, TrackSelection, ChapterSource}` (Task 7), `AppState.tracks_a_ui`/`tracks_b_ui` (Task 10), `AppState.resolved_offset_secs()` (Task 11)
- Produces: `ChaptersChoice` (`#[derive(Debug, Clone, Copy, PartialEq)]`, variants `FileA`, `FileB`, `None`), `AppState` fields `chapters_choice`, `attachments_a`, `attachments_b`, `tags_a`, `tags_b`; new `Message` variants `ChaptersChoiceChanged(ChaptersChoice)`, `ToggleAttachmentsA(bool)`, `ToggleAttachmentsB(bool)`, `ToggleTagsA(bool)`, `ToggleTagsB(bool)`; `AppState::to_merge_plan(&self, output_path: PathBuf) -> Option<MergePlan>`

- [ ] **Step 1: Write the failing tests for `to_merge_plan`**

Add to `mediamerger-app/src/state.rs`:

```rust
use mediamerger_core::mux::{ChapterSource, MergePlan, TrackSelection};
use mediamerger_core::probe::Track;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChaptersChoice {
    FileA,
    FileB,
    None,
}
```

Add fields to `AppState`:

```rust
pub chapters_choice: ChaptersChoice,
pub attachments_a: bool,
pub attachments_b: bool,
pub tags_a: bool,
pub tags_b: bool,
```

Add to `Default for AppState`:

```rust
chapters_choice: ChaptersChoice::FileA,
attachments_a: true,
attachments_b: false,
tags_a: false,
tags_b: false,
```

Add new `Message` variants:

```rust
    ChaptersChoiceChanged(ChaptersChoice),
    ToggleAttachmentsA(bool),
    ToggleAttachmentsB(bool),
    ToggleTagsA(bool),
    ToggleTagsB(bool),
```

Add to the `impl AppState` block:

```rust
    pub fn to_merge_plan(&self, output_path: PathBuf) -> Option<MergePlan> {
        let file_a = self.file_a.as_ref()?;
        let file_b = self.file_b.as_ref()?;
        let offset_secs = self.resolved_offset_secs()?;

        let tracks_from_a = selections(&file_a.tracks, &self.tracks_a_ui);
        let tracks_from_b = selections(&file_b.tracks, &self.tracks_b_ui);
        if tracks_from_a.is_empty() && tracks_from_b.is_empty() {
            return None;
        }

        let chapters = match self.chapters_choice {
            ChaptersChoice::FileA => ChapterSource::FileA,
            ChaptersChoice::FileB => ChapterSource::FileB,
            ChaptersChoice::None => ChapterSource::None,
        };

        Some(MergePlan {
            file_a: file_a.path.clone(),
            file_b: file_b.path.clone(),
            tracks_from_a,
            tracks_from_b,
            offset_secs,
            chapters,
            attachments_from_a: self.attachments_a,
            attachments_from_b: self.attachments_b,
            tags_from_a: self.tags_a,
            tags_from_b: self.tags_b,
            output_path,
        })
    }
```

Add the free function at the bottom of `mediamerger-app/src/state.rs`:

```rust
fn selections(tracks: &[Track], ui: &[TrackUiState]) -> Vec<TrackSelection> {
    tracks
        .iter()
        .zip(ui.iter())
        .filter(|(_, u)| u.selected)
        .map(|(t, u)| TrackSelection {
            track_id: t.id,
            kind: t.kind,
            set_default: u.default_flag,
            set_forced: u.forced_flag,
        })
        .collect()
}
```

Add to the `tests` module:

```rust
    fn media_file(path: &str, tracks: Vec<Track>) -> mediamerger_core::probe::MediaFile {
        mediamerger_core::probe::MediaFile {
            path: PathBuf::from(path),
            container: "Matroska".to_string(),
            tracks,
        }
    }

    #[test]
    fn to_merge_plan_none_when_no_tracks_selected() {
        let mut state = AppState::default();
        state.file_a = Some(media_file("a.mkv", vec![track(0, mediamerger_core::probe::TrackKind::Video)]));
        state.file_b = Some(media_file("b.mkv", vec![track(1, mediamerger_core::probe::TrackKind::Audio)]));
        state.tracks_a_ui = vec![TrackUiState::default()];
        state.tracks_b_ui = vec![TrackUiState::default()];
        state.offset = OffsetState::ManualOverride(1.0);

        assert!(state.to_merge_plan(PathBuf::from("out.mkv")).is_none());
    }

    #[test]
    fn to_merge_plan_none_when_offset_unresolved() {
        let mut state = AppState::default();
        state.file_a = Some(media_file("a.mkv", vec![track(0, mediamerger_core::probe::TrackKind::Video)]));
        state.file_b = Some(media_file("b.mkv", vec![track(1, mediamerger_core::probe::TrackKind::Audio)]));
        state.tracks_a_ui = vec![TrackUiState { selected: true, ..Default::default() }];
        state.tracks_b_ui = vec![TrackUiState { selected: true, ..Default::default() }];

        assert!(state.to_merge_plan(PathBuf::from("out.mkv")).is_none());
    }

    #[test]
    fn to_merge_plan_builds_plan_with_selected_tracks_only() {
        let mut state = AppState::default();
        state.file_a = Some(media_file(
            "a.mkv",
            vec![track(0, mediamerger_core::probe::TrackKind::Video), track(1, mediamerger_core::probe::TrackKind::Audio)],
        ));
        state.file_b = Some(media_file("b.mkv", vec![track(2, mediamerger_core::probe::TrackKind::Audio)]));
        state.tracks_a_ui = vec![
            TrackUiState { selected: true, ..Default::default() },
            TrackUiState { selected: false, ..Default::default() },
        ];
        state.tracks_b_ui = vec![TrackUiState { selected: true, default_flag: true, ..Default::default() }];
        state.offset = OffsetState::ManualOverride(2.0);

        let plan = state.to_merge_plan(PathBuf::from("out.mkv")).expect("plan should build");

        assert_eq!(plan.tracks_from_a.len(), 1);
        assert_eq!(plan.tracks_from_a[0].track_id, 0);
        assert_eq!(plan.tracks_from_b.len(), 1);
        assert_eq!(plan.tracks_from_b[0].track_id, 2);
        assert!(plan.tracks_from_b[0].set_default);
        assert_eq!(plan.offset_secs, 2.0);
    }
```

- [ ] **Step 2: Run tests to verify they fail, then pass**

Run: `cargo test -p mediamerger-app to_merge_plan`
Expected: PASS once `to_merge_plan` and `selections` are added exactly as above.

- [ ] **Step 3: Handle the new Extras messages in `update`**

Add to `mediamerger-app/src/main.rs`'s `match message` block:

```rust
        Message::ChaptersChoiceChanged(choice) => {
            state.chapters_choice = choice;
            Task::none()
        }
        Message::ToggleAttachmentsA(v) => {
            state.attachments_a = v;
            Task::none()
        }
        Message::ToggleAttachmentsB(v) => {
            state.attachments_b = v;
            Task::none()
        }
        Message::ToggleTagsA(v) => {
            state.tags_a = v;
            Task::none()
        }
        Message::ToggleTagsB(v) => {
            state.tags_b = v;
            Task::none()
        }
```

- [ ] **Step 4: Create `mediamerger-app/src/ui/extras.rs`**

```rust
use crate::state::{AppState, ChaptersChoice, Message};
use iced::widget::{checkbox, column, radio, row, text};
use iced::Element;

pub fn view(state: &AppState) -> Element<Message> {
    column![
        row![
            text("Chapters:"),
            radio("File A", ChaptersChoice::FileA, Some(state.chapters_choice), Message::ChaptersChoiceChanged),
            radio("File B", ChaptersChoice::FileB, Some(state.chapters_choice), Message::ChaptersChoiceChanged),
            radio("None", ChaptersChoice::None, Some(state.chapters_choice), Message::ChaptersChoiceChanged),
        ]
        .spacing(10),
        row![
            checkbox("Attachments from A", state.attachments_a).on_toggle(Message::ToggleAttachmentsA),
            checkbox("Attachments from B", state.attachments_b).on_toggle(Message::ToggleAttachmentsB),
        ]
        .spacing(10),
        row![
            checkbox("Tags from A", state.tags_a).on_toggle(Message::ToggleTagsA),
            checkbox("Tags from B", state.tags_b).on_toggle(Message::ToggleTagsB),
        ]
        .spacing(10),
    ]
    .spacing(10)
    .into()
}
```

- [ ] **Step 5: Wire extras into `ui::view`**

Edit `mediamerger-app/src/ui/mod.rs`:

```rust
mod extras;
mod file_pickers;
mod offset_panel;
mod track_table;

use crate::state::{AppState, Message};
use iced::widget::{column, text};
use iced::Element;

pub fn view(state: &AppState) -> Element<Message> {
    let mut sections = column![
        file_pickers::view(state),
        track_table::view(state),
        offset_panel::view(state),
        extras::view(state),
    ]
    .spacing(20);

    if let Some(err) = &state.framerate_error {
        sections = sections.push(text(err.to_string()));
    }

    sections.into()
}
```

- [ ] **Step 6: Run tests and build**

Run: `cargo test -p mediamerger-app`
Expected: PASS

Run: `cargo run -p mediamerger-app`
Expected: chapters radio and attachments/tags checkboxes render and toggle.

- [ ] **Step 7: Commit**

```bash
git add mediamerger-app/src/state.rs mediamerger-app/src/main.rs mediamerger-app/src/ui
git commit -m "Add extras UI and pure MergePlan construction"
```

---

## Task 13: Output picker, merge execution, and startup binary check

**Files:**
- Modify: `mediamerger-app/src/state.rs`
- Create: `mediamerger-app/src/ui/output_log.rs`
- Modify: `mediamerger-app/src/ui/mod.rs`
- Modify: `mediamerger-app/src/main.rs`

**Interfaces:**
- Consumes: `mediamerger_core::mux::{build_command, run_mux, MuxEvent}` (Tasks 7–8), `AppState::to_merge_plan` (Task 12)
- Produces: `AppState` fields `output_path: Option<PathBuf>`, `merge_progress: Option<f32>`, `log: Vec<String>`, `merge_error: Option<String>`, `missing_binaries: Vec<&'static str>`, `merge_receiver: Option<Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<MuxUiEvent>>>>`; new `Message` variants `PickOutput`, `OutputPicked(Option<PathBuf>)`, `StartMerge`, `MergeEventReceived(Option<MuxUiEvent>)`, `BinariesChecked(Vec<&'static str>)`; `MuxUiEvent` (`#[derive(Debug, Clone)]`, variants `Progress(f32)`, `Log(String)`, `Done(Result<(), String>)`)

- [ ] **Step 1: Add the new state and message plumbing**

Add to `mediamerger-app/src/state.rs`:

```rust
#[derive(Debug, Clone)]
pub enum MuxUiEvent {
    Progress(f32),
    Log(String),
    Done(Result<(), String>),
}
```

Add fields to `AppState`:

```rust
pub output_path: Option<PathBuf>,
pub merge_progress: Option<f32>,
pub log: Vec<String>,
pub merge_error: Option<String>,
pub missing_binaries: Vec<&'static str>,
pub merge_receiver: Option<std::sync::Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<MuxUiEvent>>>>,
```

Add to `Default for AppState`:

```rust
output_path: None,
merge_progress: None,
log: Vec::new(),
merge_error: None,
missing_binaries: Vec::new(),
merge_receiver: None,
```

Add new `Message` variants:

```rust
    PickOutput,
    OutputPicked(Option<PathBuf>),
    StartMerge,
    MergeEventReceived(Option<MuxUiEvent>),
    BinariesChecked(Vec<&'static str>),
```

(The `merge_receiver` field is added here in Step 1 for completeness — its `.clone()`-ability is what lets `AppState` keep deriving `Clone` once the merge-execution plumbing in Step 3 uses it.)

Add to the `tests` module (this only tests the pure predicate used by the view, not the subprocess):

```rust
    #[test]
    fn can_merge_requires_output_path_and_resolvable_plan() {
        let mut state = AppState::default();
        assert!(state.to_merge_plan(PathBuf::from("x.mkv")).is_none(), "no files loaded yet");

        state.file_a = Some(media_file("a.mkv", vec![track(0, mediamerger_core::probe::TrackKind::Video)]));
        state.file_b = Some(media_file("b.mkv", vec![track(1, mediamerger_core::probe::TrackKind::Audio)]));
        state.tracks_a_ui = vec![TrackUiState { selected: true, ..Default::default() }];
        state.tracks_b_ui = vec![TrackUiState { selected: true, ..Default::default() }];
        state.offset = OffsetState::ManualOverride(0.0);

        assert!(state.to_merge_plan(PathBuf::from("x.mkv")).is_some());
    }
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test -p mediamerger-app can_merge_requires_output_path_and_resolvable_plan`
Expected: PASS (reuses `to_merge_plan` from Task 12; no new logic to fail on).

- [ ] **Step 3: Wire startup binary check, output picker, and merge execution into `update`**

Add near the top of `mediamerger-app/src/main.rs`, inside `main()`, change the initial state constructor to also kick off the binary check:

```rust
fn main() -> iced::Result {
    application(|| (AppState::default(), Task::perform(check_binaries(), Message::BinariesChecked)), update, view)
        .title("MediaMerger")
        .window(window::Settings {
            platform_specific: window::settings::PlatformSpecific {
                application_id: "mediamerger".to_string(),
                ..Default::default()
            },
            ..Default::default()
        })
        .theme(theme)
        .subscription(subscription)
        .run()
}

async fn check_binaries() -> Vec<&'static str> {
    tokio::task::spawn_blocking(|| {
        let mut missing = Vec::new();
        for bin in ["ffmpeg", "ffprobe", "mkvmerge"] {
            let found = std::process::Command::new(bin)
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if !found {
                missing.push(bin);
            }
        }
        missing
    })
    .await
    .unwrap_or_else(|_| vec!["ffmpeg", "ffprobe", "mkvmerge"])
}
```

Merge progress needs to deliver *every* event from the worker thread, not just one, but `Task::perform` only resolves once per call. Rather than introduce a full `iced::Subscription`, the receiver lives in `AppState.merge_receiver` (added in Step 1) behind an `Arc<tokio::sync::Mutex<..>>` — `Arc`/`Mutex` are `Clone` even though the receiver inside isn't, which is what lets `AppState` keep deriving `Clone` — and each received event re-arms the next poll via a plain helper function returning `Task<Message>`.

Add to the `match message` block in `mediamerger-app/src/main.rs`:

```rust
        Message::BinariesChecked(missing) => {
            state.missing_binaries = missing;
            Task::none()
        }
        Message::PickOutput => Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .add_filter("Matroska", &["mkv"])
                    .save_file()
                    .await
                    .map(|h| h.path().to_path_buf())
            },
            Message::OutputPicked,
        ),
        Message::OutputPicked(path) => {
            state.output_path = path;
            Task::none()
        }
        Message::StartMerge => {
            let Some(output_path) = state.output_path.clone() else {
                return Task::none();
            };
            let Some(plan) = state.to_merge_plan(output_path) else {
                return Task::none();
            };
            state.merge_progress = Some(0.0);
            state.log.clear();
            state.merge_error = None;

            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<state::MuxUiEvent>();
            std::thread::spawn(move || {
                let args = mediamerger_core::mux::build_command(&plan);
                let tx_events = tx.clone();
                let result = mediamerger_core::mux::run_mux(&args, move |event| {
                    let mapped = match event {
                        mediamerger_core::mux::MuxEvent::Progress(p) => state::MuxUiEvent::Progress(p),
                        mediamerger_core::mux::MuxEvent::Log(l) => state::MuxUiEvent::Log(l),
                    };
                    let _ = tx_events.send(mapped);
                });
                let _ = tx.send(state::MuxUiEvent::Done(result.map_err(|e| e.to_string())));
            });

            state.merge_receiver = Some(std::sync::Arc::new(tokio::sync::Mutex::new(rx)));
            poll_merge_event(state)
        }
        Message::MergeEventReceived(event) => match event {
            Some(state::MuxUiEvent::Progress(p)) => {
                state.merge_progress = Some(p);
                poll_merge_event(state)
            }
            Some(state::MuxUiEvent::Log(line)) => {
                state.log.push(line);
                poll_merge_event(state)
            }
            Some(state::MuxUiEvent::Done(Ok(()))) => {
                state.merge_progress = Some(1.0);
                state.merge_receiver = None;
                Task::none()
            }
            Some(state::MuxUiEvent::Done(Err(e))) => {
                state.merge_error = Some(e);
                state.merge_progress = None;
                state.merge_receiver = None;
                Task::none()
            }
            None => {
                state.merge_receiver = None;
                Task::none()
            }
        },
```

Add this helper function near the bottom of `mediamerger-app/src/main.rs`:

```rust
fn poll_merge_event(state: &AppState) -> Task<Message> {
    let Some(receiver) = state.merge_receiver.clone() else {
        return Task::none();
    };
    Task::perform(
        async move {
            let mut rx = receiver.lock().await;
            rx.recv().await
        },
        Message::MergeEventReceived,
    )
}
```

Add `MergeEventReceived(Option<state::MuxUiEvent>)` to `Message` in `mediamerger-app/src/state.rs` (alongside the `MuxUiEvent` enum from Step 1, which keeps its `#[derive(Debug, Clone)]`). `Message` continues to derive `Clone` unchanged, since `Arc<Mutex<_>>` and `Option<MuxUiEvent>` are both `Clone`.

- [ ] **Step 4: Create `mediamerger-app/src/ui/output_log.rs`**

```rust
use crate::state::{AppState, Message};
use iced::widget::{button, column, row, text};
use iced::Element;

pub fn view(state: &AppState) -> Element<Message> {
    let output_label = match &state.output_path {
        Some(p) => p.display().to_string(),
        None => "No output selected".to_string(),
    };

    let mut col = column![
        row![
            text(output_label),
            button("Browse (Output)").on_press(Message::PickOutput),
            button("Merge").on_press(Message::StartMerge),
        ]
        .spacing(10),
    ]
    .spacing(10);

    if !state.missing_binaries.is_empty() {
        col = col.push(text(format!("Missing required tools: {}", state.missing_binaries.join(", "))));
    }
    if let Some(p) = state.merge_progress {
        col = col.push(text(format!("Progress: {:.0}%", p * 100.0)));
    }
    if let Some(err) = &state.merge_error {
        col = col.push(text(format!("Merge failed: {err}")));
    }
    for line in &state.log {
        col = col.push(text(line));
    }

    col.into()
}
```

- [ ] **Step 5: Wire output/log into `ui::view`**

Edit `mediamerger-app/src/ui/mod.rs`:

```rust
mod extras;
mod file_pickers;
mod offset_panel;
mod output_log;
mod track_table;

use crate::state::{AppState, Message};
use iced::widget::{column, text};
use iced::Element;

pub fn view(state: &AppState) -> Element<Message> {
    let mut sections = column![
        file_pickers::view(state),
        track_table::view(state),
        offset_panel::view(state),
        extras::view(state),
        output_log::view(state),
    ]
    .spacing(20);

    if let Some(err) = &state.framerate_error {
        sections = sections.push(text(err.to_string()));
    }

    sections.into()
}
```

- [ ] **Step 6: Run tests and build**

Run: `cargo test -p mediamerger-app`
Expected: PASS

Run: `cargo run -p mediamerger-app`
Expected: with tracks selected, an offset resolved, and an output path chosen, clicking "Merge" streams log lines and progress percentage updates live until completion or failure.

- [ ] **Step 7: Commit**

```bash
git add mediamerger-app/src/state.rs mediamerger-app/src/main.rs mediamerger-app/src/ui
git commit -m "Add output picker, live merge progress, and startup binary check"
```

---

## Task 14: Packaging metadata

**Files:**
- Modify: `mediamerger-app/Cargo.toml`
- Create: `mediamerger-app/assets/mediamerger.desktop`

**Interfaces:**
- None (packaging metadata only; no code interfaces).

- [ ] **Step 1: Create `mediamerger-app/assets/mediamerger.desktop`**

```
[Desktop Entry]
Type=Application
Name=MediaMerger
Comment=Merge video and audio from two encodes of the same movie into a synced MKV
Exec=mediamerger
Terminal=false
Categories=AudioVideo;Video;
StartupWMClass=mediamerger
```

- [ ] **Step 2: Add packaging metadata to `mediamerger-app/Cargo.toml`**

Append:

```toml
[package.metadata.deb]
name = "mediamerger"
maintainer = "MediaMerger Contributors"
copyright = "2026"
extended-description = "Merge video and audio tracks from two encodes of the same movie into a single synced MKV, using audio cross-correlation to compute the mux offset."
depends = "mkvtoolnix, ffmpeg"
section = "utils"
priority = "optional"
assets = [
    ["target/release/mediamerger", "usr/bin/", "755"],
    ["assets/mediamerger.desktop", "usr/share/applications/", "644"],
]

[[package.metadata.generate-rpm.assets]]
source = "target/release/mediamerger"
dest = "/usr/bin/mediamerger"
mode = "0755"

[[package.metadata.generate-rpm.assets]]
source = "mediamerger-app/assets/mediamerger.desktop"
dest = "/usr/share/applications/mediamerger.desktop"
mode = "0644"
```

- [ ] **Step 3: Verify packaging builds (if `cargo-deb`/`cargo-generate-rpm` are installed)**

Run: `cargo build --release -p mediamerger-app && cargo deb -p mediamerger-app`
Expected: produces a `.deb` in `target/debian/` declaring a dependency on `mkvtoolnix` and `ffmpeg`. If `cargo-deb` isn't installed, install it with `cargo install cargo-deb` first, or skip this verification step and note it as pending in the commit message.

- [ ] **Step 4: Commit**

```bash
git add mediamerger-app/Cargo.toml mediamerger-app/assets/mediamerger.desktop
git commit -m "Add .deb/.rpm packaging metadata and desktop entry"
```

---

## Task 15: Core end-to-end integration test

**Files:**
- Create: `mediamerger-core/tests/end_to_end.rs`

**Interfaces:**
- Consumes: `probe::identify`, `probe::check_framerate` (Tasks 2–3), `offset::detect_offset` (Task 6), `mux::{build_command, run_mux}` (Tasks 7–8)

- [ ] **Step 1: Write the integration test, generating fixtures at test time with ffmpeg**

Create `mediamerger-core/tests/end_to_end.rs`:

```rust
use mediamerger_core::{mux, offset, probe};
use std::path::PathBuf;
use std::process::Command;

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
}
fn mkvmerge_available() -> bool {
    Command::new("mkvmerge").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// Generates a short synthetic "movie": a video track with a solid color and
/// a sine-wave audio track, `duration` seconds long. `lead_in` seconds of
/// silence are prepended to the audio so File A and File B can simulate
/// differing intro lengths while sharing the same underlying content after
/// the lead-in.
fn generate_fixture(path: &PathBuf, duration_secs: u32, lead_in_secs: f64) {
    let audio_filter = format!(
        "sine=frequency=440:duration={duration_secs},adelay={}|{}",
        (lead_in_secs * 1000.0) as u64,
        (lead_in_secs * 1000.0) as u64
    );
    let status = Command::new("ffmpeg")
        .args(["-y", "-f", "lavfi", "-i"])
        .arg(format!("testsrc=duration={duration_secs}:size=320x240:rate=24"))
        .args(["-f", "lavfi", "-i"])
        .arg(audio_filter.replace("sine=", "sine="))
        .args(["-c:v", "libx264", "-c:a", "aac", "-shortest"])
        .arg(path)
        .status()
        .expect("failed to spawn ffmpeg");
    assert!(status.success(), "fixture generation failed for {path:?}");
}

#[test]
fn full_pipeline_recovers_known_offset_and_produces_synced_output() {
    if !ffmpeg_available() || !mkvmerge_available() {
        eprintln!("skipping: ffmpeg and mkvmerge must be installed to run this test");
        return;
    }

    let dir = std::env::temp_dir().join(format!("mediamerger-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file_a = dir.join("a.mkv");
    let file_b = dir.join("b.mkv");
    let output = dir.join("out.mkv");

    // File A: no lead-in. File B: 5-second longer intro before the same content.
    generate_fixture(&file_a, 60, 0.0);
    generate_fixture(&file_b, 65, 5.0);

    probe::check_framerate(&file_a, &file_b).expect("framerates should match (both 24fps)");

    let media_a = probe::identify(&file_a).unwrap();
    let media_b = probe::identify(&file_b).unwrap();
    let audio_a = media_a.tracks.iter().find(|t| t.kind == probe::TrackKind::Audio).unwrap().id;
    let audio_b = media_b.tracks.iter().find(|t| t.kind == probe::TrackKind::Audio).unwrap().id;
    let video_a = media_a.tracks.iter().find(|t| t.kind == probe::TrackKind::Video).unwrap().id;

    let result = offset::detect_offset(&file_a, audio_a, &file_b, audio_b).unwrap();
    assert!(
        (result.offset - 5.0).abs() < 0.5,
        "expected ~5s offset (File B's content lags File A's by its extra intro), got {}",
        result.offset
    );

    let plan = mux::MergePlan {
        file_a: file_a.clone(),
        file_b: file_b.clone(),
        tracks_from_a: vec![mux::TrackSelection {
            track_id: video_a,
            kind: probe::TrackKind::Video,
            set_default: true,
            set_forced: false,
        }],
        tracks_from_b: vec![mux::TrackSelection {
            track_id: audio_b,
            kind: probe::TrackKind::Audio,
            set_default: true,
            set_forced: false,
        }],
        offset_secs: result.offset,
        chapters: mux::ChapterSource::None,
        attachments_from_a: false,
        attachments_from_b: false,
        tags_from_a: false,
        tags_from_b: false,
        output_path: output.clone(),
    };

    let args = mux::build_command(&plan);
    mux::run_mux(&args, |_event| {}).expect("mux should succeed");

    let merged = probe::identify(&output).unwrap();
    assert_eq!(merged.tracks.len(), 2, "expected exactly one video and one audio track in the output");

    std::fs::remove_dir_all(&dir).ok();
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p mediamerger-core --test end_to_end -- --nocapture`
Expected: PASS if `ffmpeg` and `mkvmerge` are installed locally (prints nothing and exits 0); if either binary is missing, the test prints a skip message and passes trivially rather than failing CI on missing system tools.

- [ ] **Step 3: Commit**

```bash
git add mediamerger-core/tests/end_to_end.rs
git commit -m "Add end-to-end integration test covering probe, offset detection, and mux"
```

---

## Manual verification (after all tasks)

Automated tests validate individual units; before considering the app done, run it against two real differently-encoded copies of the same movie (per the `verify` skill) and confirm:

1. Both files load and show their real tracks with correct languages/codecs.
2. A deliberately mismatched pair (e.g. a PAL 25fps rip vs. an NTSC 23.976fps rip) shows the framerate-mismatch banner and blocks the workflow.
3. "Detect Offset" produces a plausible value and the early/late measurements agree (or, if not, the inconsistency warning appears and blocks auto-merge).
4. The merged output plays with audio and video in sync from the first frame of the extracted middle window through to the end of the file, not just at the detected windows.
5. Chapters/attachments/tags toggles behave as expected in the resulting file (inspect with `mkvmerge -J output.mkv` or `mkvinfo`).

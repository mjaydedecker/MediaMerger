# MediaMerger — Design

## Purpose

A Linux GUI application for combining two encodes of the same movie/video that
each have something the other lacks (e.g. better video in one, better audio or
different-language audio/subtitles in the other). The user picks tracks from
each file to keep, the app detects the time offset between the two files
(their intros/outros are frequently different lengths even though the content
is the same), and produces a single properly-synced `.mkv` via `mkvmerge`.

## Background

Source discussion: the two files are different encodes of the same movie
content, so audio-to-audio cross-correlation is a far more robust sync method
than frame-based video comparison — it's insensitive to crop/color-grading/
encoder differences between the two video encodes, and even differing-language
dubs usually still correlate on the shared music/effects bed. Candidate
existing tools evaluated were BBC R&D's `audio-offset-finder`, `syncstart`, and
`video-offset-finder` (frame-based, fallback only). This project reimplements
the cross-correlation step natively in Rust (see [Approach](#offset-detection)
below) rather than shelling out to those Python tools, to keep the shipped app
a single Rust binary with only `ffmpeg`/`mkvtoolnix` as runtime dependencies —
mirroring how [MediaNamer](https://github.com/mjaydedecker/MediaNamer) depends
on external `mediainfo` rather than bundling a media-parsing library.

A critical precondition: the two files' video framerates must match, or a
single fixed time offset cannot hold (e.g. 23.976 vs. 25fps PAL speedup would
cause drift over the runtime). This is checked before any offset detection is
attempted.

## Non-goals

- No speed/tempo correction (rubberband/atempo) for framerate-mismatched
  inputs — out of scope; the app detects and blocks this case instead.
- No support for more than two input files at a time.
- No video-frame-based fallback sync method (audio cross-correlation only).
- No general-purpose mkvmerge GUI features unrelated to this workflow (no
  arbitrary track editing beyond selection, default/forced flags, and the
  chapters/attachments/tags choices described below).

## Architecture

Two-crate Rust workspace, following the same shape as MediaNamer:

- **`mediamerger-core`** — all business logic, no GUI dependencies: media
  probing, audio-offset detection, mkvmerge command construction, process
  execution. Fully unit-testable without a display.
- **`mediamerger-app`** — the `iced` GUI, binary name `mediamerger`. Talks to
  `core` through async commands/subscriptions, mirrors state, renders it.

**External runtime dependencies** (declared as package dependencies in
`.deb`/`.rpm`, not bundled): `ffmpeg` / `ffprobe` (probing + PCM extraction for
offset detection) and `mkvtoolnix` / `mkvmerge` (final mux). No Python, no
bundled media-decoding libraries.

**Key crates**: `iced` 0.14 (`tokio` feature), `tokio`, `rfd` (file dialogs,
`xdg-portal` feature), `dark-light` (theme detection), `rustfft` (offset
cross-correlation), `serde` / `serde_json` (parsing `mkvmerge -J` and
`ffprobe -print_format json` output).

## `mediamerger-core` internals

### `probe` module

Runs `mkvmerge -J <file>` (JSON identification) for each input file and
deserializes into a `MediaFile` struct: path, container, and a `Vec<Track>`
where each `Track` has `{ id, kind (Video/Audio/Subtitle), codec, language,
name, default_flag, forced_flag, fps (video only), channels (audio only) }`.
Using `mkvmerge -J` (rather than `ffprobe`) means the track `id`s shown in the
UI are exactly the ids `mkvmerge` expects later for `--audio-tracks` /
`--video-tracks` / `--subtitle-tracks` — no id-mapping bugs between probing and
merging.

A second, narrower probe (`ffprobe -show_entries stream=r_frame_rate`)
cross-checks video framerate between File A and File B. If they differ beyond
a small epsilon, `probe` returns `MergerError::FramerateMismatch` immediately.
This gates offset detection entirely — the workflow blocks before any
correlation is attempted (see [Non-goals](#non-goals)).

### `offset` module — cross-correlation engine {#offset-detection}

- `extract_window(file, track_id, start_secs, duration_secs) -> Vec<f32>`:
  shells out to
  `ffmpeg -ss <start> -t <duration> -i <file> -map 0:<track_id> -vn -ac 1 -ar 16000 -f f32le -`
  and reads mono 16kHz PCM off stdout directly (no temp files).
- `cross_correlate(a: &[f32], b: &[f32]) -> (offset_secs: f32, confidence: f32)`:
  zero-pads both windows to the same power-of-two length, FFTs via `rustfft`,
  multiplies one spectrum by the conjugate of the other (GCC-PHAT-style
  phase-weighting for a sharper correlation peak than a raw cross-correlation),
  inverse-FFTs, and finds the peak lag. Confidence is the peak height
  normalized against the mean of the rest of the correlation curve.
- `detect_offset(file_a, file_b) -> OffsetResult { early_offset, late_offset,
  consistent: bool, confidence }`: runs `cross_correlate` on two windows — one
  around 25–35% into the shorter file's duration, one around 65–75% — and
  flags `consistent` if the two measurements agree within a small tolerance
  (e.g. 50ms).
  - If the file is too short for both windows to be well separated, fall back
    to fixed windows at 20%/80% with a minimum gap; if even that doesn't fit,
    run a single window and mark the result as unverified (no consistency
    check) rather than failing.

### `mux` module

- `build_command(plan: &MergePlan) -> Vec<String>` — a pure function (no side
  effects, easy to unit test as string-list assertions) that turns the user's
  selections into `mkvmerge` arguments: `--audio-tracks` / `--video-tracks` /
  `--subtitle-tracks` per input file, `--sync <tid>:<offset_ms>` on every track
  sourced from File B (File A is always the reference / timeline zero),
  `--chapters` / `--no-chapters`, `--attachments` / `--no-attachments`,
  `--no-global-tags` / `--no-track-tags` per the Extras choices, `--track-order`,
  and `-o <output>`.
- `run_mux(command) -> impl Stream<Item = MuxEvent>` — executes the command,
  parsing `mkvmerge`'s `#GUI progress` stdout lines into a progress percentage,
  and forwarding raw stdout/stderr lines for the log panel. Exit code 1 (`mkvmerge`
  warning) is treated as a completed merge with a warning; exit code 2 is
  `MergerError::MuxFailed`.

### Error type

A single `MergerError` enum: `Probe`, `FramerateMismatch`, `FfmpegNotFound`,
`MkvmergeNotFound`, `MuxFailed(String)`. The app layer matches on this one type
for all user-facing error messages.

## `mediamerger-app` (iced GUI)

### State (`AppState`)

- `file_a: Option<MediaFile>`, `file_b: Option<MediaFile>`
- Per-track `selected: HashSet<TrackId>`, plus `default_flag` / `forced_flag`
  overrides
- `offset: OffsetState` — `NotDetected | Detecting | Detected(OffsetResult) |
  ManualOverride(f32)`
- Extras: `chapters_source: ChapterSource` (A / B / None), `attachments: {a:
  bool, b: bool}`, `tags: {a: bool, b: bool}`
- `output_path: Option<PathBuf>`
- `merge_progress: Option<f32>`, `log: Vec<String>` (raw `mkvmerge` output for
  the console panel)
- `theme` (from `dark-light`)
- `missing_binaries: Vec<&'static str>` (populated at startup)

### Messages

`PickFileA` / `PickFileB` (opens `rfd` dialog) → `FileProbed(Result<MediaFile,
MergerError>)`; `ToggleTrack(file, track_id)`; `SetDefaultFlag` /
`SetForcedFlag`; `DetectOffset` → `OffsetDetected(Result<OffsetResult,
MergerError>)`; `ManualOffsetChanged(String)`; `ToggleExtra(...)`;
`PickOutput` → path; `StartMerge` → `MergeProgress(f32)` /
`MergeDone(Result<(), MergerError>)`.

### Async flow

Every long-running core call (`probe`, `detect_offset`, `run_mux`) runs via
`Command::perform` (probing, offset detection) or a `Subscription` (mux
progress is a stream, so it yields `MergeProgress` messages as `mkvmerge`
writes progress lines) — the UI thread never blocks. This mirrors MediaNamer's
existing `iced` + `tokio` pattern.

### Layout

Single window, sectioned top to bottom:

```
+----------------------------------------------------+
| File A: [ movie_video.mkv      ] [Browse]           |
| File B: [ movie_audio.mkv      ] [Browse]           |
+----------------------------------------------------+
| Tracks (File A)          | Tracks (File B)          |
| [x] Video: h264 1080p    | [ ] Video: h264 1080p    |
| [ ] Audio: EN 5.1 AC3    | [x] Audio: EN 5.1 DTS    |
| [ ] Sub: EN SRT          | [x] Sub: FR SRT          |
+----------------------------------------------------+
| Offset:  [ Detect Offset ]  -> +2.348s (conf 0.94)  |
| Verify near end: +2.351s  [OK, consistent]          |
| Manual override: [ 2.348   ] s                      |
+----------------------------------------------------+
| Extras: Chapters (A) (B) (none)                     |
|         Attachments [x] A [ ] B  Tags [ ] A [ ] B   |
+----------------------------------------------------+
| Output: [ output.mkv          ] [Browse]  [ Merge ] |
+----------------------------------------------------+
| [ collapsible log panel: raw mkvmerge output ]      |
+----------------------------------------------------+
```

### Error surfacing

- `FramerateMismatch` renders as a red banner across the track table, blocking
  track selection and the Detect/Merge buttons entirely.
- `FfmpegNotFound` / `MkvmergeNotFound` render as a one-time startup banner
  naming the missing binary, blocking the whole workflow.
- Inconsistent offset measurements (`consistent == false`) render as a warning
  banner showing both values; the offset field remains editable/unconfirmed
  until the user picks one — never silently proceeds (per explicit decision).
- Low-confidence-but-consistent results show a yellow inline note but do not
  block — two independent measurements agreeing is strong evidence even with a
  middling correlation peak.
- `MuxFailed` surfaces inline near the Merge button, with the actual
  `mkvmerge` error text already visible above it in the log panel.

## End-to-end data flow

1. User picks File A and File B via `rfd` → each triggers `probe::identify`
   (mkvmerge -J) and `probe::check_framerate` (ffprobe) concurrently → track
   tables populate. Framerate mismatch blocks here (see above).
2. User checks/unchecks tracks, sets default/forced flags, sets extras — local
   state only, no I/O.
3. User clicks **Detect Offset** → `offset::detect_offset` runs both windows.
   Consistent result auto-fills the offset field (green); inconsistent shows
   the warning banner and requires manual resolution.
4. User may override the numeric offset directly regardless of the detected
   value.
5. User picks output path via `rfd`, clicks **Merge** → `mux::build_command`
   constructs the argument list (shown in the log panel before execution, for
   transparency) → `mux::run_mux` streams progress and raw output into the UI.
6. `MergeDone(Ok)` shows success with the output path and an "open folder"
   affordance; `MergeDone(Err)` shows `MuxFailed` with the mkvmerge error
   already visible in the log panel above it.

## Testing strategy

- **`core` unit tests**: `mux::build_command` is pure — assert exact argument
  vectors for representative `MergePlan`s (simple case, each Extras variant,
  offset applied only to File-B-sourced tracks). `offset::cross_correlate`
  tested against synthetic signals (sine/noise burst shifted by a known lag)
  to verify exact offset recovery and that confidence drops on pure-noise
  input.
- **`core` integration tests**: a small fixture pair of short MKV clips (with
  a known injected offset) run through `probe` → `detect_offset` →
  `build_command` → actual `run_mux`, asserting the output file's audio starts
  at the expected point by re-probing the output rather than eyeballing it.
- **App-layer**: `iced`'s `update` function is a pure `(State, Message) ->
  (State, Command)` reducer, so state transitions (toggling a track,
  receiving `OffsetDetected`, etc.) are tested without a display.
- **Manual/exploratory**: real end-to-end runs with actual differently-encoded
  movie files before any milestone is called done — a subtly wrong `--sync`
  sign convention is the kind of bug automated tests won't reliably catch.

## Packaging

`.deb` / `.rpm`, matching MediaNamer's packaging approach, declaring package
dependencies on `mkvtoolnix` and `ffmpeg` rather than bundling either.

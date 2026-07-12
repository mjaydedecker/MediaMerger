# MediaMerger

A GNOME desktop app for merging video, audio, and subtitle tracks from two
encodes of the same movie into a single synced `.mkv` — automatically
detecting the sync offset between them via audio cross-correlation.

The typical use case: you have a high-quality video encode with mediocre
audio, and a separate encode (or disc rip) with the audio track you actually
want. MediaMerger finds how far the two are offset from each other, then
lets you pick exactly which tracks to keep from each file and mux them into
one properly-synced output.

## Features

- **Automatic sync offset detection** — cross-correlates the audio from
  both files (an early-window and late-window probe) to compute the delay
  between them, with a consistency check to flag cases where a single fixed
  offset doesn't reliably hold across the runtime.
- **Manual offset override** — type a known offset directly, with a
  one-click way to fall back to the last auto-detected value.
- **Framerate mismatch guard** — warns before aligning two sources whose
  video framerates genuinely differ (e.g. an NTSC vs. PAL-speedup transfer),
  with a manual override for cases where you know the underlying audio
  speed matches despite a video-track mismatch.
- **Per-track selection** — choose which video, audio, and subtitle tracks
  to carry over from each file, with default/forced flags set per track.
- **Extras** — pick which file's chapters to keep, and whether to carry
  over attachments (fonts, cover art) and metadata tags from either source.
- **Default output naming** — the output path defaults to File A's own
  name and location with a `— Merged` postfix, so you rarely need to Browse
  for it manually.
- **New merge** — reset the whole session in one click to start on a
  different pair of files.
- Native GNOME look and feel: follows the system's light/dark preference
  and accent color automatically.

## Requirements

MediaMerger is a thin GUI layer over standard media tooling — it shells out
to these at runtime rather than reimplementing muxing/decoding itself:

- [`ffmpeg`/`ffprobe`](https://ffmpeg.org/) — audio extraction for offset
  detection, and media probing
- [`mkvtoolnix`](https://mkvtoolnix.download/) (`mkvmerge`) — the actual
  muxing

The app checks for all three on startup and will tell you if any are
missing.

## Installation

### From a release package (Debian/Ubuntu)

```bash
sudo apt install ./mediamerger_0.1.0-1_amd64.deb
```

This also installs the `.desktop` entry and app icon into the standard
locations, so MediaMerger shows up in GNOME's app grid/Activities like any
other installed app.

### From source

Requires a recent stable Rust toolchain ([rustup](https://rustup.rs/)) plus
`ffmpeg`, `ffprobe`, and `mkvtoolnix` installed and on `PATH`.

```bash
git clone https://github.com/mjaydedecker/MediaMerger.git
cd MediaMerger
cargo build --release
./target/release/mediamerger
```

To build a `.deb` package yourself:

```bash
cargo install cargo-deb
cargo deb -p mediamerger-app
```

The resulting package is written to `target/debian/`.

## Usage

1. **Load your two files.** File A is the base — its video track and
   overall timeline are what File B gets aligned to. File B is the donor —
   typically wherever the tracks you actually want to keep from live.
2. **Detect the offset**, or type a known one manually. The waveform view
   shows both files' audio with the detected offset marked, so you can
   visually confirm the alignment looks right.
3. **Pick your tracks.** Select which video/audio/subtitle tracks to
   include from each file, and set default/forced flags as needed.
4. **Set extras** — chapters source, attachments, and tags — if you want
   more than the defaults.
5. **Choose (or confirm) the output path** and hit **Merge**.

## Project structure

Two-crate Rust workspace:

- `mediamerger-core` — the library: media probing (`mkvmerge -J`
  parsing), audio cross-correlation for offset detection, and `mkvmerge`
  command construction. No GUI dependencies — this crate is fully unit
  tested independent of the UI.
- `mediamerger-app` — the [`iced`](https://iced.rs/) GUI binary that ties
  it together.

## Contributing

Issues and pull requests are welcome. If you're changing anything in
`mediamerger-core`, please add tests — the offset-detection and muxing
logic in particular is expected to stay well covered.

## License

MIT — see [LICENSE](LICENSE).

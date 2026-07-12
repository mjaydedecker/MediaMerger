# App Icon Integration — Design

## Purpose

Wire the app icon delivered in `design_files/MediaMerger Design Help-v1.0.3-handoff.zip`
into MediaMerger: the running window's icon (dock/alt-tab/titlebar), the
`.desktop` entry, and proper installation into the standard Linux hicolor
icon theme via packaging — full integration, per the user's explicit choice.

## Assets provided

`mediamerger-design-help/project/icons/`: `mediamerger.svg` (full-color
master, gradient blue tile with a white "converging paths + forward arrow"
glyph), `mediamerger-symbolic.svg` (monochrome, for GNOME panel contexts),
and PNGs at 16/32/48/64/128/256/512px. The bundle's own `MediaMerger
Icon.dc.html` specifies exactly where these belong for packaging:
`mediamerger.svg` → `usr/share/icons/hicolor/scalable/apps/mediamerger.svg`,
`mediamerger-symbolic.svg` → `usr/share/icons/hicolor/symbolic/apps/mediamerger-symbolic.svg`.

## Technical findings

- `iced::window::Settings.icon: Option<window::Icon>` (confirmed in
  `iced_core-0.14.0/src/window/settings.rs`) is how the running window's
  icon gets set — currently `None`, unset anywhere in `main.rs`.
- The only constructor is `iced::window::icon::from_rgba(rgba: Vec<u8>,
  width: u32, height: u32) -> Result<Icon, Error>` (confirmed in
  `iced_core-0.14.0/src/window/icon.rs`) — no PNG-decoding convenience
  exists anywhere in the installed `iced` 0.14 crates. A PNG must be
  decoded into raw RGBA8 pixels before calling it.
- `mediamerger-128.png` (confirmed 128×128, 8-bit RGBA, non-interlaced) is
  a reasonable single size for the runtime window icon — window icons are
  scaled by the compositor/toolkit as needed, and 128px is a common choice
  for this purpose among desktop apps.
- Decoding the PNG needs a decoder. The `image` crate with `default-features
  = false, features = ["png"]` gives a minimal, correct one-liner
  (`image::load_from_memory(bytes)?.into_rgba8()`) without pulling in
  decoders for formats this project doesn't use.
- The `.desktop` file (`mediamerger-app/assets/mediamerger.desktop`)
  currently has no `Icon=` line at all — falls back to a generic icon in
  GNOME's Activities/app grid/file manager today.
- `Cargo.toml` already has parallel `[package.metadata.deb]` and
  `[package.metadata.generate-rpm]` asset lists (used for the existing
  binary + `.desktop` file installation) — the new hicolor icon paths
  follow the exact same list-based pattern already established there.

## Non-goals

- No PNG hicolor installs (`.../16x16/apps/...`, `.../128x128/apps/...`,
  etc.) — the design bundle's own packaging guidance only calls for the
  scalable SVG and the symbolic SVG; modern GNOME renders scalable SVG
  hicolor icons at any size without needing fixed-size PNG fallbacks.
- No change to the existing in-app UI icon system (`mediamerger-app/src/ui/icons.rs`,
  `assets/icons/*.svg`) — those are small monochrome glyphs recolored at
  runtime for a different purpose (toolbar/button icons) and are unrelated
  to the app's own identity icon. The new assets get their own directory.
- No custom window-chrome titlebar icon — this project already excludes
  custom window chrome (established non-goal from the visual redesign
  round); the window icon set via `window::Settings.icon` covers the
  standard GNOME dock/alt-tab/taskbar representation, which is what
  "the running app's icon" means without custom chrome.

## Testing strategy

Pure asset/packaging wiring — no new testable logic. Verified by a clean
`cargo build`/`cargo test`/`cargo clippy` pass and by rebuilding the `.deb`
and inspecting its contents (`dpkg-deb -c`) to confirm the icon files land
at the expected hicolor paths.

## Manual verification (requires a real GNOME desktop; not available in this sandbox)

1. Launch the app; confirm the window icon (dock, alt-tab, titlebar per
   GNOME's window-list) shows the new blue icon, not a generic placeholder.
2. Install the built `.deb`; confirm GNOME Activities/the app grid shows
   the new icon (may require an icon-cache refresh depending on the
   desktop session).

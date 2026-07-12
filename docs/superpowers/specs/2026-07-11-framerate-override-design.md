# Framerate Mismatch Override — Design

## Purpose

Let the user consciously bypass the video-framerate mismatch check when they
already know, from context outside the app, that the audio speed is fine —
without weakening the check as a default safety net for the case it actually
protects against.

## Background

`mediamerger-core::probe::check_framerate` compares File A's and File B's
first video stream's fps (0.05fps tolerance) and blocks both "Detect offset"
and "Merge" via `MergerError::FramerateMismatch` /
`AppState::blocking_reason` whenever they differ. The check exists to guard
against a real failure mode: combining a video and an audio track whose
underlying masters run at genuinely different speeds (e.g. NTSC 23.976fps vs.
PAL-speedup 25fps), where a single fixed offset can't hold for the whole
runtime.

That heuristic assumes video fps is a reliable proxy for the file's audio
speed. It isn't always: a user may deliberately mux a real audio track with a
disposable, unrelated low-quality video track (e.g. to route audio through an
external encoder that requires a video stream), producing a fps mismatch with
no bearing on the actual audio timing. The check currently has no way to
distinguish this from a genuine speed mismatch, and blocks unconditionally
with no path forward except picking different files.

Separately, the app already has a second, more direct drift-detection
mechanism: cross-correlating an early-window and a late-window of the audio
and flagging `Consistency::Inconsistent` if they disagree beyond tolerance.
This measures actual audio behavior rather than inferring it from video
metadata, and remains fully active regardless of the change below — it is
the real safety net for genuine drift.

Also verified: `offset::extract_window`'s ffmpeg invocation
(`-map 0:{track_id} -vn ...`) selects only the target audio stream and
explicitly excludes video, so the correlation math itself is unaffected by
video framerate or quality. The fps check does not protect the correlation
computation — only the "is a single fixed offset even valid" precondition.

## Design

**New state:** `AppState.framerate_override: bool` (default `false`),
alongside the existing `framerate_error: Option<MergerError>`.

**New message:** `Message::FramerateOverrideToggled(bool)`.

**Reset on file change:** `main.rs::apply_probe_result` already resets
`framerate_error` to `None` and recomputes it fresh on every probe of either
file — the existing single chokepoint for "the file pairing changed." Add
`state.framerate_override = false;` alongside that reset, so picking a new
File A or File B always clears any prior override before the (possibly new)
mismatch is (re-)evaluated.

**Gating logic (two call sites currently checking `framerate_error.is_some()`
alone):**
- `state.rs::blocking_reason()`: change the framerate branch's condition to
  `self.framerate_error.is_some() && !self.framerate_override`.
- `offset_panel.rs`'s Detect-Offset button press-gating: same added
  `&& !state.framerate_override` condition.

**UI:** `file_pickers.rs::framerate_banner` (where the mismatch warning
already renders) gains a checkbox, shown only while `framerate_error` is
`Some`: label "I know the audio speed matches — continue anyway", bound to
`framerate_override`, emitting `Message::FramerateOverrideToggled` on
toggle.

**Left unchanged:**
- The 0.05fps tolerance and `check_framerate` itself.
- The `Consistency`/early-late drift check and its own independent blocking
  behavior — stays fully active regardless of this override.
- No persistence across app restarts, and no persistence across a file
  change (explicit non-goal, see below).

## Non-goals

- No change to the underlying fps tolerance or comparison logic.
- No override persistence across file changes or app restarts — every new
  File A/File B pairing must be consciously re-acknowledged.
- No UI or behavior change to the Consistency/Inconsistent blocking path.
- No attempt to auto-detect "this video track is a disposable placeholder" —
  the override is an explicit, manual user judgment call, not a heuristic.

## Testing strategy

- `state.rs` unit tests: `blocking_reason()` returns `None` (or falls
  through to the next check) when `framerate_error` is `Some` and
  `framerate_override` is `true`; still returns the framerate message when
  `framerate_override` is `false`.
- `main.rs` unit test: `apply_probe_result(state: &mut AppState, result: ...,
  is_file_a: bool)` is a plain free function (confirmed — no GUI/runtime
  machinery required to call it directly), so add a `#[cfg(test)] mod
  tests` block to `main.rs` (none exists yet) with a test confirming
  `framerate_override` resets to `false` after a call to
  `apply_probe_result`, even when it was `true` beforehand.
- No new iced-widget-specific test for the checkbox itself, consistent with
  this project's existing approach to view code (build-checked, not unit
  tested).

## Manual verification

1. Load two files with mismatched video fps; confirm the warning banner and
   the new checkbox both appear, and Detect Offset / Merge stay disabled
   until the checkbox is checked.
2. Check the box; confirm Detect Offset becomes clickable and, once an
   offset is detected/entered, Merge becomes available (assuming all other
   merge preconditions are met).
3. Replace File A or File B with a different file; confirm the checkbox
   resets to unchecked and the warning/override cycle must be repeated for
   the new pairing.
4. With the override checked, force an early/late offset disagreement (or
   use two files that actually drift); confirm `Consistency::Inconsistent`
   still blocks the merge independently of the framerate override.

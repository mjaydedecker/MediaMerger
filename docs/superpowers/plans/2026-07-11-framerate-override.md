# Framerate Mismatch Override Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user manually acknowledge and bypass the video-framerate-mismatch precondition when they know the underlying audio speed matches, without weakening the check as a default safety net.

**Architecture:** Add a `framerate_override: bool` field to `AppState`, gate the two existing `framerate_error`-driven blocks (merge, Detect Offset) on `!framerate_override` in addition to their current condition, reset the override to `false` on every fresh file probe (the existing `apply_probe_result` chokepoint), and surface a checkbox next to the mismatch warning in `file_pickers.rs`.

**Tech Stack:** Rust, `iced` 0.14 (`mediamerger-app`), `mediamerger-core` (unchanged by this plan).

## Global Constraints

- `AppState::blocking_reason`/`to_merge_plan`/`resolved_offset_secs` remain the single source of truth for merge-readiness — this project has twice had view-layer merge-gating logic drift out of sync with these functions (see `output_log.rs`'s `merge_enabled`); do not introduce a third, independent copy of any gating condition.
- The `Consistency`/early-late drift check (`OffsetState::Detected(r) if r.consistency == Consistency::Inconsistent` in `blocking_reason`) is completely unaffected by this plan — do not touch its condition or its own blocking behavior.
- `framerate_override` must reset to `false` on every fresh probe result for either file (both `Ok` and `Err` outcomes) — never persists across a file change, per the design's explicit non-goal on persistence.
- Verify any `iced` widget API (`checkbox`, `.label()`, `.on_toggle()`) against the actually-installed crate source (`~/.cargo/registry/src/*/iced_widget-*/`) before relying on its exact signature — this codebase has repeatedly found brief-sketch APIs to differ from the installed version.
- Closures capturing `Color`/palette-derived values in `move |...|` styling callbacks must capture local `let`-bound copies, not read fields off a `&Palette` reference inside the closure (this project's established lifetime pattern).

---

### Task 1: `AppState`/`Message` plumbing and `blocking_reason` gating

**Files:**
- Modify: `mediamerger-app/src/state.rs`

**Interfaces:**
- Produces: `AppState.framerate_override: bool` (new field, `Default` = `false`); `Message::FramerateOverrideToggled(bool)` (new variant). Both consumed by Tasks 2 and 3.
- Consumes: nothing new — this task only touches `state.rs`.

- [ ] **Step 1: Add the new field to `AppState` and its `Default` impl**

In `mediamerger-app/src/state.rs`, add the field right after the existing `pub framerate_error: Option<MergerError>,` (line 61):

```rust
    pub framerate_error: Option<MergerError>,
    pub framerate_override: bool,
```

And in the `Default for AppState` impl, right after `framerate_error: None,` (line 91):

```rust
            framerate_error: None,
            framerate_override: false,
```

- [ ] **Step 2: Add the new `Message` variant**

Add `FramerateOverrideToggled(bool)` right after `FileBProbed(Result<MediaFile, MergerError>),` (line 121) in the `Message` enum:

```rust
    FileAProbed(Result<MediaFile, MergerError>),
    FileBProbed(Result<MediaFile, MergerError>),
    FramerateOverrideToggled(bool),
    RefreshSystemTheme,
```

- [ ] **Step 3: Write the failing test for the new `blocking_reason` behavior**

Add to the `tests` module at the bottom of `mediamerger-app/src/state.rs` (near the existing `blocking_reason_some_when_framerate_error_set` test):

```rust
    #[test]
    fn blocking_reason_none_when_framerate_error_overridden() {
        let mut state = AppState::default();
        state.framerate_error = Some(MergerError::Probe("framerate mismatch".to_string()));
        state.framerate_override = true;
        assert_eq!(state.blocking_reason(), None);
    }
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cargo test -p mediamerger-app blocking_reason_none_when_framerate_error_overridden -- --exact`
Expected: FAIL (current `blocking_reason` still returns `Some(...)` regardless of a nonexistent `framerate_override` field — this won't even compile yet since the field doesn't exist until Step 1's edit is in place; if Steps 1-3 are applied together before first running tests, this step instead confirms the test fails only because `blocking_reason`'s condition hasn't been updated yet, i.e. it still returns `Some(...)`).

- [ ] **Step 5: Update `blocking_reason` to respect the override**

In `mediamerger-app/src/state.rs`, change the framerate branch of `blocking_reason` (line 173):

```rust
    pub fn blocking_reason(&self) -> Option<String> {
        if self.framerate_error.is_some() && !self.framerate_override {
            return Some("video framerates do not match".to_string());
        }
```

- [ ] **Step 6: Run the full `state.rs` test suite to verify everything passes**

Run: `cargo test -p mediamerger-app state::`
Expected: PASS — all existing tests (including the unchanged `blocking_reason_some_when_framerate_error_set`, which still passes since `framerate_override` defaults to `false`) plus the new test.

- [ ] **Step 7: Commit**

```bash
git add mediamerger-app/src/state.rs
git commit -m "Add framerate_override state and gate blocking_reason on it"
```

---

**Addendum (found during Task 1 implementation):** adding `FramerateOverrideToggled`
to the `Message` enum immediately breaks `main.rs`'s exhaustive `match message { ... }`
in `update()` — Rust requires every variant handled, so `mediamerger-app` cannot
compile with the new variant present and unhandled. Task 1's commit therefore
also includes the minimal match arm in `main.rs`:

```rust
        Message::FramerateOverrideToggled(v) => {
            state.framerate_override = v;
            Task::none()
        }
```

This is the same code Task 2 Step 1 below specifies (placement in the match
differs slightly — trailing, near `ToggleLogExpanded`, rather than right
after `FileBProbed` — but is otherwise identical and correct). **Task 2's
Step 1 is therefore already done** as of Task 1's commit; Task 2 should
verify it rather than re-add it, and focus on Steps 2-4.

---

### Task 2: Wire the override into `main.rs`'s update loop and probe-result reset

**Files:**
- Modify: `mediamerger-app/src/main.rs`

**Interfaces:**
- Consumes: `AppState.framerate_override`, `Message::FramerateOverrideToggled` (Task 1).
- Produces: nothing new for later tasks — Task 3's view-layer change reads `state.framerate_override` directly, it doesn't need anything from this task's internals beyond the field already existing.

This task also fixes a third gating site the design didn't originally call out: `Message::DetectOffset`'s handler in `main.rs` independently re-checks `state.framerate_error.is_some()` before proceeding (line 178), separately from the button's own press-gating in `offset_panel.rs` (Task 3). Both must respect the override, the same way `StartMerge`'s handler independently re-checks `blocking_reason()` regardless of what the Merge button's own enabled state showed.

- [ ] **Step 1: Add the message handler**

In `mediamerger-app/src/main.rs`'s `update` function, add a new match arm right after the `Message::FileBProbed(result) => { ... }` arm (after line 122):

```rust
        Message::FramerateOverrideToggled(value) => {
            state.framerate_override = value;
            Task::none()
        }
```

- [ ] **Step 2: Reset the override at the top of `apply_probe_result`**

Change `apply_probe_result` (line 406) to reset `framerate_override` unconditionally before handling either the `Ok` or `Err` case, so a probe failure on either file also clears a stale override:

```rust
fn apply_probe_result(
    state: &mut AppState,
    result: Result<mediamerger_core::probe::MediaFile, mediamerger_core::error::MergerError>,
    is_file_a: bool,
) {
    state.framerate_override = false;
    match result {
        Ok(media_file) => {
            if is_file_a {
                AppState::sync_track_ui_len(&media_file.tracks, &mut state.tracks_a_ui);
                state.file_a = Some(media_file);
            } else {
                AppState::sync_track_ui_len(&media_file.tracks, &mut state.tracks_b_ui);
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

- [ ] **Step 3: Update the `DetectOffset` handler's independent guard**

Change the guard at the top of the `Message::DetectOffset` arm (line 178):

```rust
        Message::DetectOffset => {
            if state.framerate_error.is_some() && !state.framerate_override {
                return Task::none();
            }
```

- [ ] **Step 4: Write the failing test for the reset behavior**

`main.rs` has no `#[cfg(test)] mod tests` block yet. Add one at the end of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use mediamerger_core::probe::{MediaFile, Track, TrackKind};
    use std::path::PathBuf;

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

    fn media_file(path: &str) -> MediaFile {
        MediaFile {
            path: PathBuf::from(path),
            container: "Matroska".to_string(),
            tracks: vec![track(0, TrackKind::Video)],
            file_size_bytes: 0,
            duration_secs: None,
        }
    }

    #[test]
    fn apply_probe_result_resets_framerate_override_even_if_previously_true() {
        let mut state = AppState::default();
        state.framerate_override = true;

        apply_probe_result(&mut state, Ok(media_file("a.mkv")), true);

        assert!(!state.framerate_override, "a fresh probe result must clear any prior override");
    }

    #[test]
    fn apply_probe_result_resets_framerate_override_on_probe_error_too() {
        let mut state = AppState::default();
        state.framerate_override = true;

        apply_probe_result(&mut state, Err(mediamerger_core::error::MergerError::Probe("boom".to_string())), true);

        assert!(!state.framerate_override, "a failed probe must also clear a prior override, not just a successful one");
    }
}
```

- [ ] **Step 5: Run the tests to verify they fail**

Run: `cargo test -p mediamerger-app apply_probe_result_resets_framerate_override -- --exact`
Expected: FAIL before Step 2's edit is applied (both tests assert `!state.framerate_override` but nothing yet resets it). If applying Steps 1-3 together before first test run, confirm instead that the tests compile and would fail against the pre-Step-2 body (i.e., don't skip actually seeing a red run).

- [ ] **Step 6: Run the full test to verify it passes**

Run: `cargo test -p mediamerger-app apply_probe_result_resets_framerate_override -- --exact`
Expected: PASS (2 tests).

Run: `cargo test --workspace`
Expected: all passing, no regressions.

- [ ] **Step 7: Commit**

```bash
git add mediamerger-app/src/main.rs
git commit -m "Wire FramerateOverrideToggled and reset override on every fresh probe"
```

---

### Task 3: View layer — Detect Offset button gating and the override checkbox

**Files:**
- Modify: `mediamerger-app/src/ui/offset_panel.rs`
- Modify: `mediamerger-app/src/ui/file_pickers.rs`

**Interfaces:**
- Consumes: `AppState.framerate_override`, `Message::FramerateOverrideToggled` (Task 1); `Message` handling in `main.rs` (Task 2).
- No signature changes to either file's `pub fn view`.

This task has no new testable logic (pure `iced` view code, consistent with this project's established approach — verified by build-checking and manual verification, not unit tests). It does carry real API risk: verify `checkbox`'s constructor, `.label()`, and `.on_toggle()` against the installed `iced_widget` crate before relying on the exact calls below — `mediamerger-app/src/ui/track_table.rs:71` already uses `checkbox(ui.selected).on_toggle(move |_| on_toggle(idx))` in this exact codebase (no `.label()` there, since that checkbox has no baked-in text), which confirms the constructor and `.on_toggle()` shape; `.label()` itself should be checked the same way this project checked it for other widgets.

- [ ] **Step 1: Gate the Detect Offset button on the override**

In `mediamerger-app/src/ui/offset_panel.rs`, change line 138:

```rust
    let detect_offset_press = if state.framerate_error.is_some() && !state.framerate_override { None } else { Some(Message::DetectOffset) };
```

- [ ] **Step 2: Add the override checkbox to the framerate warning banner**

In `mediamerger-app/src/ui/file_pickers.rs`, add `checkbox` to the `iced::widget` import list (currently `use iced::widget::{button, column, container, row, text};`):

```rust
use iced::widget::{button, checkbox, column, container, row, text};
```

Then change `framerate_banner`'s first branch (the `if let Some(err) = &state.framerate_error` block) to stack the existing warning row with a new checkbox:

```rust
fn framerate_banner<'a>(state: &'a AppState, palette: &Palette) -> Option<Element<'a, Message>> {
    if let Some(err) = &state.framerate_error {
        let warning_row = row![icons::warning(palette.danger_fg), text(err.to_string()).color(palette.danger_fg)].spacing(8);
        let override_checkbox = checkbox(state.framerate_override)
            .label("I know the audio speed matches — continue anyway")
            .on_toggle(Message::FramerateOverrideToggled);
        return Some(column![warning_row, override_checkbox].spacing(6).into());
    }
    if state.file_a.is_some() && state.file_b.is_some() {
```

(Leave the rest of the function — the `file_a.is_some() && file_b.is_some()` framerate-match branch and the final `None` — unchanged.)

- [ ] **Step 3: Build and confirm no regressions**

Run: `cargo build --workspace`
Expected: zero errors. If `checkbox(...).label(...)` doesn't compile as written, check the installed `iced_widget::checkbox` module's actual builder API (e.g. `~/.cargo/registry/src/*/iced_widget-*/src/checkbox.rs`) and adapt — do not drop the label requirement, find the real equivalent call.

Run: `cargo test --workspace`
Expected: all passing (no new tests in this task; existing suite must still be green).

- [ ] **Step 4: Commit**

```bash
git add mediamerger-app/src/ui/offset_panel.rs mediamerger-app/src/ui/file_pickers.rs
git commit -m "Gate Detect Offset button and add override checkbox to framerate warning"
```

---

### Task 4: Final workspace verification

**Files:** none (verification only)

- [ ] **Step 1: Full workspace build, test, and lint**

Run: `cargo build --workspace`
Expected: clean build.

Run: `cargo test --workspace`
Expected: all tests pass, including the 3 new tests added in Tasks 1-2.

Run: `cargo clippy --workspace --all-targets`
Expected: no new warnings beyond this project's known pre-existing categories (elided-lifetime warnings on `ui/*.rs` view function signatures and `main.rs`'s `view`, `field_reassign_with_default` in `state.rs`/`main.rs` test code if any `AppState::default()` + field mutation pattern triggers it — acceptable, matches existing test style in `state.rs`).

- [ ] **Step 2: Commit if any fixup was needed**

If Step 1 required any fixes, commit them:

```bash
git add -A
git commit -m "Fix workspace build/lint issues found in framerate-override verification"
```

---

## Manual verification (requires a real GNOME desktop; not available in this sandbox)

1. Load two files with mismatched video framerates (differ by more than 0.05fps); confirm the warning banner and the new checkbox both appear, and Detect Offset / Merge stay disabled until the checkbox is checked.
2. Check the box; confirm Detect Offset becomes clickable, and — once an offset is detected or manually entered — Merge becomes available (assuming all other merge preconditions are met).
3. Replace File A or File B with a different file; confirm the checkbox resets to unchecked and the warning/override cycle must be repeated for the new pairing.
4. With the override checked, use two files whose audio genuinely drifts (early/late offsets disagree); confirm `Consistency::Inconsistent` still blocks the merge independently of the framerate override.

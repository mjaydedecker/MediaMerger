# Manual Override & New Merge — Design

## Purpose

Implement the mockup's v1.0.2 update (`design_files/MediaMerger Design Help-v1.0.2-handoff.zip`),
which adds a "Manual override" indicator with a "Use detected" reset button
to the offset panel, and a "New merge" button that resets the whole session
back to an empty state without restarting the app. Along the way, fold in
two pre-existing fidelity gaps discovered while re-reading the mockup for
this work: the offset panel's "not detected yet" state was simplified to
plain text instead of the mockup's full icon/headline/detail box, and the
track table's "No file loaded" placeholder is missing the card
background/border the loaded state has.

## Background

Diffing v1.0.1 against v1.0.2 shows the actual new content is: a manual
offset override indicator (pill + reset button) in the offset panel, a
`cleared` app state (dashed-border empty-state placeholders, a "New merge"
button, disabled Merge/Detect readiness text), and the offset input's border
color reflecting override state. The "Offset not detected yet" box was
already present, unchanged, in v1.0.1 — the app's existing implementation
just never matched it.

**State model.** The app's `OffsetState` enum (`NotDetected | Detecting |
Detected(OffsetResult) | ManualOverride(f64)`) already treats a manual text
edit as switching away from `Detected` — this naturally matches the
mockup's `manualActive` condition. What it doesn't do is retain the
detected result for "Use detected" to fall back to. Fix: add
`AppState.last_detected: Option<OffsetResult>`, set whenever a detection
succeeds and never cleared by typing a manual value; "Use detected" restores
`state.offset` from it. This keeps `blocking_reason`/`to_merge_plan`/
`resolved_offset_secs` — the project's established single sources of truth
for merge-readiness — completely untouched.

**Banner/consistency decoupling.** In the mockup, the top status banner
(icon, headline, detail, consistency pill) is driven by `effSync`
(derived from the detection's own consistency, independent of whether the
user is currently overriding the value), while the "Manual override" pill
only replaces the measured-text line below. Mapped to our state: the
banner should render from `last_detected` (if present) regardless of
whether `state.offset` is currently `Detected` or `ManualOverride`. If the
user has entered a manual value with no detection ever run
(`last_detected` is `None`), there's no consistency data to show a rich
banner for — show a distinct, simpler "no detection run" message instead.

## Non-goals

- No change to `blocking_reason`, `to_merge_plan`, or `resolved_offset_secs`'s
  gating logic itself — `last_detected` is purely additional context for
  display and for "Use detected" to restore from.
- No confirmation dialog before "New merge" resets state (matches the
  mockup's own instant-reset behavior).
- No cancellation of an in-progress `mkvmerge` subprocess — "New merge" is
  simply disabled while one is running (`merge_receiver.is_some()`), not a
  cancel button.
- No custom window-chrome duplicate of "New merge" (the mockup's preview
  toolbar shows one in its own simulated title bar; the actual page markup
  only has it once, in the footer — that's the single implementation
  target, consistent with this project's established practice of trusting
  the exported template over screenshots when they disagree).

## Design

### 1. State additions (`state.rs`)

- `AppState.last_detected: Option<OffsetResult>` (default `None`).
- `Message::NewMerge`
- `Message::UseDetectedOffset`

### 2. `main.rs` wiring

- `OffsetDetected(Ok(r))`: also set `state.last_detected = Some(r.clone())`
  alongside the existing `state.offset = Detected(r)` assignment.
- `Message::UseDetectedOffset`: if `state.last_detected` is `Some(r)`, set
  `state.offset = OffsetState::Detected(r.clone())` and
  `state.manual_offset_input = format!("{:.3}", r.offset)`.
- `Message::NewMerge`: no-op if `state.merge_receiver.is_some()` (a merge
  is actively running). Otherwise, reset to a fresh `AppState::default()`
  while preserving the environment-derived fields (`is_dark`, `accent_hex`,
  `missing_binaries`) that shouldn't reset with the session.

### 3. Offset panel (`offset_panel.rs`)

- `status_banner` restructured to key off `state.last_detected` first:
  - `Some(r)`: render the existing rich banner (icon/headline/detail/pill)
    driven by `r`'s consistency/confidence — exactly as today's `Detected`
    arm already does, just now sourced from `last_detected` instead of
    `state.offset` directly, so it persists through a manual override.
  - `None` and `state.offset` is `ManualOverride(v)`: a simpler message —
    "Manual offset entered" / "No detection has been run to verify this
    value — merge with caution or run Detect offset first." — no
    consistency pill (there's nothing to verify against yet).
  - `None` and `state.offset` is `NotDetected`: the mockup's full box —
    info-circle icon, "Offset not detected yet" headline, "Run detection to
    measure how far File B is shifted, or type a known offset below."
    detail, `chip_bg` background (neutral, not success/danger/warn).
  - `Detecting`: keep existing simple "Detecting offset…" text (mockup has
    no distinct rich box for this transient state either).
- Offset-input row: add a "Manual override" pill (bordered `accent`,
  `accent_soft` background, `accent_fg` text, pencil icon) plus a plain
  "Use detected" text-button (dim, hover to `fg`, counter-clockwise-arrow
  icon, emits `Message::UseDetectedOffset`) in place of the measured-text
  when `matches!(state.offset, OffsetState::ManualOverride(_))`. "Use
  detected" only renders when `state.last_detected.is_some()` — nothing to
  fall back to otherwise.
- Offset `text_input`'s border color becomes `palette.accent` while
  `matches!(state.offset, OffsetState::ManualOverride(_))`, `palette.border`
  otherwise — requires verifying `text_input::Style`'s actual field shape
  against the installed `iced_widget` crate (this widget currently has no
  `.style()` override anywhere in the codebase, so this is new API surface
  for this project).

### 4. New Merge button (`output_log.rs`)

- Outlined pill button (`border: 1px solid palette.border`, transparent
  background, hover to `palette.btn_bg`, refresh icon), placed next to the
  existing Merge button in the footer's button group.
- Press-gated the same way file-dialog buttons already are in this
  codebase: `on_press_maybe`, `None` while `state.merge_receiver.is_some()`.

### 5. Empty-state placeholders

- `file_pickers.rs`: when `file` is `None`, replace the current plain "No
  file selected" text row with a dashed-border box (`background:
  palette.view`, `border: 1px dashed palette.border`, `radius: 8`, centered,
  `palette.faint` text) reading "No file selected — click Browse to load" —
  nested inside the card's existing solid-border outer container, which
  stays unconditional.
- `track_table.rs`: the `None` branch currently renders bare, unstyled text
  with no card background/border at all, unlike the `Some` branch. Give it
  the same `card`/`border` container styling as the loaded case, with
  centered, padded (`16`) `palette.faint` text reading "No file loaded" —
  matching the mockup, where the card wrapper is unconditional and only the
  inner content (placeholder vs. track rows) varies.

### 6. New icons (`icons.rs` + `assets/icons/`)

Four new icon functions, following the established `icon(include_bytes!(...),
color)` pattern:
- `info` — info-circle (`NotDetected` banner)
- `edit` — pencil (Manual override pill)
- `undo` — counter-clockwise arrow (Use detected button)
- `refresh` — clockwise arrow (New merge button)

Exact SVG path data copied verbatim from the mockup's inline SVGs (see the
implementation plan for the literal markup).

## Testing strategy

- `last_detected` population: extend the existing `OffsetDetected` handling
  path — this lives in `main.rs`'s `update()`, not a pure function, but
  `apply_probe_result`-style direct unit testing already established a
  precedent for testing `main.rs` logic directly where it's a plain
  function; if `UseDetectedOffset`'s logic is extracted similarly, unit test
  it the same way. Otherwise, this is integration-level `update()` logic
  consistent with how `DetectOffset`'s existing handler isn't itself unit
  tested (only `AppState`'s pure predicates are).
- `NewMerge`'s reset behavior and its guard against resetting while a merge
  is running: unit-testable directly since it only touches `AppState`
  fields via a plain function/match arm, following the same testing
  approach `apply_probe_result`'s reset-on-probe tests already established.
- Banner-selection logic (which of the four banner states renders) is pure
  `iced` view code with no new testable logic beyond what's already covered
  by existing `OffsetResult`/`Consistency` tests in `mediamerger-core` —
  consistent with this project's established approach to view code
  (build-checked and manually verified, not unit tested).

## Manual verification (requires a real GNOME desktop; not available in this sandbox)

1. Detect an offset, then type a different manual value: confirm the top
   banner keeps showing the detected result's aligned/inconsistent status,
   while the measured-text line is replaced by the "Manual override" pill
   and "Use detected" button.
2. Click "Use detected": confirm the offset input reverts to the detected
   value and the manual-override pill disappears.
3. Type a manual offset with no prior detection run: confirm a simpler
   "no detection run" message appears instead of a consistency-pill banner,
   and no "Use detected" button appears (nothing to fall back to).
4. Click "New merge": confirm files, tracks, offset, output path, and
   extras all reset to their defaults, and the File A/B cards + track
   tables show the new empty-state placeholders.
5. Start a merge, then attempt "New merge" mid-merge: confirm the button is
   disabled/no-op until the merge finishes or fails.
6. Confirm the "Offset not detected yet" box (icon/headline/detail) renders
   correctly on a freshly-loaded file pair before any detection has run.

# MediaMerger Redesign Fidelity Fixes — Design

## Purpose

Close the gap between the shipped visual redesign and the Claude Design
mockup it was supposed to match. A systematic section-by-section comparison
(prompted by the user updating the design bundle to add file duration, and
noticing several other divergences while looking at it again) found that the
implementation deviated from the mockup in several ways beyond what the
original redesign plan accounted for. This is a correction pass, not a new
design — every fix below brings the app back in line with a mockup that
hasn't otherwise changed except for the duration addition.

## Background

Diffing the original mockup handoff against the updated
`MediaMerger Design Help-v1.0.1-handoff.zip` shows the only actual change to
the design file itself is the addition of a duration chip
(`1:24:32`/`1:24:35`) to each file's metadata row. Everything else the user
flagged — section order, missing titles, the offset panel's layout — was
already present in the *original* mockup and simply didn't make it into the
implementation the first time around.

## Non-goals (carried over, unchanged from the original redesign)

- No custom window chrome — native OS title bar/decorations stay.
- No in-app accent-color picker — accent stays auto-detected from the GNOME
  system setting.
- No estimated/approximated bitrate.

## Confirmed gaps and fixes

**1. Missing section headers.** Every section in the mockup has a numbered
(or `+`) circular badge, a title, and a one-line subtitle
(`accentSoft` badge background, `accentFg` badge text). None of this exists
in the current implementation. Fix: a shared `section_header(badge, title,
subtitle, palette) -> Element` helper (new, in `ui/mod.rs` or a new
`ui/section_header.rs`), called once per section with that section's exact
mockup copy:
- `1` / "Source files" / "Two encodes of the same movie to combine."
- `2` / "Sync offset" / "Aligns File B's timing to File A by
  cross-correlating their audio."
- `3` / "Tracks to include" / "Pick which tracks go into the merged file.
  Set default and forced flags per track."
- `+` / "Extras" / "Optional metadata to carry over."

**2. Wrong section order.** Current: File Pickers → Track Table → Offset
Panel → Extras → Output. Mockup: File Pickers → **Offset Panel** → **Track
Table** → Extras → Output. Fix: reorder `ui/mod.rs`'s section list.

**3. Missing duration.** `MediaFile` has no duration field; the mockup's new
chip needs one. Fix: add `pub duration_secs: Option<f64>` to `MediaFile`,
parsed from mkvmerge's own `-J` output (`container.properties.duration`, a
nanosecond value already present in data the app already fetches) rather
than a second `ffprobe` subprocess call. Rendered as `H:MM:SS` in a new chip
in `file_pickers.rs`, positioned per the mockup (after resolution, before
track count).

**4. Offset panel content and layout mismatches**, four distinct issues:
- The banner's detail line currently shows the raw `early {X}s · late {Y}s ·
  confidence {Z}` dump. The mockup uses friendly copy instead: "File B's
  audio starts **{offset}s** after File A. Its tracks will be delayed to
  match." (aligned case) / "Early and late probes differ by **{delta}s**.
  Enter a known offset or re-run detection before merging." (inconsistent
  case). The technical measurement numbers move to point 3 below instead of
  living in the banner.
- The banner is missing a pill-shaped "Consistent"/"Inconsistent" badge on
  its right edge (bordered in the status color, matching the mockup exactly).
- A new "Measured {early}s early · {late}s late · confidence {conf}
  ({high|low})" text belongs to the right of the Offset-input/Detect-button
  row (pushed there with a flex-equivalent spacer) — this is the layout that
  "saves space" versus today's stacked arrangement, and is genuinely
  different content from the banner's friendly copy above.
- The waveform needs real dashed vertical guide lines (one at the
  zero-position in `palette.dim`, one at the detected-offset position in
  `palette.accent`, both spanning the full height of both bar rows) — the
  current implementation only has the two bar rows with no guide-line
  overlay. This requires `iced::widget::Canvas` for precise custom drawing;
  there's no ready-made "dashed line overlay" widget in `iced` for arbitrary
  positions within a bar layout.

**5. Footer is not visually distinct.** In the mockup the footer sits in its
own bar: `headerbar`-colored background, a top border in `separator`, an
uppercase "OUTPUT FILE" label, a two-column layout (path+Browse on the left;
ready-status text stacked above a large pill-shaped, icon'd Merge button on
the right). The current footer is plain rows continuing in the body
background with a small default-styled Merge button and no section label.

This surfaces a mistake from the original redesign: `Palette.headerbar` was
removed on the reasoning that it only mattered for the custom window-chrome
titlebar this redesign explicitly excludes — but the mockup's *footer* also
uses `c.headerbar` as its background. That reasoning was incomplete;
`headerbar` needs to be re-added to `Palette` for the footer's use, not the
titlebar's.

Fix: re-add `headerbar: Color` to `Palette` (with its original mockup
values); restructure `output_log.rs`'s outer wrapper to use it as a
background with a top border in `separator`; add the uppercase label; adopt
the mockup's two-column, right-stacked layout; make the Merge button a
large pill (`border-radius` ~999px equivalent, generous padding) with an
icon. This requires one new icon asset (the "layers" merge glyph used twice
in the mockup — once on the titlebar's app icon, which this redesign
doesn't build, and once on the Merge button, which it does) — `icons.rs`
gains an 8th icon function.

## Testing strategy

- `MediaFile.duration_secs` parsing: unit test extending the existing
  `parse_mkvmerge_json` fixture with a `container.properties.duration`
  value, asserting the parsed seconds value, plus a fixture without it
  asserting `None`.
- Duration formatting (`H:MM:SS`): a small pure formatting function, unit
  tested with a few representative durations (under an hour, over ten
  hours, exact hour boundaries).
- Confidence-quality label (`(high)`/`(low)`) threshold: a pure function,
  unit tested at and around the threshold boundary.
- Everything else (section header layout, reordering, banner copy, pill
  badge, waveform canvas drawing, footer restructuring) is view code with no
  new testable logic beyond what's listed above — consistent with the rest
  of this project's approach to `iced` view code, verified by build-checking
  plus manual verification on a real desktop, not unit tests.

## Manual verification (in addition to the prior redesign's checklist)

1. Section order and headers match the mockup exactly, including copy.
2. Duration chip shows a correctly-formatted `H:MM:SS` value and disappears
   gracefully (no chip, not a blank one) when mkvmerge doesn't report it.
3. Offset panel: banner shows friendly copy + pill badge; the technical
   measured-text line appears to the right of the offset controls, not in
   the banner; waveform's dashed guide lines render at the correct
   proportional positions (zero and offset).
4. Footer renders as a visually distinct bar with the correct background,
   border, uppercase label, two-column layout, and pill-shaped Merge button
   with its icon.

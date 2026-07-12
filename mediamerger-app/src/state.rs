use mediamerger_core::error::MergerError;
use mediamerger_core::mux::{ChapterSource, MergePlan, TrackSelection};
use mediamerger_core::offset::{OffsetResult, WaveformEnvelope};
use mediamerger_core::probe::{MediaFile, Track};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum OffsetState {
    NotDetected,
    Detecting,
    Detected(OffsetResult),
    ManualOverride(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChaptersChoice {
    FileA,
    FileB,
    None,
}

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

pub fn format_duration(secs: f64) -> String {
    let total_secs = secs.round() as u64;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    format!("{hours}:{minutes:02}:{seconds:02}")
}

/// Mirrors the `< 3.0` threshold `offset_panel.rs` already uses to flag a
/// "consistent but low confidence" result - keep both in sync if this ever
/// changes; it must not become a second, different threshold.
pub fn confidence_quality_label(confidence: f32) -> &'static str {
    if confidence >= 3.0 { "high" } else { "low" }
}

/// File A's own filename and directory, unchanged, with a " — Merged.mkv"
/// postfix - not an attempt at title/year cleanup of a messy release-scene
/// filename, just the literal stem plus a postfix, matching the mockup's
/// own naming convention.
pub fn default_output_path(file_a_path: &std::path::Path) -> Option<PathBuf> {
    let stem = file_a_path.file_stem()?.to_string_lossy().into_owned();
    let parent = file_a_path.parent()?;
    Some(parent.join(format!("{stem} — Merged.mkv")))
}

#[derive(Debug, Clone)]
pub enum MuxUiEvent {
    Progress(f32),
    Log(String),
    Done(Result<(), String>),
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub file_a: Option<MediaFile>,
    pub file_b: Option<MediaFile>,
    pub tracks_a_ui: Vec<TrackUiState>,
    pub tracks_b_ui: Vec<TrackUiState>,
    pub framerate_error: Option<MergerError>,
    pub framerate_override: bool,
    pub probe_error: Option<MergerError>,
    pub is_dark: bool,
    pub accent_hex: String,
    pub offset: OffsetState,
    pub last_detected: Option<OffsetResult>,
    pub waveform: Option<WaveformEnvelope>,
    pub manual_offset_input: String,
    pub detect_offset_error: Option<String>,
    pub chapters_choice: ChaptersChoice,
    pub attachments_a: bool,
    pub attachments_b: bool,
    pub tags_a: bool,
    pub tags_b: bool,
    pub output_path: Option<PathBuf>,
    pub merge_progress: Option<f32>,
    pub log: Vec<String>,
    pub merge_error: Option<String>,
    pub missing_binaries: Vec<&'static str>,
    pub merge_receiver: Option<std::sync::Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<MuxUiEvent>>>>,
    pub picking_file_a: bool,
    pub picking_file_b: bool,
    pub log_expanded: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            file_a: None,
            file_b: None,
            tracks_a_ui: Vec::new(),
            tracks_b_ui: Vec::new(),
            framerate_error: None,
            framerate_override: false,
            probe_error: None,
            is_dark: crate::detect_is_dark(),
            accent_hex: crate::detect_accent_color(),
            offset: OffsetState::NotDetected,
            last_detected: None,
            waveform: None,
            manual_offset_input: String::new(),
            detect_offset_error: None,
            chapters_choice: ChaptersChoice::FileA,
            attachments_a: true,
            attachments_b: false,
            tags_a: false,
            tags_b: false,
            output_path: None,
            merge_progress: None,
            log: Vec::new(),
            merge_error: None,
            missing_binaries: Vec::new(),
            merge_receiver: None,
            picking_file_a: false,
            picking_file_b: false,
            log_expanded: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    PickFileA,
    PickFileB,
    FileAProbed(Result<MediaFile, MergerError>),
    FileBProbed(Result<MediaFile, MergerError>),
    FramerateOverrideToggled(bool),
    RefreshSystemTheme,
    SystemThemeDetected(bool, String),
    ToggleTrackA(usize),
    ToggleTrackB(usize),
    SetDefaultFlagA(usize, bool),
    SetDefaultFlagB(usize, bool),
    SetForcedFlagA(usize, bool),
    SetForcedFlagB(usize, bool),
    DetectOffset,
    OffsetDetected(Result<OffsetResult, MergerError>),
    WaveformExtracted(Result<WaveformEnvelope, MergerError>),
    ManualOffsetChanged(String),
    UseDetectedOffset,
    ChaptersChoiceChanged(ChaptersChoice),
    ToggleAttachmentsA(bool),
    ToggleAttachmentsB(bool),
    ToggleTagsA(bool),
    ToggleTagsB(bool),
    PickOutput,
    OutputPicked(Option<PathBuf>),
    StartMerge,
    MergeEventReceived(Option<MuxUiEvent>),
    BinariesChecked(Vec<&'static str>),
    ToggleLogExpanded,
    NewMerge,
}

#[derive(Debug, Clone, Default)]
pub struct TrackUiState {
    pub selected: bool,
    pub default_flag: bool,
    pub forced_flag: bool,
}

impl AppState {
    pub fn sync_track_ui_len(tracks: &[mediamerger_core::probe::Track], ui: &mut Vec<TrackUiState>) {
        ui.resize_with(tracks.len(), TrackUiState::default);
    }

    pub fn resolved_offset_secs(&self) -> Option<f64> {
        match &self.offset {
            OffsetState::Detected(r) => Some(r.offset),
            OffsetState::ManualOverride(v) => Some(*v),
            OffsetState::NotDetected | OffsetState::Detecting => None,
        }
    }

    /// Returns Some(reason) if the merge (or a fresh offset detection) must
    /// be blocked per the app's binding guards, None if clear to proceed.
    /// A user-entered ManualOverride always counts as the "manual
    /// confirmation" an Inconsistent result demands, regardless of the
    /// Consistency verdict that produced the pre-filled value.
    pub fn blocking_reason(&self) -> Option<String> {
        if self.framerate_error.is_some() && !self.framerate_override {
            return Some("video framerates do not match".to_string());
        }
        if let OffsetState::Detected(r) = &self.offset {
            if r.consistency == mediamerger_core::offset::Consistency::Inconsistent {
                return Some("offset measurements are inconsistent - resolve manually before merging".to_string());
            }
        }
        None
    }

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
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use mediamerger_core::offset::Consistency;
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
            width: None,
            height: None,
            sampling_rate: None,
            bits_per_sample: None,
            bitrate_bps: None,
            is_hdr10: false,
            is_dolby_vision: false,
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
            early_window_start: 0.0,
            window_duration: 180.0,
        });
        assert_eq!(state.resolved_offset_secs(), Some(2.35));
    }

    fn media_file(path: &str, tracks: Vec<Track>) -> mediamerger_core::probe::MediaFile {
        mediamerger_core::probe::MediaFile {
            path: PathBuf::from(path),
            container: "Matroska".to_string(),
            tracks,
            file_size_bytes: 0,
            duration_secs: None,
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

    #[test]
    fn blocking_reason_none_on_default_state() {
        let state = AppState::default();
        assert_eq!(state.blocking_reason(), None);
    }

    #[test]
    fn blocking_reason_some_when_framerate_error_set() {
        let mut state = AppState::default();
        state.framerate_error = Some(MergerError::Probe("framerate mismatch".to_string()));
        assert!(state.blocking_reason().is_some());
    }

    #[test]
    fn blocking_reason_none_when_framerate_error_overridden() {
        let mut state = AppState::default();
        state.framerate_error = Some(MergerError::Probe("framerate mismatch".to_string()));
        state.framerate_override = true;
        assert_eq!(state.blocking_reason(), None);
    }

    #[test]
    fn blocking_reason_some_when_offset_detected_inconsistent() {
        let mut state = AppState::default();
        state.offset = OffsetState::Detected(OffsetResult {
            early_offset: 2.0,
            late_offset: 3.0,
            consistency: Consistency::Inconsistent,
            confidence: 1.0,
            offset: 2.5,
            early_window_start: 0.0,
            window_duration: 180.0,
        });
        assert!(state.blocking_reason().is_some());
    }

    #[test]
    fn blocking_reason_none_when_manual_override_even_if_derived_from_inconsistent() {
        let mut state = AppState::default();
        // Simulates a user accepting/overriding an inconsistent detection's
        // pre-filled value: once it's a ManualOverride, the predicate no
        // longer inspects the Consistency verdict that produced it.
        state.offset = OffsetState::ManualOverride(2.5);
        assert_eq!(state.blocking_reason(), None);
    }

    #[test]
    fn parse_accent_name_maps_known_gnome_names() {
        assert_eq!(parse_accent_name("'blue'\n"), Some("#3584e4"));
        assert_eq!(parse_accent_name("purple"), Some("#9141ac"));
    }

    #[test]
    fn parse_accent_name_returns_none_for_unrecognized_value() {
        assert_eq!(parse_accent_name("'teal'\n"), None);
    }

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

    #[test]
    fn format_duration_formats_hours_minutes_seconds() {
        assert_eq!(format_duration(5072.0), "1:24:32");
    }

    #[test]
    fn format_duration_pads_minutes_and_seconds() {
        assert_eq!(format_duration(65.0), "0:01:05");
    }

    #[test]
    fn format_duration_handles_over_ten_hours() {
        assert_eq!(format_duration(37800.0), "10:30:00");
    }

    #[test]
    fn confidence_quality_label_matches_existing_low_confidence_threshold() {
        assert_eq!(confidence_quality_label(8.4), "high");
        assert_eq!(confidence_quality_label(3.0), "high");
        assert_eq!(confidence_quality_label(2.99), "low");
        assert_eq!(confidence_quality_label(2.1), "low");
    }

    #[test]
    fn default_output_path_appends_postfix_to_file_a_stem_in_place() {
        let path = default_output_path(std::path::Path::new("/home/matt/Videos/Movie.2024.mkv")).expect("should compute a default");
        assert_eq!(path, PathBuf::from("/home/matt/Videos/Movie.2024 — Merged.mkv"));
    }

    #[test]
    fn default_output_path_does_not_attempt_title_cleanup() {
        // Literal stem, dots and all - no scene-release filename parsing.
        let path = default_output_path(std::path::Path::new("/x/Evil.Dead.II.1987.UHD.BluRay.x265-GROUP.mkv")).expect("should compute a default");
        assert_eq!(path, PathBuf::from("/x/Evil.Dead.II.1987.UHD.BluRay.x265-GROUP — Merged.mkv"));
    }

    #[test]
    fn default_output_path_none_for_root_path() {
        // "/" has no filename component at all, so file_stem() is None -
        // exercises the ? early-return rather than a real file dialog path.
        assert!(default_output_path(std::path::Path::new("/")).is_none());
    }
}

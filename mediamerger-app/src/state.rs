use mediamerger_core::error::MergerError;
use mediamerger_core::mux::{ChapterSource, MergePlan, TrackSelection};
use mediamerger_core::offset::OffsetResult;
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

#[derive(Debug, Clone)]
pub struct AppState {
    pub file_a: Option<MediaFile>,
    pub file_b: Option<MediaFile>,
    pub tracks_a_ui: Vec<TrackUiState>,
    pub tracks_b_ui: Vec<TrackUiState>,
    pub framerate_error: Option<MergerError>,
    pub is_dark: bool,
    pub offset: OffsetState,
    pub manual_offset_input: String,
    pub detect_offset_error: Option<String>,
    pub chapters_choice: ChaptersChoice,
    pub attachments_a: bool,
    pub attachments_b: bool,
    pub tags_a: bool,
    pub tags_b: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            file_a: None,
            file_b: None,
            tracks_a_ui: Vec::new(),
            tracks_b_ui: Vec::new(),
            framerate_error: None,
            is_dark: crate::detect_is_dark(),
            offset: OffsetState::NotDetected,
            manual_offset_input: String::new(),
            detect_offset_error: None,
            chapters_choice: ChaptersChoice::FileA,
            attachments_a: true,
            attachments_b: false,
            tags_a: false,
            tags_b: false,
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
    ToggleTrackA(usize),
    ToggleTrackB(usize),
    SetDefaultFlagA(usize, bool),
    SetDefaultFlagB(usize, bool),
    SetForcedFlagA(usize, bool),
    SetForcedFlagB(usize, bool),
    DetectOffset,
    OffsetDetected(Result<OffsetResult, MergerError>),
    ManualOffsetChanged(String),
    ChaptersChoiceChanged(ChaptersChoice),
    ToggleAttachmentsA(bool),
    ToggleAttachmentsB(bool),
    ToggleTagsA(bool),
    ToggleTagsB(bool),
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
        });
        assert_eq!(state.resolved_offset_secs(), Some(2.35));
    }

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
}

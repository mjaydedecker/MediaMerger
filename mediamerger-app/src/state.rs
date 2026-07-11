use mediamerger_core::error::MergerError;
use mediamerger_core::offset::OffsetResult;
use mediamerger_core::probe::MediaFile;

#[derive(Debug, Clone)]
pub enum OffsetState {
    NotDetected,
    Detecting,
    Detected(OffsetResult),
    ManualOverride(f64),
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
}

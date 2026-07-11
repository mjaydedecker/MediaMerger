use mediamerger_core::error::MergerError;
use mediamerger_core::probe::MediaFile;

#[derive(Debug, Clone)]
pub struct AppState {
    pub file_a: Option<MediaFile>,
    pub file_b: Option<MediaFile>,
    pub framerate_error: Option<MergerError>,
    pub is_dark: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            file_a: None,
            file_b: None,
            framerate_error: None,
            is_dark: crate::detect_is_dark(),
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
}

use iced::{application, time, window, Element, Subscription, Task, Theme};
use state::{AppState, Message};
use std::time::Duration;

mod state;
mod ui;

fn main() -> iced::Result {
    application(|| (AppState::default(), Task::none()), update, view)
        .title("MediaMerger")
        .window(window::Settings {
            platform_specific: window::settings::PlatformSpecific {
                application_id: "mediamerger".to_string(),
                ..Default::default()
            },
            ..Default::default()
        })
        .theme(theme)
        .subscription(subscription)
        .run()
}

fn view(state: &AppState) -> Element<Message> {
    ui::view(state)
}

fn theme(state: &AppState) -> Theme {
    if state.is_dark { Theme::Dark } else { Theme::Light }
}

// dark-light falls back to a theme *name* lookup on some GNOME versions that
// doesn't reflect the color-scheme GSettings key modern Ubuntu uses. Read the
// key directly; fall back to dark-light for non-GNOME desktops.
fn detect_is_dark() -> bool {
    std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.contains("prefer-dark"))
        .unwrap_or_else(|| dark_light::detect() == dark_light::Mode::Dark)
}

fn subscription(_state: &AppState) -> Subscription<Message> {
    time::every(Duration::from_secs(10)).map(|_| Message::RefreshSystemTheme)
}

fn update(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        Message::PickFileA => Task::perform(pick_and_probe(), Message::FileAProbed),
        Message::PickFileB => Task::perform(pick_and_probe(), Message::FileBProbed),

        Message::FileAProbed(result) => {
            apply_probe_result(state, result, true);
            Task::none()
        }
        Message::FileBProbed(result) => {
            apply_probe_result(state, result, false);
            Task::none()
        }

        Message::RefreshSystemTheme => Task::perform(
            async { tokio::task::spawn_blocking(detect_is_dark).await.unwrap_or(false) },
            Message::SystemThemeDetected,
        ),
        Message::SystemThemeDetected(is_dark) => {
            if state.is_dark != is_dark {
                state.is_dark = is_dark;
            }
            Task::none()
        }

        Message::ToggleTrackA(idx) => {
            if let Some(row) = state.tracks_a_ui.get_mut(idx) {
                row.selected = !row.selected;
            }
            Task::none()
        }
        Message::ToggleTrackB(idx) => {
            if let Some(row) = state.tracks_b_ui.get_mut(idx) {
                row.selected = !row.selected;
            }
            Task::none()
        }
        Message::SetDefaultFlagA(idx, value) => {
            if let Some(row) = state.tracks_a_ui.get_mut(idx) {
                row.default_flag = value;
            }
            Task::none()
        }
        Message::SetDefaultFlagB(idx, value) => {
            if let Some(row) = state.tracks_b_ui.get_mut(idx) {
                row.default_flag = value;
            }
            Task::none()
        }
        Message::SetForcedFlagA(idx, value) => {
            if let Some(row) = state.tracks_a_ui.get_mut(idx) {
                row.forced_flag = value;
            }
            Task::none()
        }
        Message::SetForcedFlagB(idx, value) => {
            if let Some(row) = state.tracks_b_ui.get_mut(idx) {
                row.forced_flag = value;
            }
            Task::none()
        }
    }
}

async fn pick_and_probe() -> Result<mediamerger_core::probe::MediaFile, mediamerger_core::error::MergerError> {
    let handle = rfd::AsyncFileDialog::new()
        .add_filter("Video files", &["mkv", "mp4", "avi", "mov", "m4v", "webm"])
        .pick_file()
        .await;

    let path = match handle {
        Some(h) => h.path().to_path_buf(),
        None => return Err(mediamerger_core::error::MergerError::Probe("no file selected".to_string())),
    };

    tokio::task::spawn_blocking(move || mediamerger_core::probe::identify(&path))
        .await
        .unwrap_or_else(|e| Err(mediamerger_core::error::MergerError::Probe(e.to_string())))
}

fn apply_probe_result(
    state: &mut AppState,
    result: Result<mediamerger_core::probe::MediaFile, mediamerger_core::error::MergerError>,
    is_file_a: bool,
) {
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

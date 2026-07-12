use iced::{application, time, window, Element, Subscription, Task, Theme};
use state::{AppState, Message};
use std::time::Duration;

mod state;
mod theme;
mod ui;

fn main() -> iced::Result {
    application(|| (AppState::default(), Task::perform(check_binaries(), Message::BinariesChecked)), update, view)
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
        .default_font(iced::Font::with_name("Cantarell"))
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

// GNOME 47+ exposes a system accent color the same way it exposes
// light/dark; mirror the existing detect_is_dark defensive pattern (falls
// back to Adwaita blue on any failure, non-GNOME desktop, or older GNOME
// without this key).
fn detect_accent_color() -> String {
    std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "accent-color"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .as_deref()
        .and_then(state::parse_accent_name)
        .unwrap_or("#3584e4")
        .to_string()
}

fn subscription(_state: &AppState) -> Subscription<Message> {
    time::every(Duration::from_secs(10)).map(|_| Message::RefreshSystemTheme)
}

async fn check_binaries() -> Vec<&'static str> {
    tokio::task::spawn_blocking(|| {
        let mut missing = Vec::new();
        // ffmpeg/ffprobe's argument parser only strips a single leading
        // dash, so "--version" is misparsed as the unrecognized option
        // "-version" and fails even though the binary is installed and
        // working fine - confirmed against a real ffmpeg 8.0.1 install
        // ("Unrecognized option '-version'.", exit 8). mkvmerge uses a
        // standard GNU-style parser and needs the double dash.
        for (bin, version_flag) in [("ffmpeg", "-version"), ("ffprobe", "-version"), ("mkvmerge", "--version")] {
            let found = std::process::Command::new(bin)
                .arg(version_flag)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if !found {
                missing.push(bin);
            }
        }
        missing
    })
    .await
    .unwrap_or_else(|_| vec!["ffmpeg", "ffprobe", "mkvmerge"])
}

fn update(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        Message::PickFileA => {
            // Guard against opening a second native file dialog while one is
            // already showing - each click otherwise spawns its own
            // independent rfd::AsyncFileDialog, stacking dialogs and racing
            // whichever FileAProbed result lands last against the state the
            // user actually meant to keep.
            if state.picking_file_a {
                return Task::none();
            }
            state.picking_file_a = true;
            Task::perform(pick_and_probe(), Message::FileAProbed)
        }
        Message::PickFileB => {
            if state.picking_file_b {
                return Task::none();
            }
            state.picking_file_b = true;
            Task::perform(pick_and_probe(), Message::FileBProbed)
        }

        Message::FileAProbed(result) => {
            state.picking_file_a = false;
            apply_probe_result(state, result, true);
            Task::none()
        }
        Message::FileBProbed(result) => {
            state.picking_file_b = false;
            apply_probe_result(state, result, false);
            Task::none()
        }

        Message::RefreshSystemTheme => Task::perform(
            async {
                tokio::task::spawn_blocking(|| (detect_is_dark(), detect_accent_color())).await.unwrap_or((false, "#3584e4".to_string()))
            },
            |(is_dark, accent)| Message::SystemThemeDetected(is_dark, accent),
        ),
        Message::SystemThemeDetected(is_dark, accent) => {
            if state.is_dark != is_dark {
                state.is_dark = is_dark;
            }
            if state.accent_hex != accent {
                state.accent_hex = accent;
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

        Message::DetectOffset => {
            if state.framerate_error.is_some() {
                return Task::none();
            }
            state.offset = state::OffsetState::Detecting;
            state.detect_offset_error = None;
            let (Some(file_a), Some(file_b)) = (state.file_a.clone(), state.file_b.clone()) else {
                state.offset = state::OffsetState::NotDetected;
                state.detect_offset_error = Some("both files must be loaded before detecting offset".to_string());
                return Task::none();
            };
            let Some(track_a) = first_audio_track_id(&file_a) else {
                state.offset = state::OffsetState::NotDetected;
                state.detect_offset_error = Some("File A has no audio track".to_string());
                return Task::none();
            };
            let Some(track_b) = first_audio_track_id(&file_b) else {
                state.offset = state::OffsetState::NotDetected;
                state.detect_offset_error = Some("File B has no audio track".to_string());
                return Task::none();
            };
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        mediamerger_core::offset::detect_offset(&file_a.path, track_a, &file_b.path, track_b)
                    })
                    .await
                    .unwrap_or_else(|e| Err(mediamerger_core::error::MergerError::Probe(e.to_string())))
                },
                Message::OffsetDetected,
            )
        }
        Message::OffsetDetected(result) => match result {
            Ok(r) => {
                state.manual_offset_input = format!("{:.3}", r.offset);
                let (file_a, file_b) = (state.file_a.clone(), state.file_b.clone());
                let (Some(file_a), Some(file_b)) = (file_a, file_b) else {
                    state.offset = state::OffsetState::Detected(r);
                    return Task::none();
                };
                let Some(track_a) = first_audio_track_id(&file_a) else {
                    state.offset = state::OffsetState::Detected(r);
                    return Task::none();
                };
                let Some(track_b) = first_audio_track_id(&file_b) else {
                    state.offset = state::OffsetState::Detected(r);
                    return Task::none();
                };
                let (start, duration) = (r.early_window_start, r.window_duration);
                state.offset = state::OffsetState::Detected(r);
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            mediamerger_core::offset::extract_waveform(&file_a.path, track_a, &file_b.path, track_b, start, duration, 120)
                        })
                        .await
                        .unwrap_or_else(|e| Err(mediamerger_core::error::MergerError::Probe(e.to_string())))
                    },
                    Message::WaveformExtracted,
                )
            }
            Err(e) => {
                state.detect_offset_error = Some(e.to_string());
                state.offset = state::OffsetState::NotDetected;
                Task::none()
            }
        },
        Message::WaveformExtracted(result) => {
            // A waveform-fetch failure is never surfaced as a user-facing
            // error - the offset itself already succeeded and is fully
            // usable without this supplementary visualization.
            state.waveform = result.ok();
            Task::none()
        }
        Message::ManualOffsetChanged(text) => {
            if let Ok(value) = text.parse::<f64>() {
                state.offset = state::OffsetState::ManualOverride(value);
            }
            state.manual_offset_input = text;
            Task::none()
        }

        Message::ChaptersChoiceChanged(choice) => {
            state.chapters_choice = choice;
            Task::none()
        }
        Message::ToggleAttachmentsA(v) => {
            state.attachments_a = v;
            Task::none()
        }
        Message::ToggleAttachmentsB(v) => {
            state.attachments_b = v;
            Task::none()
        }
        Message::ToggleTagsA(v) => {
            state.tags_a = v;
            Task::none()
        }
        Message::ToggleTagsB(v) => {
            state.tags_b = v;
            Task::none()
        }

        Message::BinariesChecked(missing) => {
            state.missing_binaries = missing;
            Task::none()
        }
        Message::PickOutput => Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .add_filter("Matroska", &["mkv"])
                    .save_file()
                    .await
                    .map(|h| h.path().to_path_buf())
            },
            Message::OutputPicked,
        ),
        Message::OutputPicked(path) => {
            state.output_path = path;
            Task::none()
        }
        Message::StartMerge => {
            // Guard against a double-click (or any repeat dispatch) starting
            // a second mkvmerge process against the same output path while
            // the first is still running - two concurrent writers to one
            // file risks a corrupted/truncated output, and overwriting
            // merge_receiver would silently orphan the first run's worker
            // thread and drop its progress/log feed.
            if state.merge_receiver.is_some() {
                return Task::none();
            }
            if state.blocking_reason().is_some() {
                return Task::none();
            }
            let Some(output_path) = state.output_path.clone() else {
                return Task::none();
            };
            let Some(plan) = state.to_merge_plan(output_path) else {
                return Task::none();
            };
            state.log_expanded = true;
            state.merge_progress = Some(0.0);
            state.log.clear();
            state.merge_error = None;

            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<state::MuxUiEvent>();
            std::thread::spawn(move || {
                let args = mediamerger_core::mux::build_command(&plan);
                let tx_events = tx.clone();
                let result = mediamerger_core::mux::run_mux(&args, move |event| {
                    let mapped = match event {
                        mediamerger_core::mux::MuxEvent::Progress(p) => state::MuxUiEvent::Progress(p),
                        mediamerger_core::mux::MuxEvent::Log(l) => state::MuxUiEvent::Log(l),
                    };
                    let _ = tx_events.send(mapped);
                });
                let _ = tx.send(state::MuxUiEvent::Done(result.map_err(|e| e.to_string())));
            });

            state.merge_receiver = Some(std::sync::Arc::new(tokio::sync::Mutex::new(rx)));
            poll_merge_event(state)
        }
        Message::MergeEventReceived(event) => match event {
            Some(state::MuxUiEvent::Progress(p)) => {
                state.merge_progress = Some(p);
                poll_merge_event(state)
            }
            Some(state::MuxUiEvent::Log(line)) => {
                state.log.push(line);
                poll_merge_event(state)
            }
            Some(state::MuxUiEvent::Done(Ok(()))) => {
                state.merge_progress = Some(1.0);
                state.merge_receiver = None;
                Task::none()
            }
            Some(state::MuxUiEvent::Done(Err(e))) => {
                state.merge_error = Some(e);
                state.merge_progress = None;
                state.merge_receiver = None;
                Task::none()
            }
            None => {
                // The channel closed without a Done event - the worker
                // thread panicked or was dropped before it could send one.
                // Surface this rather than leaving a frozen progress bar
                // with no explanation.
                state.merge_error = Some("merge worker terminated unexpectedly".to_string());
                state.merge_progress = None;
                state.merge_receiver = None;
                Task::none()
            }
        },
        Message::FramerateOverrideToggled(v) => {
            state.framerate_override = v;
            Task::none()
        }
        Message::ToggleLogExpanded => {
            state.log_expanded = !state.log_expanded;
            Task::none()
        }
    }
}

fn poll_merge_event(state: &AppState) -> Task<Message> {
    let Some(receiver) = state.merge_receiver.clone() else {
        return Task::none();
    };
    Task::perform(
        async move {
            let mut rx = receiver.lock().await;
            rx.recv().await
        },
        Message::MergeEventReceived,
    )
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

fn first_audio_track_id(file: &mediamerger_core::probe::MediaFile) -> Option<u64> {
    file.tracks
        .iter()
        .find(|t| t.kind == mediamerger_core::probe::TrackKind::Audio)
        .map(|t| t.id)
}

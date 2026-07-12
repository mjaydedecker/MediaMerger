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
            icon: load_window_icon(),
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

fn load_window_icon() -> Option<window::Icon> {
    let bytes = include_bytes!("../assets/app-icon/mediamerger-128.png");
    let image = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    window::icon::from_rgba(image.into_raw(), width, height).ok()
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
            if state.framerate_error.is_some() && !state.framerate_override {
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
                state.last_detected = Some(r);
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
        Message::UseDetectedOffset => {
            if let Some(r) = state.last_detected {
                state.manual_offset_input = format!("{:.3}", r.offset);
                state.offset = state::OffsetState::Detected(r);
            }
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
        Message::NewMerge => {
            // Disabled while a merge is running (mirrors the merge_receiver
            // guard StartMerge already uses) - resetting file_a/file_b/log
            // etc. while the background worker thread is still sending
            // MergeEventReceived events would let those events repopulate
            // the just-reset UI mid-reset.
            if state.merge_receiver.is_some() {
                return Task::none();
            }
            let is_dark = state.is_dark;
            let accent_hex = state.accent_hex.clone();
            let missing_binaries = state.missing_binaries.clone();
            *state = AppState::default();
            state.is_dark = is_dark;
            state.accent_hex = accent_hex;
            state.missing_binaries = missing_binaries;
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
    state.framerate_override = false;
    match result {
        Ok(media_file) => {
            state.probe_error = None;
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
        // A failed probe attempt (e.g. a cancelled file dialog, or ffprobe
        // choking on an unsupported file) is a different problem from a
        // genuine framerate mismatch between two loaded files - it must not
        // land in framerate_error, which the UI unconditionally renders
        // with the "I know the audio speed matches" override checkbox.
        // file_a/file_b are untouched here, so any real framerate_error
        // already set from a prior successful pairing stays valid as-is.
        Err(e) => state.probe_error = Some(e),
    }
}

fn first_audio_track_id(file: &mediamerger_core::probe::MediaFile) -> Option<u64> {
    file.tracks
        .iter()
        .find(|t| t.kind == mediamerger_core::probe::TrackKind::Audio)
        .map(|t| t.id)
}

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

    #[test]
    fn apply_probe_result_sets_probe_error_not_framerate_error_on_failure() {
        let mut state = AppState::default();

        apply_probe_result(&mut state, Err(mediamerger_core::error::MergerError::Probe("no file selected".to_string())), true);

        assert!(state.probe_error.is_some(), "a failed probe must surface as probe_error");
        assert!(
            state.framerate_error.is_none(),
            "a generic probe failure (e.g. a cancelled file dialog) must not masquerade as a framerate mismatch"
        );
    }

    #[test]
    fn apply_probe_result_preserves_existing_framerate_error_on_later_probe_failure() {
        let mut state = AppState::default();
        state.framerate_error = Some(mediamerger_core::error::MergerError::FramerateMismatch { file_a_fps: 23.976, file_b_fps: 25.0 });

        apply_probe_result(&mut state, Err(mediamerger_core::error::MergerError::Probe("no file selected".to_string())), false);

        assert!(
            state.framerate_error.is_some(),
            "file_a/file_b are untouched on a probe failure, so an already-valid framerate mismatch must stay reported"
        );
    }

    #[test]
    fn apply_probe_result_clears_stale_probe_error_on_success() {
        let mut state = AppState::default();
        state.probe_error = Some(mediamerger_core::error::MergerError::Probe("no file selected".to_string()));

        apply_probe_result(&mut state, Ok(media_file("a.mkv")), true);

        assert!(state.probe_error.is_none(), "a successful probe must clear a stale probe_error from an earlier failed attempt");
    }

    #[test]
    fn use_detected_offset_restores_last_detected_after_manual_override() {
        let mut state = AppState::default();
        let detected = mediamerger_core::offset::OffsetResult {
            early_offset: 2.34,
            late_offset: 2.36,
            consistency: mediamerger_core::offset::Consistency::Consistent,
            confidence: 8.0,
            offset: 2.35,
            early_window_start: 0.0,
            window_duration: 180.0,
        };
        state.last_detected = Some(detected);
        state.offset = state::OffsetState::ManualOverride(9.999);
        state.manual_offset_input = "9.999".to_string();

        let _ = update(&mut state, Message::UseDetectedOffset);

        match state.offset {
            state::OffsetState::Detected(r) => assert_eq!(r.offset, 2.35),
            _ => panic!("expected offset to be restored to Detected"),
        }
        assert_eq!(state.manual_offset_input, "2.350");
    }

    #[test]
    fn use_detected_offset_is_noop_when_nothing_was_ever_detected() {
        let mut state = AppState::default();
        state.offset = state::OffsetState::ManualOverride(1.0);
        state.manual_offset_input = "1.000".to_string();

        let _ = update(&mut state, Message::UseDetectedOffset);

        match state.offset {
            state::OffsetState::ManualOverride(v) => assert_eq!(v, 1.0),
            _ => panic!("expected offset to remain ManualOverride when last_detected is None"),
        }
    }

    #[test]
    fn new_merge_resets_state_but_preserves_environment_fields() {
        let mut state = AppState::default();
        state.is_dark = true;
        state.accent_hex = "#123456".to_string();
        state.missing_binaries = vec!["ffmpeg"];
        state.file_a = Some(media_file("a.mkv"));
        state.output_path = Some(PathBuf::from("out.mkv"));
        state.attachments_a = false;
        state.offset = state::OffsetState::ManualOverride(5.0);

        let _ = update(&mut state, Message::NewMerge);

        assert!(state.file_a.is_none());
        assert!(state.output_path.is_none());
        assert!(state.attachments_a, "attachments_a should reset to its default (true)");
        assert!(matches!(state.offset, state::OffsetState::NotDetected));
        assert!(state.is_dark, "is_dark must survive the reset");
        assert_eq!(state.accent_hex, "#123456");
        assert_eq!(state.missing_binaries, vec!["ffmpeg"]);
    }

    #[test]
    fn new_merge_is_noop_while_a_merge_is_running() {
        let mut state = AppState::default();
        state.file_a = Some(media_file("a.mkv"));
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel::<state::MuxUiEvent>();
        state.merge_receiver = Some(std::sync::Arc::new(tokio::sync::Mutex::new(rx)));

        let _ = update(&mut state, Message::NewMerge);

        assert!(state.file_a.is_some(), "state must not reset while a merge is in flight");
    }
}

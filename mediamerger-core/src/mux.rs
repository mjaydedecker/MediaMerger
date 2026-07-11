use crate::error::MergerError;
use crate::probe::TrackKind;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq)]
pub enum MuxEvent {
    Progress(f32),
    Log(String),
}

#[derive(Debug, Clone)]
pub struct TrackSelection {
    pub track_id: u64,
    pub kind: TrackKind,
    pub set_default: bool,
    pub set_forced: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChapterSource {
    FileA,
    FileB,
    None,
}

#[derive(Debug, Clone)]
pub struct MergePlan {
    pub file_a: PathBuf,
    pub file_b: PathBuf,
    pub tracks_from_a: Vec<TrackSelection>,
    pub tracks_from_b: Vec<TrackSelection>,
    pub offset_secs: f64,
    pub chapters: ChapterSource,
    pub attachments_from_a: bool,
    pub attachments_from_b: bool,
    pub tags_from_a: bool,
    pub tags_from_b: bool,
    pub output_path: PathBuf,
}

fn parse_line(line: &str) -> MuxEvent {
    if let Some(rest) = line.strip_prefix("#GUI#progress ") {
        if let Ok(pct) = rest.trim().trim_end_matches('%').parse::<f32>() {
            return MuxEvent::Progress(pct / 100.0);
        }
    }
    MuxEvent::Log(line.to_string())
}

fn push_track_selection_args(args: &mut Vec<String>, selections: &[TrackSelection]) {
    for kind in [TrackKind::Video, TrackKind::Audio, TrackKind::Subtitle] {
        let ids: Vec<String> = selections
            .iter()
            .filter(|s| s.kind == kind)
            .map(|s| s.track_id.to_string())
            .collect();
        let (keep_flag, exclude_flag) = match kind {
            TrackKind::Video => ("--video-tracks", "--no-video"),
            TrackKind::Audio => ("--audio-tracks", "--no-audio"),
            TrackKind::Subtitle => ("--subtitle-tracks", "--no-subtitles"),
        };
        if ids.is_empty() {
            args.push(exclude_flag.to_string());
        } else {
            args.push(keep_flag.to_string());
            args.push(ids.join(","));
        }
    }
}

pub fn build_command(plan: &MergePlan) -> Vec<String> {
    let mut args = Vec::new();

    push_track_selection_args(&mut args, &plan.tracks_from_a);
    for sel in &plan.tracks_from_a {
        if sel.set_default {
            args.push("--default-track-flag".into());
            args.push(format!("{}:yes", sel.track_id));
        }
        if sel.set_forced {
            args.push("--forced-display-flag".into());
            args.push(format!("{}:yes", sel.track_id));
        }
    }
    if plan.chapters != ChapterSource::FileA {
        args.push("--no-chapters".into());
    }
    if !plan.attachments_from_a {
        args.push("--no-attachments".into());
    }
    if !plan.tags_from_a {
        args.push("--no-global-tags".into());
        args.push("--no-track-tags".into());
    }
    args.push(plan.file_a.to_string_lossy().into_owned());

    push_track_selection_args(&mut args, &plan.tracks_from_b);
    for sel in &plan.tracks_from_b {
        if sel.set_default {
            args.push("--default-track-flag".into());
            args.push(format!("{}:yes", sel.track_id));
        }
        if sel.set_forced {
            args.push("--forced-display-flag".into());
            args.push(format!("{}:yes", sel.track_id));
        }
        // File B's shared content occurs `offset_secs` later than File A's
        // (per cross_correlate's contract, Task 5). To align it, apply the
        // *negative* of that offset as this track's mkvmerge delay.
        let delay_ms = (-plan.offset_secs * 1000.0).round() as i64;
        args.push("--sync".into());
        args.push(format!("{}:{}", sel.track_id, delay_ms));
    }
    if plan.chapters != ChapterSource::FileB {
        args.push("--no-chapters".into());
    }
    if !plan.attachments_from_b {
        args.push("--no-attachments".into());
    }
    if !plan.tags_from_b {
        args.push("--no-global-tags".into());
        args.push("--no-track-tags".into());
    }
    args.push(plan.file_b.to_string_lossy().into_owned());

    args.push("-o".into());
    args.push(plan.output_path.to_string_lossy().into_owned());

    let mut order_parts = Vec::new();
    for sel in &plan.tracks_from_a {
        order_parts.push(format!("0:{}", sel.track_id));
    }
    for sel in &plan.tracks_from_b {
        order_parts.push(format!("1:{}", sel.track_id));
    }
    args.push("--track-order".into());
    args.push(order_parts.join(","));

    args
}

pub fn run_mux(args: &[String], mut on_event: impl FnMut(MuxEvent)) -> Result<(), MergerError> {
    let mut full_args = vec!["--gui-mode".to_string()];
    full_args.extend_from_slice(args);

    let mut child = Command::new("mkvmerge")
        .args(&full_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| MergerError::MkvmergeNotFound)?;

    // stderr must be drained concurrently with stdout, not after: mkvmerge can
    // write enough to stderr (warnings, etc.) to fill the OS pipe buffer
    // (~64KB on Linux) while we're still blocked reading stdout line-by-line,
    // which would deadlock both processes.
    let stderr = child.stderr.take().expect("stderr was piped at spawn");
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = BufReader::new(stderr).read_to_string(&mut buf);
        buf
    });

    let stdout = child.stdout.take().expect("stdout was piped at spawn");
    let reader = BufReader::new(stdout);
    for line in reader.lines() {
        let line = line.map_err(|e| MergerError::MuxFailed(e.to_string()))?;
        on_event(parse_line(&line));
    }

    let status = child.wait().map_err(|e| MergerError::MuxFailed(e.to_string()))?;
    let stderr_text = stderr_handle.join().unwrap_or_default();
    match status.code() {
        Some(0) | Some(1) => Ok(()),
        _ => Err(MergerError::MuxFailed(stderr_text)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_of(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn builds_command_for_simple_video_a_audio_b_case() {
        let plan = MergePlan {
            file_a: PathBuf::from("video_source.mkv"),
            file_b: PathBuf::from("audio_source.mkv"),
            tracks_from_a: vec![TrackSelection {
                track_id: 0,
                kind: TrackKind::Video,
                set_default: true,
                set_forced: false,
            }],
            tracks_from_b: vec![TrackSelection {
                track_id: 1,
                kind: TrackKind::Audio,
                set_default: true,
                set_forced: false,
            }],
            offset_secs: 2.348,
            chapters: ChapterSource::FileA,
            attachments_from_a: false,
            attachments_from_b: false,
            tags_from_a: false,
            tags_from_b: false,
            output_path: PathBuf::from("output.mkv"),
        };

        let args = build_command(&plan);

        assert_eq!(
            args,
            args_of(&[
                "--video-tracks", "0",
                "--no-audio",
                "--no-subtitles",
                "--default-track-flag", "0:yes",
                "--no-attachments",
                "--no-global-tags", "--no-track-tags",
                "video_source.mkv",
                "--no-video",
                "--audio-tracks", "1",
                "--no-subtitles",
                "--default-track-flag", "1:yes",
                "--sync", "1:-2348",
                "--no-chapters",
                "--no-attachments",
                "--no-global-tags", "--no-track-tags",
                "audio_source.mkv",
                "-o", "output.mkv",
                "--track-order", "0:0,1:1",
            ])
        );
    }

    #[test]
    fn no_chapters_for_both_and_attachments_kept_for_b_with_negative_offset() {
        let plan = MergePlan {
            file_a: PathBuf::from("a.mkv"),
            file_b: PathBuf::from("b.mkv"),
            tracks_from_a: vec![TrackSelection {
                track_id: 0,
                kind: TrackKind::Video,
                set_default: false,
                set_forced: false,
            }],
            tracks_from_b: vec![TrackSelection {
                track_id: 2,
                kind: TrackKind::Subtitle,
                set_default: false,
                set_forced: true,
            }],
            offset_secs: -0.5,
            chapters: ChapterSource::None,
            attachments_from_a: false,
            attachments_from_b: true,
            tags_from_a: false,
            tags_from_b: false,
            output_path: PathBuf::from("out.mkv"),
        };

        let args = build_command(&plan);

        assert_eq!(args.iter().filter(|a| a.as_str() == "--no-chapters").count(), 2);
        assert_eq!(args.iter().filter(|a| a.as_str() == "--no-attachments").count(), 1);
        assert_eq!(args.iter().filter(|a| a.as_str() == "--forced-display-flag").count(), 1);

        let sync_idx = args.iter().position(|a| a == "--sync").expect("--sync present");
        assert_eq!(args[sync_idx + 1], "2:500");
    }

    #[test]
    fn parses_gui_progress_line() {
        assert_eq!(parse_line("#GUI#progress 42%"), MuxEvent::Progress(0.42));
    }

    #[test]
    fn treats_other_lines_as_log() {
        assert_eq!(
            parse_line("Warning: some warning text"),
            MuxEvent::Log("Warning: some warning text".to_string())
        );
    }
}

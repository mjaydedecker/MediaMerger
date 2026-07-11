use mediamerger_core::{mux, offset, probe};
use std::path::PathBuf;
use std::process::Command;

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
}
fn mkvmerge_available() -> bool {
    Command::new("mkvmerge").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// Generates a short synthetic "movie": a video track with a solid color and
/// a sine-wave audio track, `duration` seconds long. `lead_in` seconds of
/// silence are prepended to the audio so File A and File B can simulate
/// differing intro lengths while sharing the same underlying content after
/// the lead-in.
fn generate_fixture(path: &PathBuf, duration_secs: u32, lead_in_secs: f64) {
    let audio_filter = format!(
        "sine=frequency=440:duration={duration_secs},adelay={}|{}",
        (lead_in_secs * 1000.0) as u64,
        (lead_in_secs * 1000.0) as u64
    );
    let status = Command::new("ffmpeg")
        .args(["-y", "-f", "lavfi", "-i"])
        .arg(format!("testsrc=duration={duration_secs}:size=320x240:rate=24"))
        .args(["-f", "lavfi", "-i"])
        .arg(audio_filter.replace("sine=", "sine="))
        .args(["-c:v", "libx264", "-c:a", "aac", "-shortest"])
        .arg(path)
        .status()
        .expect("failed to spawn ffmpeg");
    assert!(status.success(), "fixture generation failed for {path:?}");
}

#[test]
fn full_pipeline_recovers_known_offset_and_produces_synced_output() {
    if !ffmpeg_available() || !mkvmerge_available() {
        eprintln!("skipping: ffmpeg and mkvmerge must be installed to run this test");
        return;
    }

    let dir = std::env::temp_dir().join(format!("mediamerger-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file_a = dir.join("a.mkv");
    let file_b = dir.join("b.mkv");
    let output = dir.join("out.mkv");

    // File A: no lead-in. File B: 5-second longer intro before the same content.
    generate_fixture(&file_a, 60, 0.0);
    generate_fixture(&file_b, 65, 5.0);

    probe::check_framerate(&file_a, &file_b).expect("framerates should match (both 24fps)");

    let media_a = probe::identify(&file_a).unwrap();
    let media_b = probe::identify(&file_b).unwrap();
    let audio_a = media_a.tracks.iter().find(|t| t.kind == probe::TrackKind::Audio).unwrap().id;
    let audio_b = media_b.tracks.iter().find(|t| t.kind == probe::TrackKind::Audio).unwrap().id;
    let video_a = media_a.tracks.iter().find(|t| t.kind == probe::TrackKind::Video).unwrap().id;

    let result = offset::detect_offset(&file_a, audio_a, &file_b, audio_b).unwrap();
    assert!(
        (result.offset - 5.0).abs() < 0.5,
        "expected ~5s offset (File B's content lags File A's by its extra intro), got {}",
        result.offset
    );

    let plan = mux::MergePlan {
        file_a: file_a.clone(),
        file_b: file_b.clone(),
        tracks_from_a: vec![mux::TrackSelection {
            track_id: video_a,
            kind: probe::TrackKind::Video,
            set_default: true,
            set_forced: false,
        }],
        tracks_from_b: vec![mux::TrackSelection {
            track_id: audio_b,
            kind: probe::TrackKind::Audio,
            set_default: true,
            set_forced: false,
        }],
        offset_secs: result.offset,
        chapters: mux::ChapterSource::None,
        attachments_from_a: false,
        attachments_from_b: false,
        tags_from_a: false,
        tags_from_b: false,
        output_path: output.clone(),
    };

    let args = mux::build_command(&plan);
    mux::run_mux(&args, |_event| {}).expect("mux should succeed");

    let merged = probe::identify(&output).unwrap();
    assert_eq!(merged.tracks.len(), 2, "expected exactly one video and one audio track in the output");

    std::fs::remove_dir_all(&dir).ok();
}

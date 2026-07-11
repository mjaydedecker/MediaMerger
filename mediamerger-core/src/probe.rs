use crate::error::MergerError;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Video,
    Audio,
    Subtitle,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub id: u64,
    pub kind: TrackKind,
    pub codec: String,
    pub language: Option<String>,
    pub name: Option<String>,
    pub default_flag: bool,
    pub forced_flag: bool,
    pub fps: Option<f64>,
    pub channels: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct MediaFile {
    pub path: PathBuf,
    pub container: String,
    pub tracks: Vec<Track>,
}

#[derive(Deserialize)]
struct MkvmergeJson {
    container: MkvmergeContainer,
    tracks: Vec<MkvmergeTrack>,
}

#[derive(Deserialize)]
struct MkvmergeContainer {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
struct MkvmergeTrack {
    id: u64,
    #[serde(rename = "type")]
    kind: String,
    codec: String,
    properties: MkvmergeTrackProperties,
}

#[derive(Deserialize, Default)]
struct MkvmergeTrackProperties {
    #[serde(default)]
    default_track: bool,
    #[serde(default)]
    forced_track: bool,
    language: Option<String>,
    track_name: Option<String>,
    audio_channels: Option<u32>,
    default_duration: Option<u64>,
}

fn parse_mkvmerge_json(bytes: &[u8], path: &Path) -> Result<MediaFile, MergerError> {
    let parsed: MkvmergeJson =
        serde_json::from_slice(bytes).map_err(|e| MergerError::Probe(e.to_string()))?;

    let tracks = parsed
        .tracks
        .into_iter()
        .filter_map(|t| {
            let kind = match t.kind.as_str() {
                "video" => TrackKind::Video,
                "audio" => TrackKind::Audio,
                "subtitles" => TrackKind::Subtitle,
                _ => return None,
            };
            let fps = t
                .properties
                .default_duration
                .filter(|&ns| ns > 0)
                .map(|ns| 1_000_000_000.0 / ns as f64);
            Some(Track {
                id: t.id,
                kind,
                codec: t.codec,
                language: t.properties.language,
                name: t.properties.track_name,
                default_flag: t.properties.default_track,
                forced_flag: t.properties.forced_track,
                fps,
                channels: t.properties.audio_channels,
            })
        })
        .collect();

    Ok(MediaFile { path: path.to_path_buf(), container: parsed.container.kind, tracks })
}

pub fn identify(path: &Path) -> Result<MediaFile, MergerError> {
    let output = Command::new("mkvmerge")
        .arg("-J")
        .arg(path)
        .output()
        .map_err(|_| MergerError::MkvmergeNotFound)?;

    if !output.status.success() {
        return Err(MergerError::Probe(String::from_utf8_lossy(&output.stderr).to_string()));
    }

    parse_mkvmerge_json(&output.stdout, path)
}

fn parse_r_frame_rate(bytes: &[u8]) -> Result<f64, MergerError> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim();
    let (num, den) = text
        .split_once('/')
        .ok_or_else(|| MergerError::Probe(format!("unexpected r_frame_rate output: {text}")))?;
    let num: f64 = num
        .parse()
        .map_err(|_| MergerError::Probe(format!("bad numerator in r_frame_rate: {text}")))?;
    let den: f64 = den
        .parse()
        .map_err(|_| MergerError::Probe(format!("bad denominator in r_frame_rate: {text}")))?;
    if den == 0.0 {
        return Err(MergerError::Probe(format!("zero denominator in r_frame_rate: {text}")));
    }
    Ok(num / den)
}

fn fps_within_tolerance(a: f64, b: f64) -> bool {
    (a - b).abs() <= 0.05
}

fn parse_duration_output(bytes: &[u8]) -> Result<f64, MergerError> {
    let text = String::from_utf8_lossy(bytes);
    text.trim()
        .parse()
        .map_err(|_| MergerError::Probe(format!("unexpected duration output: {}", text.trim())))
}

fn ffprobe_video_fps(path: &Path) -> Result<f64, MergerError> {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=r_frame_rate", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .map_err(|_| MergerError::FfmpegNotFound)?;
    if !output.status.success() {
        return Err(MergerError::Probe(String::from_utf8_lossy(&output.stderr).to_string()));
    }
    parse_r_frame_rate(&output.stdout)
}

pub fn check_framerate(file_a: &Path, file_b: &Path) -> Result<(), MergerError> {
    let fps_a = ffprobe_video_fps(file_a)?;
    let fps_b = ffprobe_video_fps(file_b)?;
    if !fps_within_tolerance(fps_a, fps_b) {
        return Err(MergerError::FramerateMismatch { file_a_fps: fps_a, file_b_fps: fps_b });
    }
    Ok(())
}

pub fn duration_secs(path: &Path) -> Result<f64, MergerError> {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .map_err(|_| MergerError::FfmpegNotFound)?;
    if !output.status.success() {
        return Err(MergerError::Probe(String::from_utf8_lossy(&output.stderr).to_string()));
    }
    parse_duration_output(&output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_video_audio_subtitle_tracks() {
        let json = br#"{
            "container": {"type": "Matroska"},
            "tracks": [
                {"id":0,"type":"video","codec":"MPEG-4p10/AVC/h.264","properties":{"default_track":true,"forced_track":false,"default_duration":41708333}},
                {"id":1,"type":"audio","codec":"AC-3","properties":{"default_track":true,"forced_track":false,"language":"eng","audio_channels":6}},
                {"id":2,"type":"subtitles","codec":"SubRip/SRT","properties":{"default_track":false,"forced_track":false,"language":"fre","track_name":"Forced"}}
            ]
        }"#;

        let media = parse_mkvmerge_json(json, Path::new("test.mkv")).unwrap();

        assert_eq!(media.container, "Matroska");
        assert_eq!(media.tracks.len(), 3);

        assert_eq!(media.tracks[0].kind, TrackKind::Video);
        assert!((media.tracks[0].fps.unwrap() - 23.976).abs() < 0.01);

        assert_eq!(media.tracks[1].kind, TrackKind::Audio);
        assert_eq!(media.tracks[1].channels, Some(6));
        assert_eq!(media.tracks[1].language.as_deref(), Some("eng"));

        assert_eq!(media.tracks[2].kind, TrackKind::Subtitle);
        assert_eq!(media.tracks[2].language.as_deref(), Some("fre"));
        assert_eq!(media.tracks[2].name.as_deref(), Some("Forced"));
    }

    #[test]
    fn parses_ntsc_frame_rate_fraction() {
        let fps = parse_r_frame_rate(b"24000/1001\n").unwrap();
        assert!((fps - 23.976).abs() < 0.001, "got {fps}");
    }

    #[test]
    fn parses_integer_frame_rate_fraction() {
        let fps = parse_r_frame_rate(b"25/1\n").unwrap();
        assert!((fps - 25.0).abs() < 0.001, "got {fps}");
    }

    #[test]
    fn frame_rates_within_tolerance_match() {
        assert!(fps_within_tolerance(23.976, 23.98));
        assert!(!fps_within_tolerance(23.976, 25.0));
    }

    #[test]
    fn parses_duration_seconds() {
        let secs = parse_duration_output(b"7261.234000\n").unwrap();
        assert!((secs - 7261.234).abs() < 0.001, "got {secs}");
    }
}

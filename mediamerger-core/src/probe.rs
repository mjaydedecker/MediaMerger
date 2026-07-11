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
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub sampling_rate: Option<u32>,
    pub bits_per_sample: Option<u32>,
    /// Only ever a value the source container reports directly (mkvmerge's
    /// `tag_bps` property) - never estimated from file size / duration.
    pub bitrate_bps: Option<u64>,
    /// Best-effort from color/block-addition properties; false when not
    /// confidently detectable, never a guess.
    pub is_hdr10: bool,
    pub is_dolby_vision: bool,
}

#[derive(Debug, Clone)]
pub struct MediaFile {
    pub path: PathBuf,
    pub container: String,
    pub tracks: Vec<Track>,
    pub file_size_bytes: u64,
    pub duration_secs: Option<f64>,
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
    properties: Option<MkvmergeContainerProperties>,
}

#[derive(Deserialize, Default)]
struct MkvmergeContainerProperties {
    duration: Option<u64>,
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
    pixel_dimensions: Option<String>,
    audio_sampling_frequency: Option<u32>,
    audio_bits_per_sample: Option<u32>,
    tag_bps: Option<String>,
    color_transfer_characteristics: Option<u32>,
    #[serde(default)]
    block_addition_mappings: Vec<MkvmergeBlockAdditionMapping>,
}

#[derive(Deserialize)]
struct MkvmergeBlockAdditionMapping {
    id_type: Option<u32>,
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
            let (width, height) = t
                .properties
                .pixel_dimensions
                .as_deref()
                .and_then(|s| s.split_once('x'))
                .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))
                .map_or((None, None), |(w, h)| (Some(w), Some(h)));
            // Transfer characteristic 16 = SMPTE ST 2084 (PQ), 18 = ARIB
            // STD-B67 (HLG) - both are HDR transfer functions per the
            // ISO/IEC 23001-8 registry mkvmerge reports numerically.
            let is_hdr10 = matches!(t.properties.color_transfer_characteristics, Some(16) | Some(18));
            // Dolby Vision-in-MKV is conventionally signaled via a block
            // addition mapping with id_type 4. Best-effort: absent/
            // unrecognized data means `false`, never a guessed `true`.
            let is_dolby_vision = t
                .properties
                .block_addition_mappings
                .iter()
                .any(|m| m.id_type == Some(4));
            let bitrate_bps = t.properties.tag_bps.as_deref().and_then(|s| s.parse::<u64>().ok());
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
                width,
                height,
                sampling_rate: t.properties.audio_sampling_frequency,
                bits_per_sample: t.properties.audio_bits_per_sample,
                bitrate_bps,
                is_hdr10,
                is_dolby_vision,
            })
        })
        .collect();

    let file_size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let duration_secs = parsed
        .container
        .properties
        .and_then(|p| p.duration)
        .map(|ns| ns as f64 / 1_000_000_000.0);

    Ok(MediaFile {
        path: path.to_path_buf(),
        container: parsed.container.kind,
        tracks,
        file_size_bytes,
        duration_secs,
    })
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
    // Defensive: some ffprobe output-format combinations have been observed
    // to leave a trailing separator character (e.g. a stray "," from the csv
    // writer) after the last field even with no further fields requested.
    // Strip any trailing non-digit characters rather than trusting the
    // output to be exactly "NUM/DEN" with nothing else.
    let num = num.trim_end_matches(|c: char| !c.is_ascii_digit());
    let den = den.trim_end_matches(|c: char| !c.is_ascii_digit());
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
    let trimmed = text.trim();
    // Same defensive trailing-artifact stripping as parse_r_frame_rate.
    let cleaned = trimmed.trim_end_matches(|c: char| !c.is_ascii_digit());
    cleaned
        .parse()
        .map_err(|_| MergerError::Probe(format!("unexpected duration output: {trimmed}")))
}

fn ffprobe_video_fps(path: &Path) -> Result<f64, MergerError> {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=r_frame_rate", "-of", "default=noprint_wrappers=1:nokey=1"])
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
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1"])
        .arg(path)
        .output()
        .map_err(|_| MergerError::FfmpegNotFound)?;
    if !output.status.success() {
        return Err(MergerError::Probe(String::from_utf8_lossy(&output.stderr).to_string()));
    }
    parse_duration_output(&output.stdout)
}

pub fn channel_layout_label(channels: u32) -> String {
    match channels {
        1 => "1.0".to_string(),
        2 => "2.0".to_string(),
        6 => "5.1".to_string(),
        8 => "7.1".to_string(),
        n => format!("{n}ch"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_video_audio_subtitle_tracks() {
        let json = br#"{
            "container": {"type": "Matroska", "properties": {"duration": 5072000000000}},
            "tracks": [
                {"id":0,"type":"video","codec":"MPEG-4p10/AVC/h.264","properties":{"default_track":true,"forced_track":false,"default_duration":41708333,"pixel_dimensions":"3840x2160","color_transfer_characteristics":16,"block_addition_mappings":[{"id_type":4}]}},
                {"id":1,"type":"audio","codec":"AC-3","properties":{"default_track":true,"forced_track":false,"language":"eng","audio_channels":6,"audio_sampling_frequency":48000,"audio_bits_per_sample":16,"tag_bps":"640000"}},
                {"id":2,"type":"subtitles","codec":"SubRip/SRT","properties":{"default_track":false,"forced_track":false,"language":"fre","track_name":"Forced"}}
            ]
        }"#;

        let media = parse_mkvmerge_json(json, Path::new("test.mkv")).unwrap();

        assert_eq!(media.container, "Matroska");
        assert!((media.duration_secs.unwrap() - 5072.0).abs() < 0.001, "got {:?}", media.duration_secs);
        assert_eq!(media.tracks.len(), 3);

        assert_eq!(media.tracks[0].kind, TrackKind::Video);
        assert!((media.tracks[0].fps.unwrap() - 23.976).abs() < 0.01);
        assert_eq!(media.tracks[0].width, Some(3840));
        assert_eq!(media.tracks[0].height, Some(2160));
        assert!(media.tracks[0].is_hdr10, "transfer characteristic 16 (PQ) should be detected as HDR10");
        assert!(media.tracks[0].is_dolby_vision, "block addition id_type 4 should be detected as Dolby Vision");

        assert_eq!(media.tracks[1].kind, TrackKind::Audio);
        assert_eq!(media.tracks[1].channels, Some(6));
        assert_eq!(media.tracks[1].language.as_deref(), Some("eng"));
        assert_eq!(media.tracks[1].sampling_rate, Some(48000));
        assert_eq!(media.tracks[1].bits_per_sample, Some(16));
        assert_eq!(media.tracks[1].bitrate_bps, Some(640000));

        assert_eq!(media.tracks[2].kind, TrackKind::Subtitle);
        assert_eq!(media.tracks[2].language.as_deref(), Some("fre"));
        assert_eq!(media.tracks[2].name.as_deref(), Some("Forced"));
        assert_eq!(media.tracks[2].width, None);
        assert!(!media.tracks[2].is_hdr10);
        assert!(!media.tracks[2].is_dolby_vision);
    }

    #[test]
    fn missing_container_duration_yields_none() {
        let json = br#"{
            "container": {"type": "Matroska"},
            "tracks": [
                {"id":0,"type":"video","codec":"AV1","properties":{"default_track":false,"forced_track":false}}
            ]
        }"#;

        let media = parse_mkvmerge_json(json, Path::new("test.mkv")).unwrap();

        assert_eq!(media.duration_secs, None);
    }

    #[test]
    fn missing_optional_properties_yield_none_not_a_parse_error() {
        let json = br#"{
            "container": {"type": "Matroska"},
            "tracks": [
                {"id":0,"type":"video","codec":"AV1","properties":{"default_track":false,"forced_track":false}}
            ]
        }"#;

        let media = parse_mkvmerge_json(json, Path::new("test.mkv")).unwrap();

        assert_eq!(media.tracks[0].width, None);
        assert_eq!(media.tracks[0].height, None);
        assert_eq!(media.tracks[0].bitrate_bps, None);
        assert!(!media.tracks[0].is_hdr10);
        assert!(!media.tracks[0].is_dolby_vision);
    }

    #[test]
    fn channel_layout_label_maps_common_counts() {
        assert_eq!(channel_layout_label(1), "1.0");
        assert_eq!(channel_layout_label(2), "2.0");
        assert_eq!(channel_layout_label(6), "5.1");
        assert_eq!(channel_layout_label(8), "7.1");
    }

    #[test]
    fn channel_layout_label_falls_back_for_uncommon_counts() {
        assert_eq!(channel_layout_label(3), "3ch");
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

    #[test]
    fn parses_frame_rate_with_trailing_comma_artifact() {
        // Reproduces a real ffprobe output observed in the wild: "24/1,"
        // with a stray trailing comma, which previously failed to parse
        // the denominator ("1," is not a valid f64).
        let fps = parse_r_frame_rate(b"24/1,\n").unwrap();
        assert!((fps - 24.0).abs() < 0.001, "got {fps}");
    }

    #[test]
    fn parses_duration_with_trailing_comma_artifact() {
        let secs = parse_duration_output(b"7261.234000,\n").unwrap();
        assert!((secs - 7261.234).abs() < 0.001, "got {secs}");
    }
}

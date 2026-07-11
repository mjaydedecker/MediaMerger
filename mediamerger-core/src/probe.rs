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
}

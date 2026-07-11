use std::fmt;

#[derive(Debug, Clone)]
pub enum MergerError {
    Probe(String),
    FramerateMismatch { file_a_fps: f64, file_b_fps: f64 },
    FfmpegNotFound,
    MkvmergeNotFound,
    MuxFailed(String),
}

impl fmt::Display for MergerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MergerError::Probe(msg) => write!(f, "failed to probe media file: {msg}"),
            MergerError::FramerateMismatch { file_a_fps, file_b_fps } => write!(
                f,
                "video framerates differ (File A: {file_a_fps:.3} fps, File B: {file_b_fps:.3} fps); a single fixed offset cannot hold"
            ),
            MergerError::FfmpegNotFound => write!(f, "ffmpeg/ffprobe not found on PATH"),
            MergerError::MkvmergeNotFound => write!(f, "mkvmerge not found on PATH"),
            MergerError::MuxFailed(msg) => write!(f, "mkvmerge failed: {msg}"),
        }
    }
}

impl std::error::Error for MergerError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framerate_mismatch_message_names_both_values() {
        let err = MergerError::FramerateMismatch { file_a_fps: 23.976, file_b_fps: 25.0 };
        let msg = err.to_string();
        assert!(msg.contains("23.976"), "message was: {msg}");
        assert!(msg.contains("25.000") || msg.contains("25"), "message was: {msg}");
    }
}

use crate::error::MergerError;
use std::path::Path;
use std::process::Command;

pub const SAMPLE_RATE_HZ: u32 = 16000;

fn bytes_to_f32_samples(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

pub fn extract_window(
    path: &Path,
    track_id: u64,
    start_secs: f64,
    duration_secs: f64,
) -> Result<Vec<f32>, MergerError> {
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-ss"])
        .arg(start_secs.to_string())
        .arg("-t")
        .arg(duration_secs.to_string())
        .arg("-i")
        .arg(path)
        .args(["-map", &format!("0:{track_id}"), "-vn", "-ac", "1", "-ar", &SAMPLE_RATE_HZ.to_string(), "-f", "f32le", "-"])
        .output()
        .map_err(|_| MergerError::FfmpegNotFound)?;

    if !output.status.success() {
        return Err(MergerError::Probe(String::from_utf8_lossy(&output.stderr).to_string()));
    }

    Ok(bytes_to_f32_samples(&output.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_little_endian_f32_samples() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.extend_from_slice(&(-0.5f32).to_le_bytes());
        bytes.extend_from_slice(&0.25f32.to_le_bytes());

        let samples = bytes_to_f32_samples(&bytes);

        assert_eq!(samples, vec![1.0, -0.5, 0.25]);
    }

    #[test]
    fn drops_trailing_partial_sample() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.push(0); // 1 stray byte, not a full f32

        let samples = bytes_to_f32_samples(&bytes);

        assert_eq!(samples, vec![1.0]);
    }
}

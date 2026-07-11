use crate::error::MergerError;
use rustfft::{num_complex::Complex32, FftPlanner};
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

pub fn cross_correlate(a: &[f32], b: &[f32], sample_rate: f64) -> (f64, f32) {
    let n = (a.len() + b.len()).next_power_of_two();

    let mut buf_a: Vec<Complex32> = a.iter().map(|&x| Complex32::new(x, 0.0)).collect();
    buf_a.resize(n, Complex32::new(0.0, 0.0));
    let mut buf_b: Vec<Complex32> = b.iter().map(|&x| Complex32::new(x, 0.0)).collect();
    buf_b.resize(n, Complex32::new(0.0, 0.0));

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(n);
    fft.process(&mut buf_a);
    fft.process(&mut buf_b);

    let mut cross: Vec<Complex32> = buf_a
        .iter()
        .zip(buf_b.iter())
        .map(|(fa, fb)| {
            let prod = fa * fb.conj();
            let mag = prod.norm();
            if mag > 1e-12 { prod / mag } else { Complex32::new(0.0, 0.0) }
        })
        .collect();

    let ifft = planner.plan_fft_inverse(n);
    ifft.process(&mut cross);

    let mags: Vec<f32> = cross.iter().map(|c| c.norm()).collect();
    let (peak_idx, &peak_val) = mags
        .iter()
        .enumerate()
        .max_by(|(_, x), (_, y)| x.total_cmp(y))
        .expect("mags is non-empty");

    let sum: f32 = mags.iter().sum();
    let mean_other = (sum - peak_val) / (mags.len() as f32 - 1.0).max(1.0);
    let confidence = if mean_other > 1e-9 { peak_val / mean_other } else { peak_val };

    // NOTE ON SIGN: this lag convention is verified by the tests below, not by
    // derivation. If `positive_offset_means_b_lags_a` fails with the correct
    // magnitude but flipped sign, negate `lag` here — the test is the source
    // of truth for the convention documented on this function, not this comment.
    let lag = if peak_idx > n / 2 { peak_idx as i64 - n as i64 } else { peak_idx as i64 };
    let offset_secs = -lag as f64 / sample_rate;

    (offset_secs, confidence)
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

#[cfg(test)]
mod cross_correlate_tests {
    use super::*;

    fn synthetic_signal(len: usize) -> Vec<f32> {
        (0..len).map(|i| ((i as f32) * 0.1).sin() + ((i as f32) * 0.031).sin() * 0.5).collect()
    }

    #[test]
    fn positive_offset_means_b_lags_a() {
        let sample_rate = 1000.0;
        let base = synthetic_signal(1000);
        let shift = 137usize;

        let a = base.clone();
        let mut b = vec![0.0f32; shift];
        b.extend_from_slice(&base);

        let (offset_secs, confidence) = cross_correlate(&a, &b, sample_rate);
        let expected = shift as f64 / sample_rate;

        assert!((offset_secs - expected).abs() < 0.01, "offset {offset_secs} expected {expected}");
        assert!(confidence > 3.0, "confidence too low: {confidence}");
    }

    #[test]
    fn low_confidence_for_uncorrelated_noise() {
        let mut state = 12345u32;
        let mut next = move || {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            (state as f32 / u32::MAX as f32) * 2.0 - 1.0
        };
        let noise_a: Vec<f32> = (0..2000).map(|_| next()).collect();
        let noise_b: Vec<f32> = (0..2000).map(|_| next()).collect();
        let (_, noise_confidence) = cross_correlate(&noise_a, &noise_b, 1000.0);

        let signal = synthetic_signal(2000);
        let (_, signal_confidence) = cross_correlate(&signal, &signal, 1000.0);

        assert!(
            noise_confidence < signal_confidence,
            "noise confidence {noise_confidence} should be less than matched-signal confidence {signal_confidence}"
        );
    }
}

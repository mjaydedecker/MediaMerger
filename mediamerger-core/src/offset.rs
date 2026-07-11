use crate::error::MergerError;
use crate::probe;
use rustfft::{num_complex::Complex32, FftPlanner};
use std::path::Path;
use std::process::Command;

pub const SAMPLE_RATE_HZ: u32 = 16000;
const CONSISTENCY_TOLERANCE_SECS: f64 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Consistency {
    Consistent,
    Inconsistent,
    Unverified,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OffsetResult {
    pub early_offset: f64,
    pub late_offset: f64,
    pub consistency: Consistency,
    pub confidence: f32,
    pub offset: f64,
    pub early_window_start: f64,
    pub window_duration: f64,
}

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

    let confidence = confidence_from_mags(&mags, peak_val);

    // NOTE ON SIGN: this lag convention is verified by the tests below, not by
    // derivation. If `positive_offset_means_b_lags_a` fails with the correct
    // magnitude but flipped sign, negate `lag` here — the test is the source
    // of truth for the convention documented on this function, not this comment.
    let lag = if peak_idx > n / 2 { peak_idx as i64 - n as i64 } else { peak_idx as i64 };
    let offset_secs = -lag as f64 / sample_rate;

    (offset_secs, confidence)
}

/// Confidence = peak magnitude / mean magnitude of every other bin.
///
/// Accumulates the sum in f64, not f32. `mags` has one entry per FFT bin -
/// for a realistic ~180s correlation window at 16kHz, that's an FFT size
/// right around 2^23, which is exactly f32's precision limit (23-bit
/// mantissa). Summing that many f32 values in-place saturates once the
/// running total is too large to represent a further unit-magnitude
/// increment, silently dropping it - the resulting `sum` (and thus
/// `mean_other`) ends up reflecting the bin count rather than the actual
/// magnitudes, producing a `confidence` that blows up to roughly the FFT
/// size instead of a meaningful ratio (observed in the wild: a reported
/// confidence of ~8.3 million, matching 2^23 = 8,388,608 almost exactly).
/// f64's precision limit (2^52) is unreachable for any realistic window.
fn confidence_from_mags(mags: &[f32], peak_val: f32) -> f32 {
    let sum: f64 = mags.iter().map(|&m| m as f64).sum();
    let peak_val = peak_val as f64;
    let mean_other = (sum - peak_val) / (mags.len() as f64 - 1.0).max(1.0);
    if mean_other > 1e-9 { (peak_val / mean_other) as f32 } else { peak_val as f32 }
}

fn pick_windows(shorter_duration: f64) -> (f64, f64, f64) {
    let window = 180.0_f64.min(shorter_duration * 0.1).max(5.0);
    if shorter_duration >= 1200.0 {
        (shorter_duration * 0.30, shorter_duration * 0.70, window)
    } else {
        (shorter_duration * 0.20, shorter_duration * 0.80, window)
    }
}

fn measure_at(
    file_a: &Path,
    track_a: u64,
    file_b: &Path,
    track_b: u64,
    start: f64,
    window: f64,
) -> Result<(f64, f32), MergerError> {
    let a = extract_window(file_a, track_a, start, window)?;
    let b = extract_window(file_b, track_b, start, window)?;
    Ok(cross_correlate(&a, &b, SAMPLE_RATE_HZ as f64))
}

pub fn detect_offset(
    file_a: &Path,
    audio_track_a: u64,
    file_b: &Path,
    audio_track_b: u64,
) -> Result<OffsetResult, MergerError> {
    let duration_a = probe::duration_secs(file_a)?;
    let duration_b = probe::duration_secs(file_b)?;
    let shorter = duration_a.min(duration_b);

    if shorter < 120.0 {
        let window = (shorter * 0.5).max(1.0);
        let start = shorter * 0.25;
        let (offset, confidence) = measure_at(file_a, audio_track_a, file_b, audio_track_b, start, window)?;
        return Ok(OffsetResult {
            early_offset: offset,
            late_offset: offset,
            consistency: Consistency::Unverified,
            confidence,
            offset,
            early_window_start: start,
            window_duration: window,
        });
    }

    let (early_start, late_start, window) = pick_windows(shorter);
    let (early_offset, early_conf) =
        measure_at(file_a, audio_track_a, file_b, audio_track_b, early_start, window)?;
    let (late_offset, late_conf) =
        measure_at(file_a, audio_track_a, file_b, audio_track_b, late_start, window)?;

    let consistency = if (early_offset - late_offset).abs() <= CONSISTENCY_TOLERANCE_SECS {
        Consistency::Consistent
    } else {
        Consistency::Inconsistent
    };
    let offset = if consistency == Consistency::Consistent {
        (early_offset + late_offset) / 2.0
    } else {
        early_offset
    };

    Ok(OffsetResult {
        early_offset,
        late_offset,
        consistency,
        confidence: early_conf.min(late_conf),
        offset,
        early_window_start: early_start,
        window_duration: window,
    })
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

    #[test]
    fn confidence_does_not_blow_up_from_f32_summation_precision_loss() {
        // Regression test for a real bug hit in production: a naive f32
        // accumulator summing millions of unit-magnitude bins (a realistic
        // FFT size for a long correlation window) silently drops further
        // increments once the running total exceeds f32's representable
        // precision at that magnitude, corrupting mean_other and reporting
        // a confidence in the millions instead of a small ratio.
        //
        // Construct this exactly: (n-1) bins all at 1.0 and one peak bin at
        // 2.0. The true answer is unambiguous - mean_other over (n-1)
        // identical 1.0 values is exactly 1.0, so confidence must be
        // peak/mean_other = 2.0. n is chosen comfortably past f32's exact
        // integer limit (2^24) so the old f32-summing code would have
        // definitely saturated and gotten this wrong.
        let n = 20_000_000;
        let mut mags = vec![1.0f32; n];
        let peak_idx = 42;
        mags[peak_idx] = 2.0;

        let confidence = confidence_from_mags(&mags, mags[peak_idx]);

        assert!((confidence - 2.0).abs() < 0.01, "expected confidence ~2.0, got {confidence}");
    }
}

#[derive(Debug, Clone)]
pub struct WaveformEnvelope {
    pub bars_a: Vec<f32>,
    pub bars_b: Vec<f32>,
    pub window_start_secs: f64,
    pub window_duration_secs: f64,
}

fn downsample_rms(samples: &[f32], bucket_count: usize) -> Vec<f32> {
    if bucket_count == 0 {
        return Vec::new();
    }
    if samples.is_empty() {
        return vec![0.0; bucket_count];
    }
    let chunk_size = (samples.len() / bucket_count).max(1);
    let mut bars: Vec<f32> = samples
        .chunks(chunk_size)
        .map(|chunk| {
            let sum_sq: f32 = chunk.iter().map(|s| s * s).sum();
            (sum_sq / chunk.len() as f32).sqrt()
        })
        .collect();
    bars.truncate(bucket_count);
    bars.resize(bucket_count, 0.0);
    bars
}

fn normalize_joint(bars_a: &mut [f32], bars_b: &mut [f32]) {
    let peak = bars_a
        .iter()
        .chain(bars_b.iter())
        .cloned()
        .fold(0.0f32, f32::max);
    if peak > 1e-6 {
        for b in bars_a.iter_mut().chain(bars_b.iter_mut()) {
            *b /= peak;
        }
    }
}

pub fn extract_waveform(
    file_a: &Path,
    track_a: u64,
    file_b: &Path,
    track_b: u64,
    start_secs: f64,
    duration_secs: f64,
    bucket_count: usize,
) -> Result<WaveformEnvelope, MergerError> {
    let pcm_a = extract_window(file_a, track_a, start_secs, duration_secs)?;
    let pcm_b = extract_window(file_b, track_b, start_secs, duration_secs)?;

    let mut bars_a = downsample_rms(&pcm_a, bucket_count);
    let mut bars_b = downsample_rms(&pcm_b, bucket_count);
    normalize_joint(&mut bars_a, &mut bars_b);

    Ok(WaveformEnvelope {
        bars_a,
        bars_b,
        window_start_secs: start_secs,
        window_duration_secs: duration_secs,
    })
}

#[cfg(test)]
mod waveform_tests {
    use super::*;

    #[test]
    fn downsample_rms_produces_requested_bucket_count() {
        let samples: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.01).sin()).collect();
        let bars = downsample_rms(&samples, 20);
        assert_eq!(bars.len(), 20);
    }

    #[test]
    fn downsample_rms_of_silence_is_zero() {
        let samples = vec![0.0f32; 500];
        let bars = downsample_rms(&samples, 10);
        assert!(bars.iter().all(|&b| b == 0.0));
    }

    #[test]
    fn downsample_rms_handles_empty_input() {
        let bars = downsample_rms(&[], 10);
        assert_eq!(bars, vec![0.0; 10]);
    }

    #[test]
    fn normalize_joint_scales_against_shared_peak_not_per_track() {
        let mut bars_a = vec![1.0, 0.5]; // louder track
        let mut bars_b = vec![0.25, 0.1]; // quieter track
        normalize_joint(&mut bars_a, &mut bars_b);

        // Peak (1.0) came from bars_a, so bars_a's max normalizes to 1.0...
        assert!((bars_a[0] - 1.0).abs() < 1e-6);
        // ...but bars_b, being quieter, must NOT also reach 1.0 - it stays
        // proportionally smaller, preserving the real loudness difference.
        assert!(bars_b[0] < 0.5, "bars_b[0] = {}, should stay well below 1.0", bars_b[0]);
    }
}

#[cfg(test)]
mod detect_offset_tests {
    use super::*;

    #[test]
    fn long_file_uses_30_70_split_with_full_window() {
        let (early, late, window) = pick_windows(3600.0);
        assert_eq!(early, 1080.0);
        assert_eq!(late, 2520.0);
        assert_eq!(window, 180.0);
    }

    #[test]
    fn short_file_uses_20_80_split_with_smaller_window() {
        let (early, late, window) = pick_windows(300.0);
        assert_eq!(early, 60.0);
        assert_eq!(late, 240.0);
        assert!(window < 180.0, "window {window} should be smaller than the default cap");
    }
}

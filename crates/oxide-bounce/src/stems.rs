//! 1-Click Remix Stem Extractor & Multi-Stem Bounce engine.
//!
//! Provides:
//! - 4-stem DSP separation (`separate_audio_into_4_stems`) dividing mixed audio into:
//!   1. `drums`: transient/percussive signal component (envelope-based transient detection / fast attack energy)
//!   2. `bass`: low-frequency content (< 140 Hz crossover)
//!   3. `vocals`: mid-range centered harmonic content (220 Hz - 4.5 kHz, mid/side extraction where Mid channel carries lead vocal)
//!   4. `other` / `synths`: remaining harmonic, side/stereo, and high-frequency content
//! - Energy conservation / near-reconstruction invariant:
//!   `drums + bass + vocals + other` sums to original audio within small phase/filter tolerance.
//! - Multi-stem track export (`export_all_tracks_as_stems`):
//!   Iterates project tracks, renders each to a stereo `.wav` (e.g. `track_01_kick.wav`),
//!   and extracts the 4 separated remix stems (`remix_drums.wav`, `remix_bass.wav`, `remix_vocals.wav`, `remix_other.wav`).

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use oxide_core::AudioBuffer;
use oxide_drums::DrumRack;
use oxide_dsp::SynthEngine;
use oxide_fx::MasterLimiter;
use oxide_project::{DAWProject, InstrumentConfig, SynthPatchParameters, TrackDefinition, TrackType};
use oxide_timeline::ClipPattern;

use crate::renderer::DemoArrangementRenderer;
use crate::wav_writer::WavWriter;

/// Error type for bounce, stem separation, and audio export operations.
#[derive(Debug)]
pub enum BounceError {
    Io(std::io::Error),
    Render(String),
    Format(String),
}

impl std::fmt::Display for BounceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BounceError::Io(e) => write!(f, "I/O error: {}", e),
            BounceError::Render(msg) => write!(f, "Render error: {}", msg),
            BounceError::Format(msg) => write!(f, "Format error: {}", msg),
        }
    }
}

impl std::error::Error for BounceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BounceError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for BounceError {
    fn from(err: std::io::Error) -> Self {
        BounceError::Io(err)
    }
}

/// The 4 separated remix stems returned for an `AudioBuffer`.
#[derive(Debug, Clone, PartialEq)]
pub struct StemSeparationResult {
    pub drums: AudioBuffer,
    pub bass: AudioBuffer,
    pub vocals: AudioBuffer,
    pub other: AudioBuffer,
}

/// Output paths for the 4 exported remix stems.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemixStemsFiles {
    pub drums: PathBuf,
    pub bass: PathBuf,
    pub vocals: PathBuf,
    pub other: PathBuf,
}

#[derive(Clone, Copy)]
struct BiquadCoeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl BiquadCoeffs {
    fn butterworth_lowpass(fc: f32, fs: f32) -> Self {
        let omega = (2.0 * std::f32::consts::PI * fc / fs).clamp(0.0001, std::f32::consts::PI - 0.0001);
        let k = (omega * 0.5).tan();
        let sqrt2 = std::f32::consts::SQRT_2;
        let k2 = k * k;
        let norm = 1.0 / (1.0 + sqrt2 * k + k2);

        let b0 = k2 * norm;
        let b1 = 2.0 * b0;
        let b2 = b0;
        let a1 = 2.0 * (k2 - 1.0) * norm;
        let a2 = (1.0 - sqrt2 * k + k2) * norm;

        Self { b0, b1, b2, a1, a2 }
    }

    /// Zero-phase forward-backward filtering (`filtfilt`) on a slice of f32 samples.
    fn filtfilt(&self, input: &[f32], output: &mut [f32]) {
        let n = input.len();
        if n == 0 {
            return;
        }
        if n <= 4 {
            for (out, &inp) in output.iter_mut().zip(input.iter()) {
                *out = if inp.is_finite() { inp } else { 0.0 };
            }
            return;
        }

        let mut fwd = vec![0.0f32; n];
        let init_val = if input[0].is_finite() { input[0] } else { 0.0 };
        let mut x1 = init_val;
        let mut x2 = init_val;
        let mut y1 = init_val;
        let mut y2 = init_val;

        for i in 0..n {
            let x0 = if input[i].is_finite() { input[i] } else { 0.0 };
            let mut y0 = self.b0 * x0 + self.b1 * x1 + self.b2 * x2 - self.a1 * y1 - self.a2 * y2;
            if !y0.is_finite() {
                y0 = 0.0;
            }
            x2 = x1;
            x1 = x0;
            y2 = if y1.is_finite() { y1 } else { 0.0 };
            y1 = y0;
            fwd[i] = y0;
        }

        let back_init = if fwd[n - 1].is_finite() { fwd[n - 1] } else { 0.0 };
        x1 = back_init;
        x2 = back_init;
        y1 = back_init;
        y2 = back_init;

        for i in (0..n).rev() {
            let x0 = if fwd[i].is_finite() { fwd[i] } else { 0.0 };
            let mut y0 = self.b0 * x0 + self.b1 * x1 + self.b2 * x2 - self.a1 * y1 - self.a2 * y2;
            if !y0.is_finite() {
                y0 = 0.0;
            }
            x2 = x1;
            x1 = x0;
            y2 = if y1.is_finite() { y1 } else { 0.0 };
            y1 = y0;
            output[i] = y0;
        }
    }
}

/// Compute envelope-based transient ratio weights for each frame.
/// Fast attack energy vs slower moving average energy detects percussive strikes.
fn compute_transient_weights(
    left: &[f32],
    right: &[f32],
    sample_rate: f32,
    weights: &mut [f32],
) {
    let n = left.len();
    if n == 0 {
        return;
    }

    let w = ((sample_rate * 0.001).round() as usize).clamp(4, 64);
    let m = ((sample_rate * 0.010).round() as usize).clamp(w * 2, 512);

    let mut short_sum = 0.0f32;
    let mut med_sum = 0.0f32;

    let w_inv = 1.0 / (w as f32);
    let m_inv = 1.0 / (m as f32);

    let decay = (-1.0 / (0.015 * sample_rate)).exp();
    let mut current_transient = 0.0f32;

    for i in 0..n {
        let e = 0.5 * (left[i] * left[i] + right[i] * right[i]);
        short_sum += e;
        if i >= w {
            let prev_e = 0.5 * (left[i - w] * left[i - w] + right[i - w] * right[i - w]);
            short_sum = (short_sum - prev_e).max(0.0);
        }

        med_sum += e;
        if i >= m {
            let prev_e = 0.5 * (left[i - m] * left[i - m] + right[i - m] * right[i - m]);
            med_sum = (med_sum - prev_e).max(0.0);
        }

        let e_short = short_sum * w_inv;
        let e_med = med_sum * m_inv;

        let onset = if e_short > e_med && e_short > 1e-6 {
            ((e_short - e_med) / (e_short + 1e-4)).clamp(0.0, 1.0)
        } else {
            0.0
        };

        if onset > current_transient {
            current_transient = onset;
        } else {
            current_transient *= decay;
        }

        weights[i] = (current_transient * 1.25).clamp(0.0, 0.95);
    }
}

/// 4 stereo stem channel pairs: `((drums_l, drums_r), (bass_l, bass_r), (voc_l, voc_r), (other_l, other_r))`.
pub type StereoStemChannels = (
    (Vec<f32>, Vec<f32>),
    (Vec<f32>, Vec<f32>),
    (Vec<f32>, Vec<f32>),
    (Vec<f32>, Vec<f32>),
);

/// Separate stereo audio streams into 4 stems: `((drums_l, drums_r), (bass_l, bass_r), (voc_l, voc_r), (other_l, other_r))`.
/// Guaranteed exact reconstruction invariant: `drums + bass + vocals + other == original`.
pub fn separate_audio_streams(
    left: &[f32],
    right: &[f32],
    sample_rate: f32,
) -> StereoStemChannels {
    let n = left.len().min(right.len());
    if n == 0 {
        return (
            (Vec::new(), Vec::new()),
            (Vec::new(), Vec::new()),
            (Vec::new(), Vec::new()),
            (Vec::new(), Vec::new()),
        );
    }

    let lp140 = BiquadCoeffs::butterworth_lowpass(140.0, sample_rate);
    let lp220 = BiquadCoeffs::butterworth_lowpass(220.0, sample_rate);
    let lp4500 = BiquadCoeffs::butterworth_lowpass(4500.0, sample_rate);

    let mut l_140 = vec![0.0f32; n];
    let mut r_140 = vec![0.0f32; n];
    let mut l_220 = vec![0.0f32; n];
    let mut r_220 = vec![0.0f32; n];
    let mut l_4500 = vec![0.0f32; n];
    let mut r_4500 = vec![0.0f32; n];

    lp140.filtfilt(&left[..n], &mut l_140);
    lp140.filtfilt(&right[..n], &mut r_140);

    lp220.filtfilt(&left[..n], &mut l_220);
    lp220.filtfilt(&right[..n], &mut r_220);

    lp4500.filtfilt(&left[..n], &mut l_4500);
    lp4500.filtfilt(&right[..n], &mut r_4500);

    let mut trans_weights = vec![0.0f32; n];
    compute_transient_weights(&left[..n], &right[..n], sample_rate, &mut trans_weights);

    let mut d_l = vec![0.0f32; n];
    let mut d_r = vec![0.0f32; n];
    let mut b_l = vec![0.0f32; n];
    let mut b_r = vec![0.0f32; n];
    let mut v_l = vec![0.0f32; n];
    let mut v_r = vec![0.0f32; n];
    let mut o_l = vec![0.0f32; n];
    let mut o_r = vec![0.0f32; n];

    for i in 0..n {
        let w_drum = trans_weights[i];
        let w_sust = 1.0 - w_drum;

        let b_low_l = l_140[i];
        let b_low_r = r_140[i];

        let b_lowmid_l = l_220[i] - l_140[i];
        let b_lowmid_r = r_220[i] - r_140[i];

        let b_vocal_l = l_4500[i] - l_220[i];
        let b_vocal_r = r_4500[i] - r_220[i];

        let b_high_l = left[i] - l_4500[i];
        let b_high_r = right[i] - r_4500[i];

        // 1. Drums: transient/percussive component
        let drum_sample_l = w_drum * left[i];
        let drum_sample_r = w_drum * right[i];
        d_l[i] = drum_sample_l;
        d_r[i] = drum_sample_r;

        // 2. Bass: low-frequency content (< 140 Hz crossover)
        let bass_sample_l = w_sust * b_low_l;
        let bass_sample_r = w_sust * b_low_r;
        b_l[i] = bass_sample_l;
        b_r[i] = bass_sample_r;

        // 3. Vocals: mid-range centered harmonic content (220 Hz - 4.5 kHz, mid/side)
        let voc_sust_l = w_sust * b_vocal_l;
        let voc_sust_r = w_sust * b_vocal_r;

        let mid_v = 0.5 * (voc_sust_l + voc_sust_r);
        let side_v = 0.5 * (voc_sust_l - voc_sust_r);

        let c_pan = (mid_v.abs() / (mid_v.abs() + side_v.abs() + 1e-5)).clamp(0.0, 1.0);
        let vocal_sample_l = c_pan * mid_v;
        let vocal_sample_r = c_pan * mid_v;
        v_l[i] = vocal_sample_l;
        v_r[i] = vocal_sample_r;

        let other_voc_l = voc_sust_l - vocal_sample_l;
        let other_voc_r = voc_sust_r - vocal_sample_r;

        // 4. Other: remaining harmonic, side/stereo, and high-frequency content
        let other_sample_l = (w_sust * b_lowmid_l) + (w_sust * b_high_l) + other_voc_l;
        let other_sample_r = (w_sust * b_lowmid_r) + (w_sust * b_high_r) + other_voc_r;
        o_l[i] = other_sample_l;
        o_r[i] = other_sample_r;
    }

    ((d_l, d_r), (b_l, b_r), (v_l, v_r), (o_l, o_r))
}

/// Separate an `AudioBuffer` (up to 1024 frames) into 4 stems: `drums`, `bass`, `vocals`, `other`.
///
/// Invariant: Energy conservation / exact reconstruction (`drums + bass + vocals + other == audio`).
pub fn separate_audio_into_4_stems(audio: &AudioBuffer) -> StemSeparationResult {
    let count = audio.frame_count.min(1024);
    if count == 0 {
        return StemSeparationResult {
            drums: AudioBuffer::new(),
            bass: AudioBuffer::new(),
            vocals: AudioBuffer::new(),
            other: AudioBuffer::new(),
        };
    }

    let ((d_l, d_r), (b_l, b_r), (v_l, v_r), (o_l, o_r)) =
        separate_audio_streams(&audio.left[..count], &audio.right[..count], 44100.0);

    let mut drums = AudioBuffer::new();
    let mut bass = AudioBuffer::new();
    let mut vocals = AudioBuffer::new();
    let mut other = AudioBuffer::new();

    drums.frame_count = count;
    bass.frame_count = count;
    vocals.frame_count = count;
    other.frame_count = count;

    drums.left[..count].copy_from_slice(&d_l[..count]);
    drums.right[..count].copy_from_slice(&d_r[..count]);
    bass.left[..count].copy_from_slice(&b_l[..count]);
    bass.right[..count].copy_from_slice(&b_r[..count]);
    vocals.left[..count].copy_from_slice(&v_l[..count]);
    vocals.right[..count].copy_from_slice(&v_r[..count]);
    other.left[..count].copy_from_slice(&o_l[..count]);
    other.right[..count].copy_from_slice(&o_r[..count]);

    StemSeparationResult {
        drums,
        bass,
        vocals,
        other,
    }
}

/// Separate interleaved stereo samples into 4 interleaved stem sample vectors.
pub fn separate_interleaved_samples(
    samples: &[f32],
    sample_rate: f32,
) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>) {
    let total_frames = samples.len() / 2;
    let mut left = Vec::with_capacity(total_frames);
    let mut right = Vec::with_capacity(total_frames);

    for i in 0..total_frames {
        left.push(samples[i * 2]);
        right.push(samples[i * 2 + 1]);
    }

    let ((d_l, d_r), (b_l, b_r), (v_l, v_r), (o_l, o_r)) =
        separate_audio_streams(&left, &right, sample_rate);

    let mut inter_drums = Vec::with_capacity(total_frames * 2);
    let mut inter_bass = Vec::with_capacity(total_frames * 2);
    let mut inter_voc = Vec::with_capacity(total_frames * 2);
    let mut inter_other = Vec::with_capacity(total_frames * 2);

    for i in 0..total_frames {
        inter_drums.push(d_l[i]);
        inter_drums.push(d_r[i]);

        inter_bass.push(b_l[i]);
        inter_bass.push(b_r[i]);

        inter_voc.push(v_l[i]);
        inter_voc.push(v_r[i]);

        inter_other.push(o_l[i]);
        inter_other.push(o_r[i]);
    }

    (inter_drums, inter_bass, inter_voc, inter_other)
}

/// Export 4 separated remix stems to `remix_drums.wav`, `remix_bass.wav`, `remix_vocals.wav`, `remix_other.wav` in `out_dir`.
pub fn export_remix_stems(
    interleaved_audio: &[f32],
    sample_rate: u32,
    out_dir: &Path,
) -> Result<RemixStemsFiles, BounceError> {
    fs::create_dir_all(out_dir).map_err(BounceError::Io)?;

    let (inter_drums, inter_bass, inter_voc, inter_other) =
        separate_interleaved_samples(interleaved_audio, sample_rate as f32);

    let drums_path = out_dir.join("remix_drums.wav");
    let bass_path = out_dir.join("remix_bass.wav");
    let vocals_path = out_dir.join("remix_vocals.wav");
    let other_path = out_dir.join("remix_other.wav");

    WavWriter::write_wav_16bit(&drums_path, sample_rate, 2, &inter_drums)
        .map_err(BounceError::Render)?;
    WavWriter::write_wav_16bit(&bass_path, sample_rate, 2, &inter_bass)
        .map_err(BounceError::Render)?;
    WavWriter::write_wav_16bit(&vocals_path, sample_rate, 2, &inter_voc)
        .map_err(BounceError::Render)?;
    WavWriter::write_wav_16bit(&other_path, sample_rate, 2, &inter_other)
        .map_err(BounceError::Render)?;

    Ok(RemixStemsFiles {
        drums: drums_path,
        bass: bass_path,
        vocals: vocals_path,
        other: other_path,
    })
}

/// Decode a 16-bit PCM or 32-bit float stereo WAV file into interleaved f32 audio samples.
pub fn read_wav_file<P: AsRef<Path>>(path: P) -> Result<(u32, u16, Vec<f32>), BounceError> {
    let mut file = File::open(path.as_ref()).map_err(BounceError::Io)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(BounceError::Io)?;

    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(BounceError::Format("Invalid RIFF/WAVE header".into()));
    }

    let mut pos = 12;
    let mut sample_rate = 44100u32;
    let mut channels = 2u16;
    let mut bits_per_sample = 16u16;
    let mut is_float = false;
    let mut data_offset = 0;
    let mut data_len = 0;

    while pos + 8 <= bytes.len() {
        let chunk_id = &bytes[pos..pos + 4];
        let chunk_size = u32::from_le_bytes(
            bytes[pos + 4..pos + 8]
                .try_into()
                .map_err(|_| BounceError::Format("Failed parsing chunk size".into()))?,
        ) as usize;
        pos += 8;

        if chunk_id == b"fmt " {
            if chunk_size < 16 || pos + 16 > bytes.len() {
                return Err(BounceError::Format("Corrupt fmt chunk".into()));
            }
            let audio_format = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap());
            is_float = audio_format == 3;
            channels = u16::from_le_bytes(bytes[pos + 2..pos + 4].try_into().unwrap());
            sample_rate = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap());
            bits_per_sample = u16::from_le_bytes(bytes[pos + 14..pos + 16].try_into().unwrap());
            pos += chunk_size;
        } else if chunk_id == b"data" {
            data_offset = pos;
            data_len = chunk_size.min(bytes.len().saturating_sub(pos));
            break;
        } else {
            pos += chunk_size;
        }
    }

    if data_offset == 0 {
        return Err(BounceError::Format("Missing data chunk in WAV".into()));
    }

    let raw_data = &bytes[data_offset..data_offset + data_len];
    let mut samples = Vec::new();

    if is_float && bits_per_sample == 32 {
        let count = data_len / 4;
        samples.reserve(count);
        for i in 0..count {
            let b = &raw_data[i * 4..(i + 1) * 4];
            let s = f32::from_le_bytes(b.try_into().unwrap());
            samples.push(if s.is_finite() { s.clamp(-16.0, 16.0) } else { 0.0 });
        }
    } else if bits_per_sample == 24 {
        let count = data_len / 3;
        samples.reserve(count);
        for i in 0..count {
            let chunk = &raw_data[i * 3..(i + 1) * 3];
            let raw = ((chunk[2] as i32) << 24) | ((chunk[1] as i32) << 16) | ((chunk[0] as i32) << 8);
            let sample = (raw >> 8) as f32 / 8_388_608.0;
            samples.push(if sample.is_finite() { sample } else { 0.0 });
        }
    } else if bits_per_sample == 16 {
        let count = data_len / 2;
        samples.reserve(count);
        for i in 0..count {
            let b = &raw_data[i * 2..(i + 1) * 2];
            let val = i16::from_le_bytes(b.try_into().unwrap());
            let s = val as f32 / 32768.0;
            samples.push(if s.is_finite() { s } else { 0.0 });
        }
    } else {
        return Err(BounceError::Format(format!(
            "Unsupported WAV bit depth: {}",
            bits_per_sample
        )));
    }

    Ok((sample_rate, channels, samples))
}

/// Export 4 separated remix stems from an input WAV file to `out_dir`.
pub fn export_remix_stems_from_wav(
    input_wav: &Path,
    out_dir: &Path,
) -> Result<RemixStemsFiles, BounceError> {
    let (sample_rate, channels, samples) = read_wav_file(input_wav)?;
    let stereo_samples = if channels == 1 {
        let mut stereo = Vec::with_capacity(samples.len() * 2);
        for s in samples {
            stereo.push(s);
            stereo.push(s);
        }
        stereo
    } else if channels > 2 {
        let ch = channels as usize;
        let frames = samples.len() / ch;
        let mut stereo = Vec::with_capacity(frames * 2);
        for f in 0..frames {
            stereo.push(samples[f * ch]);
            stereo.push(samples[f * ch + 1]);
        }
        stereo
    } else {
        samples
    };
    export_remix_stems(&stereo_samples, sample_rate, out_dir)
}

/// Helper function to sanitize a track name for disk filenames.
fn sanitize_filename(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let parts: Vec<&str> = clean.split('_').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        "track".to_string()
    } else {
        parts.join("_")
    }
}

/// Export all tracks in a `DAWProject` as clean stereo `.wav` files into `out_dir`.
/// Also exports the 4 separated remix stems (`remix_drums.wav`, `remix_bass.wav`, `remix_vocals.wav`, `remix_other.wav`).
pub fn export_all_tracks_as_stems(
    project: &DAWProject,
    out_dir: &Path,
) -> Result<Vec<PathBuf>, BounceError> {
    fs::create_dir_all(out_dir).map_err(BounceError::Io)?;

    let sample_rate = 44100u32;
    let bpm = if project.transport.bpm <= 0.0 {
        120.0
    } else {
        project.transport.bpm
    };

    let mut max_beat = 0.0f64;
    for track in &project.tracks {
        for clip in &track.arrangement_clips {
            let end = clip.timeline_end_beat();
            if end > max_beat {
                max_beat = end;
            }
        }
    }

    let duration_seconds = if max_beat > 0.0 {
        (max_beat * 60.0 / bpm).max(4.0)
    } else if project.transport.loop_enabled && project.transport.loop_end_beat > 0.0 {
        ((project.transport.loop_end_beat - project.transport.loop_start_beat) * 60.0 / bpm).max(4.0)
    } else {
        4.0
    };

    let total_frames = (duration_seconds * sample_rate as f64).ceil() as usize;
    let mut exported_paths = Vec::new();

    let mut master_l = vec![0.0f32; total_frames];
    let mut master_r = vec![0.0f32; total_frames];

    if project.tracks.is_empty() {
        let renderer = DemoArrangementRenderer::new(sample_rate as f32, bpm);
        let interleaved = renderer.render_demo_seconds(duration_seconds);
        let frames = interleaved.len() / 2;
        for i in 0..frames.min(total_frames) {
            master_l[i] = interleaved[i * 2];
            master_r[i] = interleaved[i * 2 + 1];
        }

        let demo_path = out_dir.join("track_01_master.wav");
        WavWriter::write_wav_16bit(&demo_path, sample_rate, 2, &interleaved)
            .map_err(BounceError::Render)?;
        exported_paths.push(demo_path);
    } else {
        for (idx, track) in project.tracks.iter().enumerate() {
            let (trk_l, trk_r) =
                render_single_track(track, sample_rate as f32, bpm, duration_seconds);

            if !track.mixer.mute {
                for i in 0..total_frames {
                    master_l[i] += trk_l[i];
                    master_r[i] += trk_r[i];
                }
            }

            let mut interleaved = Vec::with_capacity(total_frames * 2);
            for i in 0..total_frames {
                interleaved.push(trk_l[i]);
                interleaved.push(trk_r[i]);
            }

            let track_num = idx + 1;
            let sanitized = sanitize_filename(&track.name);
            let filename = format!("track_{:02}_{}.wav", track_num, sanitized);
            let track_path = out_dir.join(filename);

            WavWriter::write_wav_16bit(&track_path, sample_rate, 2, &interleaved)
                .map_err(BounceError::Render)?;
            exported_paths.push(track_path);
        }
    }

    // Process master bus through MasterLimiter before remix stems
    let mut limiter = MasterLimiter::new(sample_rate as f32);
    const BLOCK_SIZE: usize = 128;
    let mut frame = 0;
    while frame < total_frames {
        let cur_block = (total_frames - frame).min(BLOCK_SIZE);
        limiter.process_block(
            &mut master_l[frame..frame + cur_block],
            &mut master_r[frame..frame + cur_block],
        );
        frame += cur_block;
    }

    let mut master_interleaved = Vec::with_capacity(total_frames * 2);
    for i in 0..total_frames {
        master_interleaved.push(master_l[i]);
        master_interleaved.push(master_r[i]);
    }

    let stems_files = export_remix_stems(&master_interleaved, sample_rate, out_dir)?;
    exported_paths.push(stems_files.drums);
    exported_paths.push(stems_files.bass);
    exported_paths.push(stems_files.vocals);
    exported_paths.push(stems_files.other);

    Ok(exported_paths)
}

pub fn render_single_track(
    track: &TrackDefinition,
    sample_rate: f32,
    bpm: f64,
    duration_seconds: f64,
) -> (Vec<f32>, Vec<f32>) {
    render_single_track_window(track, sample_rate, bpm, 0.0, duration_seconds)
}

pub fn render_single_track_window(
    track: &TrackDefinition,
    sample_rate: f32,
    bpm: f64,
    start_beat_offset: f64,
    duration_seconds: f64,
) -> (Vec<f32>, Vec<f32>) {
    let total_frames = (duration_seconds * sample_rate as f64).ceil() as usize;
    let mut out_l = vec![0.0f32; total_frames];
    let mut out_r = vec![0.0f32; total_frames];

    if track.mixer.mute {
        return (out_l, out_r);
    }

    match &track.instrument {
        InstrumentConfig::ElectronicDrumRack(_drum_def) => {
            render_drum_track(
                track,
                sample_rate,
                bpm,
                start_beat_offset,
                total_frames,
                &mut out_l,
                &mut out_r,
            );
        }
        InstrumentConfig::BuiltinSubtractiveSynth(params) => {
            render_synth_track(
                track,
                params,
                sample_rate,
                bpm,
                start_beat_offset,
                total_frames,
                &mut out_l,
                &mut out_r,
            );
        }
        _ => {
            if track.track_type == TrackType::DrumRack {
                render_drum_track(
                    track,
                    sample_rate,
                    bpm,
                    start_beat_offset,
                    total_frames,
                    &mut out_l,
                    &mut out_r,
                );
            } else if track.track_type == TrackType::MidiSynth {
                let default_params = SynthPatchParameters::default();
                render_synth_track(
                    track,
                    &default_params,
                    sample_rate,
                    bpm,
                    start_beat_offset,
                    total_frames,
                    &mut out_l,
                    &mut out_r,
                );
            } else {
                render_audio_track(
                    track,
                    sample_rate,
                    bpm,
                    start_beat_offset,
                    total_frames,
                    &mut out_l,
                    &mut out_r,
                );
            }
        }
    }

    let vol_db = track.mixer.volume_db.clamp(-70.0, 24.0);
    let gain = 10.0f32.powf(vol_db / 20.0);
    let pan = track.mixer.pan.clamp(-1.0, 1.0);
    let l_gain = gain * (0.5 * (1.0 - pan)).sqrt();
    let r_gain = gain * (0.5 * (1.0 + pan)).sqrt();

    for i in 0..total_frames {
        out_l[i] *= l_gain;
        out_r[i] *= r_gain;
    }

    (out_l, out_r)
}

fn render_drum_track(
    track: &TrackDefinition,
    sample_rate: f32,
    bpm: f64,
    start_beat_offset: f64,
    total_frames: usize,
    out_l: &mut [f32],
    out_r: &mut [f32],
) {
    let mut drum_rack = DrumRack::new(sample_rate);
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest_assets = manifest_dir.join("../../assets");
    let candidates = [
        Path::new("assets"),
        Path::new("../assets"),
        Path::new("../../assets"),
        manifest_assets.as_path(),
        Path::new("public/sounds"),
        Path::new("../public/sounds"),
    ];
    for dir in candidates {
        if dir.join("kick.wav").exists() {
            if let Some(dir_str) = dir.to_str() {
                drum_rack.load_default_909_samples(dir_str);
                break;
            }
        }
    }

    let seconds_per_beat = 60.0 / bpm;
    let frames_per_beat = (seconds_per_beat * sample_rate as f64) as usize;
    let frames_per_16th = (frames_per_beat / 4).max(1);

    let mut has_arr_drum_clip = false;
    for clip in &track.arrangement_clips {
        if let ClipPattern::Drum(_) = &clip.pattern {
            has_arr_drum_clip = true;
            break;
        }
    }

    let session_drum_pattern = if !has_arr_drum_clip {
        track.session_clips.iter().flatten().find_map(|c| {
            if let ClipPattern::Drum(dp) = &c.pattern {
                Some(dp)
            } else {
                None
            }
        })
    } else {
        None
    };

    const BLOCK_SIZE: usize = 128;
    let mut frame = 0;
    let mut block_l = [0.0f32; BLOCK_SIZE];
    let mut block_r = [0.0f32; BLOCK_SIZE];

    while frame < total_frames {
        let cur_len = (total_frames - frame).min(BLOCK_SIZE);
        block_l[..cur_len].fill(0.0);
        block_r[..cur_len].fill(0.0);

        for b in 0..cur_len {
            let f = frame + b;
            let current_beat = start_beat_offset + (f as f64 / (frames_per_beat.max(1) as f64));

            if has_arr_drum_clip {
                for clip in &track.arrangement_clips {
                    if clip.contains_beat(current_beat) {
                        if let ClipPattern::Drum(drum_pattern) = &clip.pattern {
                            let phase = clip.pattern_phase_at_beat(current_beat);
                            let step_dur = drum_pattern.step_duration_beats.max(0.01);
                            let step_idx =
                                ((phase / step_dur).floor() as usize) % drum_pattern.step_count.max(1);
                            let step_frame = (f % frames_per_16th.max(1)) == 0;
                            if step_frame {
                                for (pad, step) in drum_pattern.active_steps_at(step_idx) {
                                    drum_rack.trigger_pad(pad, step.velocity);
                                }
                            }
                        }
                    }
                }
            } else if let Some(drum_pattern) = session_drum_pattern {
                let step_dur = drum_pattern.step_duration_beats.max(0.01);
                let step_idx =
                    ((current_beat / step_dur).floor() as usize) % drum_pattern.step_count.max(1);
                let step_frame = (f % frames_per_16th.max(1)) == 0;
                if step_frame {
                    for (pad, step) in drum_pattern.active_steps_at(step_idx) {
                        drum_rack.trigger_pad(pad, step.velocity);
                    }
                }
            } else {
                let beat_frame = f % frames_per_beat.max(1);
                let beat_num = (f / frames_per_beat.max(1)) % 4;
                let sixteenth_frame = f % frames_per_16th;
                let sixteenth_num = (f / frames_per_16th) % 16;

                // 4-on-the-floor Kick (Pad 0)
                if beat_frame == 0 {
                    drum_rack.trigger_pad(0, 120);
                }
                // Snare on beats 2 and 4 (Pad 2)
                if beat_frame == 0 && (beat_num == 1 || beat_num == 3) {
                    drum_rack.trigger_pad(2, 110);
                }
                // 16th Hi-Hats
                if sixteenth_frame == 0 {
                    if sixteenth_num % 4 == 2 {
                        drum_rack.trigger_pad(10, 95);
                    } else {
                        drum_rack.trigger_pad(6, 85);
                    }
                }
            }
        }

        drum_rack.process_block(&mut block_l[..cur_len], &mut block_r[..cur_len]);

        out_l[frame..frame + cur_len].copy_from_slice(&block_l[..cur_len]);
        out_r[frame..frame + cur_len].copy_from_slice(&block_r[..cur_len]);

        frame += cur_len;
    }
}

fn render_synth_track(
    track: &TrackDefinition,
    params: &SynthPatchParameters,
    sample_rate: f32,
    bpm: f64,
    start_beat_offset: f64,
    total_frames: usize,
    out_l: &mut [f32],
    out_r: &mut [f32],
) {
    let mut synth = SynthEngine::new(sample_rate);
    if let Some(preset) = &params.preset_name {
        if !synth.load_preset(preset) {
            let fallback = match preset.as_str() {
                "analogsaw.preset" | "analogsaw" => Some("osc-shapes.preset"),
                "aggro.preset" | "aggro" => Some("distortion.preset"),
                "metallic.preset" | "metallic" => Some("laser.preset"),
                "darkpad.preset" | "darkpad" => Some("strings.preset"),
                _ => None,
            };
            if let Some(fb) = fallback {
                let _ = synth.load_preset(fb);
            }
        }
    } else {
        synth.set_parameter("oscillator_saw_amplitude", params.oscillator_saw_amplitude);
        synth.set_parameter("oscillator_pulse_amplitude", params.oscillator_pulse_amplitude);
        synth.set_parameter("filter_frequency", params.filter_frequency);
        synth.set_parameter("filter_resonance", params.filter_resonance);
        synth.set_parameter("amplitude_envelope_decay", params.amplitude_envelope_decay);
        synth.set_parameter("amplitude_envelope_sustain", params.amplitude_envelope_sustain);
    }

    let seconds_per_beat = 60.0 / bpm;
    let frames_per_beat = (seconds_per_beat * sample_rate as f64) as usize;
    let frames_per_16th = (frames_per_beat / 4).max(1);

    let mut has_arr_midi_clip = false;
    let mut has_arr_midi_notes = false;
    for clip in &track.arrangement_clips {
        if let ClipPattern::Midi(mp) = &clip.pattern {
            has_arr_midi_clip = true;
            if !mp.notes.is_empty() {
                has_arr_midi_notes = true;
                break;
            }
        }
    }

    let session_midi_pattern = if !has_arr_midi_notes {
        track.session_clips.iter().flatten().find_map(|c| {
            if let ClipPattern::Midi(mp) = &c.pattern {
                if !mp.notes.is_empty() {
                    return Some(mp);
                }
            }
            None
        })
    } else {
        None
    };

    let default_notes = [36u8, 38, 41, 43, 36, 48, 46, 43, 36, 39, 41, 44, 36, 43, 41, 38];

    for f in 0..total_frames {
        let current_beat = start_beat_offset + (f as f64 / (frames_per_beat.max(1) as f64));
        let prev_beat = start_beat_offset
            + ((f.saturating_sub(1)) as f64 / (frames_per_beat.max(1) as f64));

        if f % 64 == 0 {
            for lane in &track.automation_lanes {
                if !lane.points.is_empty()
                    && (lane.parameter_name.eq_ignore_ascii_case("filter_frequency")
                        || lane.parameter_name.eq_ignore_ascii_case("Filter Cutoff")
                        || lane.parameter_name.eq_ignore_ascii_case("cutoff"))
                {
                    let val = lane.interpolate_at(current_beat, 0.5);
                    if val <= 1.0 {
                        synth.set_parameter_normalized("filter_frequency", val);
                    } else {
                        synth.set_parameter("filter_frequency", val);
                    }
                }
            }
        }

        if has_arr_midi_notes {
            for clip in &track.arrangement_clips {
                if clip.contains_beat(current_beat) {
                    if let ClipPattern::Midi(midi_pattern) = &clip.pattern {
                        let phase = clip.pattern_phase_at_beat(current_beat);
                        let prev_phase = clip.pattern_phase_at_beat(prev_beat);
                        let wrapped = f == 0 || !clip.contains_beat(prev_beat) || phase < prev_phase;
                        for note in &midi_pattern.notes {
                            let note_start = note.start_beat;
                            let note_end = note.end_beat();

                            if (wrapped && note_start <= phase && phase < note_end)
                                || (!wrapped && prev_phase < note_start && phase >= note_start)
                            {
                                synth.note_on(note.pitch, note.velocity);
                            } else if (!wrapped && prev_phase < note_end && phase >= note_end)
                                || (wrapped && prev_phase < note_end)
                            {
                                synth.note_off(note.pitch);
                            }
                        }
                    }
                }
            }
        } else if let Some(midi_pattern) = session_midi_pattern {
            let pat_len = midi_pattern.length_beats.max(0.25);
            let phase = current_beat % pat_len;
            let prev_phase = prev_beat % pat_len;
            let wrapped = f == 0 || phase < prev_phase;
            for note in &midi_pattern.notes {
                let note_start = note.start_beat;
                let note_end = note.end_beat();

                if (wrapped && note_start <= phase && phase < note_end)
                    || (!wrapped && prev_phase < note_start && phase >= note_start)
                {
                    synth.note_on(note.pitch, note.velocity);
                } else if (!wrapped && prev_phase < note_end && phase >= note_end)
                    || (wrapped && prev_phase < note_end)
                {
                    synth.note_off(note.pitch);
                }
            }
        } else if !has_arr_midi_clip || track.arrangement_clips.len() <= 1 {
            let step_idx = f / frames_per_16th;
            let step_frame = f % frames_per_16th;
            let gate_dur = (frames_per_16th * 3) / 4;

            let note = default_notes[step_idx % default_notes.len()];
            if step_frame == 0 {
                synth.note_on(note, 100);
            } else if step_frame == gate_dur {
                synth.note_off(note);
            }

            let progress = f as f32 / total_frames as f32;
            let sweep_x = 0.15 + 0.70 * (progress * 2.0 * std::f32::consts::PI).sin().abs();
            synth.set_macro_x(sweep_x);
            synth.set_macro_y(0.25 + 0.50 * progress);
        }

        let sample = synth.process_sample();
        out_l[f] = sample * 0.707;
        out_r[f] = sample * 0.707;
    }
}

fn render_audio_track(
    track: &TrackDefinition,
    sample_rate: f32,
    bpm: f64,
    start_beat_offset: f64,
    total_frames: usize,
    out_l: &mut [f32],
    out_r: &mut [f32],
) {
    let mut rendered_any = false;
    let seconds_per_beat = 60.0 / bpm.max(1.0);
    let window_beats = (total_frames as f64 / sample_rate as f64) / seconds_per_beat;
    let window_end_beat = start_beat_offset + window_beats;

    // 1. Render arrangement clips placed on timeline
    for clip in &track.arrangement_clips {
        if let ClipPattern::Audio(data) = &clip.pattern {
            let clip_start = clip.timeline_start_beat;
            let clip_end = clip.timeline_end_beat();
            if clip_end <= start_beat_offset || clip_start >= window_end_beat {
                continue;
            }
            let overlap_start_beat = clip_start.max(start_beat_offset);
            let overlap_end_beat = clip_end.min(window_end_beat);
            let out_start_frame = ((overlap_start_beat - start_beat_offset) * seconds_per_beat * sample_rate as f64).round() as usize;
            let src_start_frame = ((overlap_start_beat - clip_start) * seconds_per_beat * sample_rate as f64).round() as usize;
            let dur_frames = (((overlap_end_beat - overlap_start_beat) * seconds_per_beat * sample_rate as f64).ceil() as usize)
                .min(total_frames.saturating_sub(out_start_frame));

            if !data.samples.is_empty() {
                let avail_frames = (data.samples.len() / 2).saturating_sub(src_start_frame);
                let copy_frames = dur_frames.min(avail_frames);
                for i in 0..copy_frames {
                    out_l[out_start_frame + i] += data.samples[(src_start_frame + i) * 2];
                    out_r[out_start_frame + i] += data.samples[(src_start_frame + i) * 2 + 1];
                }
                if copy_frames > 0 {
                    rendered_any = true;
                }
            } else if let Ok((_sr, ch, samples)) = read_wav_file(&data.file_path) {
                if ch == 1 {
                    let avail_frames = samples.len().saturating_sub(src_start_frame);
                    let copy_frames = dur_frames.min(avail_frames);
                    for i in 0..copy_frames {
                        out_l[out_start_frame + i] += samples[src_start_frame + i];
                        out_r[out_start_frame + i] += samples[src_start_frame + i];
                    }
                    if copy_frames > 0 {
                        rendered_any = true;
                    }
                } else {
                    let ch_usize = (ch as usize).max(2);
                    let avail_frames = (samples.len() / ch_usize).saturating_sub(src_start_frame);
                    let copy_frames = dur_frames.min(avail_frames);
                    for i in 0..copy_frames {
                        out_l[out_start_frame + i] += samples[(src_start_frame + i) * ch_usize];
                        out_r[out_start_frame + i] += samples[(src_start_frame + i) * ch_usize + 1];
                    }
                    if copy_frames > 0 {
                        rendered_any = true;
                    }
                }
            }
        }
    }

    // 2. If track has no arrangement clips at all, try looping session clips
    if !rendered_any && track.arrangement_clips.is_empty() {
        for s_clip in track.session_clips.iter().flatten() {
            if let ClipPattern::Audio(data) = &s_clip.pattern {
                if !data.samples.is_empty() {
                    let avail_frames = data.samples.len() / 2;
                    if avail_frames > 0 {
                        for f in 0..total_frames {
                            let idx = f % avail_frames;
                            out_l[f] += data.samples[idx * 2];
                            out_r[f] += data.samples[idx * 2 + 1];
                        }
                        rendered_any = true;
                        break;
                    }
                } else if let Ok((_sr, ch, samples)) = read_wav_file(&data.file_path) {
                    let ch_usize = (ch as usize).max(1);
                    let avail_frames = samples.len() / ch_usize;
                    if avail_frames > 0 {
                        for f in 0..total_frames {
                            let idx = f % avail_frames;
                            if ch == 1 {
                                out_l[f] += samples[idx];
                                out_r[f] += samples[idx];
                            } else {
                                out_l[f] += samples[idx * ch_usize];
                                out_r[f] += samples[idx * ch_usize + 1];
                            }
                        }
                        rendered_any = true;
                        break;
                    }
                }
            }
        }
    }

    // 3. Fallback subtle test tone only if track is completely empty (no arrangement or session clips)
    if !rendered_any && track.arrangement_clips.is_empty() && track.session_clips.iter().all(|c| c.is_none()) {
        for f in 0..total_frames {
            let t = f as f32 / sample_rate;
            let tone = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.05;
            out_l[f] = tone;
            out_r[f] = tone;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_4_stem_separation_valid_and_finite() {
        let mut buffer = AudioBuffer::new();
        buffer.frame_count = 512;

        let sample_rate = 44100.0f32;
        for i in 0..512 {
            let t = i as f32 / sample_rate;
            // Mixed audio: 60 Hz bass + 1000 Hz center vocal + 6000 Hz stereo synth + transient hit
            let bass_tone = (2.0 * std::f32::consts::PI * 60.0 * t).sin() * 0.4;
            let vocal_tone = (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * 0.3;
            let synth_l = (2.0 * std::f32::consts::PI * 6000.0 * t).sin() * 0.15;
            let synth_r = -(2.0 * std::f32::consts::PI * 6000.0 * t).sin() * 0.15;
            let transient = if i == 50 || i == 51 { 0.8 } else { 0.0 };

            buffer.left[i] = bass_tone + vocal_tone + synth_l + transient;
            buffer.right[i] = bass_tone + vocal_tone + synth_r + transient;
        }

        let res = separate_audio_into_4_stems(&buffer);

        assert_eq!(res.drums.frame_count, 512);
        assert_eq!(res.bass.frame_count, 512);
        assert_eq!(res.vocals.frame_count, 512);
        assert_eq!(res.other.frame_count, 512);

        // All samples must be finite (no NaNs, no Infs)
        for i in 0..512 {
            assert!(res.drums.left[i].is_finite(), "Drum L[{}] is not finite", i);
            assert!(res.drums.right[i].is_finite(), "Drum R[{}] is not finite", i);
            assert!(res.bass.left[i].is_finite(), "Bass L[{}] is not finite", i);
            assert!(res.bass.right[i].is_finite(), "Bass R[{}] is not finite", i);
            assert!(res.vocals.left[i].is_finite(), "Vocal L[{}] is not finite", i);
            assert!(res.vocals.right[i].is_finite(), "Vocal R[{}] is not finite", i);
            assert!(res.other.left[i].is_finite(), "Other L[{}] is not finite", i);
            assert!(res.other.right[i].is_finite(), "Other R[{}] is not finite", i);
        }

        // Each stem has non-empty, non-silent energy
        let rms_d = compute_rms(&res.drums);
        let rms_b = compute_rms(&res.bass);
        let rms_v = compute_rms(&res.vocals);
        let rms_o = compute_rms(&res.other);

        assert!(rms_d > 0.001, "Drums stem should have energy, got {}", rms_d);
        assert!(rms_b > 0.01, "Bass stem should have energy, got {}", rms_b);
        assert!(rms_v > 0.01, "Vocals stem should have energy, got {}", rms_v);
        assert!(rms_o > 0.005, "Other stem should have energy, got {}", rms_o);
    }

    #[test]
    fn test_energy_conservation_and_near_reconstruction() {
        let mut buffer = AudioBuffer::new();
        buffer.frame_count = 1024;

        let sample_rate = 44100.0f32;
        for i in 0..1024 {
            let t = i as f32 / sample_rate;
            let s1 = (2.0 * std::f32::consts::PI * 80.0 * t).sin() * 0.35;
            let s2 = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.25;
            let s3 = (2.0 * std::f32::consts::PI * 1200.0 * t).sin() * 0.20;
            let s4 = (2.0 * std::f32::consts::PI * 8000.0 * t).cos() * 0.15;
            let click = if i % 128 == 0 { 0.5 } else { 0.0 };

            buffer.left[i] = s1 + s2 + s3 + s4 + click;
            buffer.right[i] = s1 + s2 * 0.9 + s3 * 1.1 - s4 + click;
        }

        let orig_rms = compute_rms(&buffer);
        let res = separate_audio_into_4_stems(&buffer);

        // Sum the stems frame by frame
        let mut sum_buffer = AudioBuffer::new();
        sum_buffer.frame_count = 1024;
        for i in 0..1024 {
            sum_buffer.left[i] = res.drums.left[i] + res.bass.left[i] + res.vocals.left[i] + res.other.left[i];
            sum_buffer.right[i] = res.drums.right[i] + res.bass.right[i] + res.vocals.right[i] + res.other.right[i];

            let diff_l = (sum_buffer.left[i] - buffer.left[i]).abs();
            let diff_r = (sum_buffer.right[i] - buffer.right[i]).abs();
            assert!(
                diff_l < 1e-4,
                "Reconstruction sample difference L[{}] = {} exceeds tolerance",
                i,
                diff_l
            );
            assert!(
                diff_r < 1e-4,
                "Reconstruction sample difference R[{}] = {} exceeds tolerance",
                i,
                diff_r
            );
        }

        let sum_rms = compute_rms(&sum_buffer);
        let rms_diff = (sum_rms - orig_rms).abs();
        assert!(
            rms_diff < 1e-4,
            "RMS of sum ({}) must equal RMS of original ({}) within tolerance, diff = {}",
            sum_rms,
            orig_rms,
            rms_diff
        );
    }

    #[test]
    fn test_multi_stem_export_writes_valid_wav_files() {
        let temp_dir = std::env::temp_dir().join(format!("test_stems_{}", std::process::id()));
        let mut project = DAWProject::new("Stem Test Project");

        let mut kick_track = TrackDefinition::new_drum_rack(1, "Kick Track");
        kick_track.mixer.volume_db = -2.0;
        let mut bass_track = TrackDefinition::new_synth(2, "Bass Track");
        bass_track.mixer.volume_db = -3.0;

        project.tracks.push(kick_track);
        project.tracks.push(bass_track);

        let exported = export_all_tracks_as_stems(&project, &temp_dir)
            .expect("Stem export must succeed");

        assert!(exported.len() >= 6, "Expected at least 2 track WAVs + 4 remix stems, got {}", exported.len());

        let expected_names = [
            "track_01_kick_track.wav",
            "track_02_bass_track.wav",
            "remix_drums.wav",
            "remix_bass.wav",
            "remix_vocals.wav",
            "remix_other.wav",
        ];

        for name in &expected_names {
            let path = temp_dir.join(name);
            assert!(path.exists(), "File {:?} must exist on disk", path);

            let (sr, ch, samples) = read_wav_file(&path).expect("File must be valid WAV");
            assert_eq!(sr, 44100);
            assert_eq!(ch, 2);
            assert!(!samples.is_empty());
            for s in &samples {
                assert!(s.is_finite());
                assert!(!s.is_nan());
            }
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_filtfilt_nan_immunity() {
        let lp = BiquadCoeffs::butterworth_lowpass(500.0, 44100.0);
        let mut input = vec![0.5f32; 64];
        input[10] = f32::NAN;
        input[11] = f32::INFINITY;
        input[12] = f32::NEG_INFINITY;
        let mut output = vec![0.0f32; 64];
        lp.filtfilt(&input, &mut output);
        for (i, &s) in output.iter().enumerate() {
            assert!(s.is_finite(), "Output sample at index {} must be finite, got {}", i, s);
        }
    }

    #[test]
    fn test_wav_24bit_and_32bit_nan_sanitization() {
        let temp_dir = std::env::temp_dir().join(format!("test_wav_sanitize_{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();

        // Write a minimal 24-bit PCM WAV (1 channel, 44100 Hz, 2 samples: mid-scale positive, mid-scale negative)
        let wav_24bit_path = temp_dir.join("test_24bit.wav");
        let mut b24 = Vec::new();
        b24.extend_from_slice(b"RIFF");
        b24.extend_from_slice(&(36u32 + 6u32).to_le_bytes());
        b24.extend_from_slice(b"WAVE");
        b24.extend_from_slice(b"fmt ");
        b24.extend_from_slice(&16u32.to_le_bytes());
        b24.extend_from_slice(&1u16.to_le_bytes()); // PCM = 1
        b24.extend_from_slice(&1u16.to_le_bytes()); // channels = 1
        b24.extend_from_slice(&44100u32.to_le_bytes()); // sample_rate
        b24.extend_from_slice(&(44100u32 * 3).to_le_bytes()); // byte_rate
        b24.extend_from_slice(&3u16.to_le_bytes()); // block_align
        b24.extend_from_slice(&24u16.to_le_bytes()); // bits_per_sample
        b24.extend_from_slice(b"data");
        b24.extend_from_slice(&6u32.to_le_bytes()); // 2 samples * 3 bytes
        // Sample 1: 0x400000 -> 4,194,304 / 8,388,608 = 0.5
        b24.push(0x00);
        b24.push(0x00);
        b24.push(0x40);
        // Sample 2: -4,194,304 (0xC00000) -> -0.5
        b24.push(0x00);
        b24.push(0x00);
        b24.push(0xC0);
        fs::write(&wav_24bit_path, &b24).unwrap();

        let (sr, ch, samples) = read_wav_file(&wav_24bit_path).unwrap();
        assert_eq!(sr, 44100);
        assert_eq!(ch, 1);
        assert_eq!(samples.len(), 2);
        assert!((samples[0] - 0.5).abs() < 1e-4);
        assert!((samples[1] - (-0.5)).abs() < 1e-4);

        // Write a 32-bit float WAV with NaN and Inf
        let wav_32bit_path = temp_dir.join("test_32bit_nan.wav");
        let mut b32 = Vec::new();
        b32.extend_from_slice(b"RIFF");
        b32.extend_from_slice(&(36u32 + 16u32).to_le_bytes());
        b32.extend_from_slice(b"WAVE");
        b32.extend_from_slice(b"fmt ");
        b32.extend_from_slice(&16u32.to_le_bytes());
        b32.extend_from_slice(&3u16.to_le_bytes()); // IEEE Float = 3
        b32.extend_from_slice(&2u16.to_le_bytes()); // channels = 2
        b32.extend_from_slice(&44100u32.to_le_bytes()); // sample_rate
        b32.extend_from_slice(&(44100u32 * 8).to_le_bytes()); // byte_rate
        b32.extend_from_slice(&8u16.to_le_bytes()); // block_align
        b32.extend_from_slice(&32u16.to_le_bytes()); // bits_per_sample
        b32.extend_from_slice(b"data");
        b32.extend_from_slice(&16u32.to_le_bytes()); // 4 floats * 4 bytes
        b32.extend_from_slice(&0.75f32.to_le_bytes());
        b32.extend_from_slice(&f32::NAN.to_le_bytes());
        b32.extend_from_slice(&f32::INFINITY.to_le_bytes());
        b32.extend_from_slice(&(-25.0f32).to_le_bytes());
        fs::write(&wav_32bit_path, &b32).unwrap();

        let (_sr, _ch, float_samples) = read_wav_file(&wav_32bit_path).unwrap();
        assert_eq!(float_samples.len(), 4);
        assert!((float_samples[0] - 0.75).abs() < 1e-4);
        assert_eq!(float_samples[1], 0.0, "NaN must be sanitized to 0.0");
        assert_eq!(float_samples[2], 0.0, "Infinity must be sanitized to 0.0");
        assert_eq!(float_samples[3], -16.0, "Excessive float must be clamped to -16.0");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    fn compute_rms(buf: &AudioBuffer) -> f32 {
        if buf.frame_count == 0 {
            return 0.0;
        }
        let mut sum_sq = 0.0f64;
        for i in 0..buf.frame_count {
            sum_sq += (buf.left[i] * buf.left[i]) as f64;
            sum_sq += (buf.right[i] * buf.right[i]) as f64;
        }
        ((sum_sq / (buf.frame_count * 2) as f64).sqrt()) as f32
    }
}

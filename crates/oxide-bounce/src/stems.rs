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

use oxide_core::{AudioBuffer, AudioEffectDevice, MusicalTime};
use oxide_drums::DrumRack;
use oxide_dsp::{ParameterBank, SynthEngine};
use oxide_fx::{
    AutoFilter, BeatRepeat, ChorusEnsemble, Compressor, ConvolutionReverb, Corpus, EQEight, Echo,
    Erosion, FdnReverb, GlueCompressor, HybridReverb, MasterLimiter, MultibandDynamics,
    PhaserFlanger, Redux, Roar, Saturator, SpectralResonator, TempoDelay, Utility,
    MAX_DELAY_SAMPLES,
};
use oxide_project::{
    DAWProject, DrumRackDefinition, InsertEffectConfig, InstrumentConfig, MasterTrackDefinition,
    SynthPatchParameters, TrackDefinition, TrackType,
};
use oxide_timeline::{ClipPattern, DrumStepPattern, MidiPattern};

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
            let mut audio_format = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap());
            channels = u16::from_le_bytes(bytes[pos + 2..pos + 4].try_into().unwrap());
            sample_rate = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap());
            bits_per_sample = u16::from_le_bytes(bytes[pos + 14..pos + 16].try_into().unwrap());
            if audio_format == 0xFFFE && chunk_size >= 40 && pos + 26 <= bytes.len() {
                audio_format = u16::from_le_bytes(bytes[pos + 24..pos + 26].try_into().unwrap());
            }
            is_float = audio_format == 3;
            pos += chunk_size + (chunk_size & 1);
        } else if chunk_id == b"data" {
            data_offset = pos;
            data_len = chunk_size.min(bytes.len().saturating_sub(pos));
            break;
        } else {
            pos += chunk_size + (chunk_size & 1);
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
    } else if !is_float && bits_per_sample == 32 {
        let count = data_len / 4;
        samples.reserve(count);
        for i in 0..count {
            let b = &raw_data[i * 4..(i + 1) * 4];
            let val = i32::from_le_bytes(b.try_into().unwrap());
            let s = val as f32 / 2_147_483_648.0;
            samples.push(if s.is_finite() { s } else { 0.0 });
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

fn resolve_asset_or_file(path_str: &str) -> Option<PathBuf> {
    if path_str.is_empty() {
        return None;
    }
    let p = Path::new(path_str);
    if p.exists() {
        return Some(p.to_path_buf());
    }
    if p.is_relative() {
        let workspace_rel = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(p);
        if workspace_rel.exists() {
            return Some(workspace_rel);
        }
    }
    None
}

fn db_to_linear_gain(db: f32) -> f32 {
    if !db.is_finite() || db <= -70.0 {
        0.0
    } else {
        10.0_f32.powf(db.clamp(-70.0, 24.0) / 20.0)
    }
}

fn apply_gain_and_pan(left: &mut [f32], right: &mut [f32], volume_db: f32, pan: f32) {
    let gain = db_to_linear_gain(volume_db);
    let p = pan.clamp(-1.0, 1.0);
    // Stereo balance law: unity (1.0, 1.0) at center pan == 0.0, attenuates opposite side when panned.
    let l_gain = gain * (1.0 - p.max(0.0)).sqrt();
    let r_gain = gain * (1.0 + p.min(0.0)).sqrt();
    for (l, r) in left.iter_mut().zip(right.iter_mut()) {
        *l *= l_gain;
        *r *= r_gain;
    }
}

fn compute_swing_delay_beats(swing_ratio: f32, step_dur_beats: f64) -> f64 {
    if !swing_ratio.is_finite() || swing_ratio <= 0.0 || (swing_ratio - 0.5).abs() < 1e-4 {
        0.0
    } else if swing_ratio > 0.5 {
        let norm = ((swing_ratio.min(0.75) - 0.5) * 2.0) as f64;
        norm * step_dur_beats
    } else {
        (swing_ratio as f64 * 0.5) * step_dur_beats
    }
}

fn beat_to_frame_offset(beat: f64, start_beat_offset: f64, bpm: f64, sample_rate: f32) -> isize {
    ((beat - start_beat_offset) * (60.0 / bpm) * sample_rate as f64).round() as isize
}

fn deterministic_note_prob_passes(seed_a: u32, seed_b: u64, seed_c: usize, prob: f32) -> bool {
    if prob >= 0.999 {
        return true;
    }
    if prob <= 0.0 {
        return false;
    }
    let mut x = seed_a
        .wrapping_mul(0x9E37_79B9)
        .wrapping_add((seed_b as u32).wrapping_mul(0x85EB_CA6B))
        .wrapping_add((seed_c as u32).wrapping_mul(0xC2B2_AE35))
        ^ 0x27D4_EB2D;
    x ^= x >> 15;
    x = x.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 13;
    let u = (x as f32) / (u32::MAX as f32);
    u <= prob
}

fn run_audio_effect_device<D: AudioEffectDevice>(
    dev: &mut D,
    left: &mut [f32],
    right: &mut [f32],
    bpm: f64,
    start_beat_offset: f64,
    sample_rate: f32,
) {
    let total_frames = left.len().min(right.len());
    let mut buf = AudioBuffer::new();
    let mut time = MusicalTime::new(start_beat_offset, bpm, 4, 4, true);
    let mut pos = 0;
    while pos < total_frames {
        let count = (total_frames - pos).min(1024);
        buf.frame_count = count;
        buf.left[..count].copy_from_slice(&left[pos..pos + count]);
        buf.right[..count].copy_from_slice(&right[pos..pos + count]);
        dev.process_audio(&mut buf, &time);
        left[pos..pos + count].copy_from_slice(&buf.left[..count]);
        right[pos..pos + count].copy_from_slice(&buf.right[..count]);
        time.advance_frames(count, sample_rate as f64);
        pos += count;
    }
}

fn configure_eq8_from_params(eq: &mut EQEight, params: &std::collections::HashMap<String, f32>, sample_rate: f32) {
    for (key, &val) in params {
        let k = key.to_ascii_lowercase();
        match k.as_str() {
            "low_gain" | "low_gain_db" => eq.set_band_gain(1, val),
            "mid_gain" | "mid_gain_db" => eq.set_band_gain(3, val),
            "high_gain" | "high_gain_db" => eq.set_band_gain(6, val),
            "highpass_freq" | "lowcut_freq" => eq.set_band_freq(0, val),
            "lowpass_freq" | "highcut_freq" => eq.set_band_freq(7, val),
            _ => {
                for b in 0..8 {
                    let prefix_us = format!("band_{b}_");
                    let prefix_plain = format!("band{b}_");
                    if let Some(sub) = k.strip_prefix(&prefix_us).or_else(|| k.strip_prefix(&prefix_plain)) {
                        match sub {
                            "gain" | "gain_db" => eq.set_band_gain(b, val),
                            "freq" | "freq_hz" => eq.set_band_freq(b, val),
                            "q" => {
                                eq.bands[b].q = val.clamp(0.05, 30.0);
                                eq.bands[b].recompute(sample_rate);
                            }
                            "enabled" => eq.bands[b].enabled = val >= 0.5,
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

fn process_generic_device(
    device_type: &str,
    params: &std::collections::HashMap<String, f32>,
    left: &mut [f32],
    right: &mut [f32],
    sample_rate: f32,
    bpm: f64,
    start_beat_offset: f64,
) {
    let clean = device_type.trim().to_ascii_lowercase().replace([' ', '-', '_'], "");
    let get_p = |names: &[&str]| -> Option<f32> {
        for name in names {
            if let Some(&v) = params.get(*name) {
                return Some(v);
            }
        }
        None
    };

    match clean.as_str() {
        "eqeight" | "eq8" => {
            let mut eq = EQEight::new(sample_rate);
            configure_eq8_from_params(&mut eq, params, sample_rate);
            run_audio_effect_device(&mut eq, left, right, bpm, start_beat_offset, sample_rate);
        }
        "gluecompressor" | "glue" => {
            let mut dev = GlueCompressor::new(sample_rate);
            if let Some(v) = get_p(&["threshold_db", "threshold"]) { dev.threshold_db = v; }
            if let Some(v) = get_p(&["ratio"]) { dev.ratio = v.max(1.0); }
            if let Some(v) = get_p(&["attack_ms", "attack"]) { dev.attack_ms = v.max(0.01); }
            if let Some(v) = get_p(&["release_s", "release"]) { dev.release_s = v; }
            if let Some(v) = get_p(&["makeup_gain_db", "makeup"]) { dev.makeup_gain_db = v; }
            if let Some(v) = get_p(&["dry_wet", "mix", "wet"]) { dev.dry_wet = v.clamp(0.0, 1.0); }
            run_audio_effect_device(&mut dev, left, right, bpm, start_beat_offset, sample_rate);
        }
        "roar" => {
            let mut dev = Roar::new(sample_rate);
            if let Some(v) = get_p(&["drive_db", "drive"]) { dev.stage1.drive_db = v; }
            if let Some(v) = get_p(&["tone"]) { dev.stage1.tone = v.clamp(-1.0, 1.0); }
            if let Some(v) = get_p(&["dry_wet", "mix", "wet"]) { dev.dry_wet = v.clamp(0.0, 1.0); }
            if let Some(v) = get_p(&["output_gain_db", "output_gain"]) { dev.output_gain_db = v; }
            run_audio_effect_device(&mut dev, left, right, bpm, start_beat_offset, sample_rate);
        }
        "autofilter" => {
            let mut dev = AutoFilter::new(sample_rate);
            if let Some(v) = get_p(&["cutoff_hz", "cutoff", "frequency"]) { dev.cutoff_hz = v.clamp(20.0, 20000.0); }
            if let Some(v) = get_p(&["resonance"]) { dev.resonance = v.clamp(0.0, 1.0); }
            if let Some(v) = get_p(&["dry_wet", "mix", "wet"]) { dev.dry_wet = v.clamp(0.0, 1.0); }
            run_audio_effect_device(&mut dev, left, right, bpm, start_beat_offset, sample_rate);
        }
        "compressor" => {
            let mut dev = Compressor::new(sample_rate);
            if let Some(v) = get_p(&["threshold_db", "threshold"]) { dev.threshold_db = v; }
            if let Some(v) = get_p(&["ratio"]) { dev.ratio = v.max(1.0); }
            if let Some(v) = get_p(&["attack_ms", "attack"]) { dev.attack_ms = v.max(0.1); }
            if let Some(v) = get_p(&["release_ms", "release"]) { dev.release_ms = v.max(1.0); }
            if let Some(v) = get_p(&["makeup_gain_db", "makeup"]) { dev.makeup_gain_db = v; }
            if let Some(v) = get_p(&["dry_wet", "mix", "wet"]) { dev.dry_wet = v.clamp(0.0, 1.0); }
            run_audio_effect_device(&mut dev, left, right, bpm, start_beat_offset, sample_rate);
        }
        "ott" | "multibanddynamics" => {
            let mut dev = MultibandDynamics::new(sample_rate);
            if let Some(v) = get_p(&["amount", "dry_wet", "mix"]) { dev.amount = v.clamp(0.0, 1.0); }
            if let Some(v) = get_p(&["time_macro", "time"]) { dev.time_macro = v.clamp(0.1, 10.0); }
            if let Some(v) = get_p(&["input_gain_db"]) { dev.input_gain_db = v; }
            if let Some(v) = get_p(&["output_gain_db"]) { dev.output_gain_db = v; }
            run_audio_effect_device(&mut dev, left, right, bpm, start_beat_offset, sample_rate);
        }
        "saturator" => {
            let mut dev = Saturator::new();
            if let Some(v) = get_p(&["drive_db", "drive"]) { dev.set_drive_db(v); }
            if let Some(v) = get_p(&["output_gain_db", "output_gain"]) { dev.set_output_gain_db(v); }
            if let Some(v) = get_p(&["wet", "dry_wet", "mix"]) { dev.wet = v.clamp(0.0, 1.0); }
            dev.process_block(left, right);
        }
        "delay" | "tempodelay" => {
            let mut dev = TempoDelay::new(sample_rate, bpm);
            if let Some(tb) = get_p(&["time_beats", "time"]) {
                let samples = ((tb.max(0.01) as f64) * (60.0 / bpm) * sample_rate as f64).round() as usize;
                let clamped = samples.clamp(1, MAX_DELAY_SAMPLES - 1);
                dev.delay_samples_l = clamped;
                dev.delay_samples_r = clamped;
            }
            if let Some(v) = get_p(&["feedback"]) { dev.feedback = v.clamp(0.0, 0.98); }
            if let Some(v) = get_p(&["mix", "wet", "dry_wet"]) { dev.wet = v.clamp(0.0, 1.0); }
            dev.process_block(left, right);
        }
        "reverb" | "fdnreverb" => {
            let mut dev = FdnReverb::new(sample_rate);
            if let Some(decay_ms) = get_p(&["decay_ms"]) {
                dev.decay = 0.001_f32.powf(0.045 / (decay_ms.max(50.0) / 1000.0)).clamp(0.1, 0.96);
            } else if let Some(d) = get_p(&["decay"]) {
                dev.decay = d.clamp(0.1, 0.97);
            }
            if let Some(v) = get_p(&["room_size"]) { dev.room_size = v.clamp(0.1, 2.0); }
            if let Some(v) = get_p(&["damping"]) { dev.damping = v.clamp(0.0, 0.95); }
            if let Some(v) = get_p(&["mix", "wet", "dry_wet"]) { dev.wet = v.clamp(0.0, 1.0); }
            dev.process_block(left, right);
        }
        "hybridreverb" => {
            let mut dev = HybridReverb::new(sample_rate);
            if let Some(v) = get_p(&["decay_s", "decay"]) { dev.decay_s = v.max(0.1); }
            if let Some(v) = get_p(&["size", "room_size"]) { dev.size = v.clamp(0.2, 2.0); }
            if let Some(v) = get_p(&["damping"]) { dev.damping = v.clamp(0.0, 1.0); }
            if let Some(v) = get_p(&["dry_wet", "mix", "wet"]) { dev.dry_wet = v.clamp(0.0, 1.0); }
            run_audio_effect_device(&mut dev, left, right, bpm, start_beat_offset, sample_rate);
        }
        "echo" => {
            let mut dev = Echo::new(sample_rate);
            if let Some(v) = get_p(&["delay_time_l_ms", "time_ms"]) { dev.delay_time_l_ms = v.max(1.0); }
            if let Some(v) = get_p(&["delay_time_r_ms", "time_ms"]) { dev.delay_time_r_ms = v.max(1.0); }
            if let Some(v) = get_p(&["feedback"]) { dev.feedback = v.clamp(0.0, 0.98); }
            if let Some(v) = get_p(&["dry_wet", "mix", "wet"]) { dev.dry_wet = v.clamp(0.0, 1.0); }
            run_audio_effect_device(&mut dev, left, right, bpm, start_beat_offset, sample_rate);
        }
        "spectralresonator" => {
            let mut dev = SpectralResonator::new(sample_rate);
            if let Some(v) = get_p(&["root_hz", "frequency"]) { dev.root_hz = v.clamp(20.0, 8000.0); }
            if let Some(v) = get_p(&["decay_s", "decay"]) { dev.decay_s = v.max(0.05); }
            if let Some(v) = get_p(&["dry_wet", "mix", "wet"]) { dev.dry_wet = v.clamp(0.0, 1.0); }
            run_audio_effect_device(&mut dev, left, right, bpm, start_beat_offset, sample_rate);
        }
        "corpus" => {
            let mut dev = Corpus::new(sample_rate);
            if let Some(v) = get_p(&["tune_hz", "frequency"]) { dev.tune_hz = v.clamp(20.0, 8000.0); }
            if let Some(v) = get_p(&["decay_s", "decay"]) { dev.decay_s = v.max(0.01); }
            if let Some(v) = get_p(&["dry_wet", "mix", "wet"]) { dev.dry_wet = v.clamp(0.0, 1.0); }
            run_audio_effect_device(&mut dev, left, right, bpm, start_beat_offset, sample_rate);
        }
        "beatrepeat" => {
            let mut dev = BeatRepeat::new(sample_rate);
            if let Some(v) = get_p(&["dry_wet", "mix", "wet"]) { dev.dry_wet = v.clamp(0.0, 1.0); }
            run_audio_effect_device(&mut dev, left, right, bpm, start_beat_offset, sample_rate);
        }
        "chorus" | "chorusensemble" => {
            let mut dev = ChorusEnsemble::new(sample_rate);
            if let Some(v) = get_p(&["rate_hz", "rate"]) { dev.rate_hz = v.max(0.01); }
            if let Some(v) = get_p(&["depth"]) { dev.depth = v.clamp(0.0, 1.0); }
            if let Some(v) = get_p(&["dry_wet", "mix", "wet"]) { dev.dry_wet = v.clamp(0.0, 1.0); }
            run_audio_effect_device(&mut dev, left, right, bpm, start_beat_offset, sample_rate);
        }
        "phaser" | "phaserflanger" => {
            let mut dev = PhaserFlanger::new(sample_rate);
            if let Some(v) = get_p(&["rate_hz", "rate"]) { dev.rate_hz = v.max(0.01); }
            if let Some(v) = get_p(&["depth"]) { dev.depth = v.clamp(0.0, 1.0); }
            if let Some(v) = get_p(&["dry_wet", "mix", "wet"]) { dev.dry_wet = v.clamp(0.0, 1.0); }
            run_audio_effect_device(&mut dev, left, right, bpm, start_beat_offset, sample_rate);
        }
        "erosion" => {
            let mut dev = Erosion::new(sample_rate);
            if let Some(v) = get_p(&["frequency_hz", "frequency"]) { dev.frequency_hz = v.clamp(20.0, 18000.0); }
            if let Some(v) = get_p(&["amount", "dry_wet", "mix"]) { dev.amount = v.clamp(0.0, 1.0); }
            run_audio_effect_device(&mut dev, left, right, bpm, start_beat_offset, sample_rate);
        }
        "redux" => {
            let mut dev = Redux::new();
            if let Some(v) = get_p(&["downsample"]) { dev.downsample = (v.round() as usize).clamp(1, 256); }
            if let Some(v) = get_p(&["bit_depth"]) { dev.bit_depth = (v.round() as u8).clamp(1, 16); }
            if let Some(v) = get_p(&["dry_wet", "mix", "wet"]) { dev.dry_wet = v.clamp(0.0, 1.0); }
            run_audio_effect_device(&mut dev, left, right, bpm, start_beat_offset, sample_rate);
        }
        "utility" => {
            let mut dev = Utility::new(sample_rate);
            if let Some(v) = get_p(&["gain_db", "gain"]) { dev.gain_db = v; }
            if let Some(v) = get_p(&["pan"]) { dev.pan = v.clamp(-1.0, 1.0); }
            if let Some(v) = get_p(&["width"]) { dev.width = v.clamp(0.0, 4.0); }
            run_audio_effect_device(&mut dev, left, right, bpm, start_beat_offset, sample_rate);
        }
        "limiter" | "masterlimiter" => {
            let mut dev = MasterLimiter::new(sample_rate);
            if let Some(v) = get_p(&["ceiling_db", "ceiling"]) {
                dev.ceiling = 10.0_f32.powf(v.clamp(-24.0, 0.0) / 20.0);
            }
            dev.process_block(left, right);
        }
        _ => {}
    }
}

fn process_insert_fx_chain(
    insert_fx: &[InsertEffectConfig],
    left: &mut [f32],
    right: &mut [f32],
    sample_rate: f32,
    bpm: f64,
    start_beat_offset: f64,
) {
    for fx in insert_fx {
        match fx {
            InsertEffectConfig::Saturator {
                drive_db,
                curve: _,
                output_gain_db,
            } => {
                let mut sat = Saturator::new();
                sat.set_drive_db(*drive_db);
                sat.set_output_gain_db(*output_gain_db);
                sat.process_block(left, right);
            }
            InsertEffectConfig::TempoDelay {
                time_beats,
                feedback,
                mix,
            } => {
                let mut delay = TempoDelay::new(sample_rate, bpm);
                let delay_samples =
                    (((*time_beats).max(0.01) as f64) * (60.0 / bpm) * sample_rate as f64).round()
                        as usize;
                let clamped = delay_samples.clamp(1, MAX_DELAY_SAMPLES - 1);
                delay.delay_samples_l = clamped;
                delay.delay_samples_r = clamped;
                delay.feedback = feedback.clamp(0.0, 0.98);
                delay.wet = mix.clamp(0.0, 1.0);
                delay.process_block(left, right);
            }
            InsertEffectConfig::Reverb {
                ir_path,
                decay_ms,
                mix,
            } => {
                let wet = mix.clamp(0.0, 1.0);
                let mut used_conv = false;
                if let Some(path_str) = ir_path {
                    if let Some(resolved) = resolve_asset_or_file(path_str) {
                        if let Ok(mut conv) = ConvolutionReverb::load_from_file(&resolved) {
                            conv.wet = wet;
                            conv.process_block(left, right);
                            used_conv = true;
                        }
                    }
                }
                if !used_conv {
                    let mut rev = FdnReverb::new(sample_rate);
                    rev.decay = 0.001_f32
                        .powf(0.045 / (decay_ms.max(50.0) / 1000.0))
                        .clamp(0.1, 0.96);
                    rev.wet = wet;
                    rev.process_block(left, right);
                }
            }
            InsertEffectConfig::EqEight { params } => {
                let mut eq = EQEight::new(sample_rate);
                configure_eq8_from_params(&mut eq, params, sample_rate);
                run_audio_effect_device(&mut eq, left, right, bpm, start_beat_offset, sample_rate);
            }
            InsertEffectConfig::GlueCompressor {
                threshold_db,
                ratio,
                attack_ms,
                release_s,
                makeup_gain_db,
                dry_wet,
            } => {
                let mut glue = GlueCompressor::new(sample_rate);
                glue.threshold_db = *threshold_db;
                glue.ratio = ratio.max(1.0);
                glue.attack_ms = attack_ms.max(0.01);
                glue.release_s = *release_s;
                glue.makeup_gain_db = *makeup_gain_db;
                glue.dry_wet = dry_wet.clamp(0.0, 1.0);
                run_audio_effect_device(&mut glue, left, right, bpm, start_beat_offset, sample_rate);
            }
            InsertEffectConfig::Roar {
                drive_db,
                tone,
                dry_wet,
                output_gain_db,
            } => {
                let mut roar = Roar::new(sample_rate);
                roar.stage1.drive_db = *drive_db;
                roar.stage1.tone = tone.clamp(-1.0, 1.0);
                roar.dry_wet = dry_wet.clamp(0.0, 1.0);
                roar.output_gain_db = *output_gain_db;
                run_audio_effect_device(&mut roar, left, right, bpm, start_beat_offset, sample_rate);
            }
            InsertEffectConfig::AutoFilter {
                cutoff_hz,
                resonance,
                dry_wet,
            } => {
                let mut af = AutoFilter::new(sample_rate);
                af.cutoff_hz = cutoff_hz.clamp(20.0, 20000.0);
                af.resonance = resonance.clamp(0.0, 1.0);
                af.dry_wet = dry_wet.clamp(0.0, 1.0);
                run_audio_effect_device(&mut af, left, right, bpm, start_beat_offset, sample_rate);
            }
            InsertEffectConfig::Compressor {
                threshold_db,
                ratio,
                attack_ms,
                release_ms,
                makeup_gain_db,
                dry_wet,
            } => {
                let mut comp = Compressor::new(sample_rate);
                comp.threshold_db = *threshold_db;
                comp.ratio = ratio.max(1.0);
                comp.attack_ms = attack_ms.max(0.1);
                comp.release_ms = release_ms.max(1.0);
                comp.makeup_gain_db = *makeup_gain_db;
                comp.dry_wet = dry_wet.clamp(0.0, 1.0);
                run_audio_effect_device(&mut comp, left, right, bpm, start_beat_offset, sample_rate);
            }
            InsertEffectConfig::MultibandDynamics {
                amount,
                time_macro,
                input_gain_db,
                output_gain_db,
            } => {
                let mut ott = MultibandDynamics::new(sample_rate);
                ott.amount = amount.clamp(0.0, 1.0);
                ott.time_macro = time_macro.clamp(0.1, 10.0);
                ott.input_gain_db = *input_gain_db;
                ott.output_gain_db = *output_gain_db;
                run_audio_effect_device(&mut ott, left, right, bpm, start_beat_offset, sample_rate);
            }
            InsertEffectConfig::GenericDevice {
                device_type,
                params,
            } => {
                process_generic_device(
                    device_type,
                    params,
                    left,
                    right,
                    sample_rate,
                    bpm,
                    start_beat_offset,
                );
            }
        }
    }
}

pub(crate) fn apply_master_fx_and_limiter(
    master_track: &MasterTrackDefinition,
    master_l: &mut [f32],
    master_r: &mut [f32],
    sample_rate: f32,
    bpm: f64,
) {
    if master_track.saturator_enabled {
        let mut sat = Saturator::new();
        sat.set_drive_db(master_track.saturator_drive_db);
        sat.set_output_gain_db(master_track.saturator_output_db);
        sat.process_block(master_l, master_r);
    }
    if master_track.delay_mix > 0.0 {
        let mut delay = TempoDelay::new(sample_rate, bpm);
        let time_beats = if master_track.delay_time_beats > 0.0 {
            master_track.delay_time_beats as f64
        } else {
            0.5
        };
        let delay_samples = (time_beats * (60.0 / bpm) * sample_rate as f64).round() as usize;
        let clamped = delay_samples.clamp(1, MAX_DELAY_SAMPLES - 1);
        delay.delay_samples_l = clamped;
        delay.delay_samples_r = clamped;
        delay.feedback = master_track.delay_feedback.clamp(0.0, 0.98);
        delay.wet = master_track.delay_mix.clamp(0.0, 1.0);
        delay.process_block(master_l, master_r);
    }
    if master_track.reverb_mix > 0.0 {
        let mut rev = FdnReverb::new(sample_rate);
        let decay_ms = if master_track.reverb_decay_ms > 0.0 {
            master_track.reverb_decay_ms
        } else {
            1500.0
        };
        rev.decay = 0.001_f32
            .powf(0.045 / (decay_ms.max(50.0) / 1000.0))
            .clamp(0.1, 0.96);
        rev.wet = master_track.reverb_mix.clamp(0.0, 1.0);
        rev.process_block(master_l, master_r);
    }

    apply_gain_and_pan(master_l, master_r, master_track.volume_db, master_track.pan);

    let mut limiter = MasterLimiter::new(sample_rate);
    limiter.ceiling = 10.0_f32.powf(master_track.limiter_ceiling_db.clamp(-24.0, 0.0) / 20.0);
    const BLOCK_SIZE: usize = 128;
    let total_frames = master_l.len().min(master_r.len());
    let mut frame = 0;
    while frame < total_frames {
        let cur_block = (total_frames - frame).min(BLOCK_SIZE);
        limiter.process_block(
            &mut master_l[frame..frame + cur_block],
            &mut master_r[frame..frame + cur_block],
        );
        frame += cur_block;
    }
}

pub(crate) fn render_project_mix_with_stems(
    project: &DAWProject,
    sample_rate: f32,
    bpm: f64,
    start_beat_offset: f64,
    duration_seconds: f64,
    collect_all_stems: bool,
) -> (Vec<f32>, Vec<f32>, Vec<(Vec<f32>, Vec<f32>)>) {
    let total_frames = (duration_seconds * sample_rate as f64).ceil() as usize;
    let mut master_l = vec![0.0f32; total_frames];
    let mut master_r = vec![0.0f32; total_frames];
    let mut per_track_stems = if collect_all_stems {
        vec![(Vec::new(), Vec::new()); project.tracks.len()]
    } else {
        Vec::new()
    };

    let any_solo = project.tracks.iter().any(|t| t.mixer.solo);
    let return_indices: Vec<usize> = project
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| t.track_type == TrackType::Return)
        .map(|(i, _)| i)
        .collect();
    let mut return_buses: Vec<(Vec<f32>, Vec<f32>)> = (0..return_indices.len())
        .map(|_| (vec![0.0f32; total_frames], vec![0.0f32; total_frames]))
        .collect();

    // 1. Render all non-Return tracks, mix into Master and accumulate Sends into Return buses
    for (idx, track) in project.tracks.iter().enumerate() {
        if track.track_type == TrackType::Return {
            continue;
        }
        let is_active_in_mix = !track.mixer.mute && (!any_solo || track.mixer.solo);

        if is_active_in_mix || collect_all_stems {
            let (trk_l, trk_r) = if track.mixer.mute {
                (vec![0.0f32; total_frames], vec![0.0f32; total_frames])
            } else {
                render_single_track_window_with_swing(
                    track,
                    sample_rate,
                    bpm,
                    project.transport.swing_16th_ratio,
                    start_beat_offset,
                    duration_seconds,
                )
            };

            if is_active_in_mix {
                for i in 0..total_frames {
                    master_l[i] += trk_l[i];
                    master_r[i] += trk_r[i];
                }
                for (send_idx, (ret_l, ret_r)) in return_buses.iter_mut().enumerate() {
                    let send_db = track.mixer.get_send_db(send_idx);
                    if send_db > -70.0 {
                        let send_gain = 10.0_f32.powf(send_db.clamp(-70.0, 6.0) / 20.0);
                        for i in 0..total_frames {
                            ret_l[i] += trk_l[i] * send_gain;
                            ret_r[i] += trk_r[i] * send_gain;
                        }
                    }
                }
            }

            if collect_all_stems {
                per_track_stems[idx] = (trk_l, trk_r);
            }
        }
    }

    // 2. Process Return tracks (Insert FX -> Volume/Pan -> Master)
    for (ret_slot, &track_idx) in return_indices.iter().enumerate() {
        let track = &project.tracks[track_idx];
        let (mut ret_l, mut ret_r) = std::mem::take(&mut return_buses[ret_slot]);

        if track.mixer.mute {
            ret_l.fill(0.0);
            ret_r.fill(0.0);
        } else {
            process_insert_fx_chain(
                &track.insert_fx,
                &mut ret_l,
                &mut ret_r,
                sample_rate,
                bpm,
                start_beat_offset,
            );
            apply_gain_and_pan(&mut ret_l, &mut ret_r, track.mixer.volume_db, track.mixer.pan);
            for i in 0..total_frames {
                master_l[i] += ret_l[i];
                master_r[i] += ret_r[i];
            }
        }

        if collect_all_stems {
            per_track_stems[track_idx] = (ret_l, ret_r);
        }
    }

    // 3. Apply Master FX, Volume/Pan, and Brickwall Limiter
    apply_master_fx_and_limiter(
        &project.master_track,
        &mut master_l,
        &mut master_r,
        sample_rate,
        bpm,
    );

    (master_l, master_r, per_track_stems)
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

    let (mut master_l, mut master_r) = if project.tracks.is_empty() {
        let mut ml = vec![0.0f32; total_frames];
        let mut mr = vec![0.0f32; total_frames];
        let renderer = DemoArrangementRenderer::new(sample_rate as f32, bpm);
        let interleaved = renderer.render_demo_seconds(duration_seconds);
        let frames = interleaved.len() / 2;
        for i in 0..frames.min(total_frames) {
            ml[i] = interleaved[i * 2];
            mr[i] = interleaved[i * 2 + 1];
        }

        let demo_path = out_dir.join("track_01_master.wav");
        WavWriter::write_wav_16bit(&demo_path, sample_rate, 2, &interleaved)
            .map_err(BounceError::Render)?;
        exported_paths.push(demo_path);

        apply_master_fx_and_limiter(&project.master_track, &mut ml, &mut mr, sample_rate as f32, bpm);
        (ml, mr)
    } else {
        let (ml, mr, per_track_stems) = render_project_mix_with_stems(
            project,
            sample_rate as f32,
            bpm,
            0.0,
            duration_seconds,
            true,
        );

        for (idx, (track, (trk_l, trk_r))) in
            project.tracks.iter().zip(per_track_stems.iter()).enumerate()
        {
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

        (ml, mr)
    };

    let mut master_interleaved = Vec::with_capacity(total_frames * 2);
    for i in 0..total_frames {
        master_interleaved.push(master_l[i]);
        master_interleaved.push(master_r[i]);
    }
    let _ = (&mut master_l, &mut master_r);

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
    render_single_track_window_with_swing(
        track,
        sample_rate,
        bpm,
        0.0,
        start_beat_offset,
        duration_seconds,
    )
}

pub fn render_single_track_window_with_swing(
    track: &TrackDefinition,
    sample_rate: f32,
    bpm: f64,
    swing_16th_ratio: f32,
    start_beat_offset: f64,
    duration_seconds: f64,
) -> (Vec<f32>, Vec<f32>) {
    let total_frames = (duration_seconds * sample_rate as f64).ceil() as usize;
    let mut out_l = vec![0.0f32; total_frames];
    let mut out_r = vec![0.0f32; total_frames];

    if track.mixer.mute
        || track.track_type == TrackType::Return
        || track.track_type == TrackType::Group
    {
        return (out_l, out_r);
    }

    match &track.instrument {
        InstrumentConfig::ElectronicDrumRack(drum_def) => {
            render_drum_track(
                track,
                Some(drum_def),
                sample_rate,
                bpm,
                swing_16th_ratio,
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
                swing_16th_ratio,
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
                    None,
                    sample_rate,
                    bpm,
                    swing_16th_ratio,
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
                    swing_16th_ratio,
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

    process_insert_fx_chain(
        &track.insert_fx,
        &mut out_l,
        &mut out_r,
        sample_rate,
        bpm,
        start_beat_offset,
    );

    apply_gain_and_pan(&mut out_l, &mut out_r, track.mixer.volume_db, track.mixer.pan);

    (out_l, out_r)
}

#[derive(Clone, Copy)]
enum DrumTrigger {
    Pad(u8, u8),
    MidiNote(u8, u8),
}

fn schedule_drum_pattern_events(
    drum_pattern: &DrumStepPattern,
    clip_start_beat: f64,
    clip_end_beat: f64,
    pattern_offset_beats: f64,
    active_start_beat: f64,
    active_end_beat: f64,
    start_beat_offset: f64,
    bpm: f64,
    swing_16th_ratio: f32,
    sample_rate: f32,
    total_frames: usize,
    events: &mut Vec<(usize, DrumTrigger)>,
) {
    let _ = clip_end_beat;
    let step_dur = drum_pattern.step_duration_beats.max(0.01);
    let pat_len = drum_pattern.length_beats().max(0.0625);
    let swing_delay = compute_swing_delay_beats(swing_16th_ratio, step_dur);

    let rel_min = (active_start_beat - clip_start_beat) + pattern_offset_beats;
    let rel_max = (active_end_beat - clip_start_beat) + pattern_offset_beats;

    for trk in &drum_pattern.tracks {
        for (step_idx, step) in trk.steps.iter().enumerate() {
            if !step.active {
                continue;
            }
            let raw_phase = (step_idx as f64) * step_dur
                + if step_idx % 2 == 1 { swing_delay } else { 0.0 };
            let ev_phase = raw_phase.rem_euclid(pat_len);
            let k_start = ((rel_min - ev_phase) / pat_len).floor() as i64 - 1;
            let k_end = ((rel_max - ev_phase) / pat_len).ceil() as i64 + 1;

            let vel = if step.accent {
                ((step.velocity as f32 * 1.15).round().clamp(1.0, 127.0)) as u8
            } else {
                step.velocity.max(1)
            };

            for k in k_start..=k_end {
                let rel = (k as f64) * pat_len + ev_phase;
                let timeline_beat = clip_start_beat + rel - pattern_offset_beats;
                if timeline_beat >= active_start_beat - 1e-7
                    && timeline_beat < active_end_beat - 1e-7
                {
                    let f = beat_to_frame_offset(timeline_beat, start_beat_offset, bpm, sample_rate);
                    if f >= 0 && (f as usize) < total_frames {
                        events.push((f as usize, DrumTrigger::Pad(trk.pad_index, vel)));
                    }
                }
            }
        }
    }
}

fn schedule_drum_midi_events(
    midi_pattern: &MidiPattern,
    track_id: u32,
    clip_start_beat: f64,
    pattern_offset_beats: f64,
    active_start_beat: f64,
    active_end_beat: f64,
    start_beat_offset: f64,
    bpm: f64,
    swing_16th_ratio: f32,
    sample_rate: f32,
    total_frames: usize,
    events: &mut Vec<(usize, DrumTrigger)>,
) {
    let pat_len = midi_pattern.length_beats.max(0.0625);
    let swing_delay = compute_swing_delay_beats(swing_16th_ratio, 0.25);

    let rel_min = (active_start_beat - clip_start_beat) + pattern_offset_beats;
    let rel_max = (active_end_beat - clip_start_beat) + pattern_offset_beats;

    for (note_idx, note) in midi_pattern.notes.iter().enumerate() {
        let sixteenth_pos = note.start_beat / 0.25;
        let nearest = sixteenth_pos.round();
        let is_offbeat_16th = (sixteenth_pos - nearest).abs() < 0.05
            && (nearest as i64).rem_euclid(2) == 1;
        let ev_phase = (note.start_beat + if is_offbeat_16th { swing_delay } else { 0.0 })
            .rem_euclid(pat_len);

        let k_start = ((rel_min - ev_phase) / pat_len).floor() as i64 - 1;
        let k_end = ((rel_max - ev_phase) / pat_len).ceil() as i64 + 1;

        for k in k_start..=k_end {
            if !deterministic_note_prob_passes(track_id, k as u64, note_idx, note.probability) {
                continue;
            }
            let rel = (k as f64) * pat_len + ev_phase;
            let timeline_beat = clip_start_beat + rel - pattern_offset_beats;
            if timeline_beat >= active_start_beat - 1e-7
                && timeline_beat < active_end_beat - 1e-7
            {
                let f = beat_to_frame_offset(timeline_beat, start_beat_offset, bpm, sample_rate);
                if f >= 0 && (f as usize) < total_frames {
                    events.push((f as usize, DrumTrigger::MidiNote(note.pitch, note.velocity.max(1))));
                }
            }
        }
    }
}

fn configure_drum_rack_from_def(drum_rack: &mut DrumRack, drum_def: &DrumRackDefinition) {
    for pad_def in &drum_def.pads {
        let idx = pad_def.pad_index as usize;
        if let Some(resolved) = resolve_asset_or_file(&pad_def.sample_path) {
            if idx < 16 {
                let _ = drum_rack.load_sample_pad(idx, &resolved);
            }
        }
        let vol_lin = db_to_linear_gain(pad_def.volume_db);
        let pan = pad_def.pan.clamp(-1.0, 1.0);
        let pitch = pad_def.pitch_offset_semitones.clamp(-24.0, 24.0);
        let decay_s = (pad_def.decay_ms / 1000.0).max(0.005);

        if idx < 16 {
            let p = &mut drum_rack.pads[idx];
            p.midi_note = pad_def.root_pitch;
            p.volume = vol_lin;
            p.pan = pan;
            p.pitch_semitones = pitch;
            p.decay_seconds = decay_s;
            p.mute = pad_def.mute;
            p.solo = pad_def.solo;
        }
        if idx < drum_rack.all_pads.len() {
            let ap = &mut drum_rack.all_pads[idx];
            ap.midi_note = pad_def.root_pitch;
            ap.volume = vol_lin;
            ap.pan = pan;
            ap.pitch_semitones = pitch;
            ap.decay_seconds = decay_s;
            ap.mute = pad_def.mute;
            ap.solo = pad_def.solo;
        }
        drum_rack.set_pad_choke_group(idx, pad_def.choke_group);
    }
}

fn render_drum_track(
    track: &TrackDefinition,
    drum_def: Option<&DrumRackDefinition>,
    sample_rate: f32,
    bpm: f64,
    swing_16th_ratio: f32,
    start_beat_offset: f64,
    total_frames: usize,
    out_l: &mut [f32],
    out_r: &mut [f32],
) {
    if total_frames == 0 {
        return;
    }

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

    if let Some(def) = drum_def {
        configure_drum_rack_from_def(&mut drum_rack, def);
    }

    let win_beats = (total_frames as f64) * bpm / (60.0 * sample_rate as f64);
    let win_end_beat = start_beat_offset + win_beats;
    let mut events: Vec<(usize, DrumTrigger)> = Vec::new();

    if !track.arrangement_clips.is_empty() {
        for clip in &track.arrangement_clips {
            if clip.muted {
                continue;
            }
            let active_start = clip.timeline_start_beat.max(start_beat_offset);
            let active_end = clip.timeline_end_beat().min(win_end_beat);
            if active_end <= active_start {
                continue;
            }
            match &clip.pattern {
                ClipPattern::Drum(drum_pattern) => {
                    schedule_drum_pattern_events(
                        drum_pattern,
                        clip.timeline_start_beat,
                        clip.timeline_end_beat(),
                        clip.pattern_offset_beats,
                        active_start,
                        active_end,
                        start_beat_offset,
                        bpm,
                        swing_16th_ratio,
                        sample_rate,
                        total_frames,
                        &mut events,
                    );
                }
                ClipPattern::Midi(midi_pattern) => {
                    schedule_drum_midi_events(
                        midi_pattern,
                        track.id,
                        clip.timeline_start_beat,
                        clip.pattern_offset_beats,
                        active_start,
                        active_end,
                        start_beat_offset,
                        bpm,
                        swing_16th_ratio,
                        sample_rate,
                        total_frames,
                        &mut events,
                    );
                }
                ClipPattern::Audio(_) => {}
            }
        }
    } else if let Some(s_clip) = track.session_clips.iter().flatten().find(|c| {
        matches!(c.pattern, ClipPattern::Drum(_) | ClipPattern::Midi(_))
    }) {
        let clip_end = if s_clip.loop_enabled {
            win_end_beat
        } else {
            let diff = s_clip.loop_end_beat - s_clip.loop_start_beat;
            let len = if diff > 0.0 {
                diff
            } else {
                s_clip.pattern.length_beats().max(0.0625)
            };
            len.min(win_end_beat)
        };
        let active_start = start_beat_offset;
        let active_end = clip_end;
        if active_end > active_start {
            match &s_clip.pattern {
                ClipPattern::Drum(drum_pattern) => {
                    schedule_drum_pattern_events(
                        drum_pattern,
                        0.0,
                        clip_end,
                        s_clip.loop_start_beat,
                        active_start,
                        active_end,
                        start_beat_offset,
                        bpm,
                        swing_16th_ratio,
                        sample_rate,
                        total_frames,
                        &mut events,
                    );
                }
                ClipPattern::Midi(midi_pattern) => {
                    schedule_drum_midi_events(
                        midi_pattern,
                        track.id,
                        0.0,
                        s_clip.loop_start_beat,
                        active_start,
                        active_end,
                        start_beat_offset,
                        bpm,
                        swing_16th_ratio,
                        sample_rate,
                        total_frames,
                        &mut events,
                    );
                }
                ClipPattern::Audio(_) => {}
            }
        }
    }

    if events.is_empty() {
        return;
    }

    events.sort_by_key(|(f, _)| *f);

    const BLOCK_SIZE: usize = 128;
    let mut frame = 0usize;
    let mut ev_idx = 0usize;

    while frame < total_frames {
        while ev_idx < events.len() && events[ev_idx].0 == frame {
            match events[ev_idx].1 {
                DrumTrigger::Pad(pad, vel) => drum_rack.trigger_pad(pad, vel),
                DrumTrigger::MidiNote(note, vel) => drum_rack.trigger_midi_note(note, vel),
            }
            ev_idx += 1;
        }

        let next_ev_frame = events
            .get(ev_idx)
            .map(|(f, _)| *f)
            .unwrap_or(total_frames)
            .min(total_frames);
        let cur_len = (next_ev_frame - frame).min(BLOCK_SIZE).max(1);
        let end_frame = (frame + cur_len).min(total_frames);
        drum_rack.process_block(&mut out_l[frame..end_frame], &mut out_r[frame..end_frame]);
        frame = end_frame;
    }
}

const SYNTH_PARAM_NAMES: [&str; 30] = [
    "oscillator_noise_amplitude",
    "oscillator_pulse_amplitude",
    "oscillator_pulse_width",
    "oscillator_saw_amplitude",
    "oscillator_saw_detune_coarse",
    "oscillator_saw_detune_fine",
    "amplitude_envelope_attack",
    "amplitude_envelope_decay",
    "amplitude_envelope_sustain",
    "amplitude_envelope_release",
    "modulation_envelope_attack",
    "modulation_envelope_decay",
    "modulation_envelope_sustain",
    "modulation_envelope_release",
    "filter_envelope_amount",
    "filter_frequency",
    "filter_resonance",
    "lfo_frequency",
    "lfo_frequency_envelope_amount",
    "lfo_shape",
    "amplitude_lfo_amount",
    "oscillator_pulse_width_lfo_amount",
    "oscillator_pulse_width_envelope_amount",
    "filter_lfo_amount",
    "oscillator_pitch_offset_lfo_amount",
    "oscillator_pitch_offset_envelope_amount",
    "oscillator_pitch_offset",
    "global_slew_time",
    "glide_time",
    "amplitude",
];

fn load_synth_preset_with_aliases(synth: &mut SynthEngine, preset: &str) -> bool {
    if synth.load_preset(preset) {
        return true;
    }
    let fallback = match preset.trim().to_ascii_lowercase().as_str() {
        "analogsaw.preset" | "analogsaw" => Some("osc-shapes.preset"),
        "aggro.preset" | "aggro" => Some("distortion.preset"),
        "metallic.preset" | "metallic" => Some("laser.preset"),
        "darkpad.preset" | "darkpad" => Some("strings.preset"),
        "acid.preset" | "acid" => Some("wowbass.preset"),
        _ => None,
    };
    if let Some(fb) = fallback {
        synth.load_preset(fb)
    } else {
        false
    }
}

fn resolve_synth_automation_param(raw_name: &str) -> Option<&'static str> {
    let lower = raw_name.trim().to_ascii_lowercase();
    match lower.as_str() {
        "filter_frequency" | "filter cutoff" | "cutoff" | "filter_cutoff" => {
            Some("filter_frequency")
        }
        "filter_resonance" | "filter resonance" | "resonance" => Some("filter_resonance"),
        _ => SYNTH_PARAM_NAMES
            .iter()
            .copied()
            .find(|&n| n.eq_ignore_ascii_case(&lower)),
    }
}

fn schedule_synth_midi_events(
    midi_pattern: &MidiPattern,
    track_id: u32,
    clip_start_beat: f64,
    clip_end_beat: f64,
    pattern_offset_beats: f64,
    active_start_beat: f64,
    active_end_beat: f64,
    start_beat_offset: f64,
    bpm: f64,
    swing_16th_ratio: f32,
    sample_rate: f32,
    total_frames: usize,
    events: &mut Vec<(usize, bool, u8, u8)>, // (frame, is_note_on, pitch, velocity)
) {
    let pat_len = midi_pattern.length_beats.max(0.0625);
    let swing_delay = compute_swing_delay_beats(swing_16th_ratio, 0.25);

    let rel_min = (active_start_beat - clip_start_beat) + pattern_offset_beats;
    let rel_max = (active_end_beat - clip_start_beat) + pattern_offset_beats;

    for (note_idx, note) in midi_pattern.notes.iter().enumerate() {
        let sixteenth_pos = note.start_beat / 0.25;
        let nearest = sixteenth_pos.round();
        let is_offbeat_16th = (sixteenth_pos - nearest).abs() < 0.05
            && (nearest as i64).rem_euclid(2) == 1;
        let swing_off = if is_offbeat_16th { swing_delay } else { 0.0 };

        let note_rel_start = note.start_beat + swing_off;
        let note_dur = note.duration_beats.max(0.01);
        let base_phase = note_rel_start.rem_euclid(pat_len);

        let k_start = ((rel_min - base_phase - note_dur) / pat_len).floor() as i64 - 1;
        let k_end = ((rel_max - base_phase) / pat_len).ceil() as i64 + 1;

        for k in k_start..=k_end {
            if !deterministic_note_prob_passes(track_id, k as u64, note_idx, note.probability) {
                continue;
            }
            let rel_start = (k as f64) * pat_len + base_phase;
            let note_abs_start = clip_start_beat + rel_start - pattern_offset_beats;
            let note_abs_end = note_abs_start + note_dur;

            if note_abs_end <= active_start_beat || note_abs_start >= active_end_beat {
                continue;
            }

            let clamped_start = note_abs_start.max(active_start_beat);
            let clamped_end = note_abs_end.min(clip_end_beat).min(active_end_beat);
            if clamped_end <= clamped_start {
                continue;
            }

            let on_f = beat_to_frame_offset(clamped_start, start_beat_offset, bpm, sample_rate)
                .clamp(0, (total_frames.saturating_sub(1)) as isize) as usize;
            let off_f = beat_to_frame_offset(clamped_end, start_beat_offset, bpm, sample_rate)
                .clamp(on_f as isize, total_frames as isize) as usize;

            events.push((on_f, true, note.pitch, note.velocity.max(1)));
            if off_f < total_frames {
                events.push((off_f, false, note.pitch, 0));
            }
        }
    }
}

fn render_synth_track(
    track: &TrackDefinition,
    params: &SynthPatchParameters,
    sample_rate: f32,
    bpm: f64,
    swing_16th_ratio: f32,
    start_beat_offset: f64,
    total_frames: usize,
    out_l: &mut [f32],
    out_r: &mut [f32],
) {
    if total_frames == 0 {
        return;
    }

    let mut synth = SynthEngine::new(sample_rate);
    let preset_loaded = match &params.preset_name {
        Some(preset) if !preset.eq_ignore_ascii_case("Init Patch") && !preset.is_empty() => {
            load_synth_preset_with_aliases(&mut synth, preset)
        }
        _ => false,
    };

    let defaults = SynthPatchParameters::default();
    for name in SYNTH_PARAM_NAMES {
        if let Some(val) = params.get_parameter(name) {
            if preset_loaded {
                if let Some(def_val) = defaults.get_parameter(name) {
                    if (val - def_val).abs() > 1e-6 {
                        synth.set_parameter(name, val);
                    }
                }
            } else {
                synth.set_parameter(name, val);
            }
        }
    }
    synth.smoothed_params = synth.params.clone();

    let win_beats = (total_frames as f64) * bpm / (60.0 * sample_rate as f64);
    let win_end_beat = start_beat_offset + win_beats;
    let mut events: Vec<(usize, bool, u8, u8)> = Vec::new();

    if !track.arrangement_clips.is_empty() {
        for clip in &track.arrangement_clips {
            if clip.muted {
                continue;
            }
            let active_start = clip.timeline_start_beat.max(start_beat_offset);
            let active_end = clip.timeline_end_beat().min(win_end_beat);
            if active_end <= active_start {
                continue;
            }
            if let ClipPattern::Midi(midi_pattern) = &clip.pattern {
                if !midi_pattern.notes.is_empty() {
                    schedule_synth_midi_events(
                        midi_pattern,
                        track.id,
                        clip.timeline_start_beat,
                        clip.timeline_end_beat(),
                        clip.pattern_offset_beats,
                        active_start,
                        active_end,
                        start_beat_offset,
                        bpm,
                        swing_16th_ratio,
                        sample_rate,
                        total_frames,
                        &mut events,
                    );
                }
            }
        }
    } else if let Some(s_clip) = track.session_clips.iter().flatten().find(|c| {
        matches!(&c.pattern, ClipPattern::Midi(mp) if !mp.notes.is_empty())
    }) {
        if let ClipPattern::Midi(midi_pattern) = &s_clip.pattern {
            let clip_end = if s_clip.loop_enabled {
                win_end_beat
            } else {
                let diff = s_clip.loop_end_beat - s_clip.loop_start_beat;
                let len = if diff > 0.0 {
                    diff
                } else {
                    midi_pattern.length_beats.max(0.0625)
                };
                len.min(win_end_beat)
            };
            if clip_end > start_beat_offset {
                schedule_synth_midi_events(
                    midi_pattern,
                    track.id,
                    0.0,
                    clip_end,
                    s_clip.loop_start_beat,
                    start_beat_offset,
                    clip_end,
                    start_beat_offset,
                    bpm,
                    swing_16th_ratio,
                    sample_rate,
                    total_frames,
                    &mut events,
                );
            }
        }
    }

    if events.is_empty() {
        return;
    }

    // Sort by frame ascending; at identical frame, NoteOff (false) runs before NoteOn (true).
    events.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });

    let beats_per_frame = bpm / (60.0 * sample_rate as f64);
    let resolved_lanes: Vec<(&oxide_timeline::AutomationLane, &'static str)> = track
        .automation_lanes
        .iter()
        .filter(|l| !l.points.is_empty())
        .filter_map(|l| resolve_synth_automation_param(&l.parameter_name).map(|p| (l, p)))
        .collect();

    let mut ev_idx = 0usize;
    for f in 0..total_frames {
        let current_beat = start_beat_offset + (f as f64) * beats_per_frame;

        if f % 64 == 0 && !resolved_lanes.is_empty() {
            for (lane, param_name) in &resolved_lanes {
                let val = lane.interpolate_at(current_beat, 0.5);
                let is_large_domain = ParameterBank::get_descriptor(param_name)
                    .map(|d| d.max > 1.5)
                    .unwrap_or(false);
                if is_large_domain && (0.0..=1.0).contains(&val) {
                    synth.set_parameter_normalized(param_name, val);
                } else {
                    synth.set_parameter(param_name, val);
                }
            }
        }

        while ev_idx < events.len() && events[ev_idx].0 == f {
            let (_, is_on, pitch, vel) = events[ev_idx];
            if is_on {
                synth.note_on(pitch, vel);
            } else {
                synth.note_off(pitch);
            }
            ev_idx += 1;
        }

        let sample = synth.process_sample();
        out_l[f] = sample * 0.707;
        out_r[f] = sample * 0.707;
    }
}

fn mix_resampled_audio(
    samples: &[f32],
    src_channels: u16,
    src_sr: u32,
    render_sr: f32,
    src_start_seconds: f64,
    out_start_frame: usize,
    dur_frames: usize,
    gain: f32,
    looping: bool,
    out_l: &mut [f32],
    out_r: &mut [f32],
) -> bool {
    let ch = (src_channels as usize).max(1);
    let total_src_frames = samples.len() / ch;
    if total_src_frames == 0 || dur_frames == 0 || src_sr == 0 || render_sr <= 0.0 {
        return false;
    }

    let src_step = (src_sr as f64) / (render_sr as f64);
    let mut src_pos = src_start_seconds * (src_sr as f64);
    let mut rendered = false;

    for i in 0..dur_frames {
        let out_idx = out_start_frame + i;
        if out_idx >= out_l.len() {
            break;
        }
        if src_pos < 0.0 {
            src_pos += src_step;
            continue;
        }
        let mut idx0 = src_pos.floor() as usize;
        if idx0 >= total_src_frames {
            if looping {
                idx0 %= total_src_frames;
            } else {
                break;
            }
        }
        let idx1 = if looping {
            (idx0 + 1) % total_src_frames
        } else {
            (idx0 + 1).min(total_src_frames - 1)
        };
        let frac = (src_pos - src_pos.floor()) as f32;

        let l0 = samples[idx0 * ch];
        let l1 = samples[idx1 * ch];
        let (r0, r1) = if ch > 1 {
            (samples[idx0 * ch + 1], samples[idx1 * ch + 1])
        } else {
            (l0, l1)
        };

        out_l[out_idx] += (l0 + (l1 - l0) * frac) * gain;
        out_r[out_idx] += (r0 + (r1 - r0) * frac) * gain;
        rendered = true;
        src_pos += src_step;
    }

    rendered
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
    if total_frames == 0 {
        return;
    }

    let seconds_per_beat = 60.0 / bpm.max(1.0);
    let window_beats = (total_frames as f64 / sample_rate as f64) / seconds_per_beat;
    let window_end_beat = start_beat_offset + window_beats;

    // 1. Render arrangement clips placed on timeline
    if !track.arrangement_clips.is_empty() {
        for clip in &track.arrangement_clips {
            if clip.muted {
                continue;
            }
            if let ClipPattern::Audio(data) = &clip.pattern {
                let clip_start = clip.timeline_start_beat;
                let clip_end = clip.timeline_end_beat();
                if clip_end <= start_beat_offset || clip_start >= window_end_beat {
                    continue;
                }
                let overlap_start_beat = clip_start.max(start_beat_offset);
                let overlap_end_beat = clip_end.min(window_end_beat);
                let out_start_frame =
                    ((overlap_start_beat - start_beat_offset) * seconds_per_beat * sample_rate as f64)
                        .round() as usize;
                let src_start_seconds =
                    ((overlap_start_beat - clip_start) + clip.pattern_offset_beats) * seconds_per_beat;
                let dur_frames =
                    (((overlap_end_beat - overlap_start_beat) * seconds_per_beat * sample_rate as f64)
                        .ceil() as usize)
                    .min(total_frames.saturating_sub(out_start_frame));
                let clip_gain = db_to_linear_gain(data.gain_db);

                if !data.samples.is_empty() {
                    let src_sr = if data.sample_rate > 0 {
                        data.sample_rate
                    } else {
                        sample_rate as u32
                    };
                    let src_ch = data.channels.max(1);
                    mix_resampled_audio(
                        &data.samples,
                        src_ch,
                        src_sr,
                        sample_rate,
                        src_start_seconds,
                        out_start_frame,
                        dur_frames,
                        clip_gain,
                        false,
                        out_l,
                        out_r,
                    );
                } else if let Some(resolved) = resolve_asset_or_file(&data.file_path) {
                    if let Ok((wav_sr, wav_ch, samples)) = read_wav_file(&resolved) {
                        mix_resampled_audio(
                            &samples,
                            wav_ch,
                            wav_sr,
                            sample_rate,
                            src_start_seconds,
                            out_start_frame,
                            dur_frames,
                            clip_gain,
                            false,
                            out_l,
                            out_r,
                        );
                    }
                }
            }
        }
        return;
    }

    // 2. If track has no arrangement clips at all, render first audio session clip
    for s_clip in track.session_clips.iter().flatten() {
        if let ClipPattern::Audio(data) = &s_clip.pattern {
            let clip_gain = db_to_linear_gain(data.gain_db);
            let src_start_seconds = s_clip.loop_start_beat.max(0.0) * seconds_per_beat;
            if !data.samples.is_empty() {
                let src_sr = if data.sample_rate > 0 {
                    data.sample_rate
                } else {
                    sample_rate as u32
                };
                let src_ch = data.channels.max(1);
                if mix_resampled_audio(
                    &data.samples,
                    src_ch,
                    src_sr,
                    sample_rate,
                    src_start_seconds,
                    0,
                    total_frames,
                    clip_gain,
                    s_clip.loop_enabled,
                    out_l,
                    out_r,
                ) {
                    break;
                }
            } else if let Some(resolved) = resolve_asset_or_file(&data.file_path) {
                if let Ok((wav_sr, wav_ch, samples)) = read_wav_file(&resolved) {
                    if mix_resampled_audio(
                        &samples,
                        wav_ch,
                        wav_sr,
                        sample_rate,
                        src_start_seconds,
                        0,
                        total_frames,
                        clip_gain,
                        s_clip.loop_enabled,
                        out_l,
                        out_r,
                    ) {
                        break;
                    }
                }
            }
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

    #[test]
    fn test_empty_tracks_and_return_group_render_silence() {
        let mut project = DAWProject::new("Silence Check");
        project.transport.bpm = 120.0;

        project.tracks.push(TrackDefinition::new_synth(1, "Empty Synth"));
        project.tracks.push(TrackDefinition::new_drum_rack(2, "Empty Drums"));
        project.tracks.push(TrackDefinition::new_audio(3, "Empty Audio"));
        project.tracks.push(TrackDefinition::new_return(4, "Empty Return"));
        project.tracks.push(TrackDefinition::new_group(5, "Empty Group"));

        let (master_l, master_r, stems) =
            render_project_mix_with_stems(&project, 44100.0, 120.0, 0.0, 1.0, true);

        assert!(master_l.iter().all(|&s| s == 0.0), "Master L must be silent when all tracks are empty");
        assert!(master_r.iter().all(|&s| s == 0.0), "Master R must be silent when all tracks are empty");
        for (idx, (l, r)) in stems.iter().enumerate() {
            assert!(l.iter().all(|&s| s == 0.0), "Track {} stem L must be silent", idx);
            assert!(r.iter().all(|&s| s == 0.0), "Track {} stem R must be silent", idx);
        }
    }

    #[test]
    fn test_solo_mutes_non_solo_tracks_and_preserves_return_sends() {
        use oxide_timeline::{ArrangementClip, MidiNote};

        let mut project = DAWProject::new("Solo + Return Test");
        project.transport.bpm = 120.0;

        // Track 0: Synth A (soloed, with Send A at 0 dB)
        let mut t_solo = TrackDefinition::new_synth(1, "Solo Synth");
        t_solo.mixer.solo = true;
        t_solo.mixer.set_send_db(0, 0.0);
        let mut pat_a = MidiPattern::new(4.0);
        pat_a.notes.push(MidiNote {
            pitch: 60,
            velocity: 110,
            start_beat: 0.0,
            duration_beats: 1.0,
            probability: 1.0,
        });
        t_solo
            .arrangement_clips
            .push(ArrangementClip::new(10, "Clip A", 0.0, 4.0, ClipPattern::Midi(pat_a)));

        // Track 1: Synth B (not soloed, loud clip)
        let mut t_other = TrackDefinition::new_synth(2, "Other Synth");
        t_other.mixer.solo = false;
        let mut pat_b = MidiPattern::new(4.0);
        pat_b.notes.push(MidiNote {
            pitch: 36,
            velocity: 127,
            start_beat: 0.0,
            duration_beats: 2.0,
            probability: 1.0,
        });
        t_other
            .arrangement_clips
            .push(ArrangementClip::new(11, "Clip B", 0.0, 4.0, ClipPattern::Midi(pat_b)));

        // Track 2: Return A (not explicitly soloed, must still receive sends from Solo Synth)
        let mut t_ret = TrackDefinition::new_return(3, "Reverb Return");
        t_ret.insert_fx.push(InsertEffectConfig::Reverb {
            ir_path: None,
            decay_ms: 1200.0,
            mix: 1.0,
        });

        project.tracks.push(t_solo);
        project.tracks.push(t_other);
        project.tracks.push(t_ret);

        let (_ml, _mr, stems) =
            render_project_mix_with_stems(&project, 44100.0, 120.0, 0.0, 2.0, true);
        let ret_energy: f32 = stems[2].0.iter().map(|s| s * s).sum();
        assert!(ret_energy > 1e-4, "Return track must receive sends from soloed track");

        // Now clear Track 0 clips and verify Track 1 stays muted when a silent track is soloed
        project.tracks[0].arrangement_clips.clear();
        project.tracks[0].mixer.set_send_db(0, -70.0);
        let (ml2, mr2, _stems2) =
            render_project_mix_with_stems(&project, 44100.0, 120.0, 0.0, 1.0, false);
        assert!(
            ml2.iter().all(|&s| s.abs() < 1e-6) && mr2.iter().all(|&s| s.abs() < 1e-6),
            "Non-soloed Track 1 must be muted when any_solo is active"
        );
    }

    #[test]
    fn test_drum_track_renders_midi_clips_and_pad_definitions() {
        use oxide_project::DrumPadDefinition;
        use oxide_timeline::{ArrangementClip, MidiNote};

        // Track with pad 0 muted and pad 2 panned far right (+0.9)
        let mut track = TrackDefinition::new_drum_rack(1, "Drum Midi Track");
        let mut pad0 = DrumPadDefinition::new(0, "Kick", "", 36);
        pad0.mute = true;
        let mut pad2 = DrumPadDefinition::new(2, "Snare", "", 38);
        pad2.pan = 0.9;
        pad2.decay_ms = 300.0;
        pad2.volume_db = 0.0;
        let drum_def = DrumRackDefinition {
            pads: vec![pad0, pad2],
        };
        track.instrument = InstrumentConfig::ElectronicDrumRack(drum_def);

        // First render only muted pad 0 via MIDI note 36 -> must be silent
        let mut pat_muted = MidiPattern::new(2.0);
        pat_muted.notes.push(MidiNote {
            pitch: 36,
            velocity: 120,
            start_beat: 0.0,
            duration_beats: 0.25,
            probability: 1.0,
        });
        track
            .arrangement_clips
            .push(ArrangementClip::new(1, "Kicks", 0.0, 2.0, ClipPattern::Midi(pat_muted)));
        let (ml_muted, mr_muted) = render_single_track(&track, 44100.0, 120.0, 1.0);
        assert!(
            ml_muted.iter().all(|&s| s.abs() < 1e-6) && mr_muted.iter().all(|&s| s.abs() < 1e-6),
            "Muted drum pad must not sound when triggered via MidiClip"
        );

        // Now add unmuted right-panned snare (note 38) -> right channel energy must exceed left
        if let ClipPattern::Midi(ref mut mp) = track.arrangement_clips[0].pattern {
            mp.notes.push(MidiNote {
                pitch: 38,
                velocity: 120,
                start_beat: 0.0,
                duration_beats: 0.25,
                probability: 1.0,
            });
        }
        let (l, r) = render_single_track(&track, 44100.0, 120.0, 1.0);
        let energy_l: f64 = l.iter().map(|&x| (x * x) as f64).sum();
        let energy_r: f64 = r.iter().map(|&x| (x * x) as f64).sum();
        assert!(energy_r > 1e-3, "Drum MIDI note 38 must produce audible output");
        assert!(
            energy_r > energy_l * 3.0,
            "Pad pan=+0.9 must route substantially more energy to right ({}) than left ({})",
            energy_r,
            energy_l
        );
    }

    #[test]
    fn test_synth_preset_aliases_and_param_overrides_preserved() {
        use oxide_timeline::{ArrangementClip, MidiNote};

        let make_synth_track = |cutoff_override: Option<f32>| {
            let mut params = SynthPatchParameters::default();
            params.preset_name = Some("analogsaw.preset".to_string());
            if let Some(freq) = cutoff_override {
                params.filter_frequency = freq;
            }
            let mut trk = TrackDefinition::new_synth(1, "Lead");
            trk.instrument = InstrumentConfig::BuiltinSubtractiveSynth(params);
            let mut mp = MidiPattern::new(2.0);
            mp.notes.push(MidiNote {
                pitch: 48,
                velocity: 120,
                start_beat: 0.0,
                duration_beats: 1.5,
                probability: 1.0,
            });
            trk.arrangement_clips
                .push(ArrangementClip::new(1, "Clip", 0.0, 2.0, ClipPattern::Midi(mp)));
            trk
        };

        let trk_preset = make_synth_track(None);
        let trk_dark = make_synth_track(Some(120.0));

        let (l_a, _) = render_single_track(&trk_preset, 44100.0, 120.0, 1.0);
        let (l_b, _) = render_single_track(&trk_dark, 44100.0, 120.0, 1.0);

        let rms_a: f64 = (l_a.iter().map(|&s| (s * s) as f64).sum::<f64>() / l_a.len() as f64).sqrt();
        let rms_b: f64 = (l_b.iter().map(|&s| (s * s) as f64).sum::<f64>() / l_b.len() as f64).sqrt();
        let diff: f64 = l_a
            .iter()
            .zip(l_b.iter())
            .map(|(a, b)| ((a - b) * (a - b)) as f64)
            .sum();

        assert!(rms_a > 0.01 && rms_b > 0.001, "Both preset and overridden synth must sound");
        assert!(
            diff > 1e-2,
            "Numeric SynthPatchParameters override (filter_frequency=120) must change output vs preset default"
        );
    }

    #[test]
    fn test_swing_16th_ratio_delays_offbeat_notes() {
        use oxide_timeline::{ArrangementClip, DrumStep};

        let mut drum_pat = DrumStepPattern::new_16_step();
        drum_pat.set_step(2, 1, DrumStep::new(true, 120, 38)); // offbeat 16th at beat 0.25 (frame ~5513 at 120 BPM)

        let mut trk = TrackDefinition::new_drum_rack(1, "Swing Drums");
        trk.arrangement_clips
            .push(ArrangementClip::new(1, "Beat", 0.0, 4.0, ClipPattern::Drum(drum_pat)));

        let (straight_l, _) = render_single_track_window_with_swing(&trk, 44100.0, 120.0, 0.0, 0.0, 0.5);
        let (swung_l, _) = render_single_track_window_with_swing(&trk, 44100.0, 120.0, 0.66, 0.0, 0.5);

        // Straight triggers at frame 5513; swung delays by 0.08 beats (~1764 frames -> ~7277)
        let straight_window_rms: f64 = straight_l[5512..6500].iter().map(|&s| (s * s) as f64).sum();
        let swung_early_energy: f64 = swung_l[5512..6500].iter().map(|&s| (s * s) as f64).sum();
        let swung_late_energy: f64 = swung_l[7200..8500].iter().map(|&s| (s * s) as f64).sum();

        assert!(straight_window_rms > 1e-3, "Straight offbeat 16th should sound at frame 5512");
        assert!(swung_early_energy < 1e-6, "Swung offbeat 16th must still be silent at 5512..6500");
        assert!(swung_late_energy > 1e-3, "Swung offbeat 16th must sound around frame 7276");
    }

    #[test]
    fn test_master_fx_delay_reverb_and_limiter_ceiling() {
        use oxide_timeline::{ArrangementClip, MidiNote};

        let mut project = DAWProject::new("Master FX Test");
        project.transport.bpm = 120.0;
        project.master_track.saturator_enabled = true;
        project.master_track.saturator_drive_db = 10.0;
        project.master_track.delay_time_beats = 0.5;
        project.master_track.delay_feedback = 0.5;
        project.master_track.delay_mix = 0.5;
        project.master_track.reverb_decay_ms = 1200.0;
        project.master_track.reverb_mix = 0.4;
        project.master_track.limiter_ceiling_db = -3.0;

        let mut trk = TrackDefinition::new_synth(1, "Pluck");
        let mut mp = MidiPattern::new(4.0);
        mp.notes.push(MidiNote {
            pitch: 60,
            velocity: 127,
            start_beat: 0.0,
            duration_beats: 0.25,
            probability: 1.0,
        });
        trk.arrangement_clips
            .push(ArrangementClip::new(1, "Clip", 0.0, 4.0, ClipPattern::Midi(mp)));
        trk.mixer.volume_db = 3.0;
        project.tracks.push(trk);

        let (ml, mr, _) = render_project_mix_with_stems(&project, 44100.0, 120.0, 0.0, 2.0, false);

        // Peak must stay bounded near -3 dBFS (~0.708)
        let peak = ml
            .iter()
            .chain(mr.iter())
            .fold(0.0f32, |acc, &s| acc.max(s.abs()));
        assert!(peak > 0.05, "Master output must be audible");
        assert!(
            peak <= 0.76,
            "MasterLimiter at -3.0 dBFS should clamp peak around 0.708, got {}",
            peak
        );

        // Tail after beat 2 (1.0s..2.0s) must still carry delay/reverb energy
        let tail_energy: f64 = ml[44100..].iter().map(|&s| (s * s) as f64).sum();
        assert!(tail_energy > 1e-4, "Master delay/reverb should produce audible tail");
    }

    #[test]
    fn test_audio_track_resamples_mismatched_sample_rate() {
        use oxide_timeline::{ArrangementClip, AudioClipData};

        // 1.0 second mono 440 Hz sine authored at 22050 Hz
        let src_sr = 22050u32;
        let src_frames = 22050usize;
        let mut src_samples = Vec::with_capacity(src_frames);
        for i in 0..src_frames {
            let t = i as f32 / src_sr as f32;
            src_samples.push((2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5);
        }

        let mut clip_data = AudioClipData::new("/virtual/22k.wav", src_sr, 1, 2.0);
        clip_data.samples = src_samples;

        let mut trk = TrackDefinition::new_audio(1, "Audio Resample");
        trk.arrangement_clips
            .push(ArrangementClip::new(1, "Sine", 0.0, 2.0, ClipPattern::Audio(clip_data)));

        let (out_l, _) = render_single_track(&trk, 44100.0, 120.0, 1.0);
        // If 22050 Hz were NOT resampled to 44100 Hz, samples would run out at 0.5s (frame 22050)
        let second_half_energy: f64 = out_l[25000..43000].iter().map(|&s| (s * s) as f64).sum();
        assert!(
            second_half_energy > 10.0,
            "Resampled 22050 Hz clip must span the full 1.0s at 44100 Hz (energy={})",
            second_half_energy
        );

        // Count positive-going zero crossings across ~1s -> must be close to 440 Hz (not 880 Hz!)
        let crossings = out_l
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();
        assert!(
            (430..=450).contains(&crossings),
            "Resampled pitch must stay ~440 Hz (~440 zero-crossings), got {}",
            crossings
        );
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

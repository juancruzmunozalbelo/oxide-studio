//! Real-Time Groove Pool (.agr) Extraction & Polyphonic Audio-to-MIDI Conversion.
//!
//! Provides:
//! - `GrooveTemplate`: Ableton `.agr` format representation with 16-step timing offsets
//!   (-120..+120 ticks) and velocity scalings (0.0..2.0).
//!   - `extract_groove_from_audio`: extracts micro-timing and velocity profile from audio transients.
//!   - `apply_groove_to_notes`: non-destructively applies groove template to MIDI note events.
//! - `AudioToMidiConverter`:
//!   - `convert_drums_to_midi`: classifies transients via spectral centroid into Kick (36), Snare (38), or Hi-Hat (42).
//!   - `convert_melody_to_midi`: monophonic fundamental pitch estimation (YIN algorithm) to MIDI notes.
//!   - `convert_harmony_to_midi`: multi-band spectral peak chord detection for polyphonic triads/seventh chords.

use std::f64::consts::PI;

/// A transcribed MIDI note event produced by audio-to-MIDI analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscribedMidiNote {
    pub note: u8,
    pub velocity: u8,
    pub start_beat: f64,
    pub duration_beats: f64,
}

impl TranscribedMidiNote {
    pub fn new(note: u8, velocity: u8, start_beat: f64, duration_beats: f64) -> Self {
        Self {
            note,
            velocity,
            start_beat,
            duration_beats,
        }
    }
}

/// Representation of an Ableton Groove Template (`.agr`).
///
/// Contains a 16-step cycle of micro-timing displacements in MIDI ticks (-120..+120)
/// and velocity scaling factors (0.0..2.0), along with evaluation parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct GrooveTemplate {
    pub name: String,
    pub timing_offsets: [i16; 16],      // -120 .. +120 ticks (480 ticks/beat = 16th is 120 ticks)
    pub velocity_scalings: [f32; 16],   // 0.0 .. 2.0 scaling relative to unity
    pub timing_amount: f32,             // 0.0 .. 2.0 (1.0 = 100%)
    pub random_amount: f32,             // 0.0 .. 1.0 (Humanize %)
    pub quantize_amount: f32,           // 0.0 .. 1.0 (Pre-quantize to straight 16th grid)
}

impl Default for GrooveTemplate {
    fn default() -> Self {
        Self {
            name: "Default 16th Groove".to_string(),
            timing_offsets: [0; 16],
            velocity_scalings: [1.0; 16],
            timing_amount: 1.0,
            random_amount: 0.0,
            quantize_amount: 0.0,
        }
    }
}

impl GrooveTemplate {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Extract a 16-step groove profile from an audio rhythm loop.
    ///
    /// Analyzes audio transient onsets against the ideal straight 16th-note metronomic grid,
    /// extracting timing displacement in ticks (-120..+120) and relative velocity dynamics.
    pub fn extract_groove_from_audio(samples: &[f32], sample_rate: u32, bpm: f64) -> Self {
        let mut template = Self::default();
        if samples.is_empty() || sample_rate == 0 || bpm <= 0.0 {
            return template;
        }

        let seconds_per_beat = 60.0 / bpm;
        let samples_per_beat = seconds_per_beat * sample_rate as f64;
        let samples_per_16th = samples_per_beat * 0.25;

        // Peak energy calculation for normalization
        let mut step_peaks = [0.0f32; 16];
        let mut step_offsets_ticks = [0i16; 16];

        for step in 0..16 {
            let ideal_sample = step as f64 * samples_per_16th;
            let window_start = (ideal_sample - samples_per_16th * 0.45).max(0.0) as usize;
            let window_end = ((ideal_sample + samples_per_16th * 0.55) as usize).min(samples.len());

            if window_start >= window_end {
                continue;
            }

            // Find peak transient in this 16th step window
            let mut max_val = 0.0f32;
            let mut peak_idx = window_start;
            for idx in window_start..window_end {
                let abs_s = samples[idx].abs();
                if abs_s > max_val {
                    max_val = abs_s;
                    peak_idx = idx;
                }
            }

            step_peaks[step] = max_val;

            if max_val > 0.01 {
                let actual_beat = (peak_idx as f64 / sample_rate as f64) * (bpm / 60.0);
                let ideal_beat = step as f64 * 0.25;
                let beat_delta = actual_beat - ideal_beat;
                // 480 ticks per quarter note
                let ticks = (beat_delta * 480.0).round().clamp(-120.0, 120.0) as i16;
                step_offsets_ticks[step] = ticks;
            }
        }

        // Calculate average peak for velocity scaling
        let total_peak: f32 = step_peaks.iter().copied().sum();
        let active_count = step_peaks.iter().filter(|&&p| p > 0.01).count();
        let avg_peak = if active_count > 0 {
            total_peak / active_count as f32
        } else {
            1.0
        };

        for step in 0..16 {
            template.timing_offsets[step] = step_offsets_ticks[step];
            if step_peaks[step] > 0.001 && avg_peak > 0.001 {
                let scale = (step_peaks[step] / avg_peak).clamp(0.0, 2.0);
                template.velocity_scalings[step] = scale;
            } else {
                template.velocity_scalings[step] = 1.0;
            }
        }

        template
    }

    /// Apply this groove template non-destructively to a slice of MIDI notes.
    ///
    /// Notes are tuples of `(pitch, velocity, start_beat, duration_beats)`.
    pub fn apply_groove_to_notes(&self, notes: &mut [(u8, u8, f64, f64)]) {
        for (pitch, vel, start_beat, _duration) in notes.iter_mut() {
            // Find closest 16th grid step
            let step_float = *start_beat / 0.25;
            let grid_step = step_float.round();
            let grid_beat = grid_step * 0.25;
            let step_idx = (grid_step as usize) % 16;

            // 1. Pre-quantize toward straight grid
            if self.quantize_amount > 0.0 {
                let q = self.quantize_amount as f64;
                *start_beat = *start_beat * (1.0 - q) + grid_beat * q;
            }

            // 2. Timing offset displacement (ticks / 480 ticks per beat)
            let tick_offset = self.timing_offsets[step_idx] as f64;
            let timing_shift = (tick_offset / 480.0) * (self.timing_amount as f64);

            // 3. Random humanization
            let mut rand_shift = 0.0;
            if self.random_amount > 0.0 {
                // Deterministic pseudo-random variation based on note and step
                let seed = (*pitch as u32).wrapping_mul(1103515245).wrapping_add(step_idx as u32 * 12345);
                let rnd_norm = ((seed % 1000) as f64 / 500.0) - 1.0; // -1.0 .. 1.0
                rand_shift = (rnd_norm * 16.0 / 480.0) * (self.random_amount as f64);
            }

            *start_beat = (*start_beat + timing_shift + rand_shift).max(0.0);

            // 4. Velocity scaling
            let v_scale = self.velocity_scalings[step_idx];
            let effective_scale = 1.0 + (v_scale - 1.0) * self.timing_amount;
            let new_vel = (*vel as f32 * effective_scale).round().clamp(1.0, 127.0);
            *vel = new_vel as u8;
        }
    }
}

/// Converter for real-time monophonic and polyphonic audio transcription to MIDI.
pub struct AudioToMidiConverter;

impl AudioToMidiConverter {
    /// Convert percussion and drum audio into MIDI notes.
    ///
    /// Analyzes transient onsets and classifies spectral centroid:
    /// - Low centroid (< 400 Hz) -> Kick (MIDI 36)
    /// - Mid centroid (400 .. 2800 Hz) -> Snare (MIDI 38)
    /// - High centroid (>= 2800 Hz) -> Hi-Hat (MIDI 42)
    pub fn convert_drums_to_midi(
        samples: &[f32],
        sample_rate: u32,
        bpm: f64,
    ) -> Vec<TranscribedMidiNote> {
        let mut results = Vec::new();
        if samples.is_empty() || sample_rate == 0 || bpm <= 0.0 {
            return results;
        }

        let min_peak_distance = (sample_rate as f64 * 0.10) as usize; // 100ms refractory period
        let mut i = 0;

        while i < samples.len() {
            let s = samples[i].abs();
            if s > 0.08 {
                // Local peak scan within window
                let mut peak_val = s;
                let mut peak_pos = i;
                let scan_limit = (i + min_peak_distance / 2).min(samples.len());
                for j in i..scan_limit {
                    if samples[j].abs() > peak_val {
                        peak_val = samples[j].abs();
                        peak_pos = j;
                    }
                }

                // Compute spectral centroid and band energy around peak
                let window_size = 1024.min(samples.len() - peak_pos);
                if window_size >= 64 {
                    let start = peak_pos.saturating_sub(32);
                    let end = (start + window_size).min(samples.len());
                    let slice = &samples[start..end];
                    let (centroid, low_en, mid_en, high_en) =
                        compute_spectral_analysis(slice, sample_rate);

                    let note = if centroid < 400.0 || (low_en > 2.0 * mid_en && low_en > 2.0 * high_en) {
                        36 // Kick (C1)
                    } else if (low_en + mid_en) > 0.15 * high_en && centroid < 7000.0 {
                        38 // Snare (D1)
                    } else {
                        42 // Closed Hi-Hat (F#1)
                    };

                    let start_beat = (peak_pos as f64 / sample_rate as f64) * (bpm / 60.0);
                    let velocity = (peak_val * 127.0).clamp(30.0, 127.0) as u8;
                    results.push(TranscribedMidiNote::new(note, velocity, start_beat, 0.25));
                }

                i = peak_pos + min_peak_distance;
            } else {
                i += 64;
            }
        }

        results
    }

    /// Monophonic pitch estimation using the YIN difference and autocorrelation algorithm.
    ///
    /// Accurately tracks fundamental frequency f0, converting into MIDI note numbers.
    /// E.g., 440.0 Hz sine wave transcribes to MIDI Note 69 (A4).
    pub fn convert_melody_to_midi(
        samples: &[f32],
        sample_rate: u32,
        bpm: f64,
    ) -> Vec<TranscribedMidiNote> {
        let mut results = Vec::new();
        if samples.is_empty() || sample_rate == 0 || bpm <= 0.0 {
            return results;
        }

        let window_size = 2048;
        let hop_size = 512;
        let seconds_per_hop = hop_size as f64 / sample_rate as f64;
        let beats_per_hop = seconds_per_hop * (bpm / 60.0);

        let mut current_note: Option<(u8, u8, f64, f64)> = None; // note, vel, start_beat, duration_beats

        let mut pos = 0;
        while pos + window_size <= samples.len() {
            let frame = &samples[pos..pos + window_size];
            let frame_beat = (pos as f64 / sample_rate as f64) * (bpm / 60.0);

            // Compute frame RMS energy
            let sum_sq: f32 = frame.iter().map(|&x| x * x).sum();
            let rms = (sum_sq / window_size as f32).sqrt();

            if rms > 0.015 {
                if let Some(f0) = estimate_pitch_yin(frame, sample_rate) {
                    if f0 >= 55.0 && f0 <= 2000.0 {
                        let midi_pitch = (69.0 + 12.0 * (f0 / 440.0).log2()).round() as u8;
                        let vel = (rms * 127.0 * 2.0).clamp(32.0, 127.0) as u8;

                        match current_note {
                            Some((note, ref mut v, _start_beat, ref mut dur)) if note == midi_pitch => {
                                *dur += beats_per_hop;
                                *v = (*v + vel) / 2;
                            }
                            Some((note, v, start_beat, dur)) => {
                                results.push(TranscribedMidiNote::new(note, v, start_beat, dur));
                                current_note = Some((midi_pitch, vel, frame_beat, beats_per_hop));
                            }
                            None => {
                                current_note = Some((midi_pitch, vel, frame_beat, beats_per_hop));
                            }
                        }
                    }
                }
            } else if let Some((note, v, start_beat, dur)) = current_note.take() {
                results.push(TranscribedMidiNote::new(note, v, start_beat, dur));
            }

            pos += hop_size;
        }

        if let Some((note, v, start_beat, dur)) = current_note {
            results.push(TranscribedMidiNote::new(note, v, start_beat, dur));
        }

        results
    }

    /// Polyphonic multi-pitch transcription using multi-band spectral peak analysis.
    ///
    /// Identifies root, 3rd, 5th, and 7th harmonic structures while pruning overtones
    /// to transcribe clean triads and seventh chords.
    pub fn convert_harmony_to_midi(
        samples: &[f32],
        sample_rate: u32,
        bpm: f64,
    ) -> Vec<TranscribedMidiNote> {
        let mut results = Vec::new();
        if samples.is_empty() || sample_rate == 0 || bpm <= 0.0 {
            return results;
        }

        let n = 2048.min(samples.len());
        let slice = &samples[0..n];

        // Discrete spectral peak extraction across musical pitch range (C2 65 Hz to C6 1046 Hz)
        let mut detected_notes: Vec<(u8, f32)> = Vec::new();

        // Scan standard MIDI notes from 48 (C3) to 84 (C6)
        for midi_note in 48..=84 {
            let freq = 440.0 * 2.0f64.powf((midi_note as f64 - 69.0) / 12.0);
            let mag = compute_fourier_magnitude_at_freq(slice, sample_rate, freq);
            if mag > 0.05 {
                detected_notes.push((midi_note, mag));
            }
        }

        if detected_notes.is_empty() {
            return results;
        }

        // Find max magnitude
        let max_mag = detected_notes
            .iter()
            .map(|(_, m)| *m)
            .fold(0.0f32, |acc, x| acc.max(x));

        // Filter peaks with at least 18% of max magnitude
        let threshold = max_mag * 0.18;
        let candidates: Vec<(u8, f32)> = detected_notes
            .into_iter()
            .filter(|&(_, m)| m >= threshold)
            .collect();

        // Prune harmonic overtones (octaves: +12, octaves+fifth: +19)
        // If a note has an overtone at +12 or +19 that is weaker, prune it
        let mut chord_notes = Vec::new();
        for i in 0..candidates.len() {
            let (note_i, mag_i) = candidates[i];
            let mut is_overtone = false;

            for j in 0..candidates.len() {
                if i == j {
                    continue;
                }
                let (note_j, mag_j) = candidates[j];
                if (note_i == note_j + 12 || note_i == note_j + 19) && mag_i < mag_j * 0.75 {
                    is_overtone = true;
                    break;
                }
            }

            if !is_overtone {
                chord_notes.push(note_i);
            }
        }

        chord_notes.sort_unstable();
        chord_notes.dedup();

        let total_duration_beats = (samples.len() as f64 / sample_rate as f64) * (bpm / 60.0);
        let note_duration = total_duration_beats.max(1.0);

        for note in chord_notes {
            results.push(TranscribedMidiNote::new(note, 100, 0.0, note_duration));
        }

        results
    }
}

/// Helper: Compute spectral centroid and multi-band energies for transient classification.
/// Returns (centroid_hz, low_band_energy, mid_band_energy, high_band_energy).
fn compute_spectral_analysis(samples: &[f32], sample_rate: u32) -> (f64, f64, f64, f64) {
    let n = samples.len();
    if n == 0 {
        return (0.0, 0.0, 0.0, 0.0);
    }

    let num_bins = (n / 2).min(256);
    let mut num = 0.0f64;
    let mut den = 0.0f64;
    let mut low_en = 0.0f64;
    let mut mid_en = 0.0f64;
    let mut high_en = 0.0f64;

    for k in 1..num_bins {
        let freq = (k as f64 * sample_rate as f64) / n as f64;
        let mut real = 0.0f64;
        let mut imag = 0.0f64;

        for (i, &s) in samples.iter().enumerate() {
            let w = 0.5 * (1.0 - (2.0 * PI * i as f64 / n as f64).cos());
            let win_s = s as f64 * w;
            let angle = 2.0 * PI * (k * i) as f64 / n as f64;
            real += win_s * angle.cos();
            imag -= win_s * angle.sin();
        }

        let mag = (real * real + imag * imag).sqrt();
        num += freq * mag;
        den += mag;

        if freq < 250.0 {
            low_en += mag;
        } else if freq < 2500.0 {
            mid_en += mag;
        } else {
            high_en += mag;
        }
    }

    let centroid = if den > 1e-6 { num / den } else { 0.0 };
    (centroid, low_en, mid_en, high_en)
}

/// Helper: YIN algorithm for monophonic fundamental pitch estimation.
fn estimate_pitch_yin(samples: &[f32], sample_rate: u32) -> Option<f64> {
    let n = samples.len();
    let max_tau = (sample_rate as f64 / 50.0) as usize; // min 50 Hz
    let min_tau = (sample_rate as f64 / 1500.0) as usize; // max 1500 Hz

    if n < max_tau * 2 {
        return None;
    }

    let mut d = vec![0.0f64; max_tau];

    // Step 1: Difference function
    for tau in 1..max_tau {
        let mut sum = 0.0f64;
        for i in 0..max_tau {
            let diff = samples[i] as f64 - samples[i + tau] as f64;
            sum += diff * diff;
        }
        d[tau] = sum;
    }

    // Step 2: Cumulative mean normalized difference
    let mut d_prime = vec![1.0f64; max_tau];
    let mut running_sum = 0.0f64;

    for tau in 1..max_tau {
        running_sum += d[tau];
        d_prime[tau] = if running_sum > 0.0 {
            d[tau] * tau as f64 / running_sum
        } else {
            1.0
        };
    }

    // Step 3: Absolute threshold
    let threshold = 0.15;
    let mut best_tau = 0;

    for tau in min_tau..max_tau {
        if d_prime[tau] < threshold {
            let mut local_min = tau;
            while local_min + 1 < max_tau && d_prime[local_min + 1] < d_prime[local_min] {
                local_min += 1;
            }
            best_tau = local_min;
            break;
        }
    }

    if best_tau == 0 {
        // Fallback: find global minimum in valid range
        let mut min_val = f64::MAX;
        for tau in min_tau..max_tau {
            if d_prime[tau] < min_val {
                min_val = d_prime[tau];
                best_tau = tau;
            }
        }
        if min_val > 0.45 {
            return None;
        }
    }

    // Parabolic interpolation around best_tau
    let tau_f = if best_tau > 0 && best_tau + 1 < max_tau {
        let s0 = d_prime[best_tau - 1];
        let s1 = d_prime[best_tau];
        let s2 = d_prime[best_tau + 1];
        let denom = 2.0 * (2.0 * s1 - s0 - s2);
        if denom.abs() > 1e-6 {
            best_tau as f64 + (s2 - s0) / denom
        } else {
            best_tau as f64
        }
    } else {
        best_tau as f64
    };

    if tau_f > 0.0 {
        Some(sample_rate as f64 / tau_f)
    } else {
        None
    }
}

/// Helper: Compute discrete Fourier magnitude at exact target frequency.
fn compute_fourier_magnitude_at_freq(samples: &[f32], sample_rate: u32, freq: f64) -> f32 {
    let mut real = 0.0f64;
    let mut imag = 0.0f64;
    let n = samples.len();

    for (i, &s) in samples.iter().enumerate() {
        let angle = 2.0 * PI * freq * (i as f64) / (sample_rate as f64);
        real += s as f64 * angle.cos();
        imag -= s as f64 * angle.sin();
    }

    ((real * real + imag * imag).sqrt() / n as f64 * 2.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_melody_to_midi_pitch_transcription_440hz() {
        // Generate pure 440 Hz sine wave (A4, MIDI Note 69)
        let sample_rate = 44100;
        let bpm = 120.0;
        let duration_secs = 0.5;
        let total_samples = (sample_rate as f64 * duration_secs) as usize;

        let mut samples = Vec::with_capacity(total_samples);
        for i in 0..total_samples {
            let t = i as f64 / sample_rate as f64;
            let s = (2.0 * PI * 440.0 * t).sin() as f32 * 0.8;
            samples.push(s);
        }

        let notes = AudioToMidiConverter::convert_melody_to_midi(&samples, sample_rate, bpm);
        assert!(!notes.is_empty(), "Transcribed melody must not be empty");
        assert_eq!(
            notes[0].note, 69,
            "440 Hz sine wave must transcribe to MIDI note 69 (A4)"
        );
        assert!(notes[0].velocity > 50, "Velocity must reflect high amplitude");
        assert!(notes[0].duration_beats > 0.5, "Duration must reflect tone length");
    }

    #[test]
    fn test_drum_transient_classification_kick_snare_hat() {
        let sample_rate = 44100;
        let bpm = 120.0;

        // 1. Synthetic Kick: 55 Hz sine with fast exponential pitch and amp decay
        let mut kick_samples = vec![0.0f32; 8000];
        for i in 0..8000 {
            let t = i as f64 / sample_rate as f64;
            let freq = 120.0 * (-t * 30.0).exp() + 50.0;
            let env = (-t * 15.0).exp() as f32;
            kick_samples[i] = (2.0 * PI * freq * t).sin() as f32 * env;
        }

        // 2. Synthetic Snare: 200 Hz tone + broadband mid noise
        let mut snare_samples = vec![0.0f32; 8000];
        for i in 0..8000 {
            let t = i as f64 / sample_rate as f64;
            let env = (-t * 20.0).exp() as f32;
            let tone = (2.0 * PI * 220.0 * t).sin() as f32 * 0.4;
            // mid-frequency pseudo-random noise
            let noise = (((i * 1103515245 + 12345) % 1000) as f32 / 500.0 - 1.0) * 0.6;
            snare_samples[i] = (tone + noise) * env;
        }

        // 3. Synthetic Hi-Hat: alternating high-frequency noise
        let mut hat_samples = vec![0.0f32; 8000];
        for i in 0..8000 {
            let t = i as f64 / sample_rate as f64;
            let env = (-t * 45.0).exp() as f32;
            let high_tone = (2.0 * PI * 8000.0 * t).sin() as f32 * 0.8;
            hat_samples[i] = high_tone * env;
        }

        let kick_midi = AudioToMidiConverter::convert_drums_to_midi(&kick_samples, sample_rate, bpm);
        let snare_midi = AudioToMidiConverter::convert_drums_to_midi(&snare_samples, sample_rate, bpm);
        let hat_midi = AudioToMidiConverter::convert_drums_to_midi(&hat_samples, sample_rate, bpm);

        assert!(!kick_midi.is_empty());
        assert_eq!(kick_midi[0].note, 36, "Kick must be classified as MIDI 36 (C1)");

        assert!(!snare_midi.is_empty());
        assert_eq!(snare_midi[0].note, 38, "Snare must be classified as MIDI 38 (D1)");

        assert!(!hat_midi.is_empty());
        assert_eq!(hat_midi[0].note, 42, "Hi-Hat must be classified as MIDI 42 (F#1)");
    }

    #[test]
    fn test_harmony_chord_extraction_c_major_triad() {
        let sample_rate = 44100;
        let bpm = 120.0;
        let duration = 0.5;
        let total_samples = (sample_rate as f64 * duration) as usize;

        // C Major Triad: C4 (60: 261.63 Hz), E4 (64: 329.63 Hz), G4 (67: 392.00 Hz)
        let mut samples = vec![0.0f32; total_samples];
        for i in 0..total_samples {
            let t = i as f64 / sample_rate as f64;
            let c = (2.0 * PI * 261.63 * t).sin() as f32 * 0.3;
            let e = (2.0 * PI * 329.63 * t).sin() as f32 * 0.3;
            let g = (2.0 * PI * 392.00 * t).sin() as f32 * 0.3;
            samples[i] = c + e + g;
        }

        let chord = AudioToMidiConverter::convert_harmony_to_midi(&samples, sample_rate, bpm);
        let pitches: Vec<u8> = chord.iter().map(|n| n.note).collect();

        assert!(
            pitches.contains(&60),
            "C Major triad must contain root C4 (60), got: {:?}",
            pitches
        );
        assert!(
            pitches.contains(&64),
            "C Major triad must contain major third E4 (64), got: {:?}",
            pitches
        );
        assert!(
            pitches.contains(&67),
            "C Major triad must contain fifth G4 (67), got: {:?}",
            pitches
        );
    }

    #[test]
    fn test_groove_template_extract_and_apply_timing() {
        let sample_rate = 44100;
        let bpm = 120.0;
        // 1 bar = 2.0 seconds at 120 BPM
        let total_samples = sample_rate * 2;
        let mut samples = vec![0.0f32; total_samples as usize];

        // Place a swung transient at step 1 (ideal beat 0.25 = 0.125s), offset by +25 ticks (~13ms)
        let samples_per_16th = (44100.0 * 0.125) as usize;
        // Step 1 ideal sample = 5512. Put burst at 5512 + 250 (~5.6ms delay)
        let hit_sample = samples_per_16th + 250;
        for i in 0..100 {
            if hit_sample + i < samples.len() {
                samples[hit_sample + i] = 0.9;
            }
        }

        let groove = GrooveTemplate::extract_groove_from_audio(&samples, sample_rate, bpm);
        assert!(
            groove.timing_offsets[1] > 0,
            "Groove step 1 must record positive delayed timing offset, got {}",
            groove.timing_offsets[1]
        );

        // Apply groove to note at beat 0.25
        let mut notes = vec![(60u8, 100u8, 0.25f64, 0.25f64)];
        groove.apply_groove_to_notes(&mut notes);

        assert!(
            notes[0].2 > 0.25,
            "Applied groove must shift note start beat forward from 0.25, got {}",
            notes[0].2
        );
    }

    #[test]
    fn test_groove_template_negative_offsets_and_bounds() {
        let sample_rate = 44100;
        let bpm = 120.0;
        let total_samples = sample_rate * 2;
        let mut samples = vec![0.0f32; total_samples as usize];

        let samples_per_16th = (44100.0 * 0.125) as usize;
        // Step 1 ideal sample = 5512. Put early transient (rushing) at 5512 - 250 (~5.6ms early)
        let hit_sample = samples_per_16th - 250;
        for i in 0..100 {
            if hit_sample + i < samples.len() {
                samples[hit_sample + i] = 0.95;
            }
        }

        let groove = GrooveTemplate::extract_groove_from_audio(&samples, sample_rate, bpm);
        assert!(
            groove.timing_offsets[1] < 0,
            "Groove step 1 must record negative early timing offset, got {}",
            groove.timing_offsets[1]
        );
        assert!(
            groove.timing_offsets[1] >= -120,
            "Timing offset must be clamped within -120 ticks, got {}",
            groove.timing_offsets[1]
        );

        // Apply groove to note at beat 0.25
        let mut notes = vec![(60u8, 100u8, 0.25f64, 0.25f64)];
        groove.apply_groove_to_notes(&mut notes);

        assert!(
            notes[0].2 < 0.25,
            "Applied groove with negative offset must shift note start beat earlier than 0.25, got {}",
            notes[0].2
        );

        // Verify clamping and bounds
        for &offset in groove.timing_offsets.iter() {
            assert!(offset >= -120 && offset <= 120, "Offset {} out of [-120, 120]", offset);
        }
        for &vel in groove.velocity_scalings.iter() {
            assert!(vel >= 0.0 && vel <= 2.0, "Velocity scaling {} out of [0.0, 2.0]", vel);
        }
    }
}

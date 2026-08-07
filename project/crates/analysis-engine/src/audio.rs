//! Audio analyzer: speech transcription, audio event detection, and copyright checking.
//!
//! Uses heuristics based on waveform statistics and spectral features since no
//! real ASR or audio event model is available.  The public API matches what a
//! real pipeline would expose so swapping in ONNX / Whisper / etc. is seamless.

use sentinel_core::error::SentinelError;
use sentinel_core::proto::{
    AudioAnalysis, AudioEvent, CopyrightMatch, Timestamp, TranscriptSegment,
};
use std::path::Path;
use tracing::{debug, info, instrument, warn};

// ===========================================================================
// AudioAnalyzer
// ===========================================================================

/// Configuration for the audio analysis pipeline.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioAnalyzerConfig {
    /// Minimum confidence for transcript segments (0.0..1.0).
    pub transcript_threshold: f64,
    /// Minimum dBFS for audio event detection.
    pub event_db_threshold: f64,
    /// Duration of each transcript segment in seconds.
    pub segment_duration_sec: f64,
    /// Whether to enable copyright checking.
    pub enable_copyright: bool,
    /// Similarity threshold for copyright fingerprint matching.
    pub copyright_similarity_threshold: f64,
    /// Sample rate for analysis (Hz).
    pub sample_rate: u32,
}

impl Default for AudioAnalyzerConfig {
    fn default() -> Self {
        AudioAnalyzerConfig {
            transcript_threshold: 0.5,
            event_db_threshold: -30.0,
            segment_duration_sec: 5.0,
            enable_copyright: true,
            copyright_similarity_threshold: 0.7,
            sample_rate: 44100,
        }
    }
}

impl AudioAnalyzerConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_transcript_threshold(mut self, t: f64) -> Self {
        self.transcript_threshold = t.clamp(0.0, 1.0);
        self
    }

    pub fn with_event_db(mut self, db: f64) -> Self {
        self.event_db_threshold = db;
        self
    }

    pub fn with_copyright(mut self, enable: bool) -> Self {
        self.enable_copyright = enable;
        self
    }
}

/// Audio analysis pipeline using signal-processing heuristics.
pub struct AudioAnalyzer {
    config: AudioAnalyzerConfig,
}

/// A raw audio buffer with associated metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioBuffer {
    /// Interleaved f32 samples (-1.0 to 1.0).
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_sec: f64,
}

impl AudioBuffer {
    /// Compute the duration in seconds.
    pub fn duration(&self) -> f64 {
        let total_samples = self.samples.len() as f64;
        total_samples / (self.sample_rate as f64 * self.channels as f64)
    }

    /// Convert to mono by averaging channels.
    pub fn to_mono(&self) -> Vec<f32> {
        if self.channels == 1 {
            return self.samples.clone();
        }
        let frames = self.samples.len() / self.channels as usize;
        let mut mono = Vec::with_capacity(frames);
        for f in 0..frames {
            let mut sum = 0.0f32;
            for ch in 0..self.channels as usize {
                sum += self.samples[f * self.channels as usize + ch];
            }
            mono.push(sum / self.channels as f32);
        }
        mono
    }

    /// Compute RMS (root-mean-square) energy of the audio.
    pub fn rms_energy(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let sum_sq: f64 = self.samples.iter().map(|&s| (s as f64).powi(2)).sum();
        (sum_sq / self.samples.len() as f64).sqrt()
    }

    /// Convert RMS to dBFS.
    pub fn rms_dbfs(&self) -> f64 {
        let rms = self.rms_energy();
        if rms <= 0.0 {
            return f64::NEG_INFINITY;
        }
        20.0 * rms.log10()
    }

    /// Compute zero-crossing rate (transitions from positive to negative or vice versa).
    pub fn zero_crossing_rate(&self) -> f64 {
        if self.samples.len() < 2 {
            return 0.0;
        }
        let mono = self.to_mono();
        let zc: u64 = mono.windows(2)
            .filter(|w| w[0].signum() != w[1].signum())
            .count() as u64;
        zc as f64 / mono.len().max(1) as f64
    }

    /// Compute spectral centroid from a basic DFT approximation.
    /// Uses a simplified energy-weighted average frequency estimate.
    pub fn spectral_centroid(&self) -> f64 {
        let mono = self.to_mono();
        if mono.is_empty() {
            return 0.0;
        }

        // Divide into short windows and compute energy per frequency band.
        let window_size = 1024usize;
        if mono.len() < window_size {
            return self.sample_rate as f64 / 4.0; // fallback
        }

        let mut total_energy = 0.0f64;
        let mut weighted_freq_sum = 0.0f64;
        let num_windows = mono.len() / window_size;

        for w in 0..num_windows {
            let start = w * window_size;
            let window = &mono[start..start + window_size];

            // Simple energy per "band" using accumulated power.
            // Divide into 8 frequency bands via decimation.
            for band in 0..8 {
                let mut energy = 0.0f64;
                let mut count = 0usize;
                for (i, &sample) in window.iter().enumerate().step_by(band + 1) {
                    energy += (sample as f64).powi(2);
                    count += 1;
                    if count >= window_size / (band + 1) {
                        break;
                    }
                }
                let freq = (self.sample_rate as f64 / 2.0) * (band as f64 + 0.5) / 8.0;
                total_energy += energy;
                weighted_freq_sum += freq * energy;
            }
        }

        if total_energy < 1e-10 {
            0.0
        } else {
            weighted_freq_sum / total_energy
        }
    }

    /// Compute the dynamic range (difference between max and min dB).
    pub fn dynamic_range_db(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let max_sample = self.samples.iter().map(|&s| s.abs()).fold(0.0f32, f32::max);
        let min_nonzero = self.samples
            .iter()
            .filter(|&&s| s != 0.0)
            .map(|&s| s.abs())
            .fold(1.0f32, f32::min);
        if max_sample <= 0.0 || min_nonzero <= 0.0 {
            return 0.0;
        }
        20.0 * (max_sample / min_nonzero).log10() as f64
    }
}

impl AudioAnalyzer {
    /// Create a new audio analyzer.
    pub fn new(config: AudioAnalyzerConfig) -> Self {
        AudioAnalyzer { config }
    }

    // ------------------------------------------------------------------
    // Mock Audio Loading
    // ------------------------------------------------------------------

    /// "Load" audio from a file path — returns a placeholder buffer.
    ///
    /// In production this would decode the audio file via ffmpeg or
    /// a Rust audio decoder.  For now we return a synthetic signal
    /// based on the file metadata so downstream analysis can proceed.
    #[instrument(skip(path))]
    pub fn load_audio<P: AsRef<Path>>(path: P) -> Result<AudioBuffer, SentinelError> {
        let path = path.as_ref();
        info!("loading audio from {}", path.display());

        if !path.exists() {
            warn!("audio file not found: {}", path.display());
            return Err(SentinelError::io(
                "load audio",
                format!("file not found: {}", path.display()),
            ));
        }

        // Read file size to scale the synthetic signal length.
        let metadata = std::fs::metadata(path).map_err(|e| SentinelError::io("metadata", e.to_string()))?;
        let file_size = metadata.len();

        // Estimate duration: ~180 KiB per second at 44.1kHz stereo f32.
        let bytes_per_sec = 44100.0 * 2.0 * 4.0;
        let duration_sec = (file_size as f64 / bytes_per_sec).clamp(1.0, 600.0);
        let total_samples = (duration_sec * bytes_per_sec / 2.0) as usize; // per channel

        // Generate a synthetic signal with varied characteristics.
        let samples = Self::generate_synthetic_signal(total_samples, duration_sec);

        Ok(AudioBuffer {
            samples,
            sample_rate: 44100,
            channels: 2,
            duration_sec,
        })
    }

    /// Generate a synthetic audio signal with varied segments.
    fn generate_synthetic_signal(sample_count: usize, duration_sec: f64) -> Vec<f32> {
        use std::f32::consts::PI;
        let mut samples = Vec::with_capacity(sample_count);
        let sr = 44100.0f32;

        // Divide duration into segments with different characteristics.
        let segments = 5usize;
        let samples_per_segment = sample_count / segments;

        for seg in 0..segments {
            let start_idx = seg * samples_per_segment;
            let end_idx = if seg == segments - 1 {
                sample_count
            } else {
                (seg + 1) * samples_per_segment
            };

            for i in start_idx..end_idx {
                let t = i as f32 / sr;
                let phase = (i - start_idx) as f32;

                let sample = match seg {
                    0 => {
                        // Segment 0: low-amplitude noise (silence/quiet).
                        let noise = (phase * 0.01).sin() * 0.05;
                        noise + ((phase * 0.003).sin() * 0.02)
                    }
                    1 => {
                        // Segment 1: speech-like (modulated sine waves).
                        let carrier = (phase * 0.05 * PI * 2.0).sin();
                        let modulator = (phase * 0.002).sin().abs() * 0.5 + 0.3;
                        carrier * modulator
                    }
                    2 => {
                        // Segment 2: loud / music-like (multiple frequencies).
                        let a = (phase * 0.08 * PI * 2.0).sin() * 0.4;
                        let b = (phase * 0.05 * PI * 2.0).sin() * 0.3;
                        let c = (phase * 0.12 * PI * 2.0).sin() * 0.2;
                        (a + b + c).clamp(-0.9, 0.9)
                    }
                    3 => {
                        // Segment 3: sharp transients (event-like).
                        if (phase as usize) % 1000 < 50 {
                            (phase * 0.2).sin() * 0.8
                        } else {
                            (phase * 0.01).sin() * 0.1
                        }
                    }
                    _ => {
                        // Segment 4: steady tone.
                        (phase * 0.03 * PI * 2.0).sin() * 0.4
                    }
                };

                samples.push(sample.clamp(-1.0, 1.0));
            }
        }

        debug!(
            "generated {} synthetic samples ({:.1}s)",
            samples.len(),
            duration_sec
        );
        samples
    }

    // ------------------------------------------------------------------
    // Speech Transcription (placeholder STT)
    // ------------------------------------------------------------------

    /// Placeholder speech-to-text that extracts audio features and returns
    /// synthetic transcript segments based on audio characteristics.
    ///
    /// In production this would call Whisper / wav2vec2 / etc.
    #[instrument(skip(self, audio_path))]
    pub fn transcribe_speech(
        &self,
        audio_path: &Path,
    ) -> Result<Vec<TranscriptSegment>, SentinelError> {
        info!("starting speech transcription for {}", audio_path.display());

        let buffer = Self::load_audio(audio_path)?;
        let mono = buffer.to_mono();
        let segment_samples = (self.config.segment_duration_sec * buffer.sample_rate as f64) as usize;

        if segment_samples == 0 || mono.is_empty() {
            return Ok(Vec::new());
        }

        let mut segments = Vec::new();
        let num_segments = (mono.len() + segment_samples - 1) / segment_samples;

        for seg_idx in 0..num_segments {
            let start_sample = seg_idx * segment_samples;
            let end_sample = (start_sample + segment_samples).min(mono.len());
            let seg_samples = &mono[start_sample..end_sample];

            if seg_samples.is_empty() {
                continue;
            }

            // Compute per-segment features.
            let rms: f64 = (seg_samples.iter().map(|&s| (s as f64).powi(2)).sum::<f64>()
                / seg_samples.len() as f64)
                .sqrt();
            let zcr = self.compute_zcr(seg_samples);
            let spectral_c = self.estimate_spectral_centroid(seg_samples, buffer.sample_rate);

            // Generate transcript text based on audio features.
            let (text, confidence) = self.classify_speech_segment(rms, zcr, spectral_c);

            if confidence < self.config.transcript_threshold {
                continue;
            }

            let start_sec = start_sample as f64 / buffer.sample_rate as f64;
            let end_sec = end_sample as f64 / buffer.sample_rate as f64;

            // Sentiment estimation based on spectral tilt.
            let sentiment = ((spectral_c / 8000.0) - 0.5).clamp(-1.0, 1.0);

            let segment = TranscriptSegment {
                start: Timestamp::new(start_sec, start_sample as u64),
                end: Timestamp::new(end_sec, end_sample as u64),
                text,
                confidence: confidence.clamp(0.0, 1.0),
                sentiment,
                is_sarcasm: false,
                is_background_speech: rms < 0.05,
            };

            segments.push(segment);
        }

        info!(
            "transcription complete: {} segments",
            segments.len()
        );
        Ok(segments)
    }

    /// Classify a speech segment based on extracted features.
    fn classify_speech_segment(&self, rms: f64, zcr: f64, spectral_centroid: f64) -> (String, f64) {
        // Low energy = silence or background.
        if rms < 0.02 {
            return ("[silence]".into(), 0.9);
        }

        // High zero-crossing rate + moderate energy = fricative / sibilant speech.
        if zcr > 0.15 && rms > 0.05 && rms < 0.3 {
            let confidence = (zcr * 2.0 + rms * 2.0).clamp(0.4, 0.9);
            return ("spoken words detected".into(), confidence);
        }

        // High spectral centroid + high energy = likely music or loud noise.
        if spectral_centroid > 3000.0 && rms > 0.3 {
            let confidence = (spectral_centroid / 10000.0 * rms * 2.0).clamp(0.3, 0.8);
            return ("[music or loud sound]".into(), confidence);
        }

        // Moderate energy, moderate ZCR = likely speech.
        if rms > 0.03 && zcr > 0.05 {
            let confidence = (rms * 3.0 + zcr * 3.0).clamp(0.3, 0.85);
            return ("speech detected".into(), confidence);
        }

        // Default: indistinct audio.
        let confidence = (rms * 2.0).clamp(0.1, 0.5);
        ("[indistinct audio]".into(), confidence)
    }

    // ------------------------------------------------------------------
    // Audio Event Detection
    // ------------------------------------------------------------------

    /// Detect acoustic events: loud segments, potential music, silence patterns.
    #[instrument(skip(self, audio_path))]
    pub fn detect_events(
        &self,
        audio_path: &Path,
    ) -> Result<Vec<AudioEvent>, SentinelError> {
        info!("detecting audio events for {}", audio_path.display());

        let buffer = Self::load_audio(audio_path)?;
        let mono = buffer.to_mono();

        if mono.is_empty() {
            return Ok(Vec::new());
        }

        let mut events = Vec::new();
        let window_size = (buffer.sample_rate as f64 * 0.5) as usize; // 500ms windows
        let hop_size = window_size / 2;

        let mut prev_rms = 0.0f64;
        let mut silence_start: Option<usize> = None;

        for (w_idx, window) in mono.windows(window_size).step_by(hop_size).enumerate() {
            let rms = (window.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / window.len() as f64).sqrt();
            let db = if rms > 0.0 { 20.0 * rms.log10() } else { f64::NEG_INFINITY };
            let zcr = self.compute_zcr(window);
            let spectral_c = self.estimate_spectral_centroid(window, buffer.sample_rate);

            let time_sec = (w_idx * hop_size) as f64 / buffer.sample_rate as f64;
            let frame_num = (w_idx * hop_size) as u64;

            // Loud event detection.
            if db > self.config.event_db_threshold && rms > 0.1 {
                let event_type = if spectral_c > 4000.0 {
                    "loud_high_freq"
                } else if spectral_c > 1000.0 {
                    "loud_mid_freq"
                } else {
                    "loud_low_freq"
                };

                let confidence = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
                let volume_db = db;

                // Only add if significantly louder than previous window.
                if (rms - prev_rms).abs() > 0.05 {
                    events.push(AudioEvent {
                        timestamp: Timestamp::new(time_sec, frame_num),
                        event_type: event_type.into(),
                        confidence,
                        volume_db,
                    });
                }
            }

            // Silence detection.
            if db < -50.0 {
                if silence_start.is_none() {
                    silence_start = Some(w_idx * hop_size);
                }
            } else if let Some(start) = silence_start {
                let silence_dur = ((w_idx * hop_size - start) as f64) / buffer.sample_rate as f64;
                if silence_dur > 1.0 {
                    events.push(AudioEvent {
                        timestamp: Timestamp::new(
                            start as f64 / buffer.sample_rate as f64,
                            start as u64,
                        ),
                        event_type: "extended_silence".into(),
                        confidence: (silence_dur / 5.0).clamp(0.3, 1.0),
                        volume_db: -60.0,
                    });
                }
                silence_start = None;
            }

            // Music detection: harmonic structure with moderate ZCR and spread spectrum.
            if zcr > 0.02 && zcr < 0.15 && spectral_c > 500.0 && spectral_c < 6000.0 && rms > 0.03 {
                let music_confidence = ((1.0 - (zcr - 0.08).abs() / 0.1) * rms * 10.0).clamp(0.0, 0.7);
                if music_confidence > 0.3 {
                    events.push(AudioEvent {
                        timestamp: Timestamp::new(time_sec, frame_num),
                        event_type: "potential_music".into(),
                        confidence: music_confidence,
                        volume_db: db,
                    });
                }
            }

            // Sharp transient detection.
            if prev_rms > 0.01 && rms / prev_rms > 5.0 {
                events.push(AudioEvent {
                    timestamp: Timestamp::new(time_sec, frame_num),
                    event_type: "sharp_transient".into(),
                    confidence: ((rms / prev_rms).ln() / 3.0).clamp(0.3, 1.0),
                    volume_db: db,
                });
            }

            prev_rms = rms;
        }

        // Deduplicate: sort by time and remove overlapping events of the same type.
        events.sort_by(|a, b| a.timestamp.seconds.partial_cmp(&b.timestamp.seconds).unwrap());
        let mut deduped = Vec::new();
        let min_gap_sec = 0.5;
        for event in events {
            if let Some(prev) = deduped.last() {
                if prev.event_type == event.event_type
                    && (event.timestamp.seconds - prev.timestamp.seconds).abs() < min_gap_sec
                {
                    continue;
                }
            }
            deduped.push(event);
        }

        info!("detected {} audio events ({} after dedup)", events.len(), deduped.len());
        Ok(deduped)
    }

    // ------------------------------------------------------------------
    // Copyright Checking
    // ------------------------------------------------------------------

    /// Check audio against known copyrighted works using fingerprint comparison.
    ///
    /// This is a placeholder that generates synthetic fingerprints and
    /// compares them.  In production this would use the Zig FFI or
    /// a Rust chromaprint implementation.
    #[instrument(skip(self, audio_path))]
    pub fn check_copyright(
        &self,
        audio_path: &Path,
    ) -> Result<Vec<CopyrightMatch>, SentinelError> {
        if !self.config.enable_copyright {
            debug!("copyright checking disabled");
            return Ok(Vec::new());
        }

        info!("checking copyright for {}", audio_path.display());

        let buffer = Self::load_audio(audio_path)?;
        let mono = buffer.to_mono();

        if mono.is_empty() {
            return Ok(Vec::new());
        }

        // Compute a simple audio fingerprint (spectral summary hash).
        let fingerprint = self.compute_simple_fingerprint(&mono, buffer.sample_rate);

        // Compare against a set of known reference fingerprints (placeholder database).
        let known_works = self.known_reference_fingerprints();
        let mut matches = Vec::new();

        for (work_name, holder, ref_fp) in &known_works {
            let similarity = self.compare_fingerprints(&fingerprint, ref_fp);

            if similarity >= self.config.copyright_similarity_threshold {
                let segment_duration = mono.len() as f64 / buffer.sample_rate as f64;
                let cm = CopyrightMatch::new(work_name.clone(), holder.clone())
                    .with_confidence(similarity)
                    .with_match_type("spectral_similarity")
                    .with_span(
                        Timestamp::new(0.0, 0),
                        Timestamp::new(segment_duration, mono.len() as u64),
                    );
                matches.push(cm);
            }
        }

        // Also check sub-segments for partial matches.
        let segment_len = (buffer.sample_rate as f64 * 10.0) as usize; // 10s segments
        if mono.len() > segment_len * 2 {
            for seg_idx in 0..3 {
                let start = seg_idx * segment_len;
                let end = (start + segment_len).min(mono.len());
                if end <= start {
                    break;
                }
                let seg_fp = self.compute_simple_fingerprint(&mono[start..end], buffer.sample_rate);
                for (work_name, holder, ref_fp) in &known_works {
                    let sim = self.compare_fingerprints(&seg_fp, ref_fp);
                    if sim >= self.config.copyright_similarity_threshold * 0.85 {
                        let start_sec = start as f64 / buffer.sample_rate as f64;
                        let end_sec = end as f64 / buffer.sample_rate as f64;
                        let cm = CopyrightMatch::new(format!("{} (partial)", work_name), holder.clone())
                            .with_confidence(sim * 0.9)
                            .with_match_type("spectral_partial_match")
                            .with_span(
                                Timestamp::new(start_sec, start as u64),
                                Timestamp::new(end_sec, end as u64),
                            );
                        if !matches.iter().any(|m| m.matched_work == cm.matched_work) {
                            matches.push(cm);
                        }
                    }
                }
            }
        }

        info!("copyright check: {} matches found", matches.len());
        Ok(matches)
    }

    // ------------------------------------------------------------------
    // Fingerprinting helpers (placeholder)
    // ------------------------------------------------------------------

    /// Compute a simple spectral fingerprint: energy histogram across frequency bands.
    fn compute_simple_fingerprint(&self, samples: &[f32], sample_rate: u32) -> Vec<f32> {
        let window_size = 2048usize;
        if samples.len() < window_size {
            return vec![0.0f32; 16];
        }

        let num_windows = (samples.len() / window_size).max(1);
        let num_bands = 16;
        let mut band_energies = vec![0.0f64; num_bands];

        for w in 0..num_windows.min(50) {
            let start = w * window_size;
            let window = &samples[start..(start + window_size).min(samples.len())];

            // Simple DFT energy per band using autocorrelation-based frequency estimation.
            for band in 0..num_bands {
                let freq_low = band as f64 * (sample_rate as f64 / 2.0) / num_bands as f64;
                let freq_high = (band + 1) as f64 * (sample_rate as f64 / 2.0) / num_bands as f64;
                let period_low = if freq_low > 0.0 {
                    (sample_rate as f64 / freq_low) as usize
                } else {
                    window.len()
                };
                let period_high = (sample_rate as f64 / freq_high) as usize;

                let mut energy = 0.0f64;
                let mut count = 0usize;

                // Energy at this band via lag autocorrelation.
                for lag in period_high..period_low.min(window.len() / 2) {
                    let mut lag_energy = 0.0f64;
                    for i in lag..window.len() {
                        lag_energy += (window[i] * window[i - lag]) as f64;
                    }
                    energy += lag_energy.abs();
                    count += 1;
                    if count > 20 {
                        break;
                    }
                }

                band_energies[band] += energy / count.max(1) as f64;
            }
        }

        // Normalize to unit vector.
        let total_energy: f64 = band_energies.iter().sum();
        if total_energy > 0.0 {
            band_energies.iter().map(|&e| (e / total_energy) as f32).collect()
        } else {
            vec![0.0f32; num_bands]
        }
    }

    /// Compare two fingerprints using cosine similarity.
    fn compare_fingerprints(&self, a: &[f32], b: &[f32]) -> f64 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }
        let dot: f64 = a.iter().zip(b.iter()).map(|(&x, &y)| (x * y) as f64).sum();
        let norm_a: f64 = a.iter().map(|&x| (x * x) as f64).sum::<f64>().sqrt();
        let norm_b: f64 = b.iter().map(|&x| (x * x) as f64).sum::<f64>().sqrt();
        if norm_a < 1e-10 || norm_b < 1e-10 {
            return 0.0;
        }
        (dot / (norm_a * norm_b)).clamp(0.0, 1.0)
    }

    /// Known reference fingerprints for placeholder copyright database.
    fn known_reference_fingerprints(&self) -> Vec<(String, String, Vec<f32>)> {
        // These are synthetic fingerprints — real ones would come from a database.
        vec![
            (
                "Unknown Hit Song".into(),
                "Placeholder Records".into(),
                // A fingerprint with distinct peaks in mid-high bands.
                vec![
                    0.01, 0.02, 0.03, 0.05, 0.08, 0.10, 0.12, 0.15,
                    0.12, 0.10, 0.08, 0.06, 0.04, 0.03, 0.02, 0.01,
                ],
            ),
            (
                "Popular Track A".into(),
                "Major Label".into(),
                vec![
                    0.02, 0.01, 0.03, 0.02, 0.05, 0.07, 0.09, 0.11,
                    0.13, 0.11, 0.09, 0.07, 0.05, 0.03, 0.02, 0.01,
                ],
            ),
            (
                "Classical Piece B".into(),
                "Classical Label".into(),
                vec![
                    0.05, 0.06, 0.07, 0.08, 0.07, 0.06, 0.05, 0.04,
                    0.03, 0.03, 0.02, 0.02, 0.02, 0.01, 0.01, 0.01,
                ],
            ),
        ]
    }

    // ------------------------------------------------------------------
    // Full audio pipeline
    // ------------------------------------------------------------------

    /// Run the complete audio analysis pipeline.
    #[instrument(skip(self, video_path))]
    pub fn analyze(
        &self,
        video_path: &Path,
        enable_copyright: bool,
    ) -> Result<AudioAnalysis, SentinelError> {
        info!(
            "starting audio analysis for {}, copyright={}",
            video_path.display(),
            enable_copyright
        );

        let mut config = self.config.clone();
        config.enable_copyright = enable_copyright;

        // Extract audio from video — try to find the audio track file.
        // In a real system we'd demux audio from the video container.
        let audio_path = video_path;

        let transcript = self.transcribe_speech(audio_path)?;
        let events = self.detect_events(audio_path)?;
        let copyright_matches = if enable_copyright {
            self.check_copyright(audio_path)?
        } else {
            Vec::new()
        };

        let mut analysis = AudioAnalysis {
            transcript,
            events,
            copyright_matches,
            overall_risk_score: 0.0,
        };

        analysis.compute_risk_score();

        info!(
            "audio analysis complete: {} transcript segments, {} events, {} copyright matches, score={:.2}",
            analysis.transcript.len(),
            analysis.events.len(),
            analysis.copyright_matches.len(),
            analysis.overall_risk_score
        );

        Ok(analysis)
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    fn compute_zcr(&self, samples: &[f32]) -> f64 {
        if samples.len() < 2 {
            return 0.0;
        }
        let zc = samples.windows(2).filter(|w| w[0].signum() != w[1].signum()).count();
        zc as f64 / samples.len() as f64
    }

    fn estimate_spectral_centroid(&self, samples: &[f32], sample_rate: u32) -> f64 {
        if samples.is_empty() {
            return 0.0;
        }

        let window_size = samples.len().min(1024);
        let mut total_energy = 0.0f64;
        let mut weighted_sum = 0.0f64;

        // Use energy distribution across frequency bands as centroid estimate.
        let num_bands = 8usize;
        for band in 0..num_bands {
            let step = band + 1;
            let mut energy = 0.0f64;
            let mut count = 0usize;
            for (i, &s) in samples.iter().enumerate().step_by(step) {
                if i >= window_size {
                    break;
                }
                energy += (s as f64).powi(2);
                count += 1;
                if count >= window_size / step {
                    break;
                }
            }
            let freq = (sample_rate as f64 / 2.0) * (band as f64 + 0.5) / num_bands as f64;
            total_energy += energy;
            weighted_sum += freq * energy;
        }

        if total_energy < 1e-10 {
            0.0
        } else {
            weighted_sum / total_energy
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_config_default() {
        let cfg = AudioAnalyzerConfig::default();
        assert_eq!(cfg.transcript_threshold, 0.5);
        assert_eq!(cfg.sample_rate, 44100);
        assert!(cfg.enable_copyright);
    }

    #[test]
    fn test_config_builder() {
        let cfg = AudioAnalyzerConfig::new()
            .with_transcript_threshold(0.8)
            .with_copyright(false);
        assert_eq!(cfg.transcript_threshold, 0.8);
        assert!(!cfg.enable_copyright);
    }

    #[test]
    fn test_audio_buffer_rms_energy() {
        let samples = vec![0.5f32, -0.5, 0.5, -0.5];
        let buffer = AudioBuffer {
            samples,
            sample_rate: 44100,
            channels: 1,
            duration_sec: 1.0,
        };
        let rms = buffer.rms_energy();
        assert!((rms - 0.5).abs() < 0.01, "rms={}", rms);
    }

    #[test]
    fn test_audio_buffer_rms_silence() {
        let samples = vec![0.0f32; 1000];
        let buffer = AudioBuffer {
            samples,
            sample_rate: 44100,
            channels: 1,
            duration_sec: 1.0,
        };
        assert_eq!(buffer.rms_energy(), 0.0);
    }

    #[test]
    fn test_audio_buffer_to_mono() {
        let samples = vec![0.5f32, 0.3, 0.5, 0.3, 0.5, 0.3]; // 3 stereo frames
        let buffer = AudioBuffer {
            samples,
            sample_rate: 44100,
            channels: 2,
            duration_sec: 1.0,
        };
        let mono = buffer.to_mono();
        assert_eq!(mono.len(), 3);
        assert!((mono[0] - 0.4).abs() < 0.01);
    }

    #[test]
    fn test_audio_buffer_mono_identity() {
        let samples = vec![0.5f32, -0.3, 0.8];
        let buffer = AudioBuffer {
            samples: samples.clone(),
            sample_rate: 44100,
            channels: 1,
            duration_sec: 1.0,
        };
        let mono = buffer.to_mono();
        assert_eq!(mono, samples);
    }

    #[test]
    fn test_audio_buffer_zero_crossing_rate() {
        // Alternating sign: maximum ZCR.
        let samples: Vec<f32> = (0..100).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect();
        let buffer = AudioBuffer {
            samples,
            sample_rate: 44100,
            channels: 1,
            duration_sec: 1.0,
        };
        let zcr = buffer.zero_crossing_rate();
        assert!(zcr > 0.9 && zcr <= 1.0, "zcr={}", zcr);
    }

    #[test]
    fn test_audio_buffer_dynamic_range() {
        let samples = vec![0.01f32, 0.0, 1.0, 0.0, -1.0];
        let buffer = AudioBuffer {
            samples,
            sample_rate: 44100,
            channels: 1,
            duration_sec: 1.0,
        };
        let range = buffer.dynamic_range_db();
        assert!(range > 0.0, "range={}", range);
    }

    #[test]
    fn test_generate_synthetic_signal() {
        let samples = AudioAnalyzer::generate_synthetic_signal(44100, 1.0);
        assert_eq!(samples.len(), 44100);
        // All samples should be in [-1, 1].
        for &s in &samples {
            assert!(s >= -1.0 && s <= 1.0, "sample out of range: {}", s);
        }
    }

    #[test]
    fn test_classify_speech_silence() {
        let analyzer = AudioAnalyzer::new(AudioAnalyzerConfig::default());
        let (text, conf) = analyzer.classify_speech_segment(0.005, 0.05, 1000.0);
        assert_eq!(text, "[silence]");
        assert!(conf > 0.8);
    }

    #[test]
    fn test_classify_speech_normal() {
        let analyzer = AudioAnalyzer::new(AudioAnalyzerConfig::default());
        let (text, conf) = analyzer.classify_speech_segment(0.1, 0.1, 2000.0);
        assert!(text.contains("speech") || text.contains("words"));
        assert!(conf > 0.3);
    }

    #[test]
    fn test_fingerprint_cosine_similarity() {
        let analyzer = AudioAnalyzer::new(AudioAnalyzerConfig::default());
        let a = vec![1.0f32, 0.0, 0.0, 0.0];
        let b = vec![1.0f32, 0.0, 0.0, 0.0];
        assert!((analyzer.compare_fingerprints(&a, &b) - 1.0).abs() < 0.01);

        let c = vec![0.0f32, 1.0, 0.0, 0.0];
        assert!(analyzer.compare_fingerprints(&a, &c) < 0.01);
    }

    #[test]
    fn test_fingerprint_mismatched_len() {
        let analyzer = AudioAnalyzer::new(AudioAnalyzerConfig::default());
        assert_eq!(analyzer.compare_fingerprints(&[1.0], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn test_compute_simple_fingerprint() {
        let analyzer = AudioAnalyzer::new(AudioAnalyzerConfig::default());
        let samples: Vec<f32> = (0..4096).map(|i| (i as f32 * 0.01).sin()).collect();
        let fp = analyzer.compute_simple_fingerprint(&samples, 44100);
        assert_eq!(fp.len(), 16);
        // Should be a unit vector (approximately).
        let sum: f32 = fp.iter().sum();
        assert!((sum - 1.0).abs() < 0.1, "fingerprint sum={}", sum);
    }

    #[test]
    fn test_known_reference_fingerprints() {
        let analyzer = AudioAnalyzer::new(AudioAnalyzerConfig::default());
        let refs = analyzer.known_reference_fingerprints();
        assert!(!refs.is_empty());
        for (_, _, fp) in &refs {
            assert_eq!(fp.len(), 16);
        }
    }

    #[test]
    fn test_load_audio_missing() {
        let result = AudioAnalyzer::load_audio("/nonexistent/file.wav");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_audio_real_file() {
        let temp = NamedTempFile::new().unwrap();
        // Write synthetic PCM-like data.
        let data: Vec<u8> = (0..176400).map(|i| (i % 256) as u8).collect();
        std::fs::write(temp.path(), &data).unwrap();
        let buffer = AudioAnalyzer::load_audio(temp.path()).unwrap();
        assert!(!buffer.samples.is_empty());
        assert_eq!(buffer.channels, 2);
    }

    #[test]
    fn test_transcribe_speech() {
        let temp = NamedTempFile::new().unwrap();
        let data: Vec<u8> = (0..1764000).map(|i| (i % 256) as u8).collect();
        std::fs::write(temp.path(), &data).unwrap();

        let analyzer = AudioAnalyzer::new(AudioAnalyzerConfig::default());
        let segments = analyzer.transcribe_speech(temp.path()).unwrap();
        // Should produce at least some segments.
        assert!(!segments.is_empty());
        for seg in &segments {
            assert!(seg.confidence >= 0.0 && seg.confidence <= 1.0);
        }
    }

    #[test]
    fn test_detect_events() {
        let temp = NamedTempFile::new().unwrap();
        let data: Vec<u8> = (0..1764000).map(|i| (i % 256) as u8).collect();
        std::fs::write(temp.path(), &data).unwrap();

        let analyzer = AudioAnalyzer::new(AudioAnalyzerConfig::default());
        let events = analyzer.detect_events(temp.path()).unwrap();
        // Synthetic signal should produce some events.
        assert!(!events.is_empty());
    }

    #[test]
    fn test_detect_events_empty() {
        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), &[] as &[u8]).unwrap();

        let analyzer = AudioAnalyzer::new(AudioAnalyzerConfig::default());
        let result = analyzer.detect_events(temp.path());
        // Empty file generates 1s minimum duration signal.
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_copyright_disabled() {
        let temp = NamedTempFile::new().unwrap();
        let data: Vec<u8> = (0..1764000).map(|i| (i % 256) as u8).collect();
        std::fs::write(temp.path(), &data).unwrap();

        let analyzer = AudioAnalyzer::new(
            AudioAnalyzerConfig::new().with_copyright(false),
        );
        let matches = analyzer.check_copyright(temp.path()).unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn test_check_copyright_enabled() {
        let temp = NamedTempFile::new().unwrap();
        let data: Vec<u8> = (0..1764000).map(|i| (i % 256) as u8).collect();
        std::fs::write(temp.path(), &data).unwrap();

        let analyzer = AudioAnalyzer::new(AudioAnalyzerConfig::default());
        let matches = analyzer.check_copyright(temp.path()).unwrap();
        // May or may not match synthetic signal against references.
        for m in &matches {
            assert!(m.match_confidence >= 0.0 && m.match_confidence <= 1.0);
        }
    }

    #[test]
    fn test_analyze_full_pipeline() {
        let temp = NamedTempFile::new().unwrap();
        let data: Vec<u8> = (0..3528000).map(|i| (i % 256) as u8).collect();
        std::fs::write(temp.path(), &data).unwrap();

        let analyzer = AudioAnalyzer::new(AudioAnalyzerConfig::default());
        let analysis = analyzer.analyze(temp.path(), true).unwrap();

        assert!(!analysis.transcript.is_empty());
        assert!(analysis.overall_risk_score >= 0.0 && analysis.overall_risk_score <= 100.0);
    }

    #[test]
    fn test_analyze_no_copyright() {
        let temp = NamedTempFile::new().unwrap();
        let data: Vec<u8> = (0..3528000).map(|i| (i % 256) as u8).collect();
        std::fs::write(temp.path(), &data).unwrap();

        let analyzer = AudioAnalyzer::new(AudioAnalyzerConfig::default());
        let analysis = analyzer.analyze(temp.path(), false).unwrap();
        assert!(analysis.copyright_matches.is_empty());
    }

    #[test]
    fn test_audio_buffer_duration() {
        let samples = vec![0.0f32; 88200]; // 1 second at 44.1kHz stereo.
        let buffer = AudioBuffer {
            samples,
            sample_rate: 44100,
            channels: 2,
            duration_sec: 1.0,
        };
        let dur = buffer.duration();
        assert!((dur - 1.0).abs() < 0.01, "duration={}", dur);
    }

    #[test]
    fn test_spectral_centroid_nonzero() {
        let samples: Vec<f32> = (0..4096).map(|i| (i as f32 * 0.1).sin()).collect();
        let buffer = AudioBuffer {
            samples,
            sample_rate: 44100,
            channels: 1,
            duration_sec: 1.0,
        };
        let centroid = buffer.spectral_centroid();
        assert!(centroid > 0.0, "centroid={}", centroid);
    }

    #[test]
    fn test_fingerprint_consistency() {
        let analyzer = AudioAnalyzer::new(AudioAnalyzerConfig::default());
        let samples: Vec<f32> = (0..4096).map(|i| (i as f32 * 0.01).sin()).collect();
        let fp1 = analyzer.compute_simple_fingerprint(&samples, 44100);
        let fp2 = analyzer.compute_simple_fingerprint(&samples, 44100);
        assert_eq!(fp1, fp2);
    }
}

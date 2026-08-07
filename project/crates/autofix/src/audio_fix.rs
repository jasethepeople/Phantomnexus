use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use crate::ffmpeg_fixes::{
    build_concat_list, generate_duck_filter, generate_replace_audio_filter, FfmpegFilterGraph,
};
use crate::{AutoFixError, Result};

/// Type of audio replacement to use
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AudioReplacement {
    /// Silence (mute) the segment
    Silence {
        /// Duration of crossfade in/out
        crossfade_duration: f64,
    },
    /// Replace with royalty-free background music
    RoyaltyFreeMusic {
        /// Genre of replacement music
        genre: String,
        /// Duration of replacement audio needed
        duration: f64,
        /// Crossfade in duration
        crossfade_in: f64,
        /// Crossfade out duration
        crossfade_out: f64,
    },
    /// Replace with ambient/nature sounds
    AmbientSound {
        /// Type of ambient sound
        sound_type: String,
        duration: f64,
        crossfade_in: f64,
        crossfade_out: f64,
    },
    /// Replace with a beep tone
    BeepTone {
        /// Frequency in Hz
        frequency: f64,
        duration: f64,
        volume_db: f64,
    },
    /// Use audio from a specific file path
    AudioFile {
        /// Path to replacement audio file
        path: PathBuf,
        crossfade_in: f64,
        crossfade_out: f64,
    },
}

impl AudioReplacement {
    /// Get a human-readable description of this replacement
    pub fn description(&self) -> String {
        match self {
            AudioReplacement::Silence { .. } => "Silence".to_string(),
            AudioReplacement::RoyaltyFreeMusic { genre, .. } => format!("Royalty-free {} music", genre),
            AudioReplacement::AmbientSound { sound_type, .. } => format!("{} ambient sound", sound_type),
            AudioReplacement::BeepTone { frequency, .. } => format!("{:.0}Hz beep tone", frequency),
            AudioReplacement::AudioFile { path, .. } => format!("Audio from {}", path.display()),
        }
    }

    /// Get the crossfade durations
    pub fn crossfade_durations(&self) -> (f64, f64) {
        match self {
            AudioReplacement::Silence { crossfade_duration } => (*crossfade_duration, *crossfade_duration),
            AudioReplacement::RoyaltyFreeMusic { crossfade_in, crossfade_out, .. } => (*crossfade_in, *crossfade_out),
            AudioReplacement::AmbientSound { crossfade_in, crossfade_out, .. } => (*crossfade_in, *crossfade_out),
            AudioReplacement::BeepTone { .. } => (0.05, 0.05),
            AudioReplacement::AudioFile { crossfade_in, crossfade_out, .. } => (*crossfade_in, *crossfade_out),
        }
    }

    /// Generate FFmpeg filter source for this replacement type
    pub fn to_ffmpeg_source(&self, output_path: &Path) -> String {
        match self {
            AudioReplacement::Silence { .. } => {
                format!(
                    "aevalsrc=0:d={dur:.3}",
                    dur = self.duration().unwrap_or(1.0)
                )
            }
            AudioReplacement::BeepTone { frequency, duration, volume_db } => {
                let vol = 10f64.powf(volume_db / 20.0);
                format!(
                    "sine=frequency={freq:.1}:duration={dur:.3},volume={vol:.6}",
                    freq = frequency,
                    dur = duration,
                    vol = vol
                )
            }
            AudioReplacement::AudioFile { path, .. } => {
                format!("file='{}'", path.to_string_lossy())
            }
            _ => {
                // For generated audio (music/ambient), create a placeholder
                format!(
                    "aevalsrc=0:d={dur:.3}",
                    dur = self.duration().unwrap_or(1.0)
                )
            }
        }
    }

    /// Get the duration of the replacement audio
    pub fn duration(&self) -> Option<f64> {
        match self {
            AudioReplacement::Silence { crossfade_duration } => Some(*crossfade_duration * 2.0),
            AudioReplacement::RoyaltyFreeMusic { duration, .. } => Some(*duration),
            AudioReplacement::AmbientSound { duration, .. } => Some(*duration),
            AudioReplacement::BeepTone { duration, .. } => Some(*duration),
            AudioReplacement::AudioFile { path, .. } => {
                // Would need to probe the file; return None for now
                None
            }
        }
    }
}

/// Engine for audio remediation operations
pub struct AudioFixEngine {
    default_crossfade: f64,
    default_fade_curve: FadeCurve,
}

/// Fade curve shape for crossfades
#[derive(Debug, Clone, Copy)]
enum FadeCurve {
    Linear,
    Sine,
    Exponential,
}

impl AudioFixEngine {
    /// Create a new AudioFixEngine
    pub fn new(default_crossfade: f64) -> Self {
        info!("Initializing AudioFixEngine with crossfade={:.2}s", default_crossfade);
        Self {
            default_crossfade: default_crossfade.max(0.01),
            default_fade_curve: FadeCurve::Sine,
        }
    }

    /// Replace an audio segment with the specified replacement
    ///
    /// Uses FFmpeg's atrim, asetpts, afade, and acrossfade filters to
    /// seamlessly replace a segment of audio while maintaining smooth
    /// transitions at both boundaries.
    #[instrument(skip(self))]
    pub async fn replace_audio_segment(
        &self,
        video_path: &Path,
        start: f64,
        end: f64,
        replacement: &AudioReplacement,
    ) -> Result<PathBuf> {
        info!(
            "Replacing audio segment [{:.3}s - {:.3}s] with {}",
            start,
            end,
            replacement.description()
        );

        // Validate time range
        if start >= end || start < 0.0 || end < 0.0 {
            return Err(AutoFixError::InvalidTimeRange { start, end });
        }

        let segment_duration = end - start;
        let (cf_in, cf_out) = replacement.crossfade_durations();
        let crossfade = cf_in.min(cf_out).min(segment_duration / 4.0).max(0.05);

        // Get video total duration for context
        let total_duration = self.probe_duration(video_path).await?;

        // Generate output path
        let output_path = self.temp_output(video_path, "audio_replace")?;

        match replacement {
            AudioReplacement::Silence { .. } => {
                self.replace_with_silence(video_path, start, end, crossfade, &output_path)
                    .await?;
            }
            AudioReplacement::BeepTone { frequency, duration: beep_dur, volume_db } => {
                self.replace_with_beep(video_path, start, end, *frequency, *beep_dur, *volume_db, crossfade, &output_path)
                    .await?;
            }
            AudioReplacement::AudioFile { path: audio_file, crossfade_in, crossfade_out } => {
                self.replace_with_file(video_path, start, end, audio_file, *crossfade_in, *crossfade_out, &output_path)
                    .await?;
            }
            _ => {
                // For generated audio (music/ambient), use generic replacement
                self.replace_with_silence(video_path, start, end, crossfade, &output_path)
                    .await?;
            }
        }

        info!("Audio replacement complete: {}", output_path.display());
        Ok(output_path)
    }

    /// Duck (reduce) audio volume for a specific segment
    ///
    /// Uses FFmpeg's volume filter with a frame-level expression that
    /// creates a smooth envelope for ducking.
    #[instrument(skip(self))]
    pub async fn duck_audio(
        &self,
        video_path: &Path,
        start: f64,
        end: f64,
        target_db: f64,
    ) -> Result<PathBuf> {
        info!(
            "Ducking audio [{:.3}s - {:.3}s] to {:.1f}dB",
            start, end, target_db
        );

        if start >= end || start < 0.0 || end < 0.0 {
            return Err(AutoFixError::InvalidTimeRange { start, end });
        }

        let output_path = self.temp_output(video_path, "audio_duck")?;
        let total_duration = self.probe_duration(video_path).await?;

        // Build the ducking filter using volume expression
        let duck_filter = generate_duck_filter(
            start,
            end,
            total_duration,
            target_db,
            self.default_crossfade,
        );

        debug!("Duck filter: {}", duck_filter);

        // Build FFmpeg command
        let filter_complex = format!(
            "[0:a]{filter}[aout]",
            filter = duck_filter
        );

        let args = vec![
            "-y".to_string(),
            "-i".to_string(), video_path.to_string_lossy().to_string(),
            "-filter_complex".to_string(), filter_complex,
            "-map".to_string(), "0:v".to_string(),
            "-map".to_string(), "[aout]".to_string(),
            "-c:v".to_string(), "copy".to_string(),
            "-c:a".to_string(), "aac".to_string(),
            "-b:a".to_string(), "192k".to_string(),
            "-movflags".to_string(), "+faststart".to_string(),
            output_path.to_string_lossy().to_string(),
        ];

        self.run_ffmpeg(&args).await?;

        info!("Audio ducking complete: {}", output_path.display());
        Ok(output_path)
    }

    /// Crossfade multiple audio segments
    ///
    /// Creates smooth crossfades between multiple segments in the video.
    /// This is useful for stitching together processed segments.
    #[instrument(skip(self))]
    pub async fn crossfade_audio(
        &self,
        video_path: &Path,
        segments: &[(f64, f64)],
    ) -> Result<PathBuf> {
        info!("Crossfading {} audio segments", segments.len());

        if segments.is_empty() {
            return Err(AutoFixError::CrossfadeError(
                "No segments provided for crossfade".to_string(),
            ));
        }

        let output_path = self.temp_output(video_path, "audio_crossfade")?;
        let total_duration = self.probe_duration(video_path).await?;

        if segments.len() == 1 {
            // Single segment — just apply afade in/out
            let (start, end) = segments[0];
            let fade_dur = self.default_crossfade.min((end - start) / 4.0).max(0.05);

            let filter = format!(
                "[0:a]afade=t=in:st={start:.3}:d={fade:.3},\
                 afade=t=out:st={fade_out_start:.3}:d={fade:.3}[aout]",
                start = start,
                fade = fade_dur,
                fade_out_start = end - fade_dur
            );

            let args = vec![
                "-y".to_string(),
                "-i".to_string(), video_path.to_string_lossy().to_string(),
                "-filter_complex".to_string(), filter,
                "-map".to_string(), "0:v".to_string(),
                "-map".to_string(), "[aout]".to_string(),
                "-c:v".to_string(), "copy".to_string(),
                "-c:a".to_string(), "aac".to_string(),
                "-b:a".to_string(), "192k".to_string(),
                output_path.to_string_lossy().to_string(),
            ];

            self.run_ffmpeg(&args).await?;
            return Ok(output_path);
        }

        // Multi-segment: build a complex filter graph
        let mut filter_parts = Vec::new();
        let cf_duration = self.default_crossfade;

        // Create split points for each segment
        let mut current_label = "[0:a]".to_string();

        for (i, &(start, end)) in segments.iter().enumerate() {
            let next_label = if i == segments.len() - 1 {
                "[aout]".to_string()
            } else {
                format!("[a{}]", i)
            };

            let fade_in = format!("afade=t=in:st={:.3}:d={:.3}", start, cf_duration);
            let fade_out = format!("afade=t=out:st={:.3}:d={:.3}", end - cf_duration, cf_duration);

            filter_parts.push(format!("{}{},{}{}", current_label, fade_in, fade_out, next_label));
            current_label = next_label.clone();
        }

        // Apply volume normalization at the end
        filter_parts.push(format!("[aout]loudnorm=I=-14:TP=-1.5:LRA=11[aout_final]"));

        let filter_complex = filter_parts.join(";");
        debug!("Crossfade filter complex: {}", filter_complex);

        let args = vec![
            "-y".to_string(),
            "-i".to_string(), video_path.to_string_lossy().to_string(),
            "-filter_complex".to_string(), filter_complex,
            "-map".to_string(), "0:v".to_string(),
            "-map".to_string(), "[aout_final]".to_string(),
            "-c:v".to_string(), "copy".to_string(),
            "-c:a".to_string(), "aac".to_string(),
            "-b:a".to_string(), "192k".to_string(),
            "-movflags".to_string(), "+faststart".to_string(),
            output_path.to_string_lossy().to_string(),
        ];

        self.run_ffmpeg(&args).await?;

        info!("Audio crossfade complete: {}", output_path.display());
        Ok(output_path)
    }

    /// Apply loudness normalization (EBU R128) to the entire audio track
    #[instrument(skip(self))]
    pub async fn normalize_loudness(
        &self,
        video_path: &Path,
    ) -> Result<PathBuf> {
        info!("Normalizing loudness for {}", video_path.display());

        let output_path = self.temp_output(video_path, "loudnorm")?;

        let filter = "[0:a]loudnorm=I=-14:TP=-1.5:LRA=11:print_format=summary[aout]";

        let args = vec![
            "-y".to_string(),
            "-i".to_string(), video_path.to_string_lossy().to_string(),
            "-filter_complex".to_string(), filter.to_string(),
            "-map".to_string(), "0:v".to_string(),
            "-map".to_string(), "[aout]".to_string(),
            "-c:v".to_string(), "copy".to_string(),
            "-c:a".to_string(), "aac".to_string(),
            "-b:a".to_string(), "192k".to_string(),
            "-movflags".to_string(), "+faststart".to_string(),
            output_path.to_string_lossy().to_string(),
        ];

        self.run_ffmpeg(&args).await?;
        Ok(output_path)
    }

    /// Replace audio with silence (mute) for a segment
    async fn replace_with_silence(
        &self,
        video_path: &Path,
        start: f64,
        end: f64,
        crossfade: f64,
        output: &Path,
    ) -> Result<()> {
        let segment_duration = end - start;

        // Build a filter graph:
        // 1. Split audio into: before-segment, segment, after-segment
        // 2. Replace segment with silence
        // 3. Apply crossfades
        // 4. Concatenate back together

        let filter_complex = format!(
            // Split the audio stream
            "[0:a]asplit=3[a_before][a_seg][a_after];\
             \
             // Process the segment - replace with silence\
             [a_seg]atrim=start={start:.3}:end={end:.3},asetpts=PTS-STARTPTS,\
             volume=0,\
             afade=t=in:st=0:d={cf:.3},\
             afade=t=out:st={seg_cf:.3}:d={cf:.3}[a_seg_processed];\
             \
             // Process before segment\
             [a_before]atrim=start=0:end={start:.3},asetpts=PTS-STARTPTS,\
             afade=t=out:st={before_fade:.3}:d={cf:.3}[a_before_processed];\
             \
             // Process after segment\
             [a_after]atrim=start={end:.3},asetpts=PTS-STARTPTS,\
             afade=t=in:st=0:d={cf:.3}[a_after_processed];\
             \
             // Concatenate all parts\
             [a_before_processed][a_seg_processed][a_after_processed]\
             concat=n=3:v=0:a=1[aout]",
            start = start,
            end = end,
            cf = crossfade,
            seg_cf = (segment_duration - crossfade).max(0.01),
            before_fade = (start - crossfade).max(0.0),
        );

        debug!("Silence replacement filter: {}", filter_complex);

        let args = vec![
            "-y".to_string(),
            "-i".to_string(), video_path.to_string_lossy().to_string(),
            "-filter_complex".to_string(), filter_complex,
            "-map".to_string(), "0:v".to_string(),
            "-map".to_string(), "[aout]".to_string(),
            "-c:v".to_string(), "copy".to_string(),
            "-c:a".to_string(), "aac".to_string(),
            "-b:a".to_string(), "192k".to_string(),
            "-movflags".to_string(), "+faststart".to_string(),
            output.to_string_lossy().to_string(),
        ];

        self.run_ffmpeg(&args).await
    }

    /// Replace audio with a beep tone
    async fn replace_with_beep(
        &self,
        video_path: &Path,
        start: f64,
        end: f64,
        frequency: f64,
        _duration: f64,
        volume_db: f64,
        crossfade: f64,
        output: &Path,
    ) -> Result<()> {
        let segment_duration = end - start;
        let vol = 10f64.powf(volume_db / 20.0);
        let cf = crossfade.min(segment_duration / 4.0).max(0.05);

        // Generate beep using aevalsrc + sine, then mix back
        let filter_complex = format!(
            "[0:a]asplit=3[a_before][a_seg][a_after];\
             \
             [a_seg]atrim=start={start:.3}:end={end:.3},asetpts=PTS-STARTPTS,\
             volume=0[silent_seg];\
             \
             sine=frequency={freq:.1}:duration={dur:.3},\
             volume={vol:.6},\
             afade=t=in:st=0:d={cf:.3},\
             afade=t=out:st={fade_out:.3}:d={cf:.3}[beep];\
             \
             [silent_seg][beep]amix=inputs=2:duration=first[beep_mix];\
             \
             [a_before]atrim=start=0:end={start:.3},asetpts=PTS-STARTPTS[before];\
             \
             [a_after]atrim=start={end:.3},asetpts=PTS-STARTPTS[after];\
             \
             [before][beep_mix][after]concat=n=3:v=0:a=1[aout]",
            start = start,
            end = end,
            freq = frequency,
            dur = segment_duration,
            vol = vol,
            cf = cf,
            fade_out = segment_duration - cf,
        );

        debug!("Beep replacement filter: {}", filter_complex);

        let args = vec![
            "-y".to_string(),
            "-i".to_string(), video_path.to_string_lossy().to_string(),
            "-filter_complex".to_string(), filter_complex,
            "-map".to_string(), "0:v".to_string(),
            "-map".to_string(), "[aout]".to_string(),
            "-c:v".to_string(), "copy".to_string(),
            "-c:a".to_string(), "aac".to_string(),
            "-b:a".to_string(), "192k".to_string(),
            "-movflags".to_string(), "+faststart".to_string(),
            output.to_string_lossy().to_string(),
        ];

        self.run_ffmpeg(&args).await
    }

    /// Replace audio segment with audio from a file
    async fn replace_with_file(
        &self,
        video_path: &Path,
        start: f64,
        end: f64,
        audio_file: &Path,
        crossfade_in: f64,
        crossfade_out: f64,
        output: &Path,
    ) -> Result<()> {
        let segment_duration = end - start;
        let cf_in = crossfade_in.min(segment_duration / 4.0).max(0.05);
        let cf_out = crossfade_out.min(segment_duration / 4.0).max(0.05);

        let filter_complex = format!(
            "[0:a]asplit=3[a_before][a_seg][a_after];\
             \
             [a_seg]atrim=start={start:.3}:end={end:.3},asetpts=PTS-STARTPTS,volume=0[silent];\
             \
             [1:a]atrim=0:{dur:.3},asetpts=PTS-STARTPTS,\
             afade=t=in:st=0:d={cf_in:.3},\
             afade=t=out:st={fade_out:.3}:d={cf_out:.3}[replacement];\
             \
             [silent][replacement]amix=inputs=2:duration=first[replaced];\
             \
             [a_before]atrim=start=0:end={start:.3},asetpts=PTS-STARTPTS[before];\
             \
             [a_after]atrim=start={end:.3},asetpts=PTS-STARTPTS[after];\
             \
             [before][replaced][after]concat=n=3:v=0:a=1[aout]",
            start = start,
            end = end,
            dur = segment_duration,
            cf_in = cf_in,
            cf_out = cf_out,
            fade_out = segment_duration - cf_out,
        );

        let args = vec![
            "-y".to_string(),
            "-i".to_string(), video_path.to_string_lossy().to_string(),
            "-i".to_string(), audio_file.to_string_lossy().to_string(),
            "-filter_complex".to_string(), filter_complex,
            "-map".to_string(), "0:v".to_string(),
            "-map".to_string(), "[aout]".to_string(),
            "-c:v".to_string(), "copy".to_string(),
            "-c:a".to_string(), "aac".to_string(),
            "-b:a".to_string(), "192k".to_string(),
            "-shortest".to_string(),
            "-movflags".to_string(), "+faststart".to_string(),
            output.to_string_lossy().to_string(),
        ];

        self.run_ffmpeg(&args).await
    }

    /// Probe video duration using ffprobe
    async fn probe_duration(&self, video_path: &Path) -> Result<f64> {
        let output = tokio::process::Command::new("ffprobe")
            .args(&[
                "-v", "error",
                "-show_entries", "format=duration",
                "-of", "default=noprint_wrappers=1:nokey=1",
                video_path.to_str().unwrap_or(""),
            ])
            .output()
            .await
            .map_err(|e| {
                AutoFixError::FfmpegError(format!("ffprobe failed: {}", e))
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let duration = stdout.trim().parse::<f64>().unwrap_or(0.0);

        if duration <= 0.0 {
            warn!("Could not determine duration, using fallback of 3600s");
            Ok(3600.0)
        } else {
            Ok(duration)
        }
    }

    /// Generate a temporary output path
    fn temp_output(&self, source: &Path, suffix: &str) -> Result<PathBuf> {
        let stem = source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("audio");
        let ext = source
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("mp4");
        let temp = std::env::temp_dir().join(format!("{}_{}_{}.{}", stem, suffix, Uuid::new_v4(), ext));
        Ok(temp)
    }

    /// Run FFmpeg with given arguments
    async fn run_ffmpeg(&self, args: &[String]) -> Result<()> {
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        debug!("Running FFmpeg with {} args", args.len());

        let output = tokio::process::Command::new("ffmpeg")
            .args(&args_refs)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    AutoFixError::FfmpegNotFound
                } else {
                    AutoFixError::FfmpegError(format!("FFmpeg execution failed: {}", e))
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AutoFixError::FfmpegError(format!(
                "FFmpeg exited with code {:?}: {}",
                output.status.code(),
                stderr.chars().take(500).collect::<String>()
            )));
        }

        debug!("FFmpeg completed successfully");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_audio_replacement_types() {
        let silence = AudioReplacement::Silence {
            crossfade_duration: 0.3,
        };
        assert_eq!(silence.description(), "Silence");
        assert_eq!(silence.crossfade_durations(), (0.3, 0.3));

        let music = AudioReplacement::RoyaltyFreeMusic {
            genre: "jazz".to_string(),
            duration: 5.0,
            crossfade_in: 0.2,
            crossfade_out: 0.4,
        };
        assert_eq!(music.description(), "Royalty-free jazz music");
        assert_eq!(music.crossfade_durations(), (0.2, 0.4));
        assert_eq!(music.duration(), Some(5.0));

        let beep = AudioReplacement::BeepTone {
            frequency: 1000.0,
            duration: 0.5,
            volume_db: -10.0,
        };
        assert_eq!(beep.description(), "1000Hz beep tone");
        assert_eq!(beep.crossfade_durations(), (0.05, 0.05));
    }

    #[test]
    fn test_audio_replacement_to_ffmpeg_source() {
        let beep = AudioReplacement::BeepTone {
            frequency: 1000.0,
            duration: 1.0,
            volume_db: -20.0,
        };
        let source = beep.to_ffmpeg_source(Path::new("/tmp/out.mp4"));
        assert!(source.contains("sine="));
        assert!(source.contains("frequency=1000.0"));
        assert!(source.contains("volume="));
    }

    #[test]
    fn test_audio_fix_engine_creation() {
        let engine = AudioFixEngine::new(0.5);
        assert_eq!(engine.default_crossfade, 0.5);
    }

    #[test]
    fn test_crossfade_duration_clamping() {
        let engine = AudioFixEngine::new(0.01);
        assert_eq!(engine.default_crossfade, 0.01);
    }

    #[tokio::test]
    async fn test_duck_audio_filter_generation() {
        let engine = AudioFixEngine::new(0.3);

        // This will fail if ffmpeg is not installed, which is expected in test env
        // We test the filter generation logic instead
        let filter = generate_duck_filter(5.0, 10.0, 30.0, -20.0, 0.3);
        assert!(filter.contains("volume="));
        assert!(filter.contains("between(t,"));
    }

    #[test]
    fn test_generate_replace_audio_filter() {
        let filter = generate_replace_audio_filter(5.0, 10.0, 5.0, 0.3);
        assert!(filter.contains("aselect="));
        assert!(filter.contains("afade=t=in"));
        assert!(filter.contains("afade=t=out"));
        assert!(filter.contains("acrossfade="));
    }
}

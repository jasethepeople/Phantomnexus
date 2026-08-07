//! # FFmpeg Filter Graph Generation Utilities
//!
//! Provides precise, composable FFmpeg filter_complex graph generation
//! for content remediation operations. Handles audio routing, video
//! filter chains, and multi-stream synchronization.

use std::fmt::Write;

use tracing::debug;

/// A builder for FFmpeg filter_complex graphs
///
/// Provides a fluent API for constructing complex filter graphs that
/// route audio/video streams through various processing stages.
///
/// # Example
/// ```
/// use autofix::ffmpeg_fixes::FfmpegFilterGraph;
///
/// let graph = FfmpegFilterGraph::new()
///     .input_video("[0:v]")
///     .input_audio("[0:a]")
///     .blur_region(100, 100, 200, 200, 20)
///     .volume(-20.0)
///     .output("[out_v]", "[out_a]")
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct FfmpegFilterGraph {
    video_chain: Vec<String>,
    audio_chain: Vec<String>,
    video_input: String,
    audio_input: String,
    video_output: String,
    audio_output: String,
    stream_labels: Vec<String>,
    label_counter: u32,
}

impl Default for FfmpegFilterGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl FfmpegFilterGraph {
    /// Create a new empty filter graph
    pub fn new() -> Self {
        Self {
            video_chain: Vec::new(),
            audio_chain: Vec::new(),
            video_input: "[0:v]".to_string(),
            audio_input: "[0:a]".to_string(),
            video_output: "[outv]".to_string(),
            audio_output: "[outa]".to_string(),
            stream_labels: Vec::new(),
            label_counter: 0,
        }
    }

    /// Set the video input stream label
    pub fn input_video(mut self, label: impl Into<String>) -> Self {
        self.video_input = label.into();
        self
    }

    /// Set the audio input stream label
    pub fn input_audio(mut self, label: impl Into<String>) -> Self {
        self.audio_input = label.into();
        self
    }

    /// Set the output labels
    pub fn output(
        mut self,
        video_label: impl Into<String>,
        audio_label: impl Into<String>,
    ) -> Self {
        self.video_output = video_label.into();
        self.audio_output = audio_label.into();
        self
    }

    // ---- Video Filters ----

    /// Add a drawbox overlay filter
    pub fn drawbox(
        mut self,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        color: impl Into<String>,
        thickness: u32,
    ) -> Self {
        let filter = format!(
            "drawbox=x={}:y={}:w={}:h={}:color={}:t={}",
            x, y, w, h, color.into(), thickness
        );
        self.video_chain.push(filter);
        self
    }

    /// Add a boxblur filter to the video chain
    pub fn boxblur(mut self, luma_radius: u32, chroma_radius: u32) -> Self {
        let filter = format!("boxblur={}:{}", luma_radius, chroma_radius);
        self.video_chain.push(filter);
        self
    }

    /// Add a boxblur filter for a specific region (using drawbox + boxblur)
    pub fn blur_region(
        mut self,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        radius: u32,
    ) -> Self {
        let filter = format!(
            "drawbox=x={x}:y={y}:w={w}:h={h}:color=black@0:t=fill,boxblur={r}:{r}",
            x = x,
            y = y,
            w = w,
            h = h,
            r = radius
        );
        self.video_chain.push(filter);
        self
    }

    /// Add a pixelate filter (scale down then up with neighbor interpolation)
    pub fn pixelate(mut self, block_size: u32) -> Self {
        let filter = format!(
            "scale=iw/{bs}:ih/{bs},scale=iw*{bs}:ih*{bs}:flags=neighbor",
            bs = block_size.max(2)
        );
        self.video_chain.push(filter);
        self
    }

    /// Add a region-specific pixelate
    pub fn pixelate_region(
        mut self,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        block_size: u32,
    ) -> Self {
        let bs = block_size.max(2);
        let filter = format!(
            "crop={w}:{h}:{x}:{y},scale=iw/{bs}:ih/{bs},scale={w}:{h}:flags=neighbor",
            w = w, h = h, x = x, y = y, bs = bs
        );
        self.video_chain.push(filter);
        self
    }

    /// Apply delogo filter to remove logos/watermarks
    pub fn delogo(
        mut self,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
    ) -> Self {
        let filter = format!("delogo=x={}:y={}:w={}:h={}", x, y, w, h);
        self.video_chain.push(filter);
        self
    }

    /// Scale/resize the video
    pub fn scale(mut self, width: u32, height: u32) -> Self {
        let filter = format!("scale={}:{}", width, height);
        self.video_chain.push(filter);
        self
    }

    /// Apply setpts for speed/timing adjustment
    pub fn setpts(mut self, expression: impl Into<String>) -> Self {
        let filter = format!("setpts={}", expression.into());
        self.video_chain.push(filter);
        self
    }

    /// Slow down video to reduce flashing
    pub fn slow_for_safety(mut self, factor: f64) -> Self {
        let expr = format!("{:.4}*PTS", factor.max(1.0));
        self.video_chain.push(format!("setpts={}", expr));
        self
    }

    /// Apply minterpolate for motion smoothing
    pub fn minterpolate(mut self, fps: f64, mode: MinterpolateMode) -> Self {
        let mode_str = match mode {
            MinterpolateMode::Dup => "dup",
            MinterpolateMode::Blend => "blend",
            MinterpolateMode::MotionCompensated => "mci",
        };
        let filter = format!(
            "minterpolate='mi_mode={}:fps={:.2}'",
            mode_str, fps
        );
        self.video_chain.push(filter);
        self
    }

    /// Apply minterpolate with full parameters
    pub fn minterpolate_full(mut self, params: MinterpolateParams) -> Self {
        let filter = params.to_filter_string();
        self.video_chain.push(filter);
        self
    }

    /// Apply eq (equalizer) for brightness/contrast/saturation
    pub fn eq(
        mut self,
        brightness: Option<f64>,
        contrast: Option<f64>,
        saturation: Option<f64>,
    ) -> Self {
        let mut params = Vec::new();
        if let Some(b) = brightness {
            params.push(format!("brightness={:.3}", b));
        }
        if let Some(c) = contrast {
            params.push(format!("contrast={:.3}", c));
        }
        if let Some(s) = saturation {
            params.push(format!("saturation={:.3}", s));
        }
        if !params.is_empty() {
            let filter = format!("eq={}", params.join(":"));
            self.video_chain.push(filter);
        }
        self
    }

    /// Trim a video stream to a time range
    pub fn trim(mut self, start: f64, end: f64) -> Self {
        let filter = format!(
            "trim=start={:.3}:end={:.3},setpts=PTS-STARTPTS",
            start, end
        );
        self.video_chain.push(filter);
        self
    }

    /// Fade in video
    pub fn fade_in(mut self, duration: f64) -> Self {
        let filter = format!("fade=t=in:st=0:d={:.3}", duration);
        self.video_chain.push(filter);
        self
    }

    /// Fade out video
    pub fn fade_out(mut self, start: f64, duration: f64) -> Self {
        let filter = format!("fade=t=out:st={:.3}:d={:.3}", start, duration);
        self.video_chain.push(filter);
        self
    }

    /// Overlay one stream on another
    pub fn overlay(mut self, x: u32, y: u32) -> Self {
        let filter = format!("overlay={}:{}", x, y);
        self.video_chain.push(filter);
        self
    }

    // ---- Audio Filters ----

    /// Adjust volume in dB
    pub fn volume_db(mut self, db: f64) -> Self {
        let volume = 10f64.powf(db / 20.0);
        let filter = format!("volume={:.6}", volume);
        self.audio_chain.push(filter);
        self
    }

    /// Adjust volume with linear gain
    pub fn volume_linear(mut self, gain: f64) -> Self {
        let filter = format!("volume={:.6}", gain.max(0.0));
        self.audio_chain.push(filter);
        self
    }

    /// Apply volume using volume filter with expression
    pub fn volume_expr(mut self, expr: impl Into<String>) -> Self {
        let filter = format!("volume='{}'", expr.into());
        self.audio_chain.push(filter);
        self
    }

    /// Apply loudnorm (EBU R128 loudness normalization)
    pub fn loudnorm(mut self) -> Self {
        self.audio_chain.push(
            "loudnorm=I=-14:TP=-1.5:LRA=11".to_string(),
        );
        self
    }

    /// Apply loudnorm with custom parameters
    pub fn loudnorm_custom(mut self, i: f64, tp: f64, lra: f64) -> Self {
        let filter = format!("loudnorm=I={:.1}:TP={:.1}:LRA={:.1}", i, tp, lra);
        self.audio_chain.push(filter);
        self
    }

    /// Apply dynaudnorm (dynamic audio normalization)
    pub fn dynaudnorm(mut self) -> Self {
        self.audio_chain
            .push("dynaudnorm=f=100:g=15:p=0.95".to_string());
        self
    }

    /// Apply afade (audio fade)
    pub fn afade(mut self, fade_type: FadeType, start: f64, duration: f64) -> Self {
        let t = match fade_type {
            FadeType::In => "in",
            FadeType::Out => "out",
        };
        let filter = format!(
            "afade=t={}:st={:.3}:d={:.3}",
            t, start, duration
        );
        self.audio_chain.push(filter);
        self
    }

    /// Apply acrossfade between two audio streams
    pub fn acrossfade(mut self, duration: f64) -> Self {
        let filter = format!("acrossfade=d={:.3}", duration);
        self.audio_chain.push(filter);
        self
    }

    /// Trim audio stream
    pub fn atrim(mut self, start: f64, end: f64) -> Self {
        let filter = format!(
            "atrim=start={:.3}:end={:.3},asetpts=PTS-STARTPTS",
            start, end
        );
        self.audio_chain.push(filter);
        self
    }

    /// Mix two audio streams
    pub fn amix(mut self, inputs: u32, duration: AmixDuration) -> Self {
        let dur_str = match duration {
            AmixDuration::Longest => "longest",
            AmixDuration::Shortest => "shortest",
            AmixDuration::First => "first",
        };
        let filter = format!("amix=inputs={}:duration={}", inputs, dur_str);
        self.audio_chain.push(filter);
        self
    }

    /// Apply a lowpass filter
    pub fn lowpass(mut self, frequency: f64) -> Self {
        let filter = format!("lowpass=f={:.1}", frequency);
        self.audio_chain.push(filter);
        self
    }

    /// Apply a highpass filter
    pub fn highpass(mut self, frequency: f64) -> Self {
        let filter = format!("highpass=f={:.1}", frequency);
        self.audio_chain.push(filter);
        self
    }

    /// Apply an equalizer band
    pub fn equalizer(mut self, frequency: f64, width: f64, gain_db: f64) -> Self {
        let filter = format!(
            "equalizer=f={:.1}:width_type=h:width={:.1}:g={:.1}",
            frequency, width, gain_db
        );
        self.audio_chain.push(filter);
        self
    }

    // ---- Build ----

    /// Build the complete filter_complex string
    ///
    /// Chains all video and audio filters into proper FFmpeg filter_complex syntax.
    pub fn build(self) -> String {
        let mut parts = Vec::new();
        let mut current_v_label = self.video_input.clone();

        // Build video chain with proper stream labels
        for (i, filter) in self.video_chain.iter().enumerate() {
            let next_label = if i == self.video_chain.len() - 1 {
                self.video_output.clone()
            } else {
                format!("[v{}]", i)
            };

            parts.push(format!("{}{}{}", current_v_label, filter, next_label));
            current_v_label = next_label;
        }

        // Build audio chain
        let mut current_a_label = self.audio_input.clone();
        for (i, filter) in self.audio_chain.iter().enumerate() {
            let next_label = if i == self.audio_chain.len() - 1 {
                self.audio_output.clone()
            } else {
                format!("[a{}]", i)
            };

            parts.push(format!("{}{}{}", current_a_label, filter, next_label));
            current_a_label = next_label;
        }

        let result = parts.join(";");
        debug!("Built filter_complex with {} video and {} audio filters", 
            self.video_chain.len(), self.audio_chain.len());
        result
    }

    /// Build video-only filter string (no stream labels)
    pub fn build_video_only(&self) -> String {
        self.video_chain.join(",")
    }

    /// Build audio-only filter string (no stream labels)
    pub fn build_audio_only(&self) -> String {
        self.audio_chain.join(",")
    }

    /// Check if the graph has any video filters
    pub fn has_video_filters(&self) -> bool {
        !self.video_chain.is_empty()
    }

    /// Check if the graph has any audio filters
    pub fn has_audio_filters(&self) -> bool {
        !self.audio_chain.is_empty()
    }
}

/// Minterpolate mode options
#[derive(Debug, Clone, Copy)]
pub enum MinterpolateMode {
    /// Duplicate frames (fastest)
    Dup,
    /// Blend between frames
    Blend,
    /// Motion-compensated interpolation (best quality)
    MotionCompensated,
}

/// Full parameter set for minterpolate filter
#[derive(Debug, Clone)]
pub struct MinterpolateParams {
    pub fps: f64,
    pub mi_mode: MinterpolateMode,
    pub mc_mode: String,
    pub me_mode: String,
    pub me: String,
    pub vsbmc: bool,
    pub mb_size: u32,
}

impl Default for MinterpolateParams {
    fn default() -> Self {
        Self {
            fps: 60.0,
            mi_mode: MinterpolateMode::MotionCompensated,
            mc_mode: "aobmc".to_string(),
            me_mode: "bidir".to_string(),
            me: "esa".to_string(),
            vsbmc: true,
            mb_size: 16,
        }
    }
}

impl MinterpolateParams {
    /// Create new params with given FPS
    pub fn with_fps(fps: f64) -> Self {
        Self {
            fps,
            ..Default::default()
        }
    }

    /// Convert to FFmpeg filter string
    pub fn to_filter_string(&self) -> String {
        let mi_mode = match self.mi_mode {
            MinterpolateMode::Dup => "dup",
            MinterpolateMode::Blend => "blend",
            MinterpolateMode::MotionCompensated => "mci",
        };
        format!(
            "minterpolate='mi_mode={}:mc_mode={}:me_mode={}:me={}:vsbmc={}:mb_size={}:fps={:.2}'",
            mi_mode,
            self.mc_mode,
            self.me_mode,
            self.me,
            if self.vsbmc { 1 } else { 0 },
            self.mb_size,
            self.fps
        )
    }
}

/// Fade type for audio/video fades
#[derive(Debug, Clone, Copy)]
pub enum FadeType {
    In,
    Out,
}

/// Duration mode for amix
#[derive(Debug, Clone, Copy)]
pub enum AmixDuration {
    Longest,
    Shortest,
    First,
}

/// Generate a precise FFmpeg command for audio ducking with a smooth envelope
///
/// Creates a volume envelope that ducks audio during a segment with smooth fade in/out.
pub fn generate_duck_filter(
    segment_start: f64,
    segment_end: f64,
    total_duration: f64,
    duck_db: f64,
    fade_duration: f64,
) -> String {
    let fade = fade_duration.min((segment_end - segment_start) / 4.0).max(0.05);

    // Use volume expression with enable/timeline
    let duck_gain = 10f64.powf(duck_db / 20.0);

    format!(
        "volume='if(between(t,{start:.3},{fade_in_end:.3}),{default}+({duck}-{default})*((t-{start:.3})/{fade:.3}),\
        if(between(t,{fade_in_end:.3},{fade_out_start:.3}),{duck},\
        if(between(t,{fade_out_start:.3},{end:.3}),{duck}+({default}-{duck})*((t-{fade_out_start:.3})/{fade:.3}),\
        {default})))':eval=frame",
        start = segment_start,
        end = segment_end,
        fade_in_end = segment_start + fade,
        fade_out_start = segment_end - fade,
        fade = fade,
        duck = duck_gain,
        default = 1.0,
    )
}

/// Generate FFmpeg filter for audio replacement with crossfade
///
/// Creates a filter that replaces a segment of audio with new audio,
/// with crossfade at both boundaries.
pub fn generate_replace_audio_filter(
    original_start: f64,
    original_end: f64,
    replacement_duration: f64,
    crossfade_duration: f64,
) -> String {
    let cf = crossfade_duration.min(replacement_duration / 4.0).max(0.1);

    format!(
        "aselect='not(between(t,{seg_start:.3},{seg_end:.3}))',asetpts=N/SR/TB;\
         [1:a]atrim=0:{rep_dur:.3},asetpts=PTS-STARTPTS[a1];\
         [a0]afade=t=in:st=0:d={cf:.3}[a0f];\
         [a1]afade=t=out:st={rep_dur_cf:.3}:d={cf:.3}[a1f];\
         [a0f][a1f]acrossfade=d={cf:.3}",
        seg_start = original_start,
        seg_end = original_end,
        rep_dur = replacement_duration,
        rep_dur_cf = replacement_duration - cf,
        cf = cf,
    )
}

/// Generate a segment-based filter for multiple audio operations
///
/// Creates a complex filter graph that handles multiple audio segments
/// with different processing for each.
pub fn generate_segmented_audio_filter(
    segments: &[(f64, f64, f64)], // (start, end, target_db)
    total_duration: f64,
    default_db: f64,
    crossfade: f64,
) -> String {
    if segments.is_empty() {
        return format!("volume={:.6}", 10f64.powf(default_db / 20.0));
    }

    let mut conditions = Vec::new();
    let default_gain = 10f64.powf(default_db / 20.0);

    for (start, end, db) in segments {
        let gain = 10f64.powf(db / 20.0);
        conditions.push(format!(
            "if(between(t,{:.3},{:.3}),{:.6},",
            start, end, gain
        ));
    }

    // Close all conditions with default
    let mut filter = "volume='".to_string();
    filter.push_str(&conditions.join(""));
    filter.push_str(&format!("{:.6}", default_gain));
    filter.push_str(")".repeat(segments.len()).as_str());
    filter.push_str("':eval=frame");

    filter
}

/// Build a concat demuxer file list for multi-segment processing
///
/// Takes segments as (file_path, start_time, duration) tuples and generates
/// the concat file list content.
pub fn build_concat_list(segments: &[(String, Option<f64>, Option<f64>)]) -> String {
    let mut content = String::new();
    for (path, start, duration) in segments {
        content.push_str(&format!("file '{}'
", path));
        if let Some(s) = start {
            content.push_str(&format!("inpoint {:.3}
", s));
        }
        if let Some(d) = duration {
            content.push_str(&format!("outpoint {:.3}
", d));
        }
    }
    content
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_graph_basic() {
        let graph = FfmpegFilterGraph::new()
            .blur_region(100, 100, 200, 200, 20)
            .volume_db(-10.0)
            .build();

        assert!(graph.contains("drawbox"));
        assert!(graph.contains("boxblur"));
        assert!(graph.contains("volume="));
    }

    #[test]
    fn test_filter_graph_video_only() {
        let graph = FfmpegFilterGraph::new()
            .blur_region(0, 0, 100, 100, 10)
            .pixelate(4)
            .build_video_only();

        assert!(graph.contains("drawbox"));
        assert!(graph.contains("pixelate") || graph.contains("scale"));
        assert!(!graph.contains(";")); // No labels in video-only
    }

    #[test]
    fn test_filter_graph_chain_labels() {
        let graph = FfmpegFilterGraph::new()
            .blur_region(10, 10, 50, 50, 5)
            .eq(Some(0.1), Some(1.2), Some(0.8))
            .build();

        // Should have proper stream labels chaining
        let parts: Vec<&str> = graph.split(';').collect();
        assert!(!parts.is_empty());
    }

    #[test]
    fn test_minterpolate_params() {
        let params = MinterpolateParams::with_fps(60.0);
        let filter = params.to_filter_string();
        assert!(filter.contains("minterpolate"));
        assert!(filter.contains("fps=60.00"));
        assert!(filter.contains("mi_mode=mci"));
    }

    #[test]
    fn test_minterpolate_default() {
        let params = MinterpolateParams::default();
        assert_eq!(params.fps, 60.0);
        assert!(params.vsbmc);
    }

    #[test]
    fn test_generate_duck_filter() {
        let filter = generate_duck_filter(5.0, 10.0, 30.0, -20.0, 0.3);
        assert!(filter.contains("volume="));
        assert!(filter.contains("between(t,"));
        assert!(filter.contains("eval=frame"));
    }

    #[test]
    fn test_generate_segmented_audio_filter() {
        let segments = vec![(5.0, 10.0, -20.0), (15.0, 20.0, -15.0)];
        let filter = generate_segmented_audio_filter(&segments, 30.0, 0.0, 0.3);
        assert!(filter.contains("volume="));
        assert!(filter.contains("between(t,5.000,10.000)"));
        assert!(filter.contains("between(t,15.000,20.000)"));
    }

    #[test]
    fn test_generate_segmented_audio_filter_empty() {
        let filter = generate_segmented_audio_filter(&[], 30.0, -14.0, 0.3);
        assert!(filter.contains("volume="));
        assert!(!filter.contains("between"));
    }

    #[test]
    fn test_build_concat_list() {
        let segments = vec![
            ("video1.mp4".to_string(), Some(0.0), Some(5.0)),
            ("video2.mp4".to_string(), None, None),
        ];
        let list = build_concat_list(&segments);
        assert!(list.contains("file 'video1.mp4'"));
        assert!(list.contains("file 'video2.mp4'"));
        assert!(list.contains("inpoint 0.000"));
        assert!(list.contains("outpoint 5.000"));
    }

    #[test]
    fn test_blur_region_filter() {
        let graph = FfmpegFilterGraph::new()
            .blur_region(100, 200, 300, 400, 25)
            .build();
        assert!(graph.contains("drawbox=x=100:y=200:w=300:h=400"));
        assert!(graph.contains("boxblur=25:25"));
    }

    #[test]
    fn test_eq_filter() {
        let graph = FfmpegFilterGraph::new()
            .eq(Some(0.1), Some(1.2), Some(0.8))
            .build();
        assert!(graph.contains("eq="));
        assert!(graph.contains("brightness=0.100"));
        assert!(graph.contains("contrast=1.200"));
        assert!(graph.contains("saturation=0.800"));
    }

    #[test]
    fn test_loudnorm_filter() {
        let graph = FfmpegFilterGraph::new().loudnorm().build();
        assert!(graph.contains("loudnorm="));
        assert!(graph.contains("I=-14"));
    }

    #[test]
    fn test_fade_filters() {
        let graph = FfmpegFilterGraph::new()
            .fade_in(0.5)
            .fade_out(10.0, 1.0)
            .build();
        assert!(graph.contains("fade=t=in"));
        assert!(graph.contains("fade=t=out"));
    }

    #[test]
    fn test_scale_filter() {
        let graph = FfmpegFilterGraph::new().scale(1920, 1080).build();
        assert!(graph.contains("scale=1920:1080"));
    }

    #[test]
    fn test_setpts_filter() {
        let graph = FfmpegFilterGraph::new()
            .slow_for_safety(2.0)
            .build();
        assert!(graph.contains("setpts="));
        assert!(graph.contains("2.0000*PTS"));
    }

    #[test]
    fn test_delogo_filter() {
        let graph = FfmpegFilterGraph::new()
            .delogo(10, 10, 100, 50)
            .build();
        assert!(graph.contains("delogo="));
    }
}

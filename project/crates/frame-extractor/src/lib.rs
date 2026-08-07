//! # frame-extractor
//!
//! Video frame extraction library for YouTube Sentinel with FFmpeg integration,
//! scene detection, and flash detection capabilities.
//!
//! ## Overview
//!
//! This crate provides:
//! - **Frame Extraction**: Decode video files and stream frames via async channels
//! - **Scene Detection**: Detect scene transitions using histogram-based analysis
//! - **Flash Detection**: Identify potentially harmful flashing content
//! - **Video Metadata**: Extract duration, resolution, FPS, codec info
//!
//! ## Quick Start
//!
//! ```no_run
//! use frame_extractor::{FrameExtractor, ExtractorConfig};
//! use std::path::Path;
//!
//! # async fn quick_start() -> Result<(), Box<dyn std::error::Error>> {
//! // Create extractor with default configuration
//! let config = ExtractorConfig::new();
//! let extractor = FrameExtractor::new(config)?;
//!
//! // Extract frames from a video
//! let mut rx = extractor.extract_frames(Path::new("video.mp4")).await?;
//!
//! while let Some(batch) = rx.recv().await {
//!     println!("Batch {}: {} frames", batch.sequence, batch.frames.len());
//!     if batch.is_last {
//!         break;
//!     }
//! }
//!
//! // Get video metadata
//! let meta = extractor.get_video_metadata(Path::new("video.mp4")).await?;
//! println!("{}x{} at {} fps", meta.width, meta.height, meta.fps);
//!
//! // Detect scenes
//! let scenes = extractor.detect_scenes(Path::new("video.mp4")).await?;
//! println!("Found {} scene transitions", scenes.len());
//!
//! // Detect flashing
//! let flashes = extractor.detect_flashing(Path::new("video.mp4")).await?;
//! for seg in &flashes {
//!     if seg.is_concerning() {
//!         println!("WARNING: Flashing segment at {:.1}s - {:.1}s (severity: {})",
//!             seg.start_ms / 1000.0, seg.end_ms / 1000.0, seg.severity);
//!     }
//! }
//!
//! # Ok(())
//! # }
//! ```
//!
//! ## Configuration
//!
//! Use `ExtractorConfig` to customize extraction behavior:
//!
//! ```no_run
//! use frame_extractor::ExtractorConfig;
//!
//! let config = ExtractorConfig::new()
//!     .with_target_fps(10.0)
//!     .with_max_resolution(1920, 1080)
//!     .with_scene_threshold(0.3)
//!     .with_batch_size(30);
//! ```

// Module declarations
pub mod error;
pub mod types;
pub mod extractor;
pub mod ffmpeg;
pub mod scene_detection;
pub mod flash_detection;

// Re-export main types for convenience
pub use extractor::FrameExtractor;
pub use types::{
    ExtractorConfig, FlashSeverity, FlashingSegment, Frame, FrameBatch, PixelFormat,
    SceneTransition, TransitionType, VideoMetadata,
};
pub use error::{ExtractorError, Result};

// Re-export subsystems for advanced usage
pub use scene_detection::HistogramSceneDetector;
pub use flash_detection::FlashDetector;

/// Version of the frame-extractor crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Initialize the frame extractor library globally.
///
/// This should be called once at application startup before using
/// any frame extraction functionality. It initializes FFmpeg and
/// sets up the necessary runtime state.
///
/// # Returns
/// * `Result<()>` - Ok if initialization succeeds, Err otherwise.
///
/// # Example
///
/// ```
/// # use frame_extractor;
/// # fn main() -> frame_extractor::Result<()> {
/// frame_extractor::init()?;
/// // Now safe to use FrameExtractor
/// # Ok(())
/// # }
/// ```
pub fn init() -> Result<()> {
    ffmpeg::init_ffmpeg()
}

/// Convenience function to extract metadata from a video file.
///
/// This is a synchronous shortcut that doesn't require creating
/// a `FrameExtractor` instance.
///
/// # Arguments
/// * `video_path` - Path to the video file.
///
/// # Returns
/// * `Result<VideoMetadata>` - The extracted video metadata.
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
///
/// # fn main() -> frame_extractor::Result<()> {
/// frame_extractor::init()?;
/// let meta = frame_extractor::probe_video(Path::new("video.mp4"))?;
/// println!("Duration: {}s", meta.duration.as_secs_f64());
/// # Ok(())
/// # }
/// ```
pub fn probe_video(video_path: &std::path::Path) -> Result<VideoMetadata> {
    ffmpeg::get_video_metadata_ff(video_path)
}

/// Batch process multiple videos for metadata extraction.
///
/// # Arguments
/// * `video_paths` - Slice of video file paths.
///
/// # Returns
/// * `Vec<Result<VideoMetadata>>` - Results for each video (some may fail).
pub fn probe_videos(video_paths: &[&std::path::Path]) -> Vec<Result<VideoMetadata>> {
    video_paths
        .iter()
        .map(|path| ffmpeg::get_video_metadata_ff(path))
        .collect()
}

/// Utility functions for working with video frames.
pub mod utils {
    use super::types::Frame;
    use super::PixelFormat;

    /// Calculate the total pixel count for a given width and height.
    pub fn pixel_count(width: u32, height: u32) -> u64 {
        width as u64 * height as u64
    }

    /// Calculate the byte size for a frame with given dimensions and format.
    pub fn frame_size(width: u32, height: u32, format: PixelFormat) -> usize {
        (pixel_count(width, height) as usize) * format.bytes_per_pixel()
    }

    /// Calculate the approximate memory needed for a batch of frames.
    pub fn batch_memory_estimate(
        batch_size: usize,
        width: u32,
        height: u32,
        format: PixelFormat,
    ) -> usize {
        batch_size * frame_size(width, height, format)
    }

    /// Convert a YUV420 frame to RGB24.
    ///
    /// # Arguments
    /// * `y_plane` - Y plane data.
    /// * `u_plane` - U plane data.
    /// * `v_plane` - V plane data.
    /// * `width` - Frame width.
    /// * `height` - Frame height.
    ///
    /// # Returns
    /// * `Vec<u8>` - RGB24 pixel data.
    pub fn yuv420_to_rgb24(
        y_plane: &[u8],
        u_plane: &[u8],
        v_plane: &[u8],
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let w = width as usize;
        let h = height as usize;
        let mut rgb = vec![0u8; w * h * 3];

        for y in 0..h {
            for x in 0..w {
                let y_idx = y * w + x;
                let uv_idx = (y / 2) * (w / 2) + (x / 2);

                let y_val = y_plane[y_idx] as f32;
                let u_val = u_plane[uv_idx] as f32 - 128.0;
                let v_val = v_plane[uv_idx] as f32 - 128.0;

                // YUV to RGB conversion (BT.601)
                let r = (y_val + 1.402 * v_val).clamp(0.0, 255.0) as u8;
                let g = (y_val - 0.344 * u_val - 0.714 * v_val).clamp(0.0, 255.0) as u8;
                let b = (y_val + 1.772 * u_val).clamp(0.0, 255.0) as u8;

                let rgb_idx = y_idx * 3;
                rgb[rgb_idx] = r;
                rgb[rgb_idx + 1] = g;
                rgb[rgb_idx + 2] = b;
            }
        }

        rgb
    }

    /// Create a thumbnail by downsampling a frame.
    ///
    /// Uses simple nearest-neighbor downsampling.
    ///
    /// # Arguments
    /// * `frame` - Source frame.
    /// * `thumb_width` - Target thumbnail width.
    /// * `thumb_height` - Target thumbnail height.
    ///
    /// # Returns
    /// * `Option<Frame>` - Thumbnail frame or None if source format is unsupported.
    pub fn create_thumbnail(
        frame: &Frame,
        thumb_width: u32,
        thumb_height: u32,
    ) -> Option<Frame> {
        if frame.format != PixelFormat::RGB24 {
            return None;
        }

        let src_w = frame.width as usize;
        let src_h = frame.height as usize;
        let dst_w = thumb_width as usize;
        let dst_h = thumb_height as usize;

        if dst_w == 0 || dst_h == 0 || src_w == 0 || src_h == 0 {
            return None;
        }

        let mut thumb_data = vec![0u8; dst_w * dst_h * 3];

        let x_ratio = src_w as f64 / dst_w as f64;
        let y_ratio = src_h as f64 / dst_h as f64;

        for y in 0..dst_h {
            for x in 0..dst_w {
                let src_x = ((x as f64 * x_ratio) as usize).min(src_w - 1);
                let src_y = ((y as f64 * y_ratio) as usize).min(src_h - 1);
                let src_idx = (src_y * src_w + src_x) * 3;
                let dst_idx = (y * dst_w + x) * 3;

                thumb_data[dst_idx] = frame.data[src_idx];
                thumb_data[dst_idx + 1] = frame.data[src_idx + 1];
                thumb_data[dst_idx + 2] = frame.data[src_idx + 2];
            }
        }

        Some(Frame::new(
            thumb_data,
            thumb_width,
            thumb_height,
            frame.timestamp_ms,
            frame.frame_number,
            PixelFormat::RGB24,
            frame.source_path.clone(),
        ))
    }

    /// Compute the structural similarity between two frames.
    ///
    /// Returns a value between 0.0 (completely different) and 1.0 (identical).
    /// Uses a simplified mean-squared-error based approach.
    ///
    /// # Arguments
    /// * `frame1` - First frame.
    /// * `frame2` - Second frame.
    ///
    /// # Returns
    /// * `Option<f64>` - Similarity score or None if frames are incompatible.
    pub fn frame_similarity(frame1: &Frame, frame2: &Frame) -> Option<f64> {
        if frame1.format != frame2.format
            || frame1.width != frame2.width
            || frame1.height != frame2.height
            || frame1.data.len() != frame2.data.len()
        {
            return None;
        }

        if frame1.data.is_empty() {
            return Some(1.0);
        }

        // Compute mean squared error
        let mut mse: f64 = 0.0;
        for i in 0..frame1.data.len() {
            let diff = frame1.data[i] as f64 - frame2.data[i] as f64;
            mse += diff * diff;
        }
        mse /= frame1.data.len() as f64;

        // Convert MSE to similarity (peak value is 255^2 = 65025)
        let similarity = 1.0 - (mse / 65025.0).min(1.0);

        Some(similarity)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::types::Frame;

        #[test]
        fn test_pixel_count() {
            assert_eq!(pixel_count(1920, 1080), 2_073_600);
            assert_eq!(pixel_count(640, 480), 307_200);
        }

        #[test]
        fn test_frame_size() {
            assert_eq!(frame_size(10, 10, PixelFormat::RGB24), 300);
            assert_eq!(frame_size(10, 10, PixelFormat::RGBA), 400);
        }

        #[test]
        fn test_batch_memory_estimate() {
            let size = batch_memory_estimate(30, 640, 480, PixelFormat::RGB24);
            assert_eq!(size, 30 * 640 * 480 * 3);
        }

        #[test]
        fn test_yuv420_to_rgb24() {
            let w = 4u32;
            let h = 4u32;
            let y_plane = vec![128u8; 16]; // mid-gray
            let u_plane = vec![128u8; 4]; // neutral chroma
            let v_plane = vec![128u8; 4]; // neutral chroma

            let rgb = yuv420_to_rgb24(&y_plane, &u_plane, &v_plane, w, h);

            // For neutral chroma (128, 128), Y=128 should give mid-gray RGB
            assert_eq!(rgb.len(), 48); // 4*4*3
            for i in 0..rgb.len() {
                assert!((rgb[i] as i16 - 128i16).abs() <= 5, "Pixel {} should be near 128, got {}", i, rgb[i]);
            }
        }

        #[test]
        fn test_create_thumbnail() {
            let data: Vec<u8> = (0..(100 * 100 * 3)).map(|i| (i % 256) as u8).collect();
            let frame = Frame::new(
                data,
                100,
                100,
                0.0,
                0,
                PixelFormat::RGB24,
                "/tmp/test.mp4",
            );

            let thumb = create_thumbnail(&frame, 10, 10);
            assert!(thumb.is_some());
            let thumb = thumb.unwrap();
            assert_eq!(thumb.width, 10);
            assert_eq!(thumb.height, 10);
            assert_eq!(thumb.data.len(), 300);
        }

        #[test]
        fn test_create_thumbnail_unsupported_format() {
            let data = vec![0u8; 400];
            let frame = Frame::new(
                data,
                10,
                10,
                0.0,
                0,
                PixelFormat::YUV420,
                "/tmp/test.mp4",
            );

            let thumb = create_thumbnail(&frame, 5, 5);
            assert!(thumb.is_none());
        }

        #[test]
        fn test_frame_similarity_identical() {
            let data = vec![128u8; 300];
            let frame1 = Frame::new(data.clone(), 10, 10, 0.0, 0, PixelFormat::RGB24, "/tmp/test.mp4");
            let frame2 = Frame::new(data, 10, 10, 0.0, 1, PixelFormat::RGB24, "/tmp/test.mp4");

            let sim = frame_similarity(&frame1, &frame2);
            assert!(sim.is_some());
            assert!((sim.unwrap() - 1.0).abs() < 1e-10, "Identical frames should have similarity 1.0");
        }

        #[test]
        fn test_frame_similarity_different() {
            let data1 = vec![0u8; 300];   // all black
            let data2 = vec![255u8; 300]; // all white
            let frame1 = Frame::new(data1, 10, 10, 0.0, 0, PixelFormat::RGB24, "/tmp/test.mp4");
            let frame2 = Frame::new(data2, 10, 10, 0.0, 1, PixelFormat::RGB24, "/tmp/test.mp4");

            let sim = frame_similarity(&frame1, &frame2);
            assert!(sim.is_some());
            assert!(sim.unwrap() < 0.1, "Black vs white should have very low similarity");
        }

        #[test]
        fn test_frame_similarity_incompatible() {
            let data1 = vec![0u8; 300];
            let data2 = vec![0u8; 400]; // different size
            let frame1 = Frame::new(data1, 10, 10, 0.0, 0, PixelFormat::RGB24, "/tmp/test.mp4");
            let frame2 = Frame::new(data2, 10, 10, 0.0, 1, PixelFormat::RGBA, "/tmp/test.mp4");

            let sim = frame_similarity(&frame1, &frame2);
            assert!(sim.is_none());
        }
    }
}

#[cfg(test)]
mod lib_tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
        assert!(VERSION.contains("0.1.0") || VERSION.chars().any(|c| c == '.'));
    }

    #[test]
    fn test_probe_videos_empty() {
        let results = probe_videos(&[]);
        assert!(results.is_empty());
    }
}

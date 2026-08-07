use crate::error::{ExtractorError, PixelFormat, Result};
use crate::types::{Frame, FrameBatch, VideoMetadata};
use crate::ExtractorConfig;
use std::path::Path;
use tokio::sync::mpsc;
use tracing::{debug, error, info, trace, warn};

/// Initialize the FFmpeg library.
/// Must be called before any other FFmpeg operations.
pub fn init_ffmpeg() -> Result<()> {
    ffmpeg_next::init().map_err(|e| {
        ExtractorError::FFmpeg {
            message: format!("Failed to initialize FFmpeg: {}", e),
            code: None,
        }
    })?;

    // Set log level based on tracing configuration
    unsafe {
        ffmpeg_next::ffi::av_log_set_level(ffmpeg_next::ffi::AV_LOG_WARNING);
    }

    info!("FFmpeg initialized successfully");
    Ok(())
}

/// Extract frames from a video file and send them through an async channel.
///
/// This function runs in a blocking task and:
/// 1. Opens the video file using FFmpeg
/// 2. Finds the video stream
/// 3. Decodes frames and converts them to the target pixel format
/// 4. Batches frames and sends them through the channel
/// 5. Sends a final empty batch with `is_last = true`
pub async fn extract_frames_to_channel(
    video_path: impl AsRef<Path>,
    config: &ExtractorConfig,
    tx: mpsc::Sender<FrameBatch>,
) -> Result<()> {
    let path = video_path.as_ref().to_path_buf();
    let config = config.clone();

    // Run the heavy FFmpeg work in a blocking task
    tokio::task::spawn_blocking(move || {
        extract_frames_blocking(&path, &config, tx)
    })
    .await
    .map_err(|e| ExtractorError::Internal(format!("Blocking task panicked: {}", e)))??;

    Ok(())
}

/// Blocking implementation of frame extraction.
fn extract_frames_blocking(
    video_path: &Path,
    config: &ExtractorConfig,
    tx: mpsc::Sender<FrameBatch>,
) -> Result<()> {
    let path_str = video_path.to_string_lossy().to_string();
    info!("Starting frame extraction for: {}", path_str);

    // Open the input file
    let mut input = ffmpeg_next::format::input(&video_path).map_err(|e| {
        ExtractorError::InvalidVideo {
            path: video_path.to_path_buf(),
            reason: format!("Failed to open video: {}", e),
        }
    })?;

    // Find the best video stream
    let stream = input
        .streams()
        .best(ffmpeg_next::media::Type::Video)
        .ok_or_else(|| ExtractorError::NoVideoStream {
            path: video_path.to_path_buf(),
        })?;

    let stream_index = stream.index();
    let stream_time_base = stream.time_base();
    let stream_avg_frame_rate = stream.avg_frame_rate();
    let stream_duration = stream.duration();

    debug!(
        stream_index = stream_index,
        time_base = ?stream_time_base,
        avg_frame_rate = ?stream_avg_frame_rate,
        "Found video stream"
    );

    // Create decoder
    let context = ffmpeg_next::codec::context::Context::from_parameters(stream.parameters())
        .map_err(|e| ExtractorError::FFmpeg {
            message: format!("Failed to create codec context: {}", e),
            code: None,
        })?;

    let mut decoder = context.decoder().video().map_err(|e| ExtractorError::FFmpeg {
        message: format!("Failed to create video decoder: {}", e),
        code: None,
    })?;

    let source_width = decoder.width();
    let source_height = decoder.height();
    let source_format = decoder.format();

    // Calculate target dimensions
    let (target_width, target_height) =
        calculate_target_dimensions(source_width, source_height, config.max_resolution);

    debug!(
        source_width = source_width,
        source_height = source_height,
        target_width = target_width,
        target_height = target_height,
        source_format = ?source_format,
        "Decoder initialized"
    );

    // Create scaler to convert to target format and size
    let mut scaler = ffmpeg_next::software::scaling::Context::get(
        source_format,
        source_width,
        source_height,
        config.output_format.as_ffmpeg_format(),
        target_width,
        target_height,
        ffmpeg_next::software::scaling::Flags::BILINEAR,
    )
    .map_err(|e| ExtractorError::Scaling {
        reason: format!("Failed to create scaler: {}", e),
    })?;

    // Calculate source FPS
    let source_fps = if stream_avg_frame_rate.denominator() > 0 {
        stream_avg_frame_rate.numerator() as f64 / stream_avg_frame_rate.denominator() as f64
    } else {
        30.0 // fallback
    };

    // Calculate frame skip interval for target FPS
    let frame_skip = if let Some(target_fps) = config.target_fps {
        if target_fps < source_fps {
            (source_fps / target_fps).round() as u64
        } else {
            1
        }
    } else {
        1 // extract all frames
    };

    debug!(
        source_fps = source_fps,
        frame_skip = frame_skip,
        "Frame extraction parameters"
    );

    // Decode and process frames
    let mut frame = ffmpeg_next::frame::Video::empty();
    let mut scaled_frame = ffmpeg_next::frame::Video::empty();
    let mut frame_buffer: Vec<Frame> = Vec::with_capacity(config.batch_size);
    let mut frame_counter: u64 = 0;
    let mut decoded_frame_number: u64 = 0;
    let mut batch_sequence: u64 = 0;

    for (stream, packet) in input.packets() {
        if stream.index() != stream_index {
            continue;
        }

        // Send packet to decoder
        if let Err(e) = decoder.send_packet(&packet) {
            warn!("Error sending packet to decoder: {}", e);
            continue;
        }

        // Receive decoded frames
        while decoder.receive_frame(&mut frame).is_ok() {
            decoded_frame_number += 1;

            // Skip frames to match target FPS
            if frame_skip > 1 && decoded_frame_number % frame_skip != 0 {
                continue;
            }

            // Calculate timestamp in milliseconds
            let timestamp_ms = if frame.timestamp().is_some() {
                frame.timestamp().unwrap() as f64
                    * f64::from(stream_time_base.numerator())
                    / f64::from(stream_time_base.denominator())
                    * 1000.0
            } else {
                // Estimate from frame number and FPS
                frame_counter as f64 / config.target_fps.unwrap_or(source_fps) * 1000.0
            };

            // Scale the frame
            scaler.run(&frame, &mut scaled_frame).map_err(|e| {
                ExtractorError::Scaling {
                    reason: format!("Failed to scale frame: {}", e),
                }
            })?;

            // Convert to Vec<u8>
            let frame_data = ffmpeg_frame_to_vec(&scaled_frame, config.output_format);

            let extracted_frame = Frame::new(
                frame_data,
                target_width as u32,
                target_height as u32,
                timestamp_ms,
                frame_counter,
                config.output_format,
                &path_str,
            );

            frame_buffer.push(extracted_frame);
            frame_counter += 1;

            // Send batch when full
            if frame_buffer.len() >= config.batch_size {
                let batch =
                    FrameBatch::new(batch_sequence, frame_buffer.clone(), None, false);

                // Use blocking_send since we're in a blocking context
                match tx.blocking_send(batch) {
                    Ok(_) => {
                        trace!(batch_sequence = batch_sequence, "Sent frame batch");
                    }
                    Err(e) => {
                        warn!("Channel closed, stopping extraction: {}", e);
                        return Ok(());
                    }
                }

                batch_sequence += 1;
                frame_buffer.clear();
            }
        }
    }

    // Flush decoder
    if decoder.send_eof().is_ok() {
        while decoder.receive_frame(&mut frame).is_ok() {
            decoded_frame_number += 1;

            if frame_skip > 1 && decoded_frame_number % frame_skip != 0 {
                continue;
            }

            let timestamp_ms = if frame.timestamp().is_some() {
                frame.timestamp().unwrap() as f64
                    * f64::from(stream_time_base.numerator())
                    / f64::from(stream_time_base.denominator())
                    * 1000.0
            } else {
                frame_counter as f64 / config.target_fps.unwrap_or(source_fps) * 1000.0
            };

            scaler.run(&frame, &mut scaled_frame).map_err(|e| {
                ExtractorError::Scaling {
                    reason: format!("Failed to scale frame: {}", e),
                }
            })?;

            let frame_data = ffmpeg_frame_to_vec(&scaled_frame, config.output_format);

            let extracted_frame = Frame::new(
                frame_data,
                target_width as u32,
                target_height as u32,
                timestamp_ms,
                frame_counter,
                config.output_format,
                &path_str,
            );

            frame_buffer.push(extracted_frame);
            frame_counter += 1;

            if frame_buffer.len() >= config.batch_size {
                let batch = FrameBatch::new(batch_sequence, frame_buffer.clone(), None, false);

                match tx.blocking_send(batch) {
                    Ok(_) => {}
                    Err(_) => return Ok(()),
                }

                batch_sequence += 1;
                frame_buffer.clear();
            }
        }
    }

    // Send remaining frames
    if !frame_buffer.is_empty() {
        let batch = FrameBatch::new(
            batch_sequence,
            frame_buffer,
            Some(frame_counter),
            true,
        );

        match tx.blocking_send(batch) {
            Ok(_) => {}
            Err(_) => {}
        }
    } else {
        // Send empty final batch
        let batch = FrameBatch::new(batch_sequence, vec![], Some(frame_counter), true);
        let _ = tx.blocking_send(batch);
    }

    info!(
        total_frames = frame_counter,
        batches = batch_sequence + 1,
        "Frame extraction completed for: {}",
        path_str
    );

    Ok(())
}

/// Convert an FFmpeg frame to a Vec<u8>.
fn ffmpeg_frame_to_vec(frame: &ffmpeg_next::frame::Video, format: PixelFormat) -> Vec<u8> {
    let width = frame.width() as usize;
    let height = frame.height() as usize;
    let stride = frame.stride(0);
    let data = frame.data(0);

    match format {
        PixelFormat::RGB24 => {
            let expected_size = width * height * 3;
            let mut result = Vec::with_capacity(expected_size);

            for row in 0..height {
                let row_start = row * stride;
                let row_data = &data[row_start..row_start + width * 3];
                result.extend_from_slice(row_data);
            }

            result
        }
        PixelFormat::RGBA => {
            let expected_size = width * height * 4;
            let mut result = Vec::with_capacity(expected_size);

            for row in 0..height {
                let row_start = row * stride;
                let row_data = &data[row_start..row_start + width * 4];
                result.extend_from_slice(row_data);
            }

            result
        }
        PixelFormat::YUV420 => {
            // For YUV420, we need to pack the planar data
            let y_stride = frame.stride(0);
            let u_stride = frame.stride(1);
            let v_stride = frame.stride(2);
            let y_data = frame.data(0);
            let u_data = frame.data(1);
            let v_data = frame.data(2);

            // Store Y plane + UV planes
            let mut result = Vec::with_capacity(width * height * 2);

            for row in 0..height {
                let row_start = row * y_stride;
                result.extend_from_slice(&y_data[row_start..row_start + width]);
            }

            let uv_height = height / 2;
            for row in 0..uv_height {
                let u_start = row * u_stride;
                let v_start = row * v_stride;
                let uv_width = width / 2;
                for col in 0..uv_width {
                    result.push(u_data[u_start + col]);
                    result.push(v_data[v_start + col]);
                }
            }

            result
        }
    }
}

/// Calculate target dimensions while preserving aspect ratio.
fn calculate_target_dimensions(
    source_width: u32,
    source_height: u32,
    max_resolution: Option<(u32, u32)>,
) -> (u32, u32) {
    let max = match max_resolution {
        Some(m) => m,
        None => return (source_width, source_height),
    };

    if source_width <= max.0 && source_height <= max.1 {
        return (source_width, source_height);
    }

    let width_ratio = max.0 as f64 / source_width as f64;
    let height_ratio = max.1 as f64 / source_height as f64;
    let scale = width_ratio.min(height_ratio);

    let new_width = (source_width as f64 * scale) as u32;
    let new_height = (source_height as f64 * scale) as u32;

    // Ensure even dimensions (required by many codecs)
    (new_width & !1, new_height & !1)
}

/// Probe video metadata using FFmpeg.
pub fn get_video_metadata_ff(video_path: impl AsRef<Path>) -> Result<VideoMetadata> {
    let path = video_path.as_ref();
    let path_str = path.to_string_lossy().to_string();

    debug!("Probing video metadata for: {}", path_str);

    let input = ffmpeg_next::format::input(&path).map_err(|e| {
        ExtractorError::MetadataExtraction {
            path: path.to_path_buf(),
            reason: format!("Failed to open video: {}", e),
        }
    })?;

    // Get file size
    let file_size = std::fs::metadata(path)
        .map(|m| m.len())
        .unwrap_or(0);

    // Find video stream
    let stream = input
        .streams()
        .best(ffmpeg_next::media::Type::Video)
        .ok_or_else(|| ExtractorError::NoVideoStream {
            path: path.to_path_buf(),
        })?;

    let stream_index = stream.index();
    let time_base = stream.time_base();

    // Get duration
    let duration = if input.duration() > 0 {
        std::time::Duration::from_micros(input.duration() as u64)
    } else if stream.duration() > 0 {
        let dur = stream.duration() as f64
            * f64::from(time_base.numerator())
            / f64::from(time_base.denominator());
        std::time::Duration::from_secs_f64(dur)
    } else {
        std::time::Duration::from_secs(0)
    };

    // Create decoder to get detailed info
    let context = ffmpeg_next::codec::context::Context::from_parameters(stream.parameters())
        .map_err(|e| ExtractorError::MetadataExtraction {
            path: path.to_path_buf(),
            reason: format!("Failed to create codec context: {}", e),
        })?;

    let decoder = context.decoder().video().map_err(|e| ExtractorError::MetadataExtraction {
        path: path.to_path_buf(),
        reason: format!("Failed to create video decoder: {}", e),
    })?;

    let width = decoder.width();
    let height = decoder.height();
    let codec_id = decoder.id();
    let codec_name = format!("{:?}", codec_id).to_lowercase();
    let pixel_format = format!("{:?}", decoder.format());

    // Get FPS
    let avg_frame_rate = stream.avg_frame_rate();
    let fps = if avg_frame_rate.denominator() > 0 {
        avg_frame_rate.numerator() as f64 / avg_frame_rate.denominator() as f64
    } else {
        let r_frame_rate = stream.rate();
        if r_frame_rate.denominator() > 0 {
            r_frame_rate.numerator() as f64 / r_frame_rate.denominator() as f64
        } else {
            0.0
        }
    };

    // Estimate total frames
    let total_frames = if fps > 0.0 && duration.as_secs_f64() > 0.0 {
        (duration.as_secs_f64() * fps).round() as u64
    } else {
        0
    };

    // Get bitrate
    let bitrate = input.metadata().get("bitrate")
        .and_then(|b| b.parse::<u64>().ok())
        .unwrap_or_else(|| input.bit_rate() as u64);

    // Check for audio stream
    let has_audio = input
        .streams()
        .best(ffmpeg_next::media::Type::Audio)
        .is_some();

    let metadata = VideoMetadata::new(
        &path_str,
        duration,
        width,
        height,
        fps,
        total_frames,
        &codec_name,
        bitrate,
        &pixel_format,
        has_audio,
        file_size,
    );

    info!(
        path = %path_str,
        duration_secs = %metadata.duration.as_secs_f64(),
        width = width,
        height = height,
        fps = fps,
        codec = %codec_name,
        total_frames = total_frames,
        "Video metadata extracted"
    );

    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_target_dimensions_no_change() {
        // Source is smaller than max
        let (w, h) = calculate_target_dimensions(640, 480, Some((1280, 720)));
        assert_eq!(w, 640);
        assert_eq!(h, 480);
    }

    #[test]
    fn test_calculate_target_dimensions_scale_down() {
        // Source is larger, should scale down
        let (w, h) = calculate_target_dimensions(1920, 1080, Some((1280, 720)));
        assert_eq!(w, 1280);
        assert!(h <= 720);
        // Should preserve aspect ratio
        let ratio = 1920.0 / 1080.0;
        let new_ratio = w as f64 / h as f64;
        assert!((ratio - new_ratio).abs() < 0.01);
    }

    #[test]
    fn test_calculate_target_dimensions_no_max() {
        let (w, h) = calculate_target_dimensions(1920, 1080, None);
        assert_eq!(w, 1920);
        assert_eq!(h, 1080);
    }

    #[test]
    fn test_calculate_target_dimensions_even_dimensions() {
        // Odd dimensions should be rounded to even
        let (w, h) = calculate_target_dimensions(1920, 1080, Some((100, 100)));
        assert_eq!(w % 2, 0);
        assert_eq!(h % 2, 0);
    }
}

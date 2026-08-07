use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during frame extraction and video processing.
#[derive(Error, Debug, Clone)]
pub enum ExtractorError {
    /// An error originating from the FFmpeg library.
    #[error("FFmpeg error: {message} (code: {code:?})")]
    FFmpeg {
        message: String,
        code: Option<i32>,
    },

    /// An I/O error occurred while reading or writing files.
    #[error("I/O error at path '{path}': {message}")]
    Io {
        path: PathBuf,
        message: String,
    },

    /// The video file is invalid, corrupted, or unsupported.
    #[error("Invalid video file '{path}': {reason}")]
    InvalidVideo {
        path: PathBuf,
        reason: String,
    },

    /// No video stream found in the media file.
    #[error("No video stream found in '{path}'")]
    NoVideoStream {
        path: PathBuf,
    },

    /// Failed to decode a video frame.
    #[error("Frame decode error at timestamp {timestamp_ms}ms: {reason}")]
    FrameDecode {
        timestamp_ms: f64,
        reason: String,
    },

    /// GPU acceleration error.
    #[error("GPU acceleration error: {message}")]
    Gpu {
        message: String,
    },

    /// GPU is not available on this system.
    #[error("GPU acceleration requested but no GPU is available")]
    GpuNotAvailable,

    /// Scene detection algorithm failure.
    #[error("Scene detection error: {message}")]
    SceneDetection {
        message: String,
    },

    /// Flash detection algorithm failure.
    #[error("Flash detection error: {message}")]
    FlashDetection {
        message: String,
    },

    /// Channel send error (the receiver was dropped).
    #[error("Channel send error: {0}")]
    ChannelSend(String),

    /// Channel receive error.
    #[error("Channel receive error: {0}")]
    ChannelRecv(String),

    /// Invalid configuration parameter.
    #[error("Invalid configuration: {field} - {reason}")]
    InvalidConfig {
        field: String,
        reason: String,
    },

    /// Video metadata extraction failed.
    #[error("Metadata extraction failed for '{path}': {reason}")]
    MetadataExtraction {
        path: PathBuf,
        reason: String,
    },

    /// Pixel format conversion error.
    #[error("Pixel format conversion error: {from:?} -> {to:?} - {reason}")]
    PixelFormatConversion {
        from: PixelFormat,
        to: PixelFormat,
        reason: String,
    },

    /// Scaling/resolution error.
    #[error("Scaling error: {reason}")]
    Scaling {
        reason: String,
    },

    /// Batch processing error.
    #[error("Batch processing error: {reason}")]
    BatchError {
        reason: String,
    },

    /// Timeout waiting for frames.
    #[error("Timeout waiting for frames after {timeout_ms}ms")]
    Timeout {
        timeout_ms: u64,
    },

    /// A generic internal error.
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Pixel formats supported by the frame extractor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PixelFormat {
    /// 24-bit RGB (3 bytes per pixel: R, G, B).
    RGB24,
    /// 32-bit RGBA (4 bytes per pixel: R, G, B, A).
    RGBA,
    /// YUV 4:2:0 planar format.
    YUV420,
}

impl PixelFormat {
    /// Returns the number of bytes per pixel for this format.
    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            PixelFormat::RGB24 => 3,
            PixelFormat::RGBA => 4,
            PixelFormat::YUV420 => 2, // Average for 4:2:0 subsampled
        }
    }

    /// Returns the FFmpeg pixel format equivalent.
    pub fn as_ffmpeg_format(&self) -> ffmpeg_next::format::Pixel {
        match self {
            PixelFormat::RGB24 => ffmpeg_next::format::Pixel::RGB24,
            PixelFormat::RGBA => ffmpeg_next::format::Pixel::RGBA,
            PixelFormat::YUV420 => ffmpeg_next::format::Pixel::YUV420P,
        }
    }
}

/// Result type alias for extractor operations.
pub type Result<T> = std::result::Result<T, ExtractorError>;

// --- Conversions from external error types ---

impl From<ffmpeg_next::Error> for ExtractorError {
    fn from(err: ffmpeg_next::Error) -> Self {
        let code = match err {
            ffmpeg_next::Error::Other(code) => Some(code),
            ffmpeg_next::Error::Bug => Some(-1),
            ffmpeg_next::Error::Bug2 => Some(-2),
            ffmpeg_next::Error::InvalidData => Some(ffmpeg_next::util::error::EINVAL),
            ffmpeg_next::Error::InvalidData2 => Some(ffmpeg_next::util::error::EINVAL),
            ffmpeg_next::Error::BufferTooSmall => Some(ffmpeg_next::util::error::EINVAL),
            ffmpeg_next::Error::Eof => Some(ffmpeg_next::util::error::EAGAIN),
            ffmpeg_next::Error::Exit => Some(ffmpeg_next::util::error::EINTR),
            ffmpeg_next::Error::External => Some(ffmpeg_next::util::error::EIO),
            ffmpeg_next::Error::PatchWelcome => None,
            _ => None,
        };

        ExtractorError::FFmpeg {
            message: err.to_string(),
            code,
        }
    }
}

impl From<std::io::Error> for ExtractorError {
    fn from(err: std::io::Error) -> Self {
        ExtractorError::Io {
            path: PathBuf::from("<unknown>"),
            message: err.to_string(),
        }
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for ExtractorError {
    fn from(err: tokio::sync::mpsc::error::SendError<T>) -> Self {
        ExtractorError::ChannelSend(err.to_string())
    }
}

impl From<tokio::sync::mpsc::error::RecvError> for ExtractorError {
    fn from(err: tokio::sync::mpsc::error::RecvError) -> Self {
        ExtractorError::ChannelRecv(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pixel_format_bytes_per_pixel() {
        assert_eq!(PixelFormat::RGB24.bytes_per_pixel(), 3);
        assert_eq!(PixelFormat::RGBA.bytes_per_pixel(), 4);
        assert_eq!(PixelFormat::YUV420.bytes_per_pixel(), 2);
    }

    #[test]
    fn test_error_display() {
        let err = ExtractorError::InvalidVideo {
            path: PathBuf::from("/tmp/test.mp4"),
            reason: "unsupported codec".to_string(),
        };
        assert!(err.to_string().contains("Invalid video file"));
        assert!(err.to_string().contains("unsupported codec"));
    }

    #[test]
    fn test_from_ffmpeg_error() {
        let ffmpeg_err = ffmpeg_next::Error::InvalidData;
        let extractor_err: ExtractorError = ffmpeg_err.into();
        match extractor_err {
            ExtractorError::FFmpeg { message, code } => {
                assert!(!message.is_empty());
                assert!(code.is_some());
            }
            other => panic!("Expected FFmpeg variant, got {:?}", other),
        }
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let extractor_err: ExtractorError = io_err.into();
        match extractor_err {
            ExtractorError::Io { path, message } => {
                assert_eq!(path.to_str(), Some("<unknown>"));
                assert!(message.contains("file not found"));
            }
            other => panic!("Expected Io variant, got {:?}", other),
        }
    }
}

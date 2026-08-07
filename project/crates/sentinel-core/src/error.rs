use std::fmt;
use std::io;
use std::time::Duration;

/// The main error type for the Sentinel video analysis pipeline.
///
/// All recoverable and non-recoverable failures are represented here so that
/// downstream code can match on specific variants and decide whether to retry,
/// fail the job, or escalate.
#[derive(Debug, Clone, PartialEq)]
pub enum SentinelError {
    /// I/O error reading or writing files.
    Io { context: String, source: String },
    /// FFmpeg exited with a non-zero status or produced malformed output.
    FFmpeg { stage: String, stderr: String },
    /// Model inference failed (e.g. ONNX / TensorRT / tflite error).
    ModelInference { model: String, detail: String },
    /// A policy was violated and processing was halted.
    PolicyViolation { policy: String, detail: String },
    /// Serde serialization or deserialization failed.
    Serialization { format: String, detail: String },
    /// Invalid configuration value or missing required key.
    Config { key: String, detail: String },
    /// GPU memory allocation, CUDA / Vulkan failure, etc.
    GPU { device: String, detail: String },
    /// An operation exceeded its deadline.
    Timeout { operation: String, elapsed: Duration, limit: Duration },
    /// Catch-all for unexpected failures.
    Unknown { detail: String },
}

impl fmt::Display for SentinelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SentinelError::Io { context, source } => {
                write!(f, "I/O error ({context}): {source}")
            }
            SentinelError::FFmpeg { stage, stderr } => {
                write!(f, "FFmpeg failed at stage '{stage}': {stderr}")
            }
            SentinelError::ModelInference { model, detail } => {
                write!(f, "model '{model}' inference error: {detail}")
            }
            SentinelError::PolicyViolation { policy, detail } => {
                write!(f, "policy violation ({policy}): {detail}")
            }
            SentinelError::Serialization { format, detail } => {
                write!(f, "{format} serialization error: {detail}")
            }
            SentinelError::Config { key, detail } => {
                write!(f, "configuration error for key '{key}': {detail}")
            }
            SentinelError::GPU { device, detail } => {
                write!(f, "GPU error on device '{device}': {detail}")
            }
            SentinelError::Timeout {
                operation,
                elapsed,
                limit,
            } => {
                write!(
                    f,
                    "operation '{operation}' timed out after {:?} (limit: {:?})",
                    elapsed, limit
                )
            }
            SentinelError::Unknown { detail } => write!(f, "unknown error: {detail}"),
        }
    }
}

impl std::error::Error for SentinelError {}

// ---------------------------------------------------------------------------
// From conversions
// ---------------------------------------------------------------------------

impl From<io::Error> for SentinelError {
    fn from(err: io::Error) -> Self {
        SentinelError::Io {
            context: "std::io".into(),
            source: err.to_string(),
        }
    }
}

impl From<serde_json::Error> for SentinelError {
    fn from(err: serde_json::Error) -> Self {
        SentinelError::Serialization {
            format: "JSON".into(),
            detail: err.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Helper constructors
// ---------------------------------------------------------------------------

impl SentinelError {
    pub fn io<S1: Into<String>, S2: Into<String>>(context: S1, source: S2) -> Self {
        SentinelError::Io {
            context: context.into(),
            source: source.into(),
        }
    }

    pub fn ffmpeg<S1: Into<String>, S2: Into<String>>(stage: S1, stderr: S2) -> Self {
        SentinelError::FFmpeg {
            stage: stage.into(),
            stderr: stderr.into(),
        }
    }

    pub fn model_inference<S1: Into<String>, S2: Into<String>>(model: S1, detail: S2) -> Self {
        SentinelError::ModelInference {
            model: model.into(),
            detail: detail.into(),
        }
    }

    pub fn policy_violation<S1: Into<String>, S2: Into<String>>(policy: S1, detail: S2) -> Self {
        SentinelError::PolicyViolation {
            policy: policy.into(),
            detail: detail.into(),
        }
    }

    pub fn serialization<S1: Into<String>, S2: Into<String>>(format: S1, detail: S2) -> Self {
        SentinelError::Serialization {
            format: format.into(),
            detail: detail.into(),
        }
    }

    pub fn config<S1: Into<String>, S2: Into<String>>(key: S1, detail: S2) -> Self {
        SentinelError::Config {
            key: key.into(),
            detail: detail.into(),
        }
    }

    pub fn gpu<S1: Into<String>, S2: Into<String>>(device: S1, detail: S2) -> Self {
        SentinelError::GPU {
            device: device.into(),
            detail: detail.into(),
        }
    }

    pub fn timeout<S: Into<String>>(operation: S, elapsed: Duration, limit: Duration) -> Self {
        SentinelError::Timeout {
            operation: operation.into(),
            elapsed,
            limit,
        }
    }

    pub fn unknown<S: Into<String>>(detail: S) -> Self {
        SentinelError::Unknown {
            detail: detail.into(),
        }
    }

    /// Returns `true` if this error is transient and the operation may succeed
    /// on retry.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            SentinelError::Io { .. }
                | SentinelError::FFmpeg { .. }
                | SentinelError::ModelInference { .. }
                | SentinelError::GPU { .. }
                | SentinelError::Timeout { .. }
                | SentinelError::Unknown { .. }
        )
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn display_io() {
        let e = SentinelError::io("read file", "permission denied");
        assert_eq!(format!("{e}"), "I/O error (read file): permission denied");
    }

    #[test]
    fn display_ffmpeg() {
        let e = SentinelError::ffmpeg("extract", "codec not found");
        assert_eq!(
            format!("{e}"),
            "FFmpeg failed at stage 'extract': codec not found"
        );
    }

    #[test]
    fn display_timeout() {
        let e = SentinelError::timeout(
            "inference",
            Duration::from_secs(12),
            Duration::from_secs(10),
        );
        assert!(format!("{e}").contains("timed out"));
    }

    #[test]
    fn retryable_detection() {
        assert!(SentinelError::io("x", "y").is_retryable());
        assert!(SentinelError::timeout("x", Duration::from_secs(1), Duration::from_secs(2)).is_retryable());
        assert!(!SentinelError::config("x", "y").is_retryable());
        assert!(!SentinelError::policy_violation("x", "y").is_retryable());
    }

    #[test]
    fn from_io_error() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file gone");
        let sentinel: SentinelError = io_err.into();
        assert!(matches!(sentinel, SentinelError::Io { .. }));
    }

    #[test]
    fn unknown_constructor() {
        let e = SentinelError::unknown("something weird happened");
        assert!(format!("{e}").contains("something weird"));
    }
}

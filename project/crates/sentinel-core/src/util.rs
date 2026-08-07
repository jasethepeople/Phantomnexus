use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Compute the SHA-256 hash of a video file by reading it in chunks.
///
/// # Arguments
/// * `path` – Path to the video file.
///
/// # Returns
/// A lowercase hex-encoded SHA-256 digest string, or a `SentinelError::Io`
/// when the file cannot be read.
pub fn hash_video<P: AsRef<Path>>(path: P) -> Result<String, crate::error::SentinelError> {
    use std::fs::File;
    use std::io::{BufReader, Read};

    let file = File::open(path.as_ref())?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let n = reader.read(&mut buffer).map_err(|e| {
            crate::error::SentinelError::io("hash_video read", e.to_string())
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Compute SHA-256 hash from an in-memory byte slice.
pub fn hash_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Return a platform-appropriate temporary directory path for sentinel
/// scratch files.
pub fn temp_dir() -> PathBuf {
    std::env::temp_dir().join("sentinel")
}

/// Return a unique temporary directory for a specific job.
pub fn job_temp_dir(job_id: &uuid::Uuid) -> PathBuf {
    temp_dir().join(format!("job-{}", job_id.as_hyphenated()))
}

/// Format a duration in seconds as `HH:MM:SS.mmm`.
///
/// # Examples
/// ```
/// use sentinel_core::util::format_timestamp;
/// assert_eq!(format_timestamp(3661.123), "01:01:01.123");
/// assert_eq!(format_timestamp(0.0), "00:00:00.000");
/// ```
pub fn format_timestamp(seconds: f64) -> String {
    let s = seconds.max(0.0);
    let total_secs = s as u64;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    let millis = ((s.fract()) * 1000.0) as u32;
    format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, secs, millis)
}

/// Parse a duration string into seconds.
///
/// Supported formats:
/// - `42` → 42 seconds
/// - `1:30` → 90 seconds
/// - `1:30:00` → 5400 seconds
/// - `1.5` → 1.5 seconds
/// - `1h30m10s` → 5410 seconds
///
/// Returns `None` when the string cannot be parsed.
pub fn parse_duration(input: &str) -> Option<f64> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Try colon-separated HH:MM:SS or MM:SS
    if trimmed.contains(':') {
        let parts: Vec<&str> = trimmed.split(':').collect();
        match parts.len() {
            2 => {
                let mins = parts[0].parse::<f64>().ok()?;
                let secs = parts[1].parse::<f64>().ok()?;
                return Some(mins * 60.0 + secs);
            }
            3 => {
                let hours = parts[0].parse::<f64>().ok()?;
                let mins = parts[1].parse::<f64>().ok()?;
                let secs = parts[2].parse::<f64>().ok()?;
                return Some(hours * 3600.0 + mins * 60.0 + secs);
            }
            _ => return None,
        }
    }

    // Try suffixed format: 1h30m10s
    let mut total: f64 = 0.0;
    let mut current = String::new();
    for ch in trimmed.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            current.push(ch);
        } else if ch == 'h' || ch == 'H' {
            let val = current.parse::<f64>().ok()?;
            total += val * 3600.0;
            current.clear();
        } else if ch == 'm' || ch == 'M' {
            let val = current.parse::<f64>().ok()?;
            total += val * 60.0;
            current.clear();
        } else if ch == 's' || ch == 'S' {
            let val = current.parse::<f64>().ok()?;
            total += val;
            current.clear();
        } else {
            return None;
        }
    }

    if current.is_empty() {
        return Some(total);
    }

    // Plain seconds (integer or float)
    current.parse::<f64>().ok()
}

/// Estimate the working-set memory (bytes) required to analyse a video.
///
/// The heuristic accounts for:
/// - decoded frame buffers (at `fps` and resolution),
/// - model working memory (proportional to batch size),
/// - audio waveform buffer.
///
/// # Arguments
/// * `duration_seconds` – Length of the video.
/// * `width` – Frame width in pixels.
/// * `height` – Frame height in pixels.
/// * `fps` – Frames per second of the source.
/// * `batch_size` – Visual inference batch size.
/// * `enable_audio` – Whether audio processing is enabled.
pub fn estimate_memory_usage(
    duration_seconds: f64,
    width: u32,
    height: u32,
    fps: f64,
    batch_size: usize,
    enable_audio: bool,
) -> u64 {
    let frame_pixels = (width as u64) * (height as u64);
    // RGB24 = 3 bytes per pixel
    let frame_bytes = frame_pixels * 3;
    // Frame extraction ring buffer: ~2 seconds
    let frame_buffer = frame_bytes * (fps * 2.0).ceil() as u64;
    // Model working set: batch_size frames at FP32 (4 bytes per channel)
    let model_bytes = frame_pixels * 4 * batch_size as u64;
    // Audio buffer: 48kHz stereo f32 for the full duration
    let audio_bytes = if enable_audio {
        (48000_u64 * 2 * 4) * duration_seconds as u64
    } else {
        0
    };
    // Overhead for misc buffers
    let overhead = 256 * 1024 * 1024; // 256 MiB

    frame_buffer + model_bytes + audio_bytes + overhead
}

/// Format a byte count into a human-readable string using binary (IEC)
/// prefixes.
///
/// # Examples
/// ```
/// use sentinel_core::util::format_bytes;
/// assert_eq!(format_bytes(512), "512 B");
/// assert_eq!(format_bytes(1024), "1.00 KiB");
/// assert_eq!(format_bytes(1536), "1.50 KiB");
/// assert_eq!(format_bytes(1024 * 1024 * 3), "3.00 MiB");
/// ```
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes == 0 {
        return "0 B".into();
    }
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx + 1 < UNITS.len() {
        size /= 1024.0;
        unit_idx += 1;
    }
    format!("{:.2} {}", size, UNITS[unit_idx])
}

/// Format a `std::time::Duration` into a human-readable string.
///
/// # Examples
/// ```
/// use sentinel_core::util::format_duration;
/// use std::time::Duration;
/// assert_eq!(format_duration(&Duration::from_secs(90)), "1m 30s");
/// assert_eq!(format_duration(&Duration::from_secs(3661)), "1h 1m 1s");
/// ```
pub fn format_duration(d: &Duration) -> String {
    let total_secs = d.as_secs();
    if total_secs == 0 {
        return format!("{}ms", d.subsec_millis());
    }
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// Ensure that the sentinel temp directory exists.
pub fn ensure_temp_dir() -> Result<PathBuf, crate::error::SentinelError> {
    let dir = temp_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Generate a unique filename inside the temp directory.
pub fn temp_filename(prefix: &str, suffix: &str) -> Result<PathBuf, crate::error::SentinelError> {
    let dir = ensure_temp_dir()?;
    let id = uuid::Uuid::new_v4();
    let name = format!("{}_{}{}", prefix, id.as_simple(), suffix);
    Ok(dir.join(name))
}

// ===========================================================================
// Unit tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- hash_bytes ---

    #[test]
    fn hash_bytes_known() {
        let data = b"hello world";
        let h = hash_bytes(data);
        // Known SHA-256 for "hello world"
        assert_eq!(
            h,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn hash_bytes_empty() {
        let h = hash_bytes(b"");
        assert_eq!(
            h,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    // --- format_timestamp ---

    #[test]
    fn format_timestamp_zero() {
        assert_eq!(format_timestamp(0.0), "00:00:00.000");
    }

    #[test]
    fn format_timestamp_typical() {
        assert_eq!(format_timestamp(3661.123), "01:01:01.123");
    }

    #[test]
    fn format_timestamp_negative_clamped() {
        assert_eq!(format_timestamp(-5.0), "00:00:00.000");
    }

    #[test]
    fn format_timestamp_subsecond() {
        assert_eq!(format_timestamp(12.999), "00:00:12.999");
    }

    // --- parse_duration ---

    #[test]
    fn parse_duration_plain_seconds() {
        assert_eq!(parse_duration("42"), Some(42.0));
        assert_eq!(parse_duration("3.5"), Some(3.5));
    }

    #[test]
    fn parse_duration_mm_ss() {
        assert_eq!(parse_duration("1:30"), Some(90.0));
        assert_eq!(parse_duration("0:45"), Some(45.0));
    }

    #[test]
    fn parse_duration_hh_mm_ss() {
        assert_eq!(parse_duration("1:30:00"), Some(5400.0));
        assert_eq!(parse_duration("0:01:30"), Some(90.0));
    }

    #[test]
    fn parse_duration_suffixed() {
        assert_eq!(parse_duration("1h30m10s"), Some(5410.0));
        assert_eq!(parse_duration("2h"), Some(7200.0));
        assert_eq!(parse_duration("30m"), Some(1800.0));
        assert_eq!(parse_duration("45s"), Some(45.0));
    }

    #[test]
    fn parse_duration_empty() {
        assert_eq!(parse_duration(""), None);
    }

    #[test]
    fn parse_duration_invalid() {
        assert_eq!(parse_duration("abc"), None);
    }

    #[test]
    fn parse_duration_trimmed() {
        assert_eq!(parse_duration("  120  "), Some(120.0));
    }

    // --- format_bytes ---

    #[test]
    fn format_bytes_zero() {
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn format_bytes_bytes() {
        assert_eq!(format_bytes(512), "512 B");
    }

    #[test]
    fn format_bytes_kib() {
        assert_eq!(format_bytes(1024), "1.00 KiB");
        assert_eq!(format_bytes(1536), "1.50 KiB");
    }

    #[test]
    fn format_bytes_mib() {
        assert_eq!(format_bytes(1024 * 1024 * 3), "3.00 MiB");
    }

    #[test]
    fn format_bytes_gib() {
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 2), "2.00 GiB");
    }

    // --- format_duration ---

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration(&Duration::from_secs(45)), "45s");
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration(&Duration::from_secs(90)), "1m 30s");
    }

    #[test]
    fn format_duration_hours() {
        assert_eq!(
            format_duration(&Duration::from_secs(3661)),
            "1h 1m 1s"
        );
    }

    #[test]
    fn format_duration_millis() {
        assert_eq!(format_duration(&Duration::from_millis(500)), "500ms");
    }

    // --- estimate_memory_usage ---

    #[test]
    fn estimate_memory_usage_nonzero() {
        let mem = estimate_memory_usage(60.0, 1920, 1080, 30.0, 8, true);
        assert!(mem > 0);
        // Should be at least the 256 MiB overhead
        assert!(mem >= 256 * 1024 * 1024);
    }

    #[test]
    fn estimate_memory_usage_no_audio() {
        let with_audio = estimate_memory_usage(60.0, 1920, 1080, 30.0, 8, true);
        let no_audio = estimate_memory_usage(60.0, 1920, 1080, 30.0, 8, false);
        assert!(no_audio < with_audio);
    }

    // --- temp_dir helpers ---

    #[test]
    fn temp_dir_contains_sentinel() {
        let dir = temp_dir();
        assert!(dir.to_string_lossy().contains("sentinel"));
    }

    #[test]
    fn job_temp_dir_contains_job_id() {
        let id = uuid::Uuid::new_v4();
        let dir = job_temp_dir(&id);
        let name = dir.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("job-"));
    }

    #[test]
    fn ensure_temp_dir_ok() {
        let dir = ensure_temp_dir().unwrap();
        assert!(dir.exists());
    }

    #[test]
    fn temp_filename_format() {
        let path = temp_filename("frame", ".jpg").unwrap();
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("frame_"));
        assert!(name.ends_with(".jpg"));
    }

    // --- hash_video (requires a real file) ---

    #[test]
    fn hash_video_reads_file() {
        use std::io::Write;
        let dir = temp_dir();
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("hash_test.bin");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"test content for hashing").unwrap();
        drop(f);

        let h1 = hash_video(&path).unwrap();
        let h2 = hash_video(&path).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 hex is 64 chars

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn hash_video_missing_file() {
        let result = hash_video("/nonexistent/path/to/file.mp4");
        assert!(result.is_err());
    }
}

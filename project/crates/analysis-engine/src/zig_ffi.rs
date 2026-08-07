//! FFI bindings to the Zig audio fingerprinting library.
//!
//! Provides `extern "C"` declarations for Zig-compiled functions and
//! safe Rust wrappers that handle memory management and type conversions.
//!
//! # Safety
//! All Zig functions must be compiled into a shared/static library
//! and linked at build time.  The Zig allocator is assumed to use
//! `std.heap.c_allocator` so that Rust can free memory with `libc::free`.

use std::ffi::{c_char, c_double, c_float, c_int, c_void, CStr, CString};
use std::path::Path;
use std::slice;

// ===========================================================================
// Raw C types
// ===========================================================================

/// Handle to a Zig-managed spectrogram.
#[repr(C)]
pub struct ZigSpectrogram {
    _opaque: [u8; 0],
}

/// Handle to a Zig-managed audio fingerprint.
#[repr(C)]
pub struct ZigFingerprint {
    _opaque: [u8; 0],
}

/// Handle to a Zig-managed fingerprint database.
#[repr(C)]
pub struct ZigFingerprintDB {
    _opaque: [u8; 0],
}

/// Result code returned by Zig functions.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZigResultCode {
    Ok = 0,
    NullPointer = 1,
    OutOfMemory = 2,
    InvalidParameter = 3,
    DecodeError = 4,
    NotFound = 5,
    InternalError = 6,
}

/// A result struct carrying either a pointer or an error code.
#[repr(C)]
pub struct ZigResultPtr {
    pub ptr: *mut c_void,
    pub code: ZigResultCode,
}

/// Comparison result from Zig fingerprint matching.
#[repr(C)]
pub struct ZigCompareResult {
    pub similarity: c_double,
    pub offset_seconds: c_double,
    pub matched: c_int, // boolean: 1 = matched, 0 = no match
    pub code: ZigResultCode,
}

// ===========================================================================
// extern "C" declarations (Zig functions)
// ===========================================================================

extern "C" {
    /// Compute a mel-scaled spectrogram from raw audio samples.
    ///
    /// # Parameters
    /// * `samples` - Pointer to interleaved f32 sample data.
    /// * `sample_count` - Total number of samples (per channel * channels).
    /// * `sample_rate` - Sample rate in Hz (e.g. 44100).
    /// * `channels` - Number of audio channels.
    /// * `n_mels` - Number of mel frequency bins.
    /// * `n_fft` - FFT window size.
    /// * `hop_length` - Hop length between frames.
    ///
    /// # Returns
    /// A `ZigResultPtr` with a pointer to a `ZigSpectrogram` on success.
    pub fn zig_compute_spectrogram(
        samples: *const c_float,
        sample_count: c_int,
        sample_rate: c_int,
        channels: c_int,
        n_mels: c_int,
        n_fft: c_int,
        hop_length: c_int,
    ) -> ZigResultPtr;

    /// Compute an audio fingerprint from a spectrogram.
    ///
    /// # Parameters
    /// * `spec` - Pointer to a `ZigSpectrogram`.
    /// * `hash_size` - Size of the output hash (e.g. 256).
    ///
    /// # Returns
    /// A `ZigResultPtr` with a pointer to a `ZigFingerprint` on success.
    pub fn zig_compute_fingerprint(
        spec: *mut ZigSpectrogram,
        hash_size: c_int,
    ) -> ZigResultPtr;

    /// Compute a fingerprint directly from an audio file.
    ///
    /// # Parameters
    /// * `file_path` - Null-terminated file path string.
    /// * `hash_size` - Size of the output hash.
    ///
    /// # Returns
    /// A `ZigResultPtr` with a pointer to a `ZigFingerprint` on success.
    pub fn zig_fingerprint_from_file(
        file_path: *const c_char,
        hash_size: c_int,
    ) -> ZigResultPtr;

    /// Compare two fingerprints and return a similarity score.
    ///
    /// # Parameters
    /// * `fp_a` - Pointer to the first `ZigFingerprint`.
    /// * `fp_b` - Pointer to the second `ZigFingerprint`.
    ///
    /// # Returns
    /// A `ZigCompareResult` with similarity in [0.0, 1.0].
    pub fn zig_compare_fingerprints(
        fp_a: *mut ZigFingerprint,
        fp_b: *mut ZigFingerprint,
    ) -> ZigCompareResult;

    /// Compare a fingerprint against a database of known fingerprints.
    ///
    /// # Parameters
    /// * `query` - Pointer to the query `ZigFingerprint`.
    /// * `db` - Pointer to a `ZigFingerprintDB`.
    /// * `threshold` - Minimum similarity threshold (0.0..1.0).
    ///
    /// # Returns
    /// A `ZigCompareResult` with the best match found, if any.
    pub fn zig_compare_fingerprint_against_db(
        query: *mut ZigFingerprint,
        db: *mut ZigFingerprintDB,
        threshold: c_double,
    ) -> ZigCompareResult;

    /// Release memory owned by a Zig spectrogram.
    pub fn zig_free_spectrogram(spec: *mut ZigSpectrogram);

    /// Release memory owned by a Zig fingerprint.
    pub fn zig_free_fingerprint(fp: *mut ZigFingerprint);

    /// Release memory owned by a Zig fingerprint DB.
    pub fn zig_free_fingerprint_db(db: *mut ZigFingerprintDB);

    /// Get the raw hash bytes from a fingerprint.
    ///
    /// # Parameters
    /// * `fp` - Pointer to a `ZigFingerprint`.
    /// * `out_len` - Output: length of the hash buffer.
    ///
    /// # Returns
    /// Pointer to the hash byte array (owned by the fingerprint, do not free).
    pub fn zig_fingerprint_hash(
        fp: *mut ZigFingerprint,
        out_len: *mut c_int,
    ) -> *const c_float;

    /// Create a fingerprint database from a directory of reference audio files.
    pub fn zig_create_fingerprint_db(
        dir_path: *const c_char,
        hash_size: c_int,
    ) -> ZigResultPtr;

    /// Save a fingerprint database to disk.
    pub fn zig_save_fingerprint_db(
        db: *mut ZigFingerprintDB,
        file_path: *const c_char,
    ) -> ZigResultCode;

    /// Load a fingerprint database from disk.
    pub fn zig_load_fingerprint_db(file_path: *const c_char) -> ZigResultPtr;
}

// ===========================================================================
// Safe Rust wrappers
// ===========================================================================

/// Error type for Zig FFI operations.
#[derive(Debug, Clone, PartialEq)]
pub enum FingerprintError {
    NullPointer,
    OutOfMemory,
    InvalidParameter,
    DecodeError,
    NotFound,
    InternalError,
    InvalidUtf8,
    UnknownCode(i32),
}

impl std::fmt::Display for FingerprintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FingerprintError::NullPointer => write!(f, "null pointer passed to Zig"),
            FingerprintError::OutOfMemory => write!(f, "Zig allocator out of memory"),
            FingerprintError::InvalidParameter => write!(f, "invalid parameter passed to Zig"),
            FingerprintError::DecodeError => write!(f, "audio decode error in Zig"),
            FingerprintError::NotFound => write!(f, "no match found in Zig database"),
            FingerprintError::InternalError => write!(f, "internal Zig error"),
            FingerprintError::InvalidUtf8 => write!(f, "invalid UTF-8 in path string"),
            FingerprintError::UnknownCode(c) => write!(f, "unknown Zig result code: {}", c),
        }
    }
}

impl std::error::Error for FingerprintError {}

impl From<ZigResultCode> for Result<(), FingerprintError> {
    fn from(code: ZigResultCode) -> Self {
        match code {
            ZigResultCode::Ok => Ok(()),
            ZigResultCode::NullPointer => Err(FingerprintError::NullPointer),
            ZigResultCode::OutOfMemory => Err(FingerprintError::OutOfMemory),
            ZigResultCode::InvalidParameter => Err(FingerprintError::InvalidParameter),
            ZigResultCode::DecodeError => Err(FingerprintError::DecodeError),
            ZigResultCode::NotFound => Err(FingerprintError::NotFound),
            ZigResultCode::InternalError => Err(FingerprintError::InternalError),
        }
    }
}

/// Owned handle to a spectrogram computed by Zig.
pub struct SpectrogramHandle {
    ptr: *mut ZigSpectrogram,
}

// SAFETY: ZigSpectrogram is thread-safe (read-only after creation).
unsafe impl Send for SpectrogramHandle {}
unsafe impl Sync for SpectrogramHandle {}

impl SpectrogramHandle {
    /// Internal: take ownership of a raw Zig spectrogram pointer.
    fn from_raw(ptr: *mut ZigSpectrogram) -> Result<Self, FingerprintError> {
        if ptr.is_null() {
            return Err(FingerprintError::NullPointer);
        }
        Ok(Self { ptr })
    }

    /// Get the raw pointer (for passing to other Zig functions).
    pub fn as_ptr(&self) -> *mut ZigSpectrogram {
        self.ptr
    }
}

impl Drop for SpectrogramHandle {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { zig_free_spectrogram(self.ptr) };
        }
    }
}

/// Owned handle to an audio fingerprint computed by Zig.
pub struct FingerprintHandle {
    ptr: *mut ZigFingerprint,
}

unsafe impl Send for FingerprintHandle {}
unsafe impl Sync for FingerprintHandle {}

impl FingerprintHandle {
    fn from_raw(ptr: *mut ZigFingerprint) -> Result<Self, FingerprintError> {
        if ptr.is_null() {
            return Err(FingerprintError::NullPointer);
        }
        Ok(Self { ptr })
    }

    pub fn as_ptr(&self) -> *mut ZigFingerprint {
        self.ptr
    }

    /// Copy the hash bytes into a Rust `Vec<f32>`.
    pub fn hash_bytes(&self) -> Result<Vec<f32>, FingerprintError> {
        if self.ptr.is_null() {
            return Err(FingerprintError::NullPointer);
        }
        let mut len: c_int = 0;
        let raw_ptr = unsafe { zig_fingerprint_hash(self.ptr, &mut len) };
        if raw_ptr.is_null() || len <= 0 {
            return Ok(Vec::new());
        }
        let slice = unsafe { slice::from_raw_parts(raw_ptr, len as usize) };
        Ok(slice.to_vec())
    }
}

impl Drop for FingerprintHandle {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { zig_free_fingerprint(self.ptr) };
        }
    }
}

/// Owned handle to a fingerprint database.
pub struct FingerprintDbHandle {
    ptr: *mut ZigFingerprintDB,
}

unsafe impl Send for FingerprintDbHandle {}
unsafe impl Sync for FingerprintDbHandle {}

impl FingerprintDbHandle {
    fn from_raw(ptr: *mut ZigFingerprintDB) -> Result<Self, FingerprintError> {
        if ptr.is_null() {
            return Err(FingerprintError::NullPointer);
        }
        Ok(Self { ptr })
    }

    pub fn as_ptr(&self) -> *mut ZigFingerprintDB {
        self.ptr
    }
}

impl Drop for FingerprintDbHandle {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { zig_free_fingerprint_db(self.ptr) };
        }
    }
}

/// Result of comparing two fingerprints.
#[derive(Debug, Clone, PartialEq)]
pub struct FingerprintComparison {
    pub similarity: f64,
    pub offset_seconds: f64,
    pub matched: bool,
}

// ===========================================================================
// Safe wrapper functions
// ===========================================================================

/// Compute a mel-scaled spectrogram from raw audio samples.
///
/// # Arguments
/// * `samples` - Interleaved f32 audio samples.
/// * `sample_rate` - Sample rate in Hz.
/// * `channels` - Number of channels.
/// * `n_mels` - Number of mel bins.
/// * `n_fft` - FFT window size.
/// * `hop_length` - Hop length.
///
/// # Returns
/// An owned handle to the spectrogram.
pub fn compute_spectrogram(
    samples: &[f32],
    sample_rate: i32,
    channels: i32,
    n_mels: i32,
    n_fft: i32,
    hop_length: i32,
) -> Result<SpectrogramHandle, FingerprintError> {
    if samples.is_empty() {
        return Err(FingerprintError::InvalidParameter);
    }
    let result = unsafe {
        zig_compute_spectrogram(
            samples.as_ptr(),
            samples.len() as c_int,
            sample_rate as c_int,
            channels as c_int,
            n_mels as c_int,
            n_fft as c_int,
            hop_length as c_int,
        )
    };
    let code: Result<(), FingerprintError> = result.code.into();
    code?;
    SpectrogramHandle::from_raw(result.ptr as *mut ZigSpectrogram)
}

/// Compute an audio fingerprint from a spectrogram handle.
///
/// # Arguments
/// * `spectrogram` - Owned or borrowed spectrogram handle.
/// * `hash_size` - Desired hash length.
///
/// # Returns
/// An owned handle to the fingerprint.
pub fn compute_fingerprint(
    spectrogram: &SpectrogramHandle,
    hash_size: i32,
) -> Result<FingerprintHandle, FingerprintError> {
    let result = unsafe { zig_compute_fingerprint(spectrogram.as_ptr(), hash_size as c_int) };
    let code: Result<(), FingerprintError> = result.code.into();
    code?;
    FingerprintHandle::from_raw(result.ptr as *mut ZigFingerprint)
}

/// Compute a fingerprint directly from an audio file.
///
/// # Arguments
/// * `audio_path` - Path to the audio file.
/// * `hash_size` - Desired hash length.
///
/// # Returns
/// An owned handle to the fingerprint.
pub fn fingerprint_from_file<P: AsRef<Path>>(
    audio_path: P,
    hash_size: i32,
) -> Result<FingerprintHandle, FingerprintError> {
    let path_c = CString::new(
        audio_path
            .as_ref()
            .to_str()
            .ok_or(FingerprintError::InvalidUtf8)?,
    )
    .map_err(|_| FingerprintError::InvalidUtf8)?;
    let result = unsafe {
        zig_fingerprint_from_file(path_c.as_ptr(), hash_size as c_int)
    };
    let code: Result<(), FingerprintError> = result.code.into();
    code?;
    FingerprintHandle::from_raw(result.ptr as *mut ZigFingerprint)
}

/// Compare two fingerprints and return similarity.
///
/// # Arguments
/// * `fp_a` - First fingerprint.
/// * `fp_b` - Second fingerprint.
///
/// # Returns
/// A `FingerprintComparison` with similarity score in [0.0, 1.0].
pub fn compare_fingerprints(
    fp_a: &FingerprintHandle,
    fp_b: &FingerprintHandle,
) -> Result<FingerprintComparison, FingerprintError> {
    let result = unsafe { zig_compare_fingerprints(fp_a.as_ptr(), fp_b.as_ptr()) };
    if result.code != ZigResultCode::Ok {
        let code: Result<(), FingerprintError> = result.code.into();
        code?;
    }
    Ok(FingerprintComparison {
        similarity: result.similarity,
        offset_seconds: result.offset_seconds,
        matched: result.matched != 0,
    })
}

/// Compare a fingerprint against a database.
///
/// # Arguments
/// * `query` - Query fingerprint.
/// * `db` - Fingerprint database handle.
/// * `threshold` - Minimum similarity threshold (0.0..1.0).
///
/// # Returns
/// A `FingerprintComparison` for the best match, or `NotFound` if none.
pub fn compare_fingerprint_against_db(
    query: &FingerprintHandle,
    db: &FingerprintDbHandle,
    threshold: f64,
) -> Result<FingerprintComparison, FingerprintError> {
    let result = unsafe {
        zig_compare_fingerprint_against_db(
            query.as_ptr(),
            db.as_ptr(),
            threshold.clamp(0.0, 1.0),
        )
    };
    if result.code == ZigResultCode::NotFound {
        return Err(FingerprintError::NotFound);
    }
    if result.code != ZigResultCode::Ok {
        let code: Result<(), FingerprintError> = result.code.into();
        code?;
    }
    Ok(FingerprintComparison {
        similarity: result.similarity,
        offset_seconds: result.offset_seconds,
        matched: result.matched != 0,
    })
}

/// Create a fingerprint database from a directory of reference audio files.
pub fn create_fingerprint_db<P: AsRef<Path>>(
    dir_path: P,
    hash_size: i32,
) -> Result<FingerprintDbHandle, FingerprintError> {
    let path_c = CString::new(
        dir_path
            .as_ref()
            .to_str()
            .ok_or(FingerprintError::InvalidUtf8)?,
    )
    .map_err(|_| FingerprintError::InvalidUtf8)?;
    let result = unsafe { zig_create_fingerprint_db(path_c.as_ptr(), hash_size as c_int) };
    let code: Result<(), FingerprintError> = result.code.into();
    code?;
    FingerprintDbHandle::from_raw(result.ptr as *mut ZigFingerprintDB)
}

/// Save a fingerprint database to disk.
pub fn save_fingerprint_db<P: AsRef<Path>>(
    db: &FingerprintDbHandle,
    file_path: P,
) -> Result<(), FingerprintError> {
    let path_c = CString::new(
        file_path
            .as_ref()
            .to_str()
            .ok_or(FingerprintError::InvalidUtf8)?,
    )
    .map_err(|_| FingerprintError::InvalidUtf8)?;
    let code = unsafe { zig_save_fingerprint_db(db.as_ptr(), path_c.as_ptr()) };
    let res: Result<(), FingerprintError> = code.into();
    res
}

/// Load a fingerprint database from disk.
pub fn load_fingerprint_db<P: AsRef<Path>>(
    file_path: P,
) -> Result<FingerprintDbHandle, FingerprintError> {
    let path_c = CString::new(
        file_path
            .as_ref()
            .to_str()
            .ok_or(FingerprintError::InvalidUtf8)?,
    )
    .map_err(|_| FingerprintError::InvalidUtf8)?;
    let result = unsafe { zig_load_fingerprint_db(path_c.as_ptr()) };
    let code: Result<(), FingerprintError> = result.code.into();
    code?;
    FingerprintDbHandle::from_raw(result.ptr as *mut ZigFingerprintDB)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests verify the Rust-side logic and error handling.
    // Actual Zig integration tests require the Zig shared library to be linked.

    #[test]
    fn test_result_code_conversions() {
        let ok: Result<(), FingerprintError> = ZigResultCode::Ok.into();
        assert!(ok.is_ok());

        let err: Result<(), FingerprintError> = ZigResultCode::OutOfMemory.into();
        assert!(matches!(err, Err(FingerprintError::OutOfMemory)));

        let err: Result<(), FingerprintError> = ZigResultCode::NotFound.into();
        assert!(matches!(err, Err(FingerprintError::NotFound)));
    }

    #[test]
    fn test_fingerprint_error_display() {
        let e = FingerprintError::DecodeError;
        assert_eq!(format!("{}", e), "audio decode error in Zig");

        let e = FingerprintError::UnknownCode(42);
        assert!(format!("{}", e).contains("42"));
    }

    #[test]
    fn test_fingerprint_comparison() {
        let cmp = FingerprintComparison {
            similarity: 0.85,
            offset_seconds: 2.5,
            matched: true,
        };
        assert!(cmp.matched);
        assert!((cmp.similarity - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn test_empty_samples_error() {
        let result = compute_spectrogram(&[], 44100, 2, 128, 2048, 512);
        assert!(matches!(result, Err(FingerprintError::InvalidParameter)));
    }

    #[test]
    fn test_detection_candidate_iou_sanity() {
        // This module doesn't have DetectionCandidate, but we can verify
        // the FingerprintComparison struct works correctly.
        let cmp = FingerprintComparison {
            similarity: 0.0,
            offset_seconds: 0.0,
            matched: false,
        };
        assert!(!cmp.matched);
    }

    #[test]
    fn test_zig_result_code_equality() {
        assert_eq!(ZigResultCode::Ok, ZigResultCode::Ok);
        assert_ne!(ZigResultCode::Ok, ZigResultCode::InternalError);
    }

    #[test]
    fn test_fingerprint_error_clone() {
        let e = FingerprintError::OutOfMemory;
        let cloned = e.clone();
        assert_eq!(e, cloned);
    }

    #[test]
    fn test_fingerprint_comparison_clone() {
        let cmp = FingerprintComparison {
            similarity: 0.5,
            offset_seconds: 1.0,
            matched: true,
        };
        let cloned = cmp.clone();
        assert_eq!(cmp, cloned);
    }
}

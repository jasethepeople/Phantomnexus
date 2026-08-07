const std = @import("std");
const spectrogram = @import("spectrogram.zig");
const fingerprint = @import("fingerprint.zig");
const match = @import("match.zig");

// ─── Re-exports for C ABI ──────────────────────────────────────────────────

/// Volume spike detection result — C-compatible.
/// Represents a detected loudness spike in the audio.
/// Time values are in seconds.
pub const VolumeSpike = extern struct {
    /// Start time of the spike (seconds).
    start_time: f32,
    /// End time of the spike (seconds).
    end_time: f32,
    /// Peak amplitude (linear, 0..1).
    peak_amplitude: f32,
    /// Peak level in dB.
    peak_db: f32,
};

// ─── Error codes ───────────────────────────────────────────────────────────

const ERR_OK: c_int = 0;
const ERR_INVALID_PARAM: c_int = -1;
const ERR_NOT_ENOUGH_DATA: c_int = -2;
const ERR_INTERNAL: c_int = -3;

// ─── C ABI exports ─────────────────────────────────────────────────────────

/// Compute STFT spectrogram of audio samples.
///
/// Parameters:
///   `audio_samples` — Input PCM samples (mono, f32).
///   `sample_count`  — Number of samples.
///   `sample_rate`   — Sample rate in Hz.
///   `window_size`   — FFT window size (must be power of 2).
///   `hop_size`      — Hop between successive frames.
///   `out_spectrogram` — Pre-allocated output buffer for spectrogram data.
///   `out_freq_bins`   — Written with number of frequency bins (window_size/2+1).
///   `out_time_frames` — Written with number of time frames.
///
/// Returns 0 on success, negative on error.
/// Thread-safe, reentrant.
export fn compute_spectrogram(
    audio_samples: [*]const f32,
    sample_count: u32,
    sample_rate: u32,
    window_size: u32,
    hop_size: u32,
    out_spectrogram: [*]f32,
    out_freq_bins: *u32,
    out_time_frames: *u32,
) callconv(.C) c_int {
    if (sample_count == 0 or window_size == 0 or hop_size == 0) return ERR_INVALID_PARAM;
    if (!std.math.isPowerOfTwo(window_size)) return ERR_INVALID_PARAM;

    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const samples = audio_samples[0..sample_count];
    const spec = spectrogram.stft(samples, window_size, hop_size, sample_rate, allocator) catch {
        return ERR_INTERNAL;
    };
    defer spec.deinit();

    if (spec.data.len > 0) {
        @memcpy(out_spectrogram[0..spec.data.len], spec.data);
    }
    out_freq_bins.* = spec.freq_bins;
    out_time_frames.* = spec.time_frames;
    return ERR_OK;
}

/// Compute a Chromaprint-compatible fingerprint from a spectrogram.
///
/// Parameters:
///   `spectrogram`    — [freq_bins * time_frames] dB values.
///   `freq_bins`      — Number of frequency bins.
///   `time_frames`    — Number of time frames.
///   `out_fingerprint`  — Pre-allocated u8 buffer for fingerprint bytes.
///   `out_fingerprint_len` — Written with actual fingerprint length.
///
/// Returns 0 on success, negative on error.
export fn compute_fingerprint(
    spec_data: [*]const f32,
    freq_bins: u32,
    time_frames: u32,
    out_fingerprint: [*]u8,
    out_fingerprint_len: *u32,
) callconv(.C) c_int {
    if (freq_bins == 0 or time_frames < 2) return ERR_INVALID_PARAM;

    // Compute fingerprint using default sample_rate and hop_size
    // The fingerprint module uses band energies which are independent of
    // absolute frequency scaling; we use 44100/512 as defaults.
    const sample_rate: u32 = 44100;
    const hop_size: u32 = 512;

    // Call the computeFingerprint wrapper
    const max_len: u32 = 65536; // caller must provide at least this
    return fingerprint.computeFingerprint(
        spec_data,
        freq_bins,
        time_frames,
        sample_rate,
        hop_size,
        out_fingerprint,
        out_fingerprint_len,
        max_len,
    );
}

/// Compare two fingerprints and compute similarity.
///
/// Parameters:
///   `fp_a` — First fingerprint bytes.
///   `len_a` — Length of first fingerprint.
///   `fp_b` — Second fingerprint bytes.
///   `len_b` — Length of second fingerprint.
///   `out_similarity` — Written with similarity score [0.0, 1.0].
///
/// Returns 0 on success, negative on error.
export fn compare_fingerprints(
    fp_a: [*]const u8,
    len_a: u32,
    fp_b: [*]const u8,
    len_b: u32,
    out_similarity: *f32,
) callconv(.C) c_int {
    if (len_a == 0 or len_b == 0) {
        out_similarity.* = 0.0;
        return ERR_INVALID_PARAM;
    }

    out_similarity.* = match.fingerprint_similarity(fp_a, len_a, fp_b, len_b);
    return ERR_OK;
}

/// Detect volume spikes (loudness events) in audio.
///
/// Parameters:
///   `audio_samples` — Mono f32 samples.
///   `sample_count`  — Number of samples.
///   `sample_rate`   — Sample rate in Hz.
///   `window_ms`     — Analysis window in milliseconds.
///   `threshold_db`  — Detection threshold in dB (e.g., -6.0).
///   `out_spikes`    — Pre-allocated output array of VolumeSpike.
///   `max_spikes`    — Capacity of out_spikes array.
///   `out_count`     — Written with actual spike count.
///
/// Returns 0 on success, negative on error.
export fn detect_volume_spikes(
    audio_samples: [*]const f32,
    sample_count: u32,
    sample_rate: u32,
    window_ms: u32,
    threshold_db: f32,
    out_spikes: [*]VolumeSpike,
    max_spikes: u32,
    out_count: *u32,
) callconv(.C) c_int {
    if (sample_count == 0 or sample_rate == 0 or window_ms == 0 or max_spikes == 0) return ERR_INVALID_PARAM;

    const samples = audio_samples[0..sample_count];
    const window_samples = @as(u32, @intFromFloat(
        @round(@as(f32, @floatFromInt(window_ms)) / 1000.0 * @as(f32, @floatFromInt(sample_rate)),
    )));
    if (window_samples == 0) return ERR_INVALID_PARAM;

    const num_windows = sample_count / window_samples;
    if (num_windows == 0) return ERR_NOT_ENOUGH_DATA;

    var spike_count: u32 = 0;
    var in_spike = false;
    var spike_start_window: u32 = 0;
    var spike_peak_amp: f32 = 0.0;
    var spike_peak_db: f32 = -std.math.inf(f32);

    const eps: f32 = 1e-12;

    var w: u32 = 0;
    while (w < num_windows) : (w += 1) {
        const offset = w * window_samples;

        // Compute RMS of this window
        var sum_sq: f32 = 0.0;
        for (0..window_samples) |s| {
            const v = samples[offset + s];
            sum_sq += v * v;
        }
        const rms = @sqrt(sum_sq / @as(f32, @floatFromInt(window_samples)));
        const rms_db = 20.0 * std.math.log10(rms + eps);

        if (rms_db > threshold_db) {
            if (!in_spike) {
                in_spike = true;
                spike_start_window = w;
                spike_peak_amp = rms;
                spike_peak_db = rms_db;
            } else {
                if (rms > spike_peak_amp) {
                    spike_peak_amp = rms;
                    spike_peak_db = rms_db;
                }
            }
        } else {
            if (in_spike) {
                // End of spike
                if (spike_count < max_spikes) {
                    const start_time = @as(f32, @floatFromInt(spike_start_window * window_samples)) /
                        @as(f32, @floatFromInt(sample_rate));
                    const end_time = @as(f32, @floatFromInt(w * window_samples)) /
                        @as(f32, @floatFromInt(sample_rate));
                    out_spikes[spike_count] = VolumeSpike{
                        .start_time = start_time,
                        .end_time = end_time,
                        .peak_amplitude = spike_peak_amp,
                        .peak_db = spike_peak_db,
                    };
                    spike_count += 1;
                }
                in_spike = false;
                spike_peak_amp = 0.0;
                spike_peak_db = -std.math.inf(f32);
            }
        }
    }

    // Close any open spike at the end
    if (in_spike and spike_count < max_spikes) {
        const start_time = @as(f32, @floatFromInt(spike_start_window * window_samples)) /
            @as(f32, @floatFromInt(sample_rate));
        const end_time = @as(f32, @floatFromInt(num_windows * window_samples)) /
            @as(f32, @floatFromInt(sample_rate));
        out_spikes[spike_count] = VolumeSpike{
            .start_time = start_time,
            .end_time = end_time,
            .peak_amplitude = spike_peak_amp,
            .peak_db = spike_peak_db,
        };
        spike_count += 1;
    }

    out_count.* = spike_count;
    return ERR_OK;
}

/// Analyze frequency content by octave-like energy bands.
///
/// Parameters:
///   `audio_samples` — Mono f32 samples.
///   `sample_count`  — Number of samples.
///   `sample_rate`   — Sample rate in Hz.
///   `out_energy_bands` — Pre-allocated output for band energies (dB).
///   `band_count`    — Number of bands (must match fixed band count).
///
/// Returns 0 on success, negative on error.
export fn analyze_frequency_content(
    audio_samples: [*]const f32,
    sample_count: u32,
    sample_rate: u32,
    out_energy_bands: [*]f32,
    band_count: u32,
) callconv(.C) c_int {
    if (sample_count == 0 or sample_rate == 0 or band_count == 0) return ERR_INVALID_PARAM;

    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    // Use STFT to get spectral data
    const window_size: u32 = 2048;
    const hop_size: u32 = 1024;

    const samples = audio_samples[0..sample_count];
    const spec = spectrogram.stft(samples, window_size, hop_size, sample_rate, allocator) catch {
        return ERR_INTERNAL;
    };
    defer spec.deinit();

    if (spec.time_frames == 0) return ERR_NOT_ENOUGH_DATA;

    // Average spectrum across all time frames
    const freq_bins = spec.freq_bins;
    var avg_spectrum = allocator.alloc(f32, freq_bins) catch {
        return ERR_INTERNAL;
    };
    defer allocator.free(avg_spectrum);
    @memset(avg_spectrum, 0.0);

    for (0..spec.time_frames) |t| {
        const offset = t * freq_bins;
        for (0..freq_bins) |f| {
            // Convert dB to linear power, accumulate, then convert back
            const db = spec.data[offset + f];
            avg_spectrum[f] += std.math.pow(f32, 10.0, db / 10.0);
        }
    }
    const tf: f32 = @floatFromInt(spec.time_frames);
    for (avg_spectrum) |*v| {
        v.* = 10.0 * std.math.log10(v.* / tf + 1e-12);
    }

    // Divide spectrum into `band_count` equal-width frequency bands
    const nyquist: f32 = @as(f32, @floatFromInt(sample_rate)) / 2.0;
    const bin_hz = nyquist / @as(f32, @floatFromInt(freq_bins - 1));
    const band_width_hz = nyquist / @as(f32, @floatFromInt(band_count));

    for (0..band_count) |b| {
        const low_hz = @as(f32, @floatFromInt(b)) * band_width_hz;
        const high_hz = @as(f32, @floatFromInt(b + 1)) * band_width_hz;
        const low_bin: u32 = @intFromFloat(@floor(low_hz / bin_hz));
        const high_bin: u32 = @min(@intFromFloat(@ceil(high_hz / bin_hz)), freq_bins);

        var energy: f32 = 0.0;
        var f: u32 = low_bin;
        while (f < high_bin) : (f += 1) {
            energy += std.math.pow(f32, 10.0, avg_spectrum[f] / 10.0);
        }
        const band_db = 10.0 * std.math.log10(energy + 1e-12);
        out_energy_bands[b] = band_db;
    }

    return ERR_OK;
}

// ─── Module tests ──────────────────────────────────────────────────────────

test "compute_spectrogram basic" {
    const allocator = std.testing.allocator;
    const sr: u32 = 44100;
    const ws: u32 = 1024;
    const hop: u32 = 512;
    const num_samples = sr; // 1 second

    const samples = try allocator.alloc(f32, num_samples);
    defer allocator.free(samples);
    for (0..num_samples) |i| {
        const t = @as(f32, @floatFromInt(i)) / @as(f32, @floatFromInt(sr));
        samples[i] = @sin(2.0 * std.math.pi * 1000.0 * t);
    }

    const max_frames = (num_samples / hop) + 1;
    const freq_bins = ws / 2 + 1;
    const out_buf = try allocator.alloc(f32, max_frames * freq_bins);
    defer allocator.free(out_buf);

    var out_fb: u32 = 0;
    var out_tf: u32 = 0;

    const rc = compute_spectrogram(
        samples.ptr,
        @intCast(num_samples),
        sr,
        ws,
        hop,
        out_buf.ptr,
        &out_fb,
        &out_tf,
    );
    try std.testing.expect(rc == 0);
    try std.testing.expect(out_fb == freq_bins);
    try std.testing.expect(out_tf > 0);
}

test "detect_volume_spikes finds synthetic spike" {
    const allocator = std.testing.allocator;
    const sr: u32 = 44100;
    const num_samples = sr; // 1 second

    const samples = try allocator.alloc(f32, num_samples);
    defer allocator.free(samples);
    @memset(samples, 0.0);

    // Inject a loud spike at 0.3s - 0.5s (above -6 dB = amplitude > 0.5)
    const start_s: u32 = @intFromFloat(0.3 * @as(f32, @floatFromInt(sr)));
    const end_s: u32 = @intFromFloat(0.5 * @as(f32, @floatFromInt(sr)));
    for (start_s..end_s) |i| {
        samples[i] = 0.8; // well above 0.5 threshold
    }

    var spikes: [10]VolumeSpike = undefined;
    var count: u32 = 0;

    const rc = detect_volume_spikes(
        samples.ptr,
        @intCast(num_samples),
        sr,
        50, // 50ms windows
        -6.0,
        &spikes,
        10,
        &count,
    );
    try std.testing.expect(rc == 0);
    try std.testing.expect(count >= 1);

    // Check that the spike encompasses our injected region
    const spike = spikes[0];
    try std.testing.expect(spike.start_time <= 0.35);
    try std.testing.expect(spike.end_time >= 0.45);
    try std.testing.expect(spike.peak_amplitude >= 0.5);
}

test "analyze_frequency_content produces bands" {
    const allocator = std.testing.allocator;
    const sr: u32 = 44100;
    const num_samples = sr;
    const band_count: u32 = 6;

    const samples = try allocator.alloc(f32, num_samples);
    defer allocator.free(samples);

    // 1 kHz sine
    for (0..num_samples) |i| {
        const t = @as(f32, @floatFromInt(i)) / @as(f32, @floatFromInt(sr));
        samples[i] = @sin(2.0 * std.math.pi * 1000.0 * t);
    }

    var bands: [6]f32 = undefined;

    const rc = analyze_frequency_content(
        samples.ptr,
        @intCast(num_samples),
        sr,
        &bands,
        band_count,
    );
    try std.testing.expect(rc == 0);

    // The 1 kHz energy should be in one of the middle bands
    var max_band: usize = 0;
    var max_val: f32 = -std.math.inf(f32);
    for (bands, 0..) |v, i| {
        if (v > max_val) {
            max_val = v;
            max_band = i;
        }
    }
    try std.testing.expect(max_band >= 1 and max_band <= 3);
}

test "compute_fingerprint roundtrip" {
    const allocator = std.testing.allocator;
    const freq_bins: u32 = 513;
    const time_frames: u32 = 200;

    const spec = try allocator.alloc(f32, freq_bins * time_frames);
    defer allocator.free(spec);
    @memset(spec, -120.0);

    // Add tonal energy
    for (0..time_frames) |t| {
        spec[t * freq_bins + 23] = 0.0;   // ~1 kHz
        spec[t * freq_bins + 47] = -3.0;  // ~2 kHz
        spec[t * freq_bins + 93] = -6.0;  // ~4 kHz
    }

    var fp_buf: [4096]u8 = undefined;
    var fp_len: u32 = 0;

    const rc = compute_fingerprint(spec.ptr, freq_bins, time_frames, &fp_buf, &fp_len);
    try std.testing.expect(rc == 0);
    try std.testing.expect(fp_len > 0);
    try std.testing.expect(fp_len < 4096);
}

test "compare_fingerprints identical" {
    const a = [_]u8{ 0x1A, 0x2B, 0x3C, 0x4D, 0x5E };
    const b = [_]u8{ 0x1A, 0x2B, 0x3C, 0x4D, 0x5E };
    var sim: f32 = 0;

    const rc = compare_fingerprints(&a, a.len, &b, b.len, &sim);
    try std.testing.expect(rc == 0);
    try std.testing.expectApproxEqAbs(sim, 1.0, 0.001);
}

test "compare_fingerprints different" {
    const a = [_]u8{ 0x00, 0x00, 0x00, 0x00 };
    const b = [_]u8{ 0x3F, 0x3F, 0x3F, 0x3F };
    var sim: f32 = 0;

    const rc = compare_fingerprints(&a, a.len, &b, b.len, &sim);
    try std.testing.expect(rc == 0);
    try std.testing.expectApproxEqAbs(sim, 0.0, 0.01);
}

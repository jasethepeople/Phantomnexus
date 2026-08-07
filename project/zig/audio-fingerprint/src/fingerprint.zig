const std = @import("std");

/// Sub-fingerprint interval in time frames (256ms at 44100 Hz with 11025-sample hop).
/// We keep this configurable; the default is 8 frames per sub-fingerprint.
/// At typical settings (window=2048, hop=1024 @ 44100), that's ~186ms per frame,
/// so 8 frames ≈ 1.5s.  Using 2 frames gives ~372ms which is closer to 256ms.
/// We dynamically compute based on actual frame duration.
pub const SUBFINGERPRINT_INTERVAL_MS: f32 = 256.0;

/// 6 frequency bands (Hz limits): 250, 500, 1000, 2000, 4000, 8000
pub const NUM_BANDS: u32 = 6;
pub const BAND_EDGES_HZ: [NUM_BANDS + 1]f32 = .{ 0, 250, 500, 1000, 2000, 4000, 8000 };

/// A fingerprint computed from a spectrogram.
/// Layout: each sub-fingerprint is packed into ceil(NUM_BANDS/8)=1 byte.
/// For 6 bands we use the lower 6 bits of each byte.
pub const Fingerprint = struct {
    data: []u8,
    num_subfingerprints: u32,
    num_bands: u32,
    allocator: std.mem.Allocator,

    pub fn deinit(self: Fingerprint) void {
        self.allocator.free(self.data);
    }

    /// Compute per-band energy for each time frame.
    /// Returns a flat [time_frames * NUM_BANDS] array.
    fn computeBandEnergies(
        spectrogram: []const f32,
        freq_bins: u32,
        time_frames: u32,
        sample_rate: u32,
        allocator: std.mem.Allocator,
    ) ![]f32 {
        const out = try allocator.alloc(f32, time_frames * NUM_BANDS);
        @memset(out, 0);

        const nyquist: f32 = @as(f32, @floatFromInt(sample_rate)) / 2.0;
        const bin_hz = nyquist / @as(f32, @floatFromInt(freq_bins - 1));

        for (0..time_frames) |t| {
            const spec_offset = t * freq_bins;
            const out_offset = t * NUM_BANDS;

            var bin: u32 = 0;
            var band: u32 = 0;
            while (band < NUM_BANDS) : (band += 1) {
                const low_hz = BAND_EDGES_HZ[band];
                const high_hz = BAND_EDGES_HZ[band + 1];
                const low_bin: u32 = @intFromFloat(@floor(low_hz / bin_hz));
                const high_bin: u32 = @min(@intFromFloat(@ceil(high_hz / bin_hz)), freq_bins);

                var energy: f32 = 0;
                bin = low_bin;
                while (bin < high_bin) : (bin += 1) {
                    const db_val = spectrogram[spec_offset + bin];
                    // Convert dB back to linear power and accumulate
                    energy += std.math.pow(f32, 10.0, db_val / 10.0);
                }
                out[out_offset + band] = energy;
            }
        }

        return out;
    }

    /// Quantize energy differences to bits.
    /// For each band, output 1 if energy increased from previous frame, else 0.
    fn quantizeDifferences(energies: []const f32, time_frames: u32, out_bits: []u8) void {
        const num_bands = NUM_BANDS;
        std.debug.assert(out_bits.len == time_frames);
        @memset(out_bits, 0);

        for (1..time_frames) |t| {
            var byte: u8 = 0;
            for (0..num_bands) |b| {
                const prev = energies[(t - 1) * num_bands + b];
                const curr = energies[t * num_bands + b];
                if (curr > prev) {
                    byte |= @as(u8, 1) << @as(u3, @intCast(b));
                }
            }
            out_bits[t] = byte;
        }
        // Frame 0 has no previous frame, keep as 0
    }

    /// Pack sub-fingerprints. Each byte is one quantized frame.
    /// Optionally downsample to one sub-fingerprint per SUBFINGERPRINT_INTERVAL_MS.
    fn packSubfingerprints(
        bits: []const u8,
        time_frames: u32,
        sample_rate: u32,
        hop_size: u32,
        allocator: std.mem.Allocator,
    ) ![]u8 {
        const frame_duration_ms = (@as(f32, @floatFromInt(hop_size)) / @as(f32, @floatFromInt(sample_rate))) * 1000.0;
        const frames_per_sub: u32 = @max(1, @as(u32, @intFromFloat(@round(SUBFINGERPRINT_INTERVAL_MS / frame_duration_ms))));

        if (frames_per_sub <= 1) {
            // No downsampling needed; skip frame 0 (no differential data)
            const result = try allocator.alloc(u8, time_frames -| 1);
            @memcpy(result, bits[1..time_frames]);
            return result;
        }

        const num_sub = (time_frames - 1 + frames_per_sub - 1) / frames_per_sub;
        const result = try allocator.alloc(u8, num_sub);

        var i: usize = 0;
        var t: usize = 1;
        while (i < num_sub) : (i += 1) {
            // XOR-accumulate frames in this sub-fingerprint window
            var acc: u8 = 0;
            var f: usize = 0;
            while (f < frames_per_sub and t < time_frames) : (f += 1) {
                acc ^= bits[t];
                t += 1;
            }
            result[i] = acc;
        }

        return result;
    }

    /// Compute fingerprint from a spectrogram.
    /// `spectrogram` is a flat [time_frames * freq_bins] array in dB scale.
    pub fn compute(
        spectrogram: []const f32,
        freq_bins: u32,
        time_frames: u32,
        sample_rate: u32,
        hop_size: u32,
        allocator: std.mem.Allocator,
    ) !Fingerprint {
        if (time_frames < 2) return error.NotEnoughFrames;

        // Step 1: Per-band energy per frame
        const energies = try computeBandEnergies(spectrogram, freq_bins, time_frames, sample_rate, allocator);
        defer allocator.free(energies);

        // Step 2: Quantize differences
        const bits = try allocator.alloc(u8, time_frames);
        defer allocator.free(bits);
        quantizeDifferences(energies, time_frames, bits);

        // Step 3: Pack sub-fingerprints
        const packed = try packSubfingerprints(bits, time_frames, sample_rate, hop_size, allocator);

        return Fingerprint{
            .data = packed,
            .num_subfingerprints = @intCast(packed.len),
            .num_bands = NUM_BANDS,
            .allocator = allocator,
        };
    }
};

/// C-compatible wrapper: compute fingerprint from flat spectrogram buffer.
/// `spectrogram`: [freq_bins * time_frames] f32 in dB
/// `out_fingerprint`: caller-provided u8 buffer
/// `out_fingerprint_len`: written with actual length on success
pub fn computeFingerprint(
    spectrogram: [*]const f32,
    freq_bins: u32,
    time_frames: u32,
    sample_rate: u32,
    hop_size: u32,
    out_fingerprint: [*]u8,
    out_fingerprint_len: *u32,
    max_len: u32,
) c_int {
    if (time_frames < 2 or freq_bins == 0) return -1;

    const spec_slice = spectrogram[0 .. freq_bins * time_frames];

    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const fp = Fingerprint.compute(spec_slice, freq_bins, time_frames, sample_rate, hop_size, allocator) catch {
        return -1;
    };
    defer fp.deinit();

    if (fp.data.len > max_len) return -2; // buffer too small

    @memcpy(out_fingerprint[0..fp.data.len], fp.data);
    out_fingerprint_len.* = @intCast(fp.data.len);
    return 0;
}

test "Fingerprint.compute produces valid fingerprint" {
    const allocator = std.testing.allocator;
    const sample_rate: u32 = 44100;
    const freq_bins: u32 = 513;
    const time_frames: u32 = 100;

    // Synthetic spectrogram: 1 kHz tone
    const spec = try allocator.alloc(f32, freq_bins * time_frames);
    defer allocator.free(spec);
    @memset(spec, -100.0);

    // Put energy around bin 23 (1000 Hz at 44100 Hz with 1024 FFT)
    for (0..time_frames) |t| {
        spec[t * freq_bins + 23] = 0.0;
        spec[t * freq_bins + 24] = -5.0;
    }

    const fp = try Fingerprint.compute(spec, freq_bins, time_frames, sample_rate, 512, allocator);
    defer fp.deinit();

    try std.testing.expect(fp.num_subfingerprints > 0);
    try std.testing.expect(fp.num_bands == NUM_BANDS);
}

test "computeFingerprint C wrapper" {
    const allocator = std.testing.allocator;
    const freq_bins: u32 = 513;
    const time_frames: u32 = 100;
    const sr: u32 = 44100;

    const spec = try allocator.alloc(f32, freq_bins * time_frames);
    defer allocator.free(spec);
    @memset(spec, -100.0);
    for (0..time_frames) |t| {
        spec[t * freq_bins + 23] = 0.0;
    }

    const out_buf = try allocator.alloc(u8, 1024);
    defer allocator.free(out_buf);
    var out_len: u32 = 0;

    const rc = computeFingerprint(spec.ptr, freq_bins, time_frames, sr, 512, out_buf.ptr, &out_len, 1024);
    try std.testing.expect(rc == 0);
    try std.testing.expect(out_len > 0);
}

test "band edges cover expected ranges" {
    try std.testing.expect(BAND_EDGES_HZ[0] == 0);
    try std.testing.expect(BAND_EDGES_HZ[1] == 250);
    try std.testing.expect(BAND_EDGES_HZ[2] == 500);
    try std.testing.expect(BAND_EDGES_HZ[3] == 1000);
    try std.testing.expect(BAND_EDGES_HZ[4] == 2000);
    try std.testing.expect(BAND_EDGES_HZ[5] == 4000);
    try std.testing.expect(BAND_EDGES_HZ[6] == 8000);
}

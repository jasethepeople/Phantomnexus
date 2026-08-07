const std = @import("std");

/// Spectrogram result containing power spectrum in dB.
/// Layout: row-major [time_frames][freq_bins]
pub const Spectrogram = struct {
    data: []f32,
    freq_bins: u32,
    time_frames: u32,
    sample_rate: u32,
    window_size: u32,
    hop_size: u32,
    allocator: std.mem.Allocator,

    pub fn deinit(self: Spectrogram) void {
        self.allocator.free(self.data);
    }

    /// Get a pointer to the start of a given time frame's frequency data.
    pub fn frame(self: Spectrogram, t: u32) []f32 {
        const start = t * self.freq_bins;
        return self.data[start .. start + self.freq_bins];
    }
};

/// Compute a Hanning window of the given size.
pub fn hanning_window(size: u32, allocator: std.mem.Allocator) ![]f32 {
    const win = try allocator.alloc(f32, size);
    const N: f32 = @floatFromInt(size);
    for (win, 0..) |*w, i| {
        const n: f32 = @floatFromInt(i);
        w.* = 0.5 - 0.5 * @cos((2.0 * std.math.pi * n) / (N - 1.0));
    }
    return win;
}

/// In-place iterative Cooley-Tukey FFT (radix-2, DIF variant).
/// `inout_real` and `inout_imag` must have the same length which must be a power of 2.
pub fn fft(inout_real: []f32, inout_imag: []f32) void {
    const n = inout_real.len;
    std.debug.assert(n == inout_imag.len);
    std.debug.assert(std.math.isPowerOfTwo(n));
    if (n <= 1) return;

    // Bit-reversal permutation
    var i: usize = 0;
    var j: usize = 0;
    while (i < n) : (i += 1) {
        if (i < j) {
            std.mem.swap(f32, &inout_real[i], &inout_real[j]);
            std.mem.swap(f32, &inout_imag[i], &inout_imag[j]);
        }
        var k: usize = n >> 1;
        while (k > 0 and j & k != 0) : (k >>= 1) {
            j ^= k;
        }
        j |= k;
    }

    // Iterative FFT
    var stage: usize = 1;
    while (stage < n) : (stage <<= 1) {
        const step = stage << 1;
        var group: usize = 0;
        while (group < stage) : (group += 1) {
            const angle = -std.math.pi * @as(f32, @floatFromInt(group)) / @as(f32, @floatFromInt(stage));
            const wr = @cos(angle);
            const wi = @sin(angle);
            var a: usize = group;
            while (a < n) : (a += step) {
                const b = a + stage;
                const tr = inout_real[b] * wr - inout_imag[b] * wi;
                const ti = inout_real[b] * wi + inout_imag[b] * wr;
                inout_real[b] = inout_real[a] - tr;
                inout_imag[b] = inout_imag[a] - ti;
                inout_real[a] = inout_real[a] + tr;
                inout_imag[a] = inout_imag[a] + ti;
            }
        }
    }
}

/// Compute magnitude spectrum (power in dB) from complex FFT output.
/// Fills `out` with `out.len = n/2 + 1` bins containing 10*log10(|X|^2).
pub fn magnitude_spectrum_db(real: []const f32, imag: []const f32, out: []f32) void {
    const n = real.len;
    const out_len = n / 2 + 1;
    std.debug.assert(out.len >= out_len);
    const eps: f32 = 1e-12;

    for (0..out_len) |k| {
        const r = real[k];
        const i = imag[k];
        const power = r * r + i * i;
        out[k] = 10.0 * std.math.log10(power + eps);
    }
}

/// Short-Time Fourier Transform.
/// Returns a spectrogram with shape [time_frames][freq_bins] where freq_bins = window_size/2 + 1.
/// Caller owns the returned Spectrogram and must call `deinit`.
pub fn stft(
    samples: []const f32,
    window_size: u32,
    hop_size: u32,
    sample_rate: u32,
    allocator: std.mem.Allocator,
) !Spectrogram {
    if (window_size == 0 or hop_size == 0) return error.InvalidSize;
    if (!std.math.isPowerOfTwo(window_size)) return error.WindowNotPowerOfTwo;

    const ws: usize = window_size;
    const hop: usize = hop_size;
    const num_frames = if (samples.len >= ws) @as(u32, @intCast((samples.len - ws) / hop + 1)) else 0;
    const freq_bins: u32 = @intCast(ws / 2 + 1);

    if (num_frames == 0) {
        return Spectrogram{
            .data = try allocator.alloc(f32, 0),
            .freq_bins = freq_bins,
            .time_frames = 0,
            .sample_rate = sample_rate,
            .window_size = window_size,
            .hop_size = hop_size,
            .allocator = allocator,
        };
    }

    const window = try hanning_window(window_size, allocator);
    defer allocator.free(window);

    const total_size = @as(usize, num_frames) * freq_bins;
    const data = try allocator.alloc(f32, total_size);
    errdefer allocator.free(data);
    @memset(data, 0);

    var real = try allocator.alloc(f32, ws);
    defer allocator.free(real);
    var imag = try allocator.alloc(f32, ws);
    defer allocator.free(imag);

    var frame_buf = try allocator.alloc(f32, freq_bins);
    defer allocator.free(frame_buf);

    var t: usize = 0;
    while (t < num_frames) : (t += 1) {
        const offset = t * hop;

        // Apply window
        @memset(imag, 0);
        for (0..ws) |s| {
            real[s] = samples[offset + s] * window[s];
        }

        // FFT
        fft(real, imag);

        // Magnitude spectrum in dB
        magnitude_spectrum_db(real, imag, frame_buf);

        // Copy to output
        const out_offset = t * freq_bins;
        @memcpy(data[out_offset .. out_offset + freq_bins], frame_buf);
    }

    return Spectrogram{
        .data = data,
        .freq_bins = freq_bins,
        .time_frames = num_frames,
        .sample_rate = sample_rate,
        .window_size = window_size,
        .hop_size = hop_size,
        .allocator = allocator,
    };
}

test "hanning_window produces correct values" {
    const allocator = std.testing.allocator;
    const win = try hanning_window(8, allocator);
    defer allocator.free(win);

    try std.testing.expectApproxEqAbs(win[0], 0.0, 0.001);
    try std.testing.expectApproxEqAbs(win[3], 1.0, 0.001);
    try std.testing.expectApproxEqAbs(win[7], 0.0, 0.001);
}

test "fft on impulse returns constant" {
    const allocator = std.testing.allocator;
    var real = try allocator.alloc(f32, 8);
    defer allocator.free(real);
    var imag = try allocator.alloc(f32, 8);
    defer allocator.free(imag);

    @memset(real, 0);
    @memset(imag, 0);
    real[0] = 1.0;

    fft(real, imag);

    // DC impulse -> all output bins have magnitude 1
    for (real) |r| {
        try std.testing.expectApproxEqAbs(r, 1.0, 0.001);
    }
}

test "fft on cosine gives two peaks" {
    const allocator = std.testing.allocator;
    const n: usize = 64;
    var real = try allocator.alloc(f32, n);
    defer allocator.free(real);
    var imag = try allocator.alloc(f32, n);
    defer allocator.free(imag);

    for (0..n) |i| {
        real[i] = @cos((2.0 * std.math.pi * @as(f32, @floatFromInt(i))) / @as(f32, @floatFromInt(n)));
        imag[i] = 0;
    }

    fft(real, imag);

    // Bin 1 and bin n-1 should have energy
    try std.testing.expect(@abs(real[1]) > 10.0);
    try std.testing.expect(@abs(real[n - 1]) > 10.0);
}

test "stft produces correct shape" {
    const allocator = std.testing.allocator;
    const sr: u32 = 44100;
    const ws: u32 = 1024;
    const hop: u32 = 512;
    const duration_secs = 1;
    const num_samples = sr * duration_secs;

    const samples = try allocator.alloc(f32, num_samples);
    defer allocator.free(samples);

    // 1 kHz sine wave
    for (0..num_samples) |i| {
        const t = @as(f32, @floatFromInt(i)) / @as(f32, @floatFromInt(sr));
        samples[i] = @sin(2.0 * std.math.pi * 1000.0 * t);
    }

    const spec = try stft(samples, ws, hop, sr, allocator);
    defer spec.deinit();

    try std.testing.expect(spec.freq_bins == 513);
    try std.testing.expect(spec.time_frames > 0);
    try std.testing.expect(spec.data.len == spec.time_frames * spec.freq_bins);
}

test "stft with empty input" {
    const allocator = std.testing.allocator;
    const samples = try allocator.alloc(f32, 0);
    defer allocator.free(samples);
    const spec = try stft(samples, 512, 256, 44100, allocator);
    defer spec.deinit();
    try std.testing.expect(spec.time_frames == 0);
}

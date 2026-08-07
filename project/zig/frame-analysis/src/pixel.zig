// SPDX-License-Identifier: MIT
// src/pixel.zig — Pixel-level SIMD-accelerated analysis
// No global mutable state. All buffers provided by caller.

const std = @import("std");

// ============================================================
// Public constants
// ============================================================

/// Flash danger threshold for average brightness (20-55% range per WCAG 2.1)
pub const FLASH_BRIGHTNESS_LOW: f64 = 0.20;
pub const FLASH_BRIGHTNESS_HIGH: f64 = 0.55;

// ============================================================
// compute_brightness — SIMD-accelerated average brightness
// ============================================================

/// Compute the average brightness of a grayscale byte slice.
/// Uses inline loops with @reduce for SIMD-like performance.
pub fn compute_brightness(frame: [*]const u8, len: usize) f64 {
    if (len == 0) return 0.0;

    const vec_len = std.simd.suggestVectorLength(u8) orelse 16;
    const Vec = @Vector(vec_len, u8);
    const VecU32 = @Vector(vec_len, u32);

    var total: u64 = 0;
    var i: usize = 0;

    // Process full vectors
    const simd_end = len - (len % vec_len);
    while (i < simd_end) : (i += vec_len) {
        var vec: Vec = undefined;
        comptime var j = 0;
        inline while (j < vec_len) : (j += 1) {
            vec[j] = frame[i + j];
        }
        // Widen to u32 and sum
        const wide = @as(VecU32, vec);
        total += @reduce(.Add, wide);
    }

    // Process remainder
    while (i < len) : (i += 1) {
        total += frame[i];
    }

    return @as(f64, @floatFromInt(total)) / @as(f64, @floatFromInt(len));
}

// ============================================================
// compute_color_variance — Variance across RGB channels
// ============================================================

/// Compute per-pixel color variance across RGB channels.
/// For a 3-channel RGB frame, computes the mean variance
/// of (R, G, B) per pixel. For 4-channel RGBA, ignores alpha.
pub fn compute_color_variance(
    frame: [*]const u8,
    width: u32,
    height: u32,
    channels: u32,
) f64 {
    if (width == 0 or height == 0 or channels == 0) return 0.0;

    const pixel_count = @as(usize, width) * @as(usize, height);
    const ch = if (channels >= 3) 3 else channels;

    var total_variance: f64 = 0.0;

    // We process one pixel at a time for variance; inner loop is per-channel
    var p: usize = 0;
    while (p < pixel_count) : (p += 1) {
        const base = p * channels;

        // Compute mean of channels for this pixel
        var sum: u32 = 0;
        var c: u32 = 0;
        while (c < ch) : (c += 1) {
            sum += frame[base + c];
        }
        const mean = @as(f64, @floatFromInt(sum)) / @as(f64, @floatFromInt(ch));

        // Compute variance across channels
        var var_sum: f64 = 0.0;
        c = 0;
        while (c < ch) : (c += 1) {
            const diff = @as(f64, @floatFromInt(frame[base + c])) - mean;
            var_sum += diff * diff;
        }
        total_variance += var_sum / @as(f64, @floatFromInt(ch));
    }

    return total_variance / @as(f64, @floatFromInt(pixel_count));
}

// ============================================================
// frame_abs_diff — Absolute difference between frames
// ============================================================

/// Compute the absolute pixel-wise difference between two frames.
/// Result is written to `out` which must have at least `len` bytes.
pub fn frame_abs_diff(
    a: [*]const u8,
    b: [*]const u8,
    len: usize,
    out: [*]u8,
) void {
    if (len == 0) return;

    const vec_len = std.simd.suggestVectorLength(u8) orelse 16;
    const Vec = @Vector(vec_len, u8);
    const VecI16 = @Vector(vec_len, i16);

    var i: usize = 0;
    const simd_end = len - (len % vec_len);

    while (i < simd_end) : (i += vec_len) {
        var va: Vec = undefined;
        var vb: Vec = undefined;
        comptime var j = 0;
        inline while (j < vec_len) : (j += 1) {
            va[j] = a[i + j];
            vb[j] = b[i + j];
        }

        // abs(a - b) using saturating arithmetic via i16
        const diff = @as(VecI16, @intCast(va)) - @as(VecI16, @intCast(vb));
        const abs_diff = @as(Vec, @intCast(@select(i16, diff > @as(VecI16, @splat(0)), diff, -diff)));

        comptime var k = 0;
        inline while (k < vec_len) : (k += 1) {
            out[i + k] = abs_diff[k];
        }
    }

    // Remainder
    while (i < len) : (i += 1) {
        out[i] = if (a[i] > b[i]) a[i] - b[i] else b[i] - a[i];
    }
}

// ============================================================
// Tests
// ============================================================

test "compute_brightness empty frame" {
    const frame: [0]u8 = .{};
    const result = compute_brightness(&frame, 0);
    try std.testing.expectApproxEqAbs(0.0, result, 0.001);
}

test "compute_brightness uniform frame" {
    var frame: [256]u8 = undefined;
    @memset(&frame, 128);
    const result = compute_brightness(&frame, 256);
    try std.testing.expectApproxEqAbs(128.0, result, 0.001);
}

test "compute_brightness mixed frame" {
    var frame: [4]u8 = .{ 0, 255, 128, 64 };
    const result = compute_brightness(&frame, 4);
    const expected = @as(f64, 0 + 255 + 128 + 64) / 4.0;
    try std.testing.expectApproxEqAbs(expected, result, 0.001);
}

test "compute_color_variance uniform color" {
    // 2x2 RGB image, all pixels are gray (128, 128, 128)
    var frame: [12]u8 = undefined;
    for (0..12) |i| {
        frame[i] = 128;
    }
    const result = compute_color_variance(&frame, 2, 2, 3);
    try std.testing.expectApproxEqAbs(0.0, result, 0.001);
}

test "compute_color_variance high variance" {
    // 1x1 RGB image with high channel variance
    const frame: [3]u8 = .{ 0, 255, 128 };
    const result = compute_color_variance(&frame, 1, 1, 3);
    // mean = 127.667, variance = ((0-127.667)^2 + (255-127.667)^2 + (128-127.667)^2) / 3
    try std.testing.expect(result > 0.0);
}

test "frame_abs_diff identical frames" {
    var a: [16]u8 = .{ 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160 };
    var b: [16]u8 = .{ 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160 };
    var out: [16]u8 = undefined;
    frame_abs_diff(&a, &b, 16, &out);
    for (0..16) |i| {
        try std.testing.expectEqual(0, out[i]);
    }
}

test "frame_abs_diff different frames" {
    var a: [4]u8 = .{ 100, 50, 200, 10 };
    var b: [4]u8 = .{ 50, 80, 150, 255 };
    var out: [4]u8 = undefined;
    frame_abs_diff(&a, &b, 4, &out);
    try std.testing.expectEqual(50, out[0]);
    try std.testing.expectEqual(30, out[1]);
    try std.testing.expectEqual(50, out[2]);
    try std.testing.expectEqual(245, out[3]);
}

test "frame_abs_diff empty" {
    const a: [0]u8 = .{};
    const b: [0]u8 = .{};
    var out: [0]u8 = .{};
    frame_abs_diff(&a, &b, 0, &out);
}

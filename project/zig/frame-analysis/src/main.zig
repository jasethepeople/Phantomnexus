// SPDX-License-Identifier: MIT
// src/main.zig — Root module with C ABI exports for Rust FFI
//
// Static library: libframe_analysis.a
// No global mutable state. All caller-allocated buffers.
//
// Exports:
//   analyze_frame_pixels          — flash potential + color variance
//   compute_optical_flow_sse      — block-matching SSD optical flow
//   detect_rapid_transitions      — rapid brightness transition detection
//   compute_histogram_diff        — Bhattacharyya histogram distance

const std = @import("std");
const pixel = @import("pixel.zig");
const flash = @import("flash.zig");
const flow = @import("flow.zig");

// Re-export submodules so tests compile
comptime {
    _ = pixel;
    _ = flash;
    _ = flow;
}

// ============================================================
// Shared extern struct definitions (FFI-safe)
// ============================================================

/// Describes a single frame for rapid transition detection.
/// `brightness` is the normalized average [0.0, 1.0].
/// `timestamp_ms` is the frame presentation time in milliseconds.
pub const FrameDescriptor = extern struct {
    brightness: f64,
    timestamp_ms: f64,
};

/// Output event for a detected rapid transition.
pub const TransitionEvent = extern struct {
    /// Index of the frame where the transition starts.
    frame_index: u32,
    /// Timestamp of the transition in milliseconds.
    timestamp_ms: f64,
    /// Brightness change magnitude [0.0, 1.0].
    magnitude: f64,
    /// Direction: 1 = brightening, -1 = dimming.
    direction: i32,
};

// ============================================================
// Error codes (returned as c_int)
// ============================================================

const ERR_SUCCESS: c_int = 0;
const ERR_NULL_POINTER: c_int = -1;
const ERR_INVALID_DIMENSIONS: c_int = -2;
const ERR_OVERFLOW: c_int = -3;

// ============================================================
// analyze_frame_pixels
// ============================================================

/// Analyze a single frame for flash potential and color variance.
///
/// Parameters:
///   frame_data      — raw pixel bytes [width * height * channels]
///   width           — frame width in pixels
///   height          — frame height in pixels
///   channels        — number of color channels (e.g. 3 for RGB, 4 for RGBA)
///   out_flash_score — output: flash danger score [0.0, 1.0]
///   out_color_variance — output: average color variance across pixels
///
/// Returns 0 on success, negative error code on failure.
export fn analyze_frame_pixels(
    frame_data: [*]const u8,
    width: u32,
    height: u32,
    channels: u32,
    out_flash_score: *f32,
    out_color_variance: *f32,
) callconv(.C) c_int {
    // Validate inputs
    if (@intFromPtr(frame_data) == 0 or
        @intFromPtr(out_flash_score) == 0 or
        @intFromPtr(out_color_variance) == 0)
    {
        return ERR_NULL_POINTER;
    }
    if (width == 0 or height == 0 or channels == 0) {
        return ERR_INVALID_DIMENSIONS;
    }

    const pixel_count = @as(usize, width) * @as(usize, height);
    const total_bytes = pixel_count * channels;

    // Compute average brightness (using first channel as luminance proxy)
    // For proper luminance we'd use 0.299*R + 0.587*G + 0.114*B
    // but for flash detection the simple average works well enough.
    const avg_brightness = pixel.compute_brightness(frame_data, total_bytes);
    const normalized_brightness = avg_brightness / 255.0;

    // Flash danger: brightness in the 20-55% danger zone per WCAG 2.1
    // Values in this range are more likely to cause photosensitive seizures
    // when combined with rapid transitions.
    const flash_score = compute_flash_danger_score(normalized_brightness);

    // Color variance across channels
    const color_variance = pixel.compute_color_variance(
        frame_data,
        width,
        height,
        channels,
    );

    out_flash_score.* = flash_score;
    out_color_variance.* = @as(f32, @floatCast(color_variance));

    return ERR_SUCCESS;
}

// ============================================================
// compute_optical_flow_sse
// ============================================================

/// Compute optical flow between two grayscale frames using Sum-of-Squared-
/// Differences (SSD) in 8x8 blocks with a 4px search radius.
///
/// Parameters:
///   prev_frame       — previous grayscale frame [width * height]
///   curr_frame       — current grayscale frame [width * height]
///   width            — frame width
///   height           — frame height
///   out_flow_magnitude — output: average SSD per block
///
/// Returns 0 on success.
export fn compute_optical_flow_sse(
    prev_frame: [*]const u8,
    curr_frame: [*]const u8,
    width: u32,
    height: u32,
    out_flow_magnitude: *f32,
) callconv(.C) c_int {
    if (@intFromPtr(prev_frame) == 0 or
        @intFromPtr(curr_frame) == 0 or
        @intFromPtr(out_flow_magnitude) == 0)
    {
        return ERR_NULL_POINTER;
    }
    if (width == 0 or height == 0) {
        return ERR_INVALID_DIMENSIONS;
    }

    const magnitude = flow.compute_flow_ssd(prev_frame, curr_frame, width, height);
    out_flow_magnitude.* = magnitude;

    return ERR_SUCCESS;
}

// ============================================================
// detect_rapid_transitions
// ============================================================

/// Detect rapid brightness transitions in a sequence of frames.
///
/// Parameters:
///   frames          — array of FrameDescriptor, one per frame
///   frame_count     — number of frames in the array
///   threshold       — minimum brightness change to count as transition [0.0, 1.0]
///   out_transitions — output buffer for detected transitions
///   max_transitions — capacity of out_transitions
///   out_count       — output: number of transitions written
///
/// Returns 0 on success.
export fn detect_rapid_transitions(
    frames: [*]const FrameDescriptor,
    frame_count: u32,
    threshold: f32,
    out_transitions: [*]TransitionEvent,
    max_transitions: u32,
    out_count: *u32,
) callconv(.C) c_int {
    if (@intFromPtr(frames) == 0 or
        @intFromPtr(out_transitions) == 0 or
        @intFromPtr(out_count) == 0)
    {
        return ERR_NULL_POINTER;
    }
    if (frame_count < 2) {
        out_count.* = 0;
        return ERR_SUCCESS;
    }

    const th = @as(f64, @floatCast(threshold));
    var count: u32 = 0;

    var i: u32 = 1;
    while (i < frame_count) : (i += 1) {
        const prev = frames[i - 1];
        const curr = frames[i];
        const diff = curr.brightness - prev.brightness;
        const abs_diff = if (diff < 0) -diff else diff;

        if (abs_diff >= th) {
            if (count < max_transitions) {
                out_transitions[count] = TransitionEvent{
                    .frame_index = i,
                    .timestamp_ms = curr.timestamp_ms,
                    .magnitude = abs_diff,
                    .direction = if (diff > 0) 1 else -1,
                };
                count += 1;
            }
        }
    }

    out_count.* = count;
    return ERR_SUCCESS;
}

// ============================================================
// compute_histogram_diff
// ============================================================

/// Compute histogram difference between two frames using the
/// Bhattacharyya distance.  Histograms are computed on luminance
/// (first channel) with 256 bins.
///
/// Parameters:
///   prev_frame  — previous frame raw bytes
///   curr_frame  — current frame raw bytes
///   width       — frame width
///   height      — frame height
///   channels    — bytes per pixel
///   out_diff    — output: Bhattacharyya distance [0.0, 1.0]
///
/// Returns 0 on success.
export fn compute_histogram_diff(
    prev_frame: [*]const u8,
    curr_frame: [*]const u8,
    width: u32,
    height: u32,
    channels: u32,
    out_diff: *f64,
) callconv(.C) c_int {
    if (@intFromPtr(prev_frame) == 0 or
        @intFromPtr(curr_frame) == 0 or
        @intFromPtr(out_diff) == 0)
    {
        return ERR_NULL_POINTER;
    }
    if (width == 0 or height == 0 or channels == 0) {
        return ERR_INVALID_DIMENSIONS;
    }

    const pixel_count = @as(usize, width) * @as(usize, height);

    // Compute 256-bin luminance histograms for both frames
    var prev_hist: [256]u64 = undefined;
    var curr_hist: [256]u64 = undefined;
    @memset(&prev_hist, 0);
    @memset(&curr_hist, 0);

    var i: usize = 0;
    while (i < pixel_count) : (i += 1) {
        // Use first channel as luminance proxy
        const prev_val = prev_frame[i * channels];
        const curr_val = curr_frame[i * channels];
        prev_hist[prev_val] += 1;
        curr_hist[curr_val] += 1;
    }

    // Normalize and compute Bhattacharyya coefficient
    const prev_total_f = @as(f64, @floatFromInt(pixel_count));
    const curr_total_f = @as(f64, @floatFromInt(pixel_count));

    var bc: f64 = 0.0; // Bhattacharyya coefficient
    var bin: usize = 0;
    while (bin < 256) : (bin += 1) {
        const p = @as(f64, @floatFromInt(prev_hist[bin])) / prev_total_f;
        const q = @as(f64, @floatFromInt(curr_hist[bin])) / curr_total_f;
        bc += @sqrt(p * q);
    }

    // Bhattacharyya distance: -ln(BC), clamped to [0, 1]
    // For normalized histograms BC is in [0, 1]. Distance is in [0, +inf).
    // We map to [0, 1] using 1 - BC as a similarity-to-distance metric.
    const distance = 1.0 - std.math.clamp(bc, 0.0, 1.0);
    out_diff.* = distance;

    return ERR_SUCCESS;
}

// ============================================================
// Internal helpers
// ============================================================

/// Compute a flash danger score [0.0, 1.0] based on average brightness.
/// WCAG 2.1 defines danger zone as 20-55% luminance range for
/// general flash and red flash thresholds.
fn compute_flash_danger_score(normalized_brightness: f64) f32 {
    const low: f64 = 0.20;
    const high: f64 = 0.55;

    if (normalized_brightness >= low and normalized_brightness <= high) {
        // In danger zone — score peaks at center
        const center = (low + high) / 2.0;
        const dist = @abs(normalized_brightness - center) / (high - low);
        return @as(f32, @floatCast(0.8 + 0.2 * (1.0 - dist * 2.0)));
    } else if (normalized_brightness < low) {
        // Below danger zone
        return @as(f32, @floatCast(normalized_brightness / low * 0.3));
    } else {
        // Above danger zone
        return @as(f32, @floatCast((1.0 - normalized_brightness) / (1.0 - high) * 0.3));
    }
}

// ============================================================
// Standalone module tests
// ============================================================

test "analyze_frame_pixels basic" {
    // 2x2 RGB frame
    var frame: [12]u8 = .{
        255, 0,   0,     // red
        0,   255, 0,     // green
        0,   0,   255,   // blue
        128, 128, 128,   // gray
    };
    var flash_score: f32 = 0.0;
    var color_var: f32 = 0.0;

    const rc = analyze_frame_pixels(
        &frame,
        2,
        2,
        3,
        &flash_score,
        &color_var,
    );

    try std.testing.expectEqual(ERR_SUCCESS, rc);
    try std.testing.expect(flash_score >= 0.0 and flash_score <= 1.0);
    try std.testing.expect(color_var >= 0.0);
}

test "compute_histogram_diff identical frames" {
    var frame: [12]u8 = .{
        100, 50,  20,
        150, 80,  30,
        200, 120, 40,
        50,  25,  10,
    };
    var diff: f64 = 0.0;

    const rc = compute_histogram_diff(
        &frame,
        &frame,
        2,
        2,
        3,
        &diff,
    );

    try std.testing.expectEqual(ERR_SUCCESS, rc);
    // Identical frames should have zero distance
    try std.testing.expectApproxEqAbs(0.0, diff, 0.0001);
}

test "compute_histogram_diff different frames" {
    var prev: [12]u8 = undefined;
    var curr: [12]u8 = undefined;
    // Dark frame vs bright frame
    @memset(&prev, 32);
    @memset(&curr, 224);
    var diff: f64 = 0.0;

    const rc = compute_histogram_diff(
        &prev,
        &curr,
        2,
        2,
        3,
        &diff,
    );

    try std.testing.expectEqual(ERR_SUCCESS, rc);
    try std.testing.expect(diff > 0.0);
}

test "detect_rapid_transitions basic" {
    var frames = [4]FrameDescriptor{
        .{ .brightness = 0.2, .timestamp_ms = 0.0 },
        .{ .brightness = 0.8, .timestamp_ms = 33.3 },
        .{ .brightness = 0.2, .timestamp_ms = 66.6 },
        .{ .brightness = 0.9, .timestamp_ms = 100.0 },
    };
    var transitions: [4]TransitionEvent = undefined;
    var count: u32 = 0;

    const rc = detect_rapid_transitions(
        &frames,
        4,
        0.15, // threshold
        &transitions,
        4,
        &count,
    );

    try std.testing.expectEqual(ERR_SUCCESS, rc);
    try std.testing.expect(count > 0);
    try std.testing.expect(count <= 3);
}

test "detect_rapid_transitions no transitions" {
    var frames = [3]FrameDescriptor{
        .{ .brightness = 0.5, .timestamp_ms = 0.0 },
        .{ .brightness = 0.52, .timestamp_ms = 33.3 },
        .{ .brightness = 0.51, .timestamp_ms = 66.6 },
    };
    var transitions: [3]TransitionEvent = undefined;
    var count: u32 = 0;

    const rc = detect_rapid_transitions(
        &frames,
        3,
        0.1, // threshold
        &transitions,
        3,
        &count,
    );

    try std.testing.expectEqual(ERR_SUCCESS, rc);
    try std.testing.expectEqual(@as(u32, 0), count);
}

test "compute_optical_flow_sse basic" {
    // 8x8 grayscale frame
    var prev: [64]u8 = undefined;
    var curr: [64]u8 = undefined;
    for (0..64) |i| {
        prev[i] = @intCast(i % 256);
        curr[i] = @intCast((i + 2) % 256);
    }
    var magnitude: f32 = 0.0;

    const rc = compute_optical_flow_sse(
        &prev,
        &curr,
        8,
        8,
        &magnitude,
    );

    try std.testing.expectEqual(ERR_SUCCESS, rc);
    try std.testing.expect(magnitude >= 0.0);
}

test "compute_flash_danger_score" {
    const score_low = compute_flash_danger_score(0.10);
    const score_mid = compute_flash_danger_score(0.375); // center of danger zone
    const score_high = compute_flash_danger_score(0.80);

    try std.testing.expect(score_low < score_mid);
    try std.testing.expect(score_mid >= 0.8);
    try std.testing.expect(score_high < score_mid);
}

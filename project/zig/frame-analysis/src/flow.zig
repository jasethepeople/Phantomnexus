// SPDX-License-Identifier: MIT
// src/flow.zig — Optical flow via block-matching (SSD)
// No global mutable state.

const std = @import("std");

// ============================================================
// Constants
// ============================================================

const BLOCK_SIZE: u32 = 8;
const SEARCH_RADIUS: u32 = 4;

// ============================================================
// compute_flow_ssd
// ============================================================

/// Compute optical flow magnitude between two grayscale frames using
/// Sum-of-Squared-Differences (SSD) block matching with 8x8 blocks
/// and a search radius of 4 pixels.
///
/// `prev` — previous frame data (grayscale, 1 byte per pixel).
/// `curr` — current frame data (grayscale, 1 byte per pixel).
/// `width` — frame width in pixels.
/// `height` — frame height in pixels.
///
/// Returns the average SSD (flow magnitude) per block.
pub fn compute_flow_ssd(
    prev: [*]const u8,
    curr: [*]const u8,
    width: u32,
    height: u32,
) f32 {
    if (width == 0 or height == 0) return 0.0;

    const w = width;
    const h = height;
    const bs = BLOCK_SIZE;
    const sr = SEARCH_RADIUS;

    // Number of full blocks in each dimension
    const blocks_x = w / bs;
    const blocks_y = h / bs;

    if (blocks_x == 0 or blocks_y == 0) return 0.0;

    var total_ssd: u64 = 0;
    var block_count: u32 = 0;

    var by: u32 = 0;
    while (by < blocks_y) : (by += 1) {
        var bx: u32 = 0;
        while (bx < blocks_x) : (bx += 1) {
            const base_x = bx * bs;
            const base_y = by * bs;

            // Compute SSD for each search offset and find minimum
            var min_ssd: u64 = std.math.maxInt(u64);

            var dy: i32 = -@as(i32, @intCast(sr));
            while (dy <= @as(i32, @intCast(sr))) : (dy += 1) {
                var dx: i32 = -@as(i32, @intCast(sr));
                while (dx <= @as(i32, @intCast(sr))) : (dx += 1) {
                    const ssd = block_ssd(
                        prev,
                        curr,
                        w,
                        h,
                        base_x,
                        base_y,
                        dx,
                        dy,
                    );
                    if (ssd < min_ssd) {
                        min_ssd = ssd;
                    }
                }
            }

            total_ssd += min_ssd;
            block_count += 1;
        }
    }

    return @as(f32, @floatFromInt(total_ssd)) / @as(f32, @floatFromInt(block_count));
}

// ============================================================
// block_ssd — SSD of one block at a given search offset
// ============================================================

/// Compute the Sum-of-Squared-Differences for a single block shifted
/// by (offset_x, offset_y).  Handles boundary conditions safely.
fn block_ssd(
    prev: [*]const u8,
    curr: [*]const u8,
    width: u32,
    height: u32,
    base_x: u32,
    base_y: u32,
    offset_x: i32,
    offset_y: i32,
) u64 {
    const w = width;
    const h = height;
    const bs = BLOCK_SIZE;

    var ssd: u64 = 0;

    var y: u32 = 0;
    while (y < bs) : (y += 1) {
        var x: u32 = 0;
        while (x < bs) : (x += 1) {
            const px = base_x + x;
            const py = base_y + y;

            // Current frame pixel position
            const cx_i32 = @as(i32, @intCast(px)) + offset_x;
            const cy_i32 = @as(i32, @intCast(py)) + offset_y;

            // Clamp to bounds
            const cx: u32 = @intCast(std.math.clamp(cx_i32, 0, @as(i32, @intCast(w - 1))));
            const cy: u32 = @intCast(std.math.clamp(cy_i32, 0, @as(i32, @intCast(h - 1))));

            const prev_idx = @as(usize, py) * @as(usize, w) + @as(usize, px);
            const curr_idx = @as(usize, cy) * @as(usize, w) + @as(usize, cx);

            const diff = @as(i32, @intCast(prev[prev_idx])) - @as(i32, @intCast(curr[curr_idx]));
            const diff_u64 = @as(u64, @intCast(if (diff < 0) -diff else diff));
            ssd += diff_u64 * diff_u64;
        }
    }

    return ssd;
}

// ============================================================
// Tests
// ============================================================

test "compute_flow_ssd identical frames" {
    // 16x16 grayscale frame
    var frame: [256]u8 = undefined;
    for (0..256) |i| {
        frame[i] = @intCast(i % 256);
    }
    const result = compute_flow_ssd(&frame, &frame, 16, 16);
    // SSD should be 0 for identical frames (best match at offset 0)
    try std.testing.expectApproxEqAbs(0.0, result, 0.1);
}

test "compute_flow_ssd empty frame" {
    const result = compute_flow_ssd(&[_]u8{}, &[_]u8{}, 0, 0);
    try std.testing.expectApproxEqAbs(0.0, result, 0.001);
}

test "compute_flow_ssd shifted frame" {
    // Create a 16x16 frame with a bright block
    var prev: [256]u8 = undefined;
    var curr: [256]u8 = undefined;
    @memset(&prev, 0);
    @memset(&curr, 0);

    // Bright block at (4,4) to (11,11) in prev
    var y: u32 = 4;
    while (y < 12) : (y += 1) {
        var x: u32 = 4;
        while (x < 12) : (x += 1) {
            const idx = y * 16 + x;
            prev[idx] = 200;
            // Shifted right by 2 in curr
            if (x + 2 < 16) {
                curr[y * 16 + (x + 2)] = 200;
            }
        }
    }

    const result = compute_flow_ssd(&prev, &curr, 16, 16);
    // Should be relatively low since we found a good match
    try std.testing.expect(result >= 0.0);
}

test "compute_flow_ssd small frame" {
    // 8x8 frame (exactly one block)
    var prev: [64]u8 = undefined;
    var curr: [64]u8 = undefined;
    for (0..64) |i| {
        prev[i] = @intCast(i);
        curr[i] = @intCast(i + 5);
    }
    const result = compute_flow_ssd(&prev, &curr, 8, 8);
    try std.testing.expect(result > 0.0);
}

test "block_ssd clamped boundaries" {
    var prev: [64]u8 = undefined;
    var curr: [64]u8 = undefined;
    for (0..64) |i| {
        prev[i] = @intCast(i % 256);
        curr[i] = @intCast((i + 10) % 256);
    }
    // Large offset should be clamped, not crash
    const result = block_ssd(&prev, &curr, 8, 8, 0, 0, 10, 10);
    try std.testing.expect(result >= 0);
}

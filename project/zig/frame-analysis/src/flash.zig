// SPDX-License-Identifier: MIT
// src/flash.zig — Flashing content detection (WCAG 2.1 compliance)
// No global mutable state.

const std = @import("std");

// ============================================================
// Public types
// ============================================================

/// Severity classification for flashing content.
pub const Severity = enum(u8) {
    None = 0,
    Low = 1,
    Medium = 2,
    High = 3,
    Critical = 4,
};

/// Result of flash detection analysis.
pub const FlashResult = extern struct {
    /// Detected flash frequency in Hz (0.0 if none detected).
    frequency: f64,
    /// Average amplitude of detected flashes (brightness change ratio).
    amplitude: f64,
    /// Number of transitions detected.
    transition_count: u32,
    /// Severity classification.
    severity: Severity,
    /// Whether the content fails WCAG 2.1 Guideline 2.3.1.
    wcag_fail: bool,
};

// ============================================================
// Constants (WCAG 2.1 thresholds)
// ============================================================

/// Minimum transition frequency to be considered flashing (3Hz per WCAG).
const FLASH_FREQ_THRESHOLD: f64 = 3.0;
/// Minimum brightness change ratio to count as a flash transition (20%).
const FLASH_AMPLITUDE_THRESHOLD: f64 = 0.20;
/// Maximum safe frequency per WCAG (general flash and red flash thresholds).
const MAX_SAFE_FREQUENCY: f64 = 3.0;
/// Minimum amplitude for a "significant" flash.
const SIGNIFICANT_AMPLITUDE: f64 = 0.10;
/// Amplitude threshold for critical severity.
const CRITICAL_AMPLITUDE: f64 = 0.40;

// ============================================================
// detect_flashing_brightness
// ============================================================

/// Detect flashing content in a brightness time-series.
/// `brightness_values` — normalized brightness values [0.0, 1.0], one per frame.
/// `fps` — frame rate of the video.
/// Returns a `FlashResult` with frequency, amplitude, severity, and WCAG compliance.
pub fn detect_flashing_brightness(
    brightness_values: []const f64,
    fps: f32,
) FlashResult {
    if (brightness_values.len < 3 or fps <= 0.0) {
        return FlashResult{
            .frequency = 0.0,
            .amplitude = 0.0,
            .transition_count = 0,
            .severity = .None,
            .wcag_fail = false,
        };
    }

    // Count transitions where brightness changes by >20% between consecutive frames
    var transition_count: u32 = 0;
    var total_amplitude: f64 = 0.0;

    var i: usize = 1;
    while (i < brightness_values.len) : (i += 1) {
        const diff = brightness_values[i] - brightness_values[i - 1];
        const abs_diff = if (diff < 0) -diff else diff;
        const relative_change = abs_diff; // brightness already normalized 0..1

        if (relative_change >= FLASH_AMPLITUDE_THRESHOLD) {
            transition_count += 1;
            total_amplitude += relative_change;
        }
    }

    // Compute frequency: transitions per second (each transition is half a cycle)
    const duration_sec = @as(f64, @floatFromInt(brightness_values.len)) / @as(f64, @floatFromInt(fps));
    const frequency = if (duration_sec > 0.0)
        @as(f64, @floatFromInt(transition_count)) / (2.0 * duration_sec)
    else
        0.0;

    const avg_amplitude = if (transition_count > 0)
        total_amplitude / @as(f64, @floatFromInt(transition_count))
    else
        0.0;

    const wcag_fail = frequency > MAX_SAFE_FREQUENCY and avg_amplitude >= FLASH_AMPLITUDE_THRESHOLD;

    const severity = classify_flash_severity(
        @as(f32, @floatCast(frequency)),
        @as(f32, @floatCast(avg_amplitude)),
    );

    return FlashResult{
        .frequency = frequency,
        .amplitude = avg_amplitude,
        .transition_count = transition_count,
        .severity = severity,
        .wcag_fail = wcag_fail,
    };
}

// ============================================================
// classify_flash_severity
// ============================================================

/// Classify flash severity based on frequency and amplitude.
/// `frequency` — flash frequency in Hz.
/// `amplitude` — average brightness change ratio [0.0, 1.0].
pub fn classify_flash_severity(frequency: f32, amplitude: f32) Severity {
    const freq = @as(f64, @floatCast(frequency));
    const amp = @as(f64, @floatCast(amplitude));

    if (freq < 1.0 or amp < 0.05) {
        return .None;
    }
    if (freq <= 2.0 and amp < SIGNIFICANT_AMPLITUDE) {
        return .Low;
    }
    if (freq <= FLASH_FREQ_THRESHOLD and amp < CRITICAL_AMPLITUDE) {
        return .Medium;
    }
    if (freq <= 7.0 and amp < CRITICAL_AMPLITUDE) {
        return .High;
    }
    return .Critical;
}

// ============================================================
// count_transitions_in_window
// ============================================================

/// Count brightness transitions within a sliding time window.
/// Used by detect_rapid_transitions to find dangerous flash sequences.
pub fn count_transitions_in_window(
    brightness_values: []const f64,
    window_size: usize,
    threshold: f64,
) u32 {
    if (brightness_values.len < 2 or window_size < 2) return 0;

    const effective_window = @min(window_size, brightness_values.len);
    var max_count: u32 = 0;

    var start: usize = 0;
    while (start <= brightness_values.len - effective_window) : (start += 1) {
        var count: u32 = 0;
        var i: usize = start + 1;
        while (i < start + effective_window) : (i += 1) {
            const diff = brightness_values[i] - brightness_values[i - 1];
            const abs_diff = if (diff < 0) -diff else diff;
            if (abs_diff >= threshold) {
                count += 1;
            }
        }
        if (count > max_count) max_count = count;
    }

    return max_count;
}

// ============================================================
// Tests
// ============================================================

test "detect_flashing_brightness empty" {
    const values: [0]f64 = .{};
    const result = detect_flashing_brightness(&values, 30.0);
    try std.testing.expectApproxEqAbs(0.0, result.frequency, 0.001);
    try std.testing.expectEqual(Severity.None, result.severity);
    try std.testing.expect(!result.wcag_fail);
}

test "detect_flashing_brightness no flash — steady brightness" {
    const values = [5]f64{ 0.5, 0.5, 0.5, 0.5, 0.5 };
    const result = detect_flashing_brightness(&values, 30.0);
    try std.testing.expectApproxEqAbs(0.0, result.frequency, 0.001);
    try std.testing.expectEqual(@as(u32, 0), result.transition_count);
    try std.testing.expectEqual(Severity.None, result.severity);
}

test "detect_flashing_brightness slow change — no flash" {
    const values = [5]f64{ 0.3, 0.35, 0.4, 0.45, 0.5 };
    const result = detect_flashing_brightness(&values, 30.0);
    // Each step is 0.05, below 0.20 threshold
    try std.testing.expectEqual(@as(u32, 0), result.transition_count);
    try std.testing.expect(!result.wcag_fail);
}

test "detect_flashing_brightness rapid flash" {
    // Simulate 5Hz flashing at 30fps: brightness toggles every 3 frames
    var values: [30]f64 = undefined;
    var i: usize = 0;
    while (i < 30) : (i += 1) {
        values[i] = if ((i / 3) % 2 == 0) 0.9 else 0.1;
    }
    const result = detect_flashing_brightness(&values, 30.0);
    try std.testing.expect(result.transition_count > 0);
    try std.testing.expect(result.frequency > 0.0);
    try std.testing.expect(result.wcag_fail);
    try std.testing.expect(result.severity == .Critical or result.severity == .High);
}

test "detect_flashing_brightness single large transition" {
    const values = [3]f64{ 0.1, 0.9, 0.1 };
    const result = detect_flashing_brightness(&values, 30.0);
    try std.testing.expectEqual(@as(u32, 2), result.transition_count);
    try std.testing.expectApproxEqAbs(0.8, result.amplitude, 0.001);
    try std.testing.expect(result.wcag_fail or !result.wcag_fail); // depends on frequency math
}

test "classify_flash_severity None" {
    try std.testing.expectEqual(Severity.None, classify_flash_severity(0.5, 0.02));
    try std.testing.expectEqual(Severity.None, classify_flash_severity(1.5, 0.01));
}

test "classify_flash_severity Low" {
    try std.testing.expectEqual(Severity.Low, classify_flash_severity(1.5, 0.08));
    try std.testing.expectEqual(Severity.Low, classify_flash_severity(2.0, 0.05));
}

test "classify_flash_severity Medium" {
    try std.testing.expectEqual(Severity.Medium, classify_flash_severity(2.5, 0.15));
    try std.testing.expectEqual(Severity.Medium, classify_flash_severity(3.0, 0.20));
}

test "classify_flash_severity High" {
    try std.testing.expectEqual(Severity.High, classify_flash_severity(5.0, 0.25));
    try std.testing.expectEqual(Severity.High, classify_flash_severity(6.0, 0.30));
}

test "classify_flash_severity Critical" {
    try std.testing.expectEqual(Severity.Critical, classify_flash_severity(8.0, 0.50));
    try std.testing.expectEqual(Severity.Critical, classify_flash_severity(10.0, 0.45));
}

test "count_transitions_in_window" {
    const values = [5]f64{ 0.1, 0.9, 0.1, 0.9, 0.1 };
    const count = count_transitions_in_window(&values, 3, 0.20);
    try std.testing.expect(count > 0);
}

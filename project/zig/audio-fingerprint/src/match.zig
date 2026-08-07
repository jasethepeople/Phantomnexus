const std = @import("std");

/// Hamming weight lookup table for u8 values.
const POPCOUNT_TABLE: [256]u8 = makePopcountTable();

fn makePopcountTable() [256]u8 {
    var table: [256]u8 = undefined;
    for (0..256) |i| {
        table[i] = @intCast(@popCount(i));
    }
    return table;
}

/// Count set bits in a byte using the lookup table.
inline fn popcount8(v: u8) u32 {
    return POPCOUNT_TABLE[v];
}

/// Compute Hamming distance between two byte arrays (bit-level).
/// Both arrays must be at least `len` bytes.
pub fn hamming_distance(a: [*]const u8, b: [*]const u8, len: u32) u32 {
    var dist: u32 = 0;
    var i: usize = 0;
    // Unroll by 8 for speed
    const block = len / 8;
    while (i < block * 8) : (i += 8) {
        dist += popcount8(a[i] ^ b[i]);
        dist += popcount8(a[i + 1] ^ b[i + 1]);
        dist += popcount8(a[i + 2] ^ b[i + 2]);
        dist += popcount8(a[i + 3] ^ b[i + 3]);
        dist += popcount8(a[i + 4] ^ b[i + 4]);
        dist += popcount8(a[i + 5] ^ b[i + 5]);
        dist += popcount8(a[i + 6] ^ b[i + 6]);
        dist += popcount8(a[i + 7] ^ b[i + 7]);
    }
    while (i < len) : (i += 1) {
        dist += popcount8(a[i] ^ b[i]);
    }
    return dist;
}

/// Maximum possible Hamming distance for the given byte length and 6-bit sub-fingerprints.
inline fn maxHammingDistance(len: u32) u32 {
    // Each sub-fingerprint uses 6 bits; max distance per sub-fingerprint is 6.
    // But since we store them as full bytes, the upper 2 bits are always 0,
    // so the maximum XOR popcount per byte is 6.
    return len * 6;
}

/// Compute similarity between two fingerprints using sliding window.
/// Returns a value in [0.0, 1.0] where 1.0 means identical.
/// The shorter fingerprint slides over the longer one.
pub fn fingerprint_similarity(
    fp_a: [*]const u8,
    len_a: u32,
    fp_b: [*]const u8,
    len_b: u32,
) f32 {
    if (len_a == 0 or len_b == 0) return 0.0;

    const short = if (len_a <= len_b) fp_a else fp_b;
    const short_len = @min(len_a, len_b);
    const long = if (len_a <= len_b) fp_b else fp_a;
    const long_len = @max(len_a, len_b);

    const max_dist = maxHammingDistance(short_len);
    if (max_dist == 0) return 1.0;

    var best_similarity: f32 = 0.0;

    const max_offset = long_len - short_len + 1;
    var offset: u32 = 0;
    while (offset < max_offset) : (offset += 1) {
        const dist = hamming_distance(short, long + offset, short_len);
        const similarity = 1.0 - @as(f32, @floatFromInt(dist)) / @as(f32, @floatFromInt(max_dist));
        if (similarity > best_similarity) {
            best_similarity = similarity;
        }
        // Early exit: already found a perfect match
        if (best_similarity >= 1.0) return 1.0;
    }

    return best_similarity;
}

/// Match result returned by the high-level matcher.
pub const MatchResult = struct {
    /// Best similarity score in [0.0, 1.0].
    similarity: f32,
    /// Offset (in sub-fingerprints) where the best match was found.
    offset: u32,
    /// True if similarity >= threshold.
    is_match: bool,
    /// Match threshold used.
    threshold: f32,
};

/// Default copyright match threshold.
pub const DEFAULT_MATCH_THRESHOLD: f32 = 0.95;

/// Find the best match of `query` within `reference`.
/// Returns the best MatchResult found.
pub fn findBestMatch(
    query: [*]const u8,
    query_len: u32,
    reference: [*]const u8,
    reference_len: u32,
    threshold: f32,
) MatchResult {
    const short = if (query_len <= reference_len) query else reference;
    const short_len = @min(query_len, reference_len);
    const long = if (query_len <= reference_len) reference else query;
    const long_len = @max(query_len, reference_len);

    if (short_len == 0) {
        return MatchResult{
            .similarity = 0.0,
            .offset = 0,
            .is_match = false,
            .threshold = threshold,
        };
    }

    const max_dist = maxHammingDistance(short_len);
    if (max_dist == 0) {
        return MatchResult{
            .similarity = 1.0,
            .offset = 0,
            .is_match = true,
            .threshold = threshold,
        };
    }

    var best_similarity: f32 = 0.0;
    var best_offset: u32 = 0;

    const max_offset = long_len - short_len + 1;
    var offset: u32 = 0;
    while (offset < max_offset) : (offset += 1) {
        const dist = hamming_distance(short, long + offset, short_len);
        const similarity = 1.0 - @as(f32, @floatFromInt(dist)) / @as(f32, @floatFromInt(max_dist));
        if (similarity > best_similarity) {
            best_similarity = similarity;
            best_offset = offset;
        }
        if (best_similarity >= 1.0) break;
    }

    return MatchResult{
        .similarity = best_similarity,
        .offset = best_offset,
        .is_match = best_similarity >= threshold,
        .threshold = threshold,
    };
}

/// C-compatible compare wrapper.
/// Returns 0 on success, -1 on error.
pub fn compareFingerprints(
    fp_a: [*]const u8,
    len_a: u32,
    fp_b: [*]const u8,
    len_b: u32,
    out_similarity: *f32,
) c_int {
    if (len_a == 0 or len_b == 0) {
        out_similarity.* = 0.0;
        return 0;
    }

    out_similarity.* = fingerprint_similarity(fp_a, len_a, fp_b, len_b);
    return 0;
}

test "hamming_distance same array is zero" {
    const a = [_]u8{ 0xAA, 0x55, 0xFF, 0x00 };
    const dist = hamming_distance(&a, &a, 4);
    try std.testing.expect(dist == 0);
}

test "hamming_distance of complements" {
    const a = [_]u8{ 0x00, 0x00, 0x00, 0x00 };
    const b = [_]u8{ 0x3F, 0x3F, 0x3F, 0x3F }; // 6 bits set in each byte
    const dist = hamming_distance(&a, &b, 4);
    try std.testing.expect(dist == 4 * 6); // 6 differences per byte
}

test "hamming_distance counts bits correctly" {
    const a = [_]u8{ 0x01 }; // bit 0 set
    const b = [_]u8{ 0x02 }; // bit 1 set
    const dist = hamming_distance(&a, &b, 1);
    try std.testing.expect(dist == 2); // two bits differ
}

test "fingerprint_similarity identical" {
    const a = [_]u8{ 0x1A, 0x2B, 0x3C, 0x4D, 0x5E };
    const sim = fingerprint_similarity(&a, 5, &a, 5);
    try std.testing.expectApproxEqAbs(sim, 1.0, 0.001);
}

test "fingerprint_similarity completely different" {
    const a = [_]u8{ 0x00, 0x00, 0x00, 0x00 };
    const b = [_]u8{ 0x3F, 0x3F, 0x3F, 0x3F };
    const sim = fingerprint_similarity(&a, 4, &b, 4);
    try std.testing.expectApproxEqAbs(sim, 0.0, 0.001);
}

test "fingerprint_similarity sliding window" {
    // Reference has pattern embedded in the middle
    var ref = [_]u8{ 0x00, 0x00, 0x1A, 0x2B, 0x3C, 0x00, 0x00 };
    const query = [_]u8{ 0x1A, 0x2B, 0x3C };
    const sim = fingerprint_similarity(&query, 3, &ref, 7);
    try std.testing.expectApproxEqAbs(sim, 1.0, 0.001);
}

test "findBestMatch returns correct offset" {
    const ref = [_]u8{ 0x00, 0x11, 0x22, 0x33, 0x44, 0x55 };
    const query = [_]u8{ 0x33, 0x44, 0x55 };
    const result = findBestMatch(&query, 3, &ref, 6, DEFAULT_MATCH_THRESHOLD);
    try std.testing.expectApproxEqAbs(result.similarity, 1.0, 0.001);
    try std.testing.expect(result.offset == 3);
    try std.testing.expect(result.is_match == true);
}

test "popcount table is correct" {
    try std.testing.expect(POPCOUNT_TABLE[0x00] == 0);
    try std.testing.expect(POPCOUNT_TABLE[0x01] == 1);
    try std.testing.expect(POPCOUNT_TABLE[0x02] == 1);
    try std.testing.expect(POPCOUNT_TABLE[0x03] == 2);
    try std.testing.expect(POPCOUNT_TABLE[0x0F] == 4);
    try std.testing.expect(POPCOUNT_TABLE[0xFF] == 8);
    try std.testing.expect(POPCOUNT_TABLE[0x3F] == 6);
}

test "compareFingerprints C wrapper" {
    const a = [_]u8{ 0x1A, 0x2B, 0x3C, 0x4D };
    const b = [_]u8{ 0x1A, 0x2B, 0x3C, 0x4D };
    var sim: f32 = 0;
    const rc = compareFingerprints(&a, 4, &b, 4, &sim);
    try std.testing.expect(rc == 0);
    try std.testing.expectApproxEqAbs(sim, 1.0, 0.001);
}

test "threshold boundary" {
    const a = [_]u8{ 0x00, 0x00, 0x00, 0x00 };
    const b = [_]u8{ 0x00, 0x00, 0x00, 0x00 };
    const result = findBestMatch(&a, 4, &b, 4, DEFAULT_MATCH_THRESHOLD);
    try std.testing.expect(result.is_match == true);
    try std.testing.expectApproxEqAbs(result.similarity, 1.0, 0.001);
}

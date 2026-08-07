//! Shared inference utilities for pre- and post-processing ML outputs.
//!
//! Contains Non-Maximum Suppression (NMS), softmax activation, and
//! letterboxing helpers used by both visual and audio analysis pipelines.

use ndarray::{Array1, Array2, ArrayView1, ArrayView2, Axis};

// ===========================================================================
// Non-Maximum Suppression (NMS)
// ===========================================================================

/// A detection candidate used during NMS.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectionCandidate {
    pub class_id: usize,
    pub confidence: f64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl DetectionCandidate {
    pub fn new(class_id: usize, confidence: f64, x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            class_id,
            confidence: confidence.clamp(0.0, 1.0),
            x,
            y,
            width,
            height,
        }
    }

    /// Compute intersection-over-union (IoU) with another candidate.
    pub fn iou(&self, other: &DetectionCandidate) -> f64 {
        let x1 = (self.x - self.width / 2.0).max(other.x - other.width / 2.0);
        let y1 = (self.y - self.height / 2.0).max(other.y - other.height / 2.0);
        let x2 = (self.x + self.width / 2.0).min(other.x + other.width / 2.0);
        let y2 = (self.y + self.height / 2.0).min(other.y + other.height / 2.0);

        let inter_width = (x2 - x1).max(0.0);
        let inter_height = (y2 - y1).max(0.0);
        let inter_area = inter_width * inter_height;

        let area_self = self.width * self.height;
        let area_other = other.width * other.height;
        let union_area = area_self + area_other - inter_area;

        if union_area <= 0.0 {
            0.0
        } else {
            inter_area / union_area
        }
    }

    /// Convert center-format box to normalized [x1, y1, x2, y2].
    pub fn to_xyxy(&self) -> [f64; 4] {
        [
            (self.x - self.width / 2.0).clamp(0.0, 1.0),
            (self.y - self.height / 2.0).clamp(0.0, 1.0),
            (self.x + self.width / 2.0).clamp(0.0, 1.0),
            (self.y + self.height / 2.0).clamp(0.0, 1.0),
        ]
    }
}

/// Apply Non-Maximum Suppression (NMS) to a list of detection candidates.
///
/// # Arguments
/// * `candidates` - Raw detection candidates from model output.
/// * `conf_threshold` - Minimum confidence to keep a candidate (0.0..1.0).
/// * `nms_threshold` - IoU threshold above which overlapping boxes are suppressed.
///
/// # Returns
/// Filtered candidates after NMS, sorted by confidence descending.
pub fn non_max_suppression(
    candidates: &[DetectionCandidate],
    conf_threshold: f64,
    nms_threshold: f64,
) -> Vec<DetectionCandidate> {
    // 1. Filter by confidence.
    let mut filtered: Vec<DetectionCandidate> = candidates
        .iter()
        .filter(|c| c.confidence >= conf_threshold)
        .cloned()
        .collect();

    // 2. Sort by confidence descending.
    filtered.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

    // 3. Greedy NMS loop.
    let mut suppressed = vec![false; filtered.len()];
    let mut result = Vec::new();

    for i in 0..filtered.len() {
        if suppressed[i] {
            continue;
        }
        let current = &filtered[i];
        result.push(current.clone());

        for j in (i + 1)..filtered.len() {
            if suppressed[j] {
                continue;
            }
            if filtered[j].class_id == current.class_id && current.iou(&filtered[j]) > nms_threshold {
                suppressed[j] = true;
            }
        }
    }

    result
}

/// Batch NMS: run NMS per-class independently (class-aware NMS).
pub fn class_aware_nms(
    candidates: &[DetectionCandidate],
    conf_threshold: f64,
    nms_threshold: f64,
) -> Vec<DetectionCandidate> {
    let mut all_results = Vec::new();
    let mut class_ids: Vec<usize> = candidates.iter().map(|c| c.class_id).collect();
    class_ids.sort_unstable();
    class_ids.dedup();

    for class_id in class_ids {
        let class_candidates: Vec<DetectionCandidate> = candidates
            .iter()
            .filter(|c| c.class_id == class_id)
            .cloned()
            .collect();
        let mut kept = non_max_suppression(&class_candidates, conf_threshold, nms_threshold);
        all_results.append(&mut kept);
    }

    // Re-sort combined results by confidence.
    all_results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
    all_results
}

// ===========================================================================
// Softmax
// ===========================================================================

/// Compute the softmax of a 1-D array.
pub fn softmax_1d(input: &ArrayView1<f64>) -> Array1<f64> {
    let max_val = input.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let exps: Vec<f64> = input.iter().map(|&x| (x - max_val).exp()).collect();
    let sum_exps: f64 = exps.iter().sum();
    if sum_exps == 0.0 {
        Array1::zeros(input.len())
    } else {
        Array1::from_vec(exps.into_iter().map(|e| e / sum_exps).collect())
    }
}

/// Compute softmax along a specified axis of a 2-D array.
pub fn softmax_2d(input: &ArrayView2<f64>, axis: Axis) -> Array2<f64> {
    let mut output = input.to_owned();
    if axis == Axis(0) {
        for col in 0..input.ncols() {
            let col_view = input.column(col);
            let sm = softmax_1d(&col_view.view());
            for (row, &val) in sm.iter().enumerate() {
                output[[row, col]] = val;
            }
        }
    } else {
        for row in 0..input.nrows() {
            let row_view = input.row(row);
            let sm = softmax_1d(&row_view.view());
            for (col, &val) in sm.iter().enumerate() {
                output[[row, col]] = val;
            }
        }
    }
    output
}

// ===========================================================================
// Letterboxing
// ===========================================================================

/// Compute letterbox parameters to fit an image into a target size
/// while preserving aspect ratio.
#[derive(Debug, Clone, PartialEq)]
pub struct LetterboxParams {
    pub new_width: u32,
    pub new_height: u32,
    pub offset_x: u32,
    pub offset_y: u32,
    pub scale: f64,
}

/// Compute letterbox scaling parameters.
///
/// Fits a `(width x height)` image into `(target_size x target_size)`
/// while preserving aspect ratio, adding padding as needed.
pub fn compute_letterbox(width: u32, height: u32, target_size: u32) -> LetterboxParams {
    let scale = (target_size as f64 / width as f64).min(target_size as f64 / height as f64);
    let new_width = (width as f64 * scale).round() as u32;
    let new_height = (height as f64 * scale).round() as u32;
    let offset_x = ((target_size - new_width) / 2).max(0);
    let offset_y = ((target_size - new_height) / 2).max(0);
    LetterboxParams {
        new_width,
        new_height,
        offset_x,
        offset_y,
        scale,
    }
}

/// Reverse-map normalized coordinates from letterboxed space back to
/// the original image coordinate space.
pub fn reverse_letterbox_coords(
    bbox: &[f64; 4],
    orig_width: u32,
    orig_height: u32,
    target_size: u32,
) -> [f64; 4] {
    let params = compute_letterbox(orig_width, orig_height, target_size);
    [
        ((bbox[0] - params.offset_x as f64) / (params.new_width as f64)).clamp(0.0, 1.0),
        ((bbox[1] - params.offset_y as f64) / (params.new_height as f64)).clamp(0.0, 1.0),
        ((bbox[2] - params.offset_x as f64) / (params.new_width as f64)).clamp(0.0, 1.0),
        ((bbox[3] - params.offset_y as f64) / (params.new_height as f64)).clamp(0.0, 1.0),
    ]
}

// ===========================================================================
// Score utilities
// ===========================================================================

/// Sigmoid activation function.
pub fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// Apply sigmoid to a slice in-place.
pub fn sigmoid_in_place(values: &mut [f64]) {
    for v in values.iter_mut() {
        *v = sigmoid(*v);
    }
}

/// Compute the top-k indices and values from a 1-D array.
pub fn top_k(values: &[f64], k: usize) -> Vec<(usize, f64)> {
    let mut indexed: Vec<(usize, f64)> = values
        .iter()
        .enumerate()
        .map(|(i, &v)| (i, v))
        .collect();
    indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    indexed.into_iter().take(k).collect()
}

/// Clamp a bounding box to [0.0, 1.0] range and ensure x2 > x1, y2 > y1.
pub fn clamp_bbox(x1: f64, y1: f64, x2: f64, y2: f64) -> [f64; 4] {
    let cx1 = x1.clamp(0.0, 1.0);
    let cy1 = y1.clamp(0.0, 1.0);
    let cx2 = x2.clamp(0.0, 1.0).max(cx1 + 0.001);
    let cy2 = y2.clamp(0.0, 1.0).max(cy1 + 0.001);
    [cx1, cy1, cx2, cy2]
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::array;

    #[test]
    fn test_nms_basic() {
        let candidates = vec![
            DetectionCandidate::new(0, 0.9, 0.5, 0.5, 0.2, 0.2),
            DetectionCandidate::new(0, 0.8, 0.52, 0.52, 0.2, 0.2), // overlaps with first
            DetectionCandidate::new(1, 0.85, 0.3, 0.3, 0.1, 0.1), // different class
        ];
        let result = non_max_suppression(&candidates, 0.5, 0.5);
        // Should keep high-conf box and the different-class box.
        assert!(!result.is_empty());
        assert_eq!(result[0].confidence, 0.9);
    }

    #[test]
    fn test_nms_conf_threshold() {
        let candidates = vec![
            DetectionCandidate::new(0, 0.4, 0.5, 0.5, 0.2, 0.2),
            DetectionCandidate::new(0, 0.95, 0.7, 0.7, 0.2, 0.2),
        ];
        let result = non_max_suppression(&candidates, 0.5, 0.5);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].confidence, 0.95);
    }

    #[test]
    fn test_nms_identical_boxes() {
        let candidates = vec![
            DetectionCandidate::new(0, 0.9, 0.5, 0.5, 0.2, 0.2),
            DetectionCandidate::new(0, 0.85, 0.5, 0.5, 0.2, 0.2), // same position
        ];
        let result = non_max_suppression(&candidates, 0.5, 0.5);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_class_aware_nms() {
        let candidates = vec![
            DetectionCandidate::new(0, 0.9, 0.5, 0.5, 0.2, 0.2),
            DetectionCandidate::new(0, 0.75, 0.52, 0.52, 0.2, 0.2),
            DetectionCandidate::new(1, 0.85, 0.5, 0.5, 0.2, 0.2), // same pos, different class
        ];
        let result = class_aware_nms(&candidates, 0.5, 0.5);
        // Should keep best of class 0 and class 1.
        assert!(result.len() >= 2);
    }

    #[test]
    fn test_softmax_1d() {
        let input = array![1.0, 2.0, 3.0];
        let output = softmax_1d(&input.view());
        assert!((output.sum() - 1.0).abs() < 1e-6);
        assert!(output[2] > output[1]);
        assert!(output[1] > output[0]);
    }

    #[test]
    fn test_softmax_1d_uniform() {
        let input = array![0.0, 0.0, 0.0];
        let output = softmax_1d(&input.view());
        assert!((output.sum() - 1.0).abs() < 1e-6);
        assert!((output[0] - 0.3333).abs() < 0.001);
    }

    #[test]
    fn test_softmax_2d_row_axis() {
        let input = array![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]];
        let output = softmax_2d(&input.view(), Axis(1));
        for row in 0..2 {
            let sum: f64 = output.row(row).sum();
            assert!((sum - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_letterbox_params() {
        let params = compute_letterbox(1920, 1080, 640);
        assert_eq!(params.new_height, 640); // height is the limiting dimension
        assert_eq!(params.offset_y, 0);
        assert!(params.offset_x > 0);
        assert!(params.scale > 0.0);
    }

    #[test]
    fn test_letterbox_square_image() {
        let params = compute_letterbox(640, 640, 640);
        assert_eq!(params.new_width, 640);
        assert_eq!(params.new_height, 640);
        assert_eq!(params.offset_x, 0);
        assert_eq!(params.offset_y, 0);
        assert_eq!(params.scale, 1.0);
    }

    #[test]
    fn test_letterbox_portrait() {
        let params = compute_letterbox(1080, 1920, 640);
        assert_eq!(params.new_width, 640);
        assert!(params.offset_y == 0);
        assert!(params.offset_x > 0);
    }

    #[test]
    fn test_reverse_letterbox() {
        let bbox = [100.0, 50.0, 200.0, 150.0];
        let reversed = reverse_letterbox_coords(&bbox, 1920, 1080, 640);
        assert!(reversed[0] >= 0.0 && reversed[0] <= 1.0);
        assert!(reversed[1] >= 0.0 && reversed[1] <= 1.0);
    }

    #[test]
    fn test_sigmoid() {
        assert!((sigmoid(0.0) - 0.5).abs() < 1e-6);
        assert!(sigmoid(5.0) > 0.99);
        assert!(sigmoid(-5.0) < 0.01);
    }

    #[test]
    fn test_sigmoid_in_place() {
        let mut vals = [0.0, 1.0, -1.0, 2.0, -2.0];
        sigmoid_in_place(&mut vals);
        assert!((vals[0] - 0.5).abs() < 1e-6);
        assert!(vals[1] > 0.5);
        assert!(vals[2] < 0.5);
    }

    #[test]
    fn test_top_k() {
        let vals = [0.1, 0.9, 0.3, 0.7, 0.5];
        let top = top_k(&vals, 3);
        assert_eq!(top.len(), 3);
        assert_eq!(top[0].0, 1); // index of 0.9
        assert_eq!(top[0].1, 0.9);
        assert_eq!(top[1].0, 3); // index of 0.7
    }

    #[test]
    fn test_top_k_large_k() {
        let vals = [0.5, 0.3];
        let top = top_k(&vals, 5);
        assert_eq!(top.len(), 2);
    }

    #[test]
    fn test_clamp_bbox() {
        let clamped = clamp_bbox(-0.1, 0.2, 1.2, 0.15);
        assert_eq!(clamped[0], 0.0);
        assert_eq!(clamped[1], 0.2);
        assert_eq!(clamped[2], 1.0);
        assert!(clamped[3] > clamped[1]); // y2 > y1
    }

    #[test]
    fn test_detection_candidate_iou_identical() {
        let a = DetectionCandidate::new(0, 0.9, 0.5, 0.5, 0.2, 0.2);
        let b = DetectionCandidate::new(0, 0.8, 0.5, 0.5, 0.2, 0.2);
        assert!((a.iou(&b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_detection_candidate_iou_no_overlap() {
        let a = DetectionCandidate::new(0, 0.9, 0.1, 0.1, 0.1, 0.1);
        let b = DetectionCandidate::new(0, 0.8, 0.9, 0.9, 0.1, 0.1);
        assert_eq!(a.iou(&b), 0.0);
    }

    #[test]
    fn test_detection_candidate_to_xyxy() {
        let c = DetectionCandidate::new(0, 0.9, 0.5, 0.5, 0.2, 0.2);
        let xyxy = c.to_xyxy();
        assert!((xyxy[0] - 0.4).abs() < 1e-6);
        assert!((xyxy[1] - 0.4).abs() < 1e-6);
        assert!((xyxy[2] - 0.6).abs() < 1e-6);
        assert!((xyxy[3] - 0.6).abs() < 1e-6);
    }

    #[test]
    fn test_detection_candidate_iou_partial() {
        let a = DetectionCandidate::new(0, 0.9, 0.5, 0.5, 0.2, 0.2);
        let b = DetectionCandidate::new(0, 0.8, 0.55, 0.55, 0.2, 0.2);
        let iou = a.iou(&b);
        assert!(iou > 0.0 && iou < 1.0);
    }
}

//! Visual analyzer: YOLO-style object detection, OCR, and scene classification.
//!
//! Uses rule-based heuristics (color analysis, edge detection, contrast scanning)
//! since no real ML model is available.  The public API matches what a real
//! ONNX-based pipeline would expose so swapping in `ort` later is seamless.

use frame_extractor::types::{ExtractorConfig, Frame, PixelFormat};
use sentinel_core::proto::{
    BoundingBox, DetectedObject, DetectedText, FlashingSegment, SceneTransition, Severity,
    Timestamp, ViolationCategory, VisualAnalysis,
};
use sentinel_core::error::SentinelError;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info, instrument, warn};

// ===========================================================================
// VisualAnalyzer
// ===========================================================================

/// Configuration for the visual analysis pipeline.
#[derive(Debug, Clone, PartialEq)]
pub struct VisualAnalyzerConfig {
    /// Confidence threshold for object detection (0.0..1.0).
    pub detection_threshold: f64,
    /// Confidence threshold for OCR text detection.
    pub ocr_threshold: f64,
    /// Whether to enable flashing / seizure-risk detection.
    pub enable_flashing_detection: bool,
    /// Whether to enable scene classification.
    pub enable_scene_classification: bool,
    /// Maximum number of objects to return per frame.
    pub max_objects_per_frame: usize,
    /// Maximum number of text regions to return per frame.
    pub max_text_regions: usize,
}

impl Default for VisualAnalyzerConfig {
    fn default() -> Self {
        VisualAnalyzerConfig {
            detection_threshold: 0.5,
            ocr_threshold: 0.6,
            enable_flashing_detection: true,
            enable_scene_classification: true,
            max_objects_per_frame: 20,
            max_text_regions: 10,
        }
    }
}

impl VisualAnalyzerConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_detection_threshold(mut self, t: f64) -> Self {
        self.detection_threshold = t.clamp(0.0, 1.0);
        self
    }

    pub fn with_flashing(mut self, enable: bool) -> Self {
        self.enable_flashing_detection = enable;
        self
    }

    pub fn with_scene_classification(mut self, enable: bool) -> Self {
        self.enable_scene_classification = enable;
        self
    }
}

/// Visual analysis pipeline using heuristic-based detectors.
pub struct VisualAnalyzer {
    config: VisualAnalyzerConfig,
}

impl VisualAnalyzer {
    /// Create a new visual analyzer.
    pub fn new(config: VisualAnalyzerConfig) -> Self {
        VisualAnalyzer { config }
    }

    // ------------------------------------------------------------------
    // Object Detection Heuristics
    // ------------------------------------------------------------------

    /// Detect objects across a batch of frames using color/edge heuristics.
    ///
    /// Detection rules:
    /// - **Large red regions** (>10% of frame, avg R > 150, R > G+B) → potential blood/violence
    /// - **Face-like regions** (oval-shaped skin-tone clusters) → person
    /// - **Text regions** (high-contrast rectangular zones) → text overlay
    /// - **Bright flashing regions** → potential seizure risk
    /// - **Dark regions with high contrast edges** → potential weapons/objects
    #[instrument(skip(self, frames), fields(frame_count = frames.len()))]
    pub fn detect_objects(&self, frames: &[Frame]) -> Result<Vec<DetectedObject>, SentinelError> {
        let mut all_objects = Vec::new();
        let threshold = self.config.detection_threshold;

        for frame in frames.iter().take(self.config.max_objects_per_frame * 2) {
            if frame.format != PixelFormat::RGB24 {
                warn!("skipping frame {}: unsupported pixel format", frame.frame_number);
                continue;
            }
            if frame.data.is_empty() {
                continue;
            }

            // Run all heuristics on this frame.
            let ts = Timestamp::new(frame.timestamp_ms / 1000.0, frame.frame_number);

            if let Some(obj) = self.detect_large_red_regions(frame, &ts, threshold) {
                all_objects.extend(obj);
            }
            if let Some(obj) = self.detect_face_regions(frame, &ts, threshold) {
                all_objects.extend(obj);
            }
            if let Some(obj) = self.detect_dark_object_regions(frame, &ts, threshold) {
                all_objects.extend(obj);
            }
            if let Some(obj) = self.detect_bright_uniform_regions(frame, &ts, threshold) {
                all_objects.extend(obj);
            }
        }

        // Sort by confidence and cap results.
        all_objects.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
        all_objects.truncate(self.config.max_objects_per_frame * frames.len().max(1));

        info!("detected {} objects across {} frames", all_objects.len(), frames.len());
        Ok(all_objects)
    }

    /// Detect large red regions that may indicate blood or violent content.
    fn detect_large_red_regions(
        &self,
        frame: &Frame,
        ts: &Timestamp,
        threshold: f64,
    ) -> Option<Vec<DetectedObject>> {
        let pixel_count = frame.data.len() / 3;
        if pixel_count == 0 {
            return None;
        }

        let mut red_pixels = 0u64;
        let mut red_total_r = 0u64;
        let mut red_total_g = 0u64;
        let mut red_total_b = 0u64;
        let mut red_min_x = frame.width;
        let mut red_max_x = 0u32;
        let mut red_min_y = frame.height;
        let mut red_max_y = 0u32;

        for y in 0..frame.height {
            for x in 0..frame.width {
                let idx = ((y * frame.width + x) * 3) as usize;
                let r = frame.data[idx] as u32;
                let g = frame.data[idx + 1] as u32;
                let b = frame.data[idx + 2] as u32;

                // Red-dominant pixel: R > 120 and R > G + B
                if r > 120 && r > g + b {
                    red_pixels += 1;
                    red_total_r += r as u64;
                    red_total_g += g as u64;
                    red_total_b += b as u64;
                    red_min_x = red_min_x.min(x);
                    red_max_x = red_max_x.max(x);
                    red_min_y = red_min_y.min(y);
                    red_max_y = red_max_y.max(y);
                }
            }
        }

        let red_ratio = red_pixels as f64 / pixel_count as f64;
        if red_ratio < 0.02 {
            return None;
        }

        let avg_r = if red_pixels > 0 {
            (red_total_r as f64 / red_pixels as f64) / 255.0
        } else {
            0.0
        };
        let avg_g = if red_pixels > 0 {
            (red_total_g as f64 / red_pixels as f64) / 255.0
        } else {
            0.0
        };
        let avg_b = if red_pixels > 0 {
            (red_total_b as f64 / red_pixels as f64) / 255.0
        } else {
            0.0
        };

        // Confidence based on redness intensity and coverage.
        let confidence = ((avg_r * 0.6) + (red_ratio * 4.0) - (avg_g + avg_b) * 0.2)
            .clamp(0.0, 1.0);

        if confidence < threshold {
            return None;
        }

        let w = frame.width as f64;
        let h = frame.height as f64;

        let bbox = BoundingBox::new(
            red_min_x as f64 / w,
            red_min_y as f64 / h,
            red_max_x as f64 / w,
            red_max_y as f64 / h,
        );

        // Determine label based on characteristics.
        let (label, category) = if red_ratio > 0.15 && avg_r > 0.7 {
            ("potential_blood", ViolationCategory::Violence)
        } else if red_ratio > 0.05 {
            ("red_region", ViolationCategory::Unspecified)
        } else {
            ("red_tint", ViolationCategory::Unspecified)
        };

        let obj = DetectedObject::new(label, confidence)
            .with_bbox(bbox)
            .at_timestamp(ts.clone())
            .with_category(category);

        Some(vec![obj])
    }

    /// Detect face-like regions using skin-tone heuristics.
    fn detect_face_regions(
        &self,
        frame: &Frame,
        ts: &Timestamp,
        threshold: f64,
    ) -> Option<Vec<DetectedObject>> {
        let pixel_count = frame.data.len() / 3;
        if pixel_count == 0 {
            return None;
        }

        // Skin tone detector: RGB ranges approximating human skin.
        // R > 95, G > 40, B > 20, and R > G > B, with R - G > 15.
        let mut skin_pixels = 0u64;
        let mut skin_min_x = frame.width;
        let mut skin_max_x = 0u32;
        let mut skin_min_y = frame.height;
        let mut skin_max_y = 0u32;

        // We scan in a coarse grid to be more efficient.
        let step = (frame.width.max(frame.height) / 64).max(1);

        for y in (0..frame.height).step_by(step as usize) {
            for x in (0..frame.width).step_by(step as usize) {
                let idx = ((y * frame.width + x) * 3) as usize;
                let r = frame.data[idx];
                let g = frame.data[idx + 1];
                let b = frame.data[idx + 2];

                if r > 95 && g > 40 && b > 20
                    && r > g && g > b
                    && (r as i16 - g as i16) > 15
                {
                    skin_pixels += 1;
                    skin_min_x = skin_min_x.min(x);
                    skin_max_x = skin_max_x.max(x);
                    skin_min_y = skin_min_y.min(y);
                    skin_max_y = skin_max_y.max(y);
                }
            }
        }

        let skin_ratio = skin_pixels as f64 / (pixel_count as f64 / (step * step) as f64);
        if skin_ratio < 0.01 {
            return None;
        }

        // Estimate face count based on skin pixel coverage.
        let estimated_faces = (skin_ratio * 10.0).round().clamp(1.0, 10.0) as usize;
        let confidence = (skin_ratio * 3.0).clamp(0.0, 1.0);

        if confidence < threshold {
            return None;
        }

        let w = frame.width as f64;
        let h = frame.height as f64;

        let mut objects = Vec::new();
        // Generate bounding boxes for estimated faces, distributed in the skin region.
        for i in 0..estimated_faces.min(5) {
            let frac_x1 = (skin_min_x as f64 + (skin_max_x - skin_min_x) as f64 * i as f64 / estimated_faces.max(1) as f64) / w;
            let frac_y1 = skin_min_y as f64 / h;
            let frac_x2 = (skin_min_x as f64 + (skin_max_x - skin_min_x) as f64 * (i + 1) as f64 / estimated_faces.max(1) as f64) / w;
            let frac_y2 = skin_max_y as f64 / h;

            let face_bbox = BoundingBox::new(
                frac_x1.clamp(0.0, 1.0),
                frac_y1.clamp(0.0, 1.0),
                frac_x2.clamp(0.0, 1.0),
                frac_y2.clamp(0.0, 1.0),
            );

            let obj = DetectedObject::new("person_face", confidence * 0.9)
                .with_bbox(face_bbox)
                .at_timestamp(ts.clone())
                .with_category(ViolationCategory::Unspecified);

            objects.push(obj);
        }

        Some(objects)
    }

    /// Detect dark regions with sharp edges (potential weapons, objects).
    fn detect_dark_object_regions(
        &self,
        frame: &Frame,
        ts: &Timestamp,
        threshold: f64,
    ) -> Option<Vec<DetectedObject>> {
        // Look for dark regions (avg brightness < 80) with high local contrast.
        let mut dark_edge_regions: Vec<(u32, u32, u32, u32, f64)> = Vec::new();

        let grid_size = 32u32;
        let step_x = (frame.width / grid_size).max(1);
        let step_y = (frame.height / grid_size).max(1);

        for gy in 0..grid_size {
            for gx in 0..grid_size {
                let start_x = gx * step_x;
                let end_x = ((gx + 1) * step_x).min(frame.width);
                let start_y = gy * step_y;
                let end_y = ((gy + 1) * step_y).min(frame.height);

                if end_x <= start_x || end_y <= start_y {
                    continue;
                }

                let mut total_brightness = 0u64;
                let mut edge_count = 0u32;
                let mut pixel_count = 0u32;

                for y in start_y..end_y {
                    for x in start_x..end_x {
                        let idx = ((y * frame.width + x) * 3) as usize;
                        let r = frame.data[idx] as u32;
                        let g = frame.data[idx + 1] as u32;
                        let b = frame.data[idx + 2] as u32;
                        let lum = (0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64) as u32;
                        total_brightness += lum as u64;
                        pixel_count += 1;

                        // Simple Sobel-like edge detection (compare with right and below pixels).
                        if x + 1 < end_x && y + 1 < end_y {
                            let idx_r = ((y * frame.width + (x + 1)) * 3) as usize;
                            let idx_d = (((y + 1) * frame.width + x) * 3) as usize;
                            let lum_r = (0.299 * frame.data[idx_r] as f64
                                + 0.587 * frame.data[idx_r + 1] as f64
                                + 0.114 * frame.data[idx_r + 2] as f64) as i32;
                            let lum_d = (0.299 * frame.data[idx_d] as f64
                                + 0.587 * frame.data[idx_d + 1] as f64
                                + 0.114 * frame.data[idx_d + 2] as f64) as i32;
                            let diff_r = (lum as i32 - lum_r).abs();
                            let diff_d = (lum as i32 - lum_d).abs();
                            if diff_r > 30 || diff_d > 30 {
                                edge_count += 1;
                            }
                        }
                    }
                }

                if pixel_count == 0 {
                    continue;
                }

                let avg_brightness = total_brightness as f64 / pixel_count as f64;
                let edge_density = edge_count as f64 / pixel_count as f64;

                // Dark + high edge density = potential object.
                if avg_brightness < 80.0 && edge_density > 0.05 {
                    let conf = ((1.0 - avg_brightness / 255.0) * 0.5 + edge_density * 5.0).clamp(0.0, 1.0);
                    if conf >= threshold * 0.5 {
                        dark_edge_regions.push((start_x, start_y, end_x, end_y, conf));
                    }
                }
            }
        }

        if dark_edge_regions.is_empty() {
            return None;
        }

        // Merge nearby regions and select top ones.
        dark_edge_regions.sort_by(|a, b| b.4.partial_cmp(&a.4).unwrap());
        let top_n = dark_edge_regions.len().min(3);

        let w = frame.width as f64;
        let h = frame.height as f64;
        let mut objects = Vec::new();

        for (sx, sy, ex, ey, conf) in &dark_edge_regions[..top_n] {
            let bbox = BoundingBox::new(
                *sx as f64 / w,
                *sy as f64 / h,
                *ex as f64 / w,
                *ey as f64 / h,
            );

            let obj = DetectedObject::new("dark_object", *conf * 0.7)
                .with_bbox(bbox)
                .at_timestamp(ts.clone())
                .with_category(ViolationCategory::Violence);

            objects.push(obj);
        }

        Some(objects)
    }

    /// Detect bright uniform regions (potential overexposure, studio lighting).
    fn detect_bright_uniform_regions(
        &self,
        frame: &Frame,
        ts: &Timestamp,
        threshold: f64,
    ) -> Option<Vec<DetectedObject>> {
        // Bright uniform regions may indicate studio/stage content.
        let mut bright_pixels = 0u64;
        let mut total_var = 0u64;
        let pixel_count = frame.data.len() / 3;
        if pixel_count == 0 {
            return None;
        }

        for chunk in frame.data.chunks_exact(3) {
            let lum = (0.299 * chunk[0] as f64 + 0.587 * chunk[1] as f64 + 0.114 * chunk[2] as f64) as u32;
            if lum > 200 {
                bright_pixels += 1;
            }
            let var = chunk.iter().map(|&v| {
                let diff = v as i32 - lum as i32;
                (diff * diff) as u64
            }).sum::<u64>() / 3;
            total_var += var;
        }

        let bright_ratio = bright_pixels as f64 / pixel_count as f64;
        let avg_var = total_var as f64 / pixel_count as f64;
        let uniformity = (1.0 - (avg_var.sqrt() / 128.0)).max(0.0);

        let confidence = (bright_ratio * uniformity * 2.0).clamp(0.0, 1.0);
        if confidence < threshold {
            return None;
        }

        let obj = DetectedObject::new("bright_uniform_region", confidence)
            .with_bbox(BoundingBox::new(0.0, 0.0, 1.0, 1.0))
            .at_timestamp(ts.clone())
            .with_category(ViolationCategory::Unspecified);

        Some(vec![obj])
    }

    // ------------------------------------------------------------------
    // Text Detection (OCR placeholder)
    // ------------------------------------------------------------------

    /// Detect text regions by looking for high-contrast rectangular zones.
    ///
    /// Heuristic: scan horizontal band for sharp transitions between
    /// dark and light pixels — characteristic of printed text.
    #[instrument(skip(self, frames), fields(frame_count = frames.len()))]
    pub fn detect_text(&self, frames: &[Frame]) -> Result<Vec<DetectedText>, SentinelError> {
        let mut all_texts = Vec::new();
        let threshold = self.config.ocr_threshold;

        for frame in frames.iter().take(self.config.max_text_regions * 2) {
            if frame.format != PixelFormat::RGB24 || frame.data.is_empty() {
                continue;
            }

            let ts = Timestamp::new(frame.timestamp_ms / 1000.0, frame.frame_number);

            // Scan horizontal bands for high-contrast striping (text lines).
            if let Some(regions) = self.scan_text_regions(frame, &ts, threshold) {
                all_texts.extend(regions);
            }
        }

        all_texts.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
        all_texts.truncate(self.config.max_text_regions * frames.len().max(1));

        info!("detected {} text regions across {} frames", all_texts.len(), frames.len());
        Ok(all_texts)
    }

    /// Scan for text-like regions using horizontal transition density.
    fn scan_text_regions(
        &self,
        frame: &Frame,
        ts: &Timestamp,
        threshold: f64,
    ) -> Option<Vec<DetectedText>> {
        let mut text_regions = Vec::new();
        let band_height = (frame.height / 10).max(8);

        for band_idx in 0..10 {
            let y_start = band_idx * band_height;
            let y_end = (y_start + band_height).min(frame.height);
            if y_end <= y_start {
                continue;
            }

            // Measure horizontal luminance transitions in this band.
            let mut transitions = 0u32;
            let mut total_lum: Vec<f64> = Vec::new();

            for y in y_start..y_end {
                for x in 1..frame.width {
                    let idx_prev = ((y * frame.width + (x - 1)) * 3) as usize;
                    let idx_curr = ((y * frame.width + x) * 3) as usize;
                    let lum_prev = 0.299 * frame.data[idx_prev] as f64
                        + 0.587 * frame.data[idx_prev + 1] as f64
                        + 0.114 * frame.data[idx_prev + 2] as f64;
                    let lum_curr = 0.299 * frame.data[idx_curr] as f64
                        + 0.587 * frame.data[idx_curr + 1] as f64
                        + 0.114 * frame.data[idx_curr + 2] as f64;

                    total_lum.push(lum_curr);

                    if (lum_curr - lum_prev).abs() > 40.0 {
                        transitions += 1;
                    }
                }
            }

            let pixels_in_band = ((y_end - y_start) * (frame.width - 1)) as u32;
            if pixels_in_band == 0 {
                continue;
            }

            let transition_density = transitions as f64 / pixels_in_band as f64;

            // Text typically has 5-30% transition density horizontally.
            if transition_density > 0.03 && transition_density < 0.35 {
                let avg_lum = total_lum.iter().sum::<f64>() / total_lum.len().max(1) as f64;

                // High contrast text detection score.
                let variance = total_lum
                    .iter()
                    .map(|&l| (l - avg_lum).powi(2))
                    .sum::<f64>()
                    / total_lum.len().max(1) as f64;
                let std_dev = variance.sqrt();

                let confidence = (transition_density * 8.0 * (std_dev / 128.0)).clamp(0.0, 1.0);

                if confidence >= threshold {
                    let w = frame.width as f64;
                    let h = frame.height as f64;

                    let bbox = BoundingBox::new(
                        0.05,
                        y_start as f64 / h,
                        0.95,
                        y_end as f64 / h,
                    );

                    // Classify text type based on band position.
                    let text_type = if band_idx < 2 {
                        "potential_title_text"
                    } else if band_idx > 7 {
                        "potential_subtitle_text"
                    } else {
                        "potential_body_text"
                    };

                    let detected = DetectedText::new(text_type, confidence)
                        .with_bbox(bbox)
                        .at_timestamp(ts.clone());

                    text_regions.push(detected);
                }
            }
        }

        if text_regions.is_empty() {
            None
        } else {
            Some(text_regions)
        }
    }

    // ------------------------------------------------------------------
    // Scene Classification
    // ------------------------------------------------------------------

    /// Scene classification result.
    #[derive(Debug, Clone, PartialEq)]
pub struct SceneClassification {
        pub dominant_scene: String,
        pub confidence: f64,
        pub scene_scores: HashMap<String, f64>,
    }

    /// Heuristic scene classification based on color distribution and frame-to-frame motion.
    ///
    /// Analyzes:
    /// - Color histogram to classify scene type (indoor, outdoor, studio, dark, bright).
    /// - Frame-to-frame brightness variance for motion estimation.
    /// - Average luminance for lighting condition.
    #[instrument(skip(self, frames))]
    pub fn classify_scene(&self, frames: &[Frame]) -> Result<SceneClassification, SentinelError> {
        if frames.is_empty() {
            return Ok(SceneClassification {
                dominant_scene: "unknown".into(),
                confidence: 0.0,
                scene_scores: HashMap::new(),
            });
        }

        let mut total_r = 0u64;
        let mut total_g = 0u64;
        let mut total_b = 0u64;
        let mut total_saturation = 0f64;
        let mut total_luminance = 0f64;
        let mut total_variance = 0f64;
        let mut frame_count = 0u64;

        let mut prev_avg_lum = 0f64;
        let mut motion_score = 0f64;

        for frame in frames {
            if frame.format != PixelFormat::RGB24 || frame.data.is_empty() {
                continue;
            }

            let pixel_count = frame.data.len() / 3;
            if pixel_count == 0 {
                continue;
            }

            let mut frame_r = 0u64;
            let mut frame_g = 0u64;
            let mut frame_b = 0u64;
            let mut frame_lum = 0f64;

            for chunk in frame.data.chunks_exact(3) {
                let r = chunk[0] as f64;
                let g = chunk[1] as f64;
                let b = chunk[2] as f64;

                frame_r += r as u64;
                frame_g += g as u64;
                frame_b += b as u64;

                let lum = 0.299 * r + 0.587 * g + 0.114 * b;
                frame_lum += lum;

                // Saturation approximation (max - min) / max.
                let max_c = r.max(g).max(b);
                let min_c = r.min(g).min(b);
                let sat = if max_c > 0.0 { (max_c - min_c) / max_c } else { 0.0 };
                total_saturation += sat;
            }

            let avg_lum = frame_lum / pixel_count as f64;
            total_luminance += avg_lum;
            total_r += frame_r;
            total_g += frame_g;
            total_b += frame_b;

            // Motion = change in average luminance between frames.
            if frame_count > 0 {
                motion_score += (avg_lum - prev_avg_lum).abs();
            }
            prev_avg_lum = avg_lum;

            // Variance within frame.
            let mut frame_var = 0f64;
            for chunk in frame.data.chunks_exact(3) {
                let r = chunk[0] as f64;
                let g = chunk[1] as f64;
                let b = chunk[2] as f64;
                let lum = 0.299 * r + 0.587 * g + 0.114 * b;
                frame_var += (lum - avg_lum).powi(2);
            }
            total_variance += frame_var / pixel_count as f64;

            frame_count += 1;
        }

        if frame_count == 0 {
            return Ok(SceneClassification {
                dominant_scene: "unknown".into(),
                confidence: 0.0,
                scene_scores: HashMap::new(),
            });
        }

        let n = frame_count as f64;
        let avg_lum = total_luminance / n;
        let avg_sat = total_saturation / (n * (frames[0].data.len() / 3) as f64);
        let avg_var = total_variance / n;
        let motion = motion_score / (n - 1.0).max(1.0);

        let total_pixels = n * (frames[0].data.len() / 3) as f64;
        let mean_r = total_r as f64 / total_pixels;
        let mean_g = total_g as f64 / total_pixels;
        let mean_b = total_b as f64 / total_pixels;

        // Heuristic scoring for each scene type.
        let mut scores: HashMap<String, f64> = HashMap::new();

        // Outdoor: high saturation, balanced colors, high luminance variance.
        let outdoor_score = (avg_sat * 0.5 + (avg_var / 5000.0).min(1.0) * 0.3 + (1.0 - (avg_lum / 255.0)) * 0.2).clamp(0.0, 1.0);
        scores.insert("outdoor".into(), outdoor_score);

        // Indoor: lower saturation, warmer tones (more red), lower variance.
        let indoor_score = ((1.0 - avg_sat) * 0.3 + (mean_r / 255.0) * 0.4 + (1.0 - avg_var / 5000.0).min(1.0) * 0.3).clamp(0.0, 1.0);
        scores.insert("indoor".into(), indoor_score);

        // Studio: uniform lighting (low variance), balanced colors, moderate brightness.
        let studio_score = ((1.0 - avg_var / 2000.0).max(0.0) * 0.5 + avg_sat * 0.3 + (1.0 - motion / 20.0).max(0.0) * 0.2).clamp(0.0, 1.0);
        scores.insert("studio".into(), studio_score);

        // Dark / night scene: low average luminance.
        let dark_score = ((1.0 - avg_lum / 128.0).max(0.0) * 0.7 + (avg_var / 3000.0).min(1.0) * 0.3).clamp(0.0, 1.0);
        scores.insert("dark_scene".into(), dark_score);

        // High motion: rapid brightness changes.
        let action_score = ((motion / 30.0).min(1.0) * 0.7 + avg_var / 5000.0 * 0.3).clamp(0.0, 1.0);
        scores.insert("high_motion".into(), action_score);

        // Find dominant scene.
        let (dominant, max_score) = scores
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(k, v)| (k.clone(), *v))
            .unwrap_or_else(|| ("unknown".into(), 0.0));

        Ok(SceneClassification {
            dominant_scene: dominant,
            confidence: max_score,
            scene_scores: scores,
        })
    }

    // ------------------------------------------------------------------
    // Flashing Detection
    // ------------------------------------------------------------------

    /// Detect flashing segments that may trigger photosensitive epilepsy.
    #[instrument(skip(self, frames))]
    pub fn detect_flashing(&self, frames: &[Frame]) -> Result<Vec<FlashingSegment>, SentinelError> {
        if frames.len() < 3 {
            return Ok(Vec::new());
        }

        let mut segments = Vec::new();
        let mut flash_start_idx = 0usize;
        let mut in_flash = false;
        let mut prev_brightness = 0f64;
        let mut flash_count = 0u32;
        let mut max_change = 0f64;
        let mut peak_luminances = Vec::new();

        for (i, frame) in frames.iter().enumerate() {
            let brightness = frame.average_brightness();
            peak_luminances.push(brightness);

            let change = (brightness - prev_brightness).abs();

            if change > 40.0 {
                if !in_flash {
                    in_flash = true;
                    flash_start_idx = i;
                    flash_count = 0;
                    max_change = 0.0;
                }
                flash_count += 1;
                max_change = max_change.max(change);
            } else if in_flash && change < 20.0 {
                // End of flash segment.
                if flash_count >= 2 {
                    let start_frame = frames[flash_start_idx].frame_number;
                    let end_frame = frame.frame_number;
                    let start_sec = frames[flash_start_idx].timestamp_ms / 1000.0;
                    let end_sec = frame.timestamp_ms / 1000.0;
                    let duration = (end_sec - start_sec).max(0.033);
                    let frequency = flash_count as f64 / duration;

                    let severity = if frequency > 20.0 && max_change > 100.0 {
                        Severity::Critical
                    } else if frequency > 10.0 && max_change > 80.0 {
                        Severity::High
                    } else if frequency > 3.0 && max_change > 40.0 {
                        Severity::Medium
                    } else {
                        Severity::Low
                    };

                    let segment = FlashingSegment {
                        start: Timestamp::new(start_sec, start_frame),
                        end: Timestamp::new(end_sec, end_frame),
                        frequency_hz: frequency,
                        severity,
                    };

                    segments.push(segment);
                }
                in_flash = false;
            }

            prev_brightness = brightness;
        }

        info!("detected {} flashing segments", segments.len());
        Ok(segments)
    }

    // ------------------------------------------------------------------
    // Full visual pipeline
    // ------------------------------------------------------------------

    /// Run the complete visual analysis pipeline on a video file.
    ///
    /// This function extracts frames using the frame-extractor, then
    /// runs object detection, text detection, scene classification, and
    /// flashing detection on the extracted frames.
    #[instrument(skip(self, video_path, config))]
    pub fn analyze(
        &self,
        video_path: &Path,
        config: &ExtractorConfig,
    ) -> Result<VisualAnalysis, SentinelError> {
        info!("starting visual analysis for {}", video_path.display());

        // Extract frames using frame-extractor.
        let frames = self.extract_frames(video_path, config)?;

        if frames.is_empty() {
            warn!("no frames extracted from {}", video_path.display());
            return Ok(VisualAnalysis::empty());
        }

        debug!("extracted {} frames", frames.len());

        // Run all analysis sub-pipelines.
        let objects = self.detect_objects(&frames)?;
        let texts = self.detect_text(&frames)?;
        let flashing = if self.config.enable_flashing_detection {
            self.detect_flashing(&frames)?
        } else {
            Vec::new()
        };

        // Scene transitions are detected via frame-to-frame differences.
        let transitions = self.detect_scene_transitions(&frames)?;

        // Build the result.
        let mut analysis = VisualAnalysis {
            objects,
            texts,
            transitions,
            flashing,
            overall_risk_score: 0.0,
        };

        analysis.compute_risk_score();

        info!(
            "visual analysis complete: {} objects, {} texts, {} flashing, score={:.2}",
            analysis.objects.len(),
            analysis.texts.len(),
            analysis.flashing.len(),
            analysis.overall_risk_score
        );

        Ok(analysis)
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    /// Extract frames from a video file using the frame-extractor crate.
    ///
    /// Uses tokio's block_in_place to run the async frame extraction
    /// and collect all frames from the channel into a Vec.
    fn extract_frames(
        &self,
        video_path: &Path,
        config: &ExtractorConfig,
    ) -> Result<Vec<Frame>, SentinelError> {
        use frame_extractor::FrameExtractor;

        let path = video_path.to_path_buf();
        let cfg = config.clone();

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::try_current()
                .map_err(|e| SentinelError::io("runtime", e.to_string()))?
                .block_on(async {
                    let extractor = FrameExtractor::new(cfg).map_err(|e| {
                        SentinelError::io("extractor", format!("{:?}", e))
                    })?;

                    let mut rx = extractor
                        .extract_frames(&path)
                        .await
                        .map_err(|e| SentinelError::FFmpeg {
                            stage: "extract_frames".into(),
                            stderr: format!("{:?}", e),
                        })?;

                    let mut all_frames = Vec::new();
                    while let Some(batch) = rx.recv().await {
                        for frame in batch.frames {
                            all_frames.push(frame);
                        }
                        if batch.is_last {
                            break;
                        }
                    }

                    Ok(all_frames)
                })
        })
    }

    /// Detect scene transitions by measuring frame-to-frame brightness changes.
    fn detect_scene_transitions(
        &self,
        frames: &[Frame],
    ) -> Result<Vec<SceneTransition>, SentinelError> {
        if frames.len() < 2 {
            return Ok(Vec::new());
        }

        let mut transitions = Vec::new();
        let threshold = 0.30; // 30% brightness change = scene cut.

        for window in frames.windows(2) {
            let prev = &window[0];
            let curr = &window[1];

            let prev_brightness = prev.average_brightness();
            let curr_brightness = curr.average_brightness();

            if prev_brightness < f64::EPSILON {
                continue;
            }

            let change_ratio = (curr_brightness - prev_brightness).abs() / prev_brightness;

            if change_ratio > threshold {
                let severity = if change_ratio > 0.6 {
                    Severity::High
                } else if change_ratio > 0.4 {
                    Severity::Medium
                } else {
                    Severity::Low
                };

                let ts = Timestamp::new(curr.timestamp_ms / 1000.0, curr.frame_number);
                let transition_type = if change_ratio > 0.5 {
                    "hard_cut"
                } else {
                    "soft_transition"
                };

                transitions.push(SceneTransition::new(
                    transition_type,
                    ts,
                    severity,
                ));
            }
        }

        Ok(transitions)
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_solid_frame(r: u8, g: u8, b: u8, w: u32, h: u32, ts_ms: f64, frame_no: u64) -> Frame {
        let data = vec![r, g, b].into_iter().cycle().take((w * h * 3) as usize).collect();
        Frame::new(data, w, h, ts_ms, frame_no, PixelFormat::RGB24, "/tmp/test.mp4")
    }

    fn make_gradient_frame(w: u32, h: u32, ts_ms: f64, frame_no: u64) -> Frame {
        let mut data = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                let val = ((x + y) % 256) as u8;
                data.push(val);
                data.push(val / 2);
                data.push(val / 3);
            }
        }
        Frame::new(data, w, h, ts_ms, frame_no, PixelFormat::RGB24, "/tmp/test.mp4")
    }

    #[test]
    fn test_config_default() {
        let cfg = VisualAnalyzerConfig::default();
        assert_eq!(cfg.detection_threshold, 0.5);
        assert!(cfg.enable_flashing_detection);
        assert!(cfg.enable_scene_classification);
    }

    #[test]
    fn test_config_builder() {
        let cfg = VisualAnalyzerConfig::new()
            .with_detection_threshold(0.7)
            .with_flashing(false);
        assert_eq!(cfg.detection_threshold, 0.7);
        assert!(!cfg.enable_flashing_detection);
    }

    #[test]
    fn test_detect_objects_empty() {
        let analyzer = VisualAnalyzer::new(VisualAnalyzerConfig::default());
        let objects = analyzer.detect_objects(&[]).unwrap();
        assert!(objects.is_empty());
    }

    #[test]
    fn test_detect_objects_red_frame() {
        let analyzer = VisualAnalyzer::new(VisualAnalyzerConfig::default());
        let frames = vec![make_solid_frame(200, 30, 30, 64, 64, 0.0, 1)];
        let objects = analyzer.detect_objects(&frames).unwrap();
        // Should detect a red region.
        assert!(!objects.is_empty());
        assert!(objects.iter().any(|o| o.label.contains("red") || o.label.contains("blood")));
    }

    #[test]
    fn test_detect_objects_normal_frame() {
        let analyzer = VisualAnalyzer::new(VisualAnalyzerConfig::default());
        let frames = vec![make_solid_frame(128, 128, 128, 64, 64, 0.0, 1)];
        let objects = analyzer.detect_objects(&frames).unwrap();
        // Gray frame should not trigger many detections.
        // Face detection might trigger on skin-tone-like gray.
        assert!(objects.len() < 10);
    }

    #[test]
    fn test_detect_objects_dark_frame() {
        let analyzer = VisualAnalyzer::new(VisualAnalyzerConfig::default());
        let frames = vec![make_solid_frame(20, 20, 20, 64, 64, 0.0, 1)];
        let objects = analyzer.detect_objects(&frames).unwrap();
        // Dark frame with no edges — may still detect dark_object based on edge heuristics.
        assert!(objects.len() < 10);
    }

    #[test]
    fn test_detect_text_empty() {
        let analyzer = VisualAnalyzer::new(VisualAnalyzerConfig::default());
        let texts = analyzer.detect_text(&[]).unwrap();
        assert!(texts.is_empty());
    }

    #[test]
    fn test_detect_text_high_contrast() {
        let analyzer = VisualAnalyzer::new(VisualAnalyzerConfig::default());
        // Create a frame with alternating bright/dark stripes (high horizontal transitions).
        let w = 128u32;
        let h = 32u32;
        let mut data = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                if x % 4 < 2 {
                    data.push(255);
                    data.push(255);
                    data.push(255);
                } else {
                    data.push(0);
                    data.push(0);
                    data.push(0);
                }
            }
        }
        let frame = Frame::new(data, w, h, 0.0, 1, PixelFormat::RGB24, "/tmp/test.mp4");
        let texts = analyzer.detect_text(&[frame]).unwrap();
        // Should detect text-like patterns.
        assert!(!texts.is_empty());
    }

    #[test]
    fn test_classify_scene_empty() {
        let analyzer = VisualAnalyzer::new(VisualAnalyzerConfig::default());
        let scene = analyzer.classify_scene(&[]).unwrap();
        assert_eq!(scene.dominant_scene, "unknown");
        assert_eq!(scene.confidence, 0.0);
    }

    #[test]
    fn test_classify_scene_dark() {
        let analyzer = VisualAnalyzer::new(VisualAnalyzerConfig::default());
        let frames = vec![
            make_solid_frame(10, 10, 15, 64, 64, 0.0, 1),
            make_solid_frame(12, 8, 14, 64, 64, 33.0, 2),
        ];
        let scene = analyzer.classify_scene(&frames).unwrap();
        assert_eq!(scene.dominant_scene, "dark_scene");
        assert!(scene.confidence > 0.0);
    }

    #[test]
    fn test_classify_scene_bright() {
        let analyzer = VisualAnalyzer::new(VisualAnalyzerConfig::default());
        let frames = vec![
            make_solid_frame(240, 240, 240, 64, 64, 0.0, 1),
            make_solid_frame(245, 245, 245, 64, 64, 33.0, 2),
        ];
        let scene = analyzer.classify_scene(&frames).unwrap();
        // Bright uniform scenes may classify as studio or outdoor.
        assert!(scene.scene_scores.contains_key("studio"));
        assert!(scene.confidence > 0.0);
    }

    #[test]
    fn test_detect_flashing_insufficient_frames() {
        let analyzer = VisualAnalyzer::new(VisualAnalyzerConfig::default());
        let frames = vec![make_solid_frame(128, 128, 128, 64, 64, 0.0, 1)];
        let flashing = analyzer.detect_flashing(&frames).unwrap();
        assert!(flashing.is_empty());
    }

    #[test]
    fn test_detect_flashing_detected() {
        let analyzer = VisualAnalyzer::new(VisualAnalyzerConfig::default());
        // Create alternating bright/dark frames.
        let mut frames = Vec::new();
        for i in 0..12 {
            let brightness = if i % 2 == 0 { 255u8 } else { 10u8 };
            frames.push(make_solid_frame(brightness, brightness, brightness, 64, 64, i as f64 * 33.0, i as u64));
        }
        let flashing = analyzer.detect_flashing(&frames).unwrap();
        assert!(!flashing.is_empty());
        // Flashing segments should have severity >= Low.
        for seg in &flashing {
            assert!(!matches!(seg.severity, Severity::None));
        }
    }

    #[test]
    fn test_detect_flashing_no_flash() {
        let analyzer = VisualAnalyzer::new(VisualAnalyzerConfig::default());
        // Gradual brightness change — no flashing.
        let mut frames = Vec::new();
        for i in 0..12 {
            let b = (100 + i * 5) as u8;
            frames.push(make_solid_frame(b, b, b, 64, 64, i as f64 * 33.0, i as u64));
        }
        let flashing = analyzer.detect_flashing(&frames).unwrap();
        assert!(flashing.is_empty());
    }

    #[test]
    fn test_scene_transitions_empty() {
        let analyzer = VisualAnalyzer::new(VisualAnalyzerConfig::default());
        let transitions = analyzer.detect_scene_transitions(&[]).unwrap();
        assert!(transitions.is_empty());
    }

    #[test]
    fn test_scene_transitions_detected() {
        let analyzer = VisualAnalyzer::new(VisualAnalyzerConfig::default());
        // Dramatic brightness change between frames.
        let frames = vec![
            make_solid_frame(250, 250, 250, 64, 64, 0.0, 1),
            make_solid_frame(250, 250, 250, 64, 64, 33.0, 2),
            make_solid_frame(20, 20, 20, 64, 64, 66.0, 3),
            make_solid_frame(20, 20, 20, 64, 64, 100.0, 4),
        ];
        let transitions = analyzer.detect_scene_transitions(&frames).unwrap();
        assert!(!transitions.is_empty());
    }

    #[test]
    fn test_analyze_with_empty_video_path() {
        let analyzer = VisualAnalyzer::new(VisualAnalyzerConfig::default());
        let config = ExtractorConfig::new();
        // A non-existent path will fail during frame extraction.
        let result = analyzer.analyze(Path::new("/nonexistent/file.mp4"), &config);
        assert!(result.is_err() || result.unwrap().objects.is_empty());
    }

    #[test]
    fn test_scene_classification_scores_sum() {
        let analyzer = VisualAnalyzer::new(VisualAnalyzerConfig::default());
        let frames = vec![
            make_gradient_frame(64, 64, 0.0, 1),
            make_gradient_frame(64, 64, 33.0, 2),
        ];
        let scene = analyzer.classify_scene(&frames).unwrap();
        // Each score should be in [0, 1].
        for (key, score) in &scene.scene_scores {
            assert!(*score >= 0.0 && *score <= 1.0, "{} score out of range: {}", key, score);
        }
        assert!(scene.scene_scores.len() >= 4);
    }

    #[test]
    fn test_detect_text_uniform() {
        let analyzer = VisualAnalyzer::new(VisualAnalyzerConfig::default());
        // Uniform frame should have no text.
        let frames = vec![make_solid_frame(128, 128, 128, 128, 128, 0.0, 1)];
        let texts = analyzer.detect_text(&frames).unwrap();
        // No high-contrast transitions in uniform frame.
        assert!(texts.is_empty());
    }

    #[test]
    fn test_detect_objects_gradient() {
        let analyzer = VisualAnalyzer::new(VisualAnalyzerConfig::default());
        let frames = vec![make_gradient_frame(64, 64, 0.0, 1)];
        let objects = analyzer.detect_objects(&frames).unwrap();
        // Gradient frames should not produce many high-confidence detections.
        assert!(objects.len() < 15);
    }

    #[test]
    fn test_face_detection_skin_tone() {
        let analyzer = VisualAnalyzer::new(VisualAnalyzerConfig::default());
        // Skin-tone-ish frame (warm, slightly red).
        let mut data = Vec::with_capacity(64 * 64 * 3);
        for _ in 0..(64 * 64) {
            data.push(180u8); // R
            data.push(140u8); // G
            data.push(110u8); // B
        }
        let frame = Frame::new(data, 64, 64, 0.0, 1, PixelFormat::RGB24, "/tmp/test.mp4");
        let objects = analyzer.detect_objects(&[frame]).unwrap();
        // Should detect at least one face region.
        assert!(
            objects.iter().any(|o| o.label.contains("face")),
            "expected face detection in skin-tone frame, got: {:?}",
            objects.iter().map(|o| &o.label).collect::<Vec<_>>()
        );
    }
}

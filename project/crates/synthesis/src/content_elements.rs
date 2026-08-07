//! Content element extraction and representation.
//!
//! Defines the `ContentElement` enum — the unified representation of any
//! piece of content detected across visual, audio, text, or metadata modalities.
//! Each variant carries modality-specific fields and implements shared
//! behaviour (`risk_weight`, `timestamp`, `to_string`).

use serde::{Deserialize, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------
// BoundingBox — normalised coordinates `0.0..=1.0`
// ---------------------------------------------------------------------------

/// A normalised axis-aligned bounding box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

impl BoundingBox {
    pub fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        BoundingBox { x1, y1, x2, y2 }
    }

    pub fn width(&self) -> f64 {
        (self.x2 - self.x1).abs()
    }

    pub fn height(&self) -> f64 {
        (self.y2 - self.y1).abs()
    }

    pub fn area(&self) -> f64 {
        self.width() * self.height()
    }

    pub fn is_valid(&self) -> bool {
        self.x1 >= 0.0
            && self.y1 >= 0.0
            && self.x2 <= 1.0
            && self.y2 <= 1.0
            && self.x2 > self.x1
            && self.y2 > self.y1
    }
}

impl Default for BoundingBox {
    fn default() -> Self {
        BoundingBox::new(0.0, 0.0, 1.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// Timestamp — seconds + frame number
// ---------------------------------------------------------------------------

/// A point in the video timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Timestamp {
    pub seconds: f64,
    pub frame_number: u64,
}

impl Timestamp {
    pub fn new(seconds: f64, frame_number: u64) -> Self {
        Timestamp {
            seconds,
            frame_number,
        }
    }

    /// Format seconds as `HH:MM:SS.mmm`.
    pub fn formatted(&self) -> String {
        let total_secs = self.seconds as u64;
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let secs = total_secs % 60;
        let millis = ((self.seconds.fract()) * 1000.0) as u32;
        format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, secs, millis)
    }
}

impl Default for Timestamp {
    fn default() -> Self {
        Timestamp::new(0.0, 0)
    }
}

// ---------------------------------------------------------------------------
// RiskIndicator — a structured risk signal attached to a content element
// ---------------------------------------------------------------------------

/// A single risk indicator found within a content element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskIndicator {
    pub label: String,
    pub weight: f64,
    pub category: RiskCategory,
}

impl RiskIndicator {
    pub fn new<S: Into<String>>(label: S, weight: f64, category: RiskCategory) -> Self {
        RiskIndicator {
            label: label.into(),
            weight: weight.clamp(0.0, 1.0),
            category,
        }
    }
}

/// Coarse risk category used by the policy engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RiskCategory {
    Violence,
    Adult,
    Hate,
    Harmful,
    Misinformation,
    Copyright,
    Spam,
    None,
}

impl RiskCategory {
    /// Base risk multiplier for the category.
    pub fn base_weight(&self) -> f64 {
        match self {
            RiskCategory::Violence => 0.85,
            RiskCategory::Adult => 0.90,
            RiskCategory::Hate => 0.95,
            RiskCategory::Harmful => 0.88,
            RiskCategory::Misinformation => 0.60,
            RiskCategory::Copyright => 0.50,
            RiskCategory::Spam => 0.30,
            RiskCategory::None => 0.0,
        }
    }
}

impl fmt::Display for RiskCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            RiskCategory::Violence => "violence",
            RiskCategory::Adult => "adult",
            RiskCategory::Hate => "hate",
            RiskCategory::Harmful => "harmful",
            RiskCategory::Misinformation => "misinformation",
            RiskCategory::Copyright => "copyright",
            RiskCategory::Spam => "spam",
            RiskCategory::None => "none",
        })
    }
}

// ---------------------------------------------------------------------------
// ContentElement — unified content representation
// ---------------------------------------------------------------------------

/// A single content element extracted from any analysis modality.
///
/// This is the central abstraction used by the synthesis engine.  Every
/// detected object, transcript segment, OCR result, or metadata field is
/// normalised into one of these variants so that downstream modules can
/// operate on a homogeneous stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ContentElement {
    /// An element originating from the visual-analysis pipeline.
    Visual {
        label: String,
        confidence: f64,
        timestamp: Timestamp,
        bbox: BoundingBox,
        risk_indicators: Vec<RiskIndicator>,
    },
    /// An element originating from the audio-analysis pipeline.
    Audio {
        event_type: String,
        confidence: f64,
        timestamp: Timestamp,
        transcript: Option<String>,
        risk_indicators: Vec<RiskIndicator>,
    },
    /// Text extracted via OCR or other text-detection pipelines.
    Text {
        content: String,
        source: TextSource,
        confidence: f64,
        risk_indicators: Vec<RiskIndicator>,
    },
    /// Metadata-derived element (title, description, tags, etc.).
    Metadata {
        field: String,
        value: String,
        risk_indicators: Vec<RiskIndicator>,
    },
}

/// Where a `Text` element came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TextSource {
    Ocr,
    Transcript,
    Embedded,
    Caption,
}

impl fmt::Display for TextSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            TextSource::Ocr => "ocr",
            TextSource::Transcript => "transcript",
            TextSource::Embedded => "embedded",
            TextSource::Caption => "caption",
        })
    }
}

// ---------------------------------------------------------------------------
// ContentElement — methods
// ---------------------------------------------------------------------------

impl ContentElement {
    /// Compute the weighted risk score for this element.
    ///
    /// The score is the product of:
    /// - the element's own confidence,
    /// - the sum of its risk-indicator weights,
    /// - capped at `1.0`.
    pub fn risk_weight(&self) -> f64 {
        let (confidence, indicators) = match self {
            ContentElement::Visual {
                confidence,
                risk_indicators,
                ..
            } => (*confidence, risk_indicators),
            ContentElement::Audio {
                confidence,
                risk_indicators,
                ..
            } => (*confidence, risk_indicators),
            ContentElement::Text {
                confidence,
                risk_indicators,
                ..
            } => (*confidence, risk_indicators),
            ContentElement::Metadata {
                risk_indicators, ..
            } => (1.0, risk_indicators),
        };

        let indicator_sum: f64 = indicators
            .iter()
            .map(|ri| ri.weight * ri.category.base_weight())
            .sum();

        (confidence * (1.0 + indicator_sum)).min(1.0)
    }

    /// Return the timestamp associated with this element, if any.
    pub fn timestamp(&self) -> Option<&Timestamp> {
        match self {
            ContentElement::Visual { timestamp, .. } => Some(timestamp),
            ContentElement::Audio { timestamp, .. } => Some(timestamp),
            ContentElement::Text { .. } => None,
            ContentElement::Metadata { .. } => None,
        }
    }

    /// Return the timestamp in seconds as `f64`, or `0.0` for atemporal elements.
    pub fn timestamp_seconds(&self) -> f64 {
        self.timestamp().map_or(0.0, |ts| ts.seconds)
    }

    /// Return the label / event-type / content of this element as a display
    /// string.
    pub fn to_display_string(&self) -> String {
        match self {
            ContentElement::Visual { label, .. } => format!("[VISUAL] {}", label),
            ContentElement::Audio { event_type, transcript, .. } => {
                if let Some(tr) = transcript {
                    format!("[AUDIO] {} | {}", event_type, tr)
                } else {
                    format!("[AUDIO] {}", event_type)
                }
            }
            ContentElement::Text { content, source, .. } => {
                format!("[TEXT:{}] {}", source, content)
            }
            ContentElement::Metadata { field, value, .. } => {
                format!("[META:{}] {}", field, value)
            }
        }
    }

    /// Return all risk indicator labels as a flat list.
    pub fn risk_labels(&self) -> Vec<&str> {
        match self {
            ContentElement::Visual { risk_indicators, .. }
            | ContentElement::Audio { risk_indicators, .. }
            | ContentElement::Text { risk_indicators, .. }
            | ContentElement::Metadata { risk_indicators, .. } => {
                risk_indicators.iter().map(|ri| ri.label.as_str()).collect()
            }
        }
    }

    /// Return the confidence score for this element.
    pub fn confidence(&self) -> f64 {
        match self {
            ContentElement::Visual { confidence, .. } => *confidence,
            ContentElement::Audio { confidence, .. } => *confidence,
            ContentElement::Text { confidence, .. } => *confidence,
            ContentElement::Metadata { .. } => 1.0,
        }
    }

    // ------------------------------------------------------------------
    // Convenience constructors
    // ------------------------------------------------------------------

    /// Build a `Visual` element.
    pub fn visual<S: Into<String>>(
        label: S,
        confidence: f64,
        timestamp: Timestamp,
        bbox: BoundingBox,
    ) -> Self {
        ContentElement::Visual {
            label: label.into(),
            confidence: confidence.clamp(0.0, 1.0),
            timestamp,
            bbox,
            risk_indicators: Vec::new(),
        }
    }

    /// Build an `Audio` element.
    pub fn audio<S: Into<String>>(
        event_type: S,
        confidence: f64,
        timestamp: Timestamp,
        transcript: Option<String>,
    ) -> Self {
        ContentElement::Audio {
            event_type: event_type.into(),
            confidence: confidence.clamp(0.0, 1.0),
            timestamp,
            transcript,
            risk_indicators: Vec::new(),
        }
    }

    /// Build a `Text` element.
    pub fn text<S: Into<String>>(content: S, source: TextSource, confidence: f64) -> Self {
        ContentElement::Text {
            content: content.into(),
            source,
            confidence: confidence.clamp(0.0, 1.0),
            risk_indicators: Vec::new(),
        }
    }

    /// Build a `Metadata` element.
    pub fn metadata<S1: Into<String>, S2: Into<String>>(field: S1, value: S2) -> Self {
        ContentElement::Metadata {
            field: field.into(),
            value: value.into(),
            risk_indicators: Vec::new(),
        }
    }

    // ------------------------------------------------------------------
    // Builder-style setters
    // ------------------------------------------------------------------

    /// Add a risk indicator to this element (consumes and returns `Self`).
    pub fn with_risk_indicator(mut self, indicator: RiskIndicator) -> Self {
        match &mut self {
            ContentElement::Visual { risk_indicators, .. }
            | ContentElement::Audio { risk_indicators, .. }
            | ContentElement::Text { risk_indicators, .. }
            | ContentElement::Metadata { risk_indicators, .. } => {
                risk_indicators.push(indicator);
            }
        }
        self
    }
}

impl fmt::Display for ContentElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_display_string())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bbox_validity() {
        let valid = BoundingBox::new(0.1, 0.1, 0.9, 0.9);
        assert!(valid.is_valid());
        assert!((valid.area() - 0.64).abs() < 1e-10);

        let invalid = BoundingBox::new(0.9, 0.1, 0.1, 0.9);
        assert!(!invalid.is_valid());
    }

    #[test]
    fn timestamp_formatting() {
        let ts = Timestamp::new(3661.123, 100);
        assert_eq!(ts.formatted(), "01:01:01.123");
    }

    #[test]
    fn risk_category_weights() {
        assert!(RiskCategory::Hate.base_weight() > RiskCategory::Violence.base_weight());
        assert!(RiskCategory::Violence.base_weight() > RiskCategory::Spam.base_weight());
        assert_eq!(RiskCategory::None.base_weight(), 0.0);
    }

    #[test]
    fn content_element_visual_risk() {
        let elem = ContentElement::visual(
            "weapon",
            0.9,
            Timestamp::new(10.0, 300),
            BoundingBox::new(0.2, 0.2, 0.8, 0.8),
        )
        .with_risk_indicator(RiskIndicator::new("firearm", 0.8, RiskCategory::Violence));

        assert!(elem.risk_weight() > 0.0);
        assert!(elem.risk_weight() <= 1.0);
        assert_eq!(elem.timestamp_seconds(), 10.0);
        assert!(elem.to_display_string().contains("weapon"));
    }

    #[test]
    fn content_element_audio_risk() {
        let elem = ContentElement::audio(
            "gunshot",
            0.85,
            Timestamp::new(5.0, 150),
            Some("loud bang".into()),
        )
        .with_risk_indicator(RiskIndicator::new("violence", 0.9, RiskCategory::Violence));

        assert!(elem.risk_weight() > 0.0);
        assert_eq!(elem.timestamp_seconds(), 5.0);
        assert!(elem.to_display_string().contains("gunshot"));
    }

    #[test]
    fn content_element_text_no_timestamp() {
        let elem = ContentElement::text("hello world", TextSource::Ocr, 0.95);
        assert_eq!(elem.timestamp_seconds(), 0.0);
        assert_eq!(elem.timestamp(), None);
        assert!(elem.to_display_string().contains("OCR"));
    }

    #[test]
    fn content_element_metadata_risk() {
        let elem = ContentElement::metadata("title", "Clickbait Title!!!")
            .with_risk_indicator(RiskIndicator::new("clickbait", 0.7, RiskCategory::Spam));

        assert!(elem.risk_weight() > 0.0);
        assert!(elem.to_display_string().contains("Clickbait"));
    }

    #[test]
    fn risk_labels_collection() {
        let elem = ContentElement::visual("fight", 0.8, Timestamp::new(1.0, 30), BoundingBox::default())
            .with_risk_indicator(RiskIndicator::new("violence", 0.7, RiskCategory::Violence))
            .with_risk_indicator(RiskIndicator::new("blood", 0.5, RiskCategory::Harmful));

        let labels = elem.risk_labels();
        assert_eq!(labels.len(), 2);
        assert!(labels.contains(&"violence"));
        assert!(labels.contains(&"blood"));
    }

    #[test]
    fn confidence_clamping() {
        let elem = ContentElement::visual("x", 1.5, Timestamp::new(0.0, 0), BoundingBox::default());
        assert_eq!(elem.confidence(), 1.0);
    }

    #[test]
    fn text_source_display() {
        assert_eq!(format!("{}", TextSource::Ocr), "ocr");
        assert_eq!(format!("{}", TextSource::Transcript), "transcript");
    }
}

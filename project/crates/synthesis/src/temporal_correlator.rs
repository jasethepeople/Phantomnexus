//! Temporal correlator — detects temporal patterns across modalities.
//!
//! Analyses timestamps from visual and audio analysis results to identify
//! six event patterns: escalation, repetition, coincidence, cluster,
//! intermittent, and sustained.  Produces a risk timeline with timestamps.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// TemporalCorrelation — a detected temporal pattern
// ---------------------------------------------------------------------------

/// A single temporal correlation event detected between visual and audio
/// analysis streams.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemporalCorrelation {
    /// The kind of temporal pattern detected.
    pub pattern: TemporalPattern,
    /// Human-readable description.
    pub description: String,
    /// Start timestamp in seconds.
    pub start_seconds: f64,
    /// End timestamp in seconds.
    pub end_seconds: f64,
    /// Confidence score (0.0..1.0).
    pub confidence: f64,
    /// Risk contribution score (0.0..100.0).
    pub risk_score: f64,
    /// Associated visual event labels.
    pub visual_events: Vec<String>,
    /// Associated audio event labels.
    pub audio_events: Vec<String>,
}

impl TemporalCorrelation {
    pub fn new(
        pattern: TemporalPattern,
        description: impl Into<String>,
        start_seconds: f64,
        end_seconds: f64,
        confidence: f64,
        risk_score: f64,
    ) -> Self {
        TemporalCorrelation {
            pattern,
            description: description.into(),
            start_seconds,
            end_seconds,
            confidence: confidence.clamp(0.0, 1.0),
            risk_score: risk_score.clamp(0.0, 100.0),
            visual_events: Vec::new(),
            audio_events: Vec::new(),
        }
    }

    pub fn with_visual_events(mut self, events: Vec<String>) -> Self {
        self.visual_events = events;
        self
    }

    pub fn with_audio_events(mut self, events: Vec<String>) -> Self {
        self.audio_events = events;
        self
    }

    /// Duration of the correlation window in seconds.
    pub fn duration_seconds(&self) -> f64 {
        (self.end_seconds - self.start_seconds).max(0.0)
    }
}

// ---------------------------------------------------------------------------
// TemporalPattern — six supported pattern types
// ---------------------------------------------------------------------------

/// The six temporal patterns the correlator can detect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemporalPattern {
    /// Risk increases monotonically over a time window.
    Escalation,
    /// The same event type occurs multiple times in a window.
    Repetition,
    /// Visual and audio events occur at the same timestamp.
    Coincidence,
    /// Multiple events cluster tightly in a short window.
    Cluster,
    /// Events occur sporadically with irregular gaps.
    Intermittent,
    /// Events persist continuously over a long duration.
    Sustained,
}

impl TemporalPattern {
    pub fn label(&self) -> &'static str {
        match self {
            TemporalPattern::Escalation => "escalation",
            TemporalPattern::Repetition => "repetition",
            TemporalPattern::Coincidence => "coincidence",
            TemporalPattern::Cluster => "cluster",
            TemporalPattern::Intermittent => "intermittent",
            TemporalPattern::Sustained => "sustained",
        }
    }

    /// Base risk multiplier for this pattern type.
    pub fn risk_multiplier(&self) -> f64 {
        match self {
            // Escalating risk is most concerning — it suggests building tension.
            TemporalPattern::Escalation => 1.5,
            // Repetition reinforces a theme but is moderate risk.
            TemporalPattern::Repetition => 1.2,
            // Coincidence (audio+visual alignment) confirms the event.
            TemporalPattern::Coincidence => 1.4,
            // Cluster suggests a concentrated problematic segment.
            TemporalPattern::Cluster => 1.3,
            // Intermittent is harder to assess — moderate.
            TemporalPattern::Intermittent => 1.0,
            // Sustained events are significant but predictable.
            TemporalPattern::Sustained => 1.1,
        }
    }
}

// ---------------------------------------------------------------------------
// Internal: timestamped event representations
// ---------------------------------------------------------------------------

/// A simplified event extracted from analysis results for temporal processing.
#[derive(Debug, Clone)]
struct TimedEvent {
    timestamp: f64,
    label: String,
    modality: Modality,
    risk_weight: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Modality {
    Visual,
    Audio,
}

// ---------------------------------------------------------------------------
// RiskTimelineEntry — a point on the risk timeline
// ---------------------------------------------------------------------------

/// A single entry in the aggregated risk timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskTimelineEntry {
    pub timestamp_seconds: f64,
    pub aggregated_risk: f64,
    pub event_count: usize,
    pub labels: Vec<String>,
}

// ---------------------------------------------------------------------------
// TemporalCorrelator
// ---------------------------------------------------------------------------

/// Correlates visual and audio events over time to detect temporal patterns.
#[derive(Debug, Clone)]
pub struct TemporalCorrelator {
    /// Minimum time gap (seconds) to consider events separate.
    pub min_gap_seconds: f64,
    /// Window size for cluster detection (seconds).
    pub cluster_window_seconds: f64,
    /// Threshold for sustained event detection (seconds).
    pub sustained_threshold_seconds: f64,
    /// Threshold for repetition (minimum repeat count).
    pub repetition_threshold: usize,
    /// Maximum gap for intermittent pattern (seconds).
    pub intermittent_max_gap: f64,
    /// Risk score threshold for an event to be considered significant.
    pub risk_threshold: f64,
}

impl Default for TemporalCorrelator {
    fn default() -> Self {
        TemporalCorrelator {
            min_gap_seconds: 0.5,
            cluster_window_seconds: 3.0,
            sustained_threshold_seconds: 10.0,
            repetition_threshold: 3,
            intermittent_max_gap: 5.0,
            risk_threshold: 0.3,
        }
    }
}

impl TemporalCorrelator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Correlate visual and audio analysis results to detect temporal patterns.
    ///
    /// Accepts generic analysis structs that provide objects/events with
    /// timestamps and risk-related labels.  Uses sentinel_core proto types
    /// directly for integration with the analysis pipeline.
    pub fn correlate_events(
        &self,
        visual: &sentinel_core::proto::VisualAnalysis,
        audio: &sentinel_core::proto::AudioAnalysis,
    ) -> Vec<TemporalCorrelation> {
        let mut correlations = Vec::new();

        // Extract timed events from both modalities.
        let mut all_events = self.extract_visual_events(visual);
        all_events.extend(self.extract_audio_events(audio));

        if all_events.is_empty() {
            return correlations;
        }

        // Sort by timestamp.
        all_events.sort_by(|a, b| {
            a.timestamp
                .partial_cmp(&b.timestamp)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Detect each pattern type.
        correlations.extend(self.detect_escalation(&all_events));
        correlations.extend(self.detect_repetition(&all_events));
        correlations.extend(self.detect_coincidence(&all_events));
        correlations.extend(self.detect_cluster(&all_events));
        correlations.extend(self.detect_intermittent(&all_events));
        correlations.extend(self.detect_sustained(&all_events));

        // Sort by start time.
        correlations.sort_by(|a, b| {
            a.start_seconds
                .partial_cmp(&b.start_seconds)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        correlations
    }

    /// Build a risk timeline from the analysis results.
    ///
    /// Aggregates risk scores into time-binned entries for dashboard display.
    pub fn build_risk_timeline(
        &self,
        visual: &sentinel_core::proto::VisualAnalysis,
        audio: &sentinel_core::proto::AudioAnalysis,
        bin_size_seconds: f64,
    ) -> Vec<RiskTimelineEntry> {
        let mut all_events = self.extract_visual_events(visual);
        all_events.extend(self.extract_audio_events(audio));

        if all_events.is_empty() || bin_size_seconds <= 0.0 {
            return Vec::new();
        }

        all_events.sort_by(|a, b| {
            a.timestamp
                .partial_cmp(&b.timestamp)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let max_time = all_events
            .last()
            .map(|e| e.timestamp)
            .unwrap_or(0.0);

        let num_bins = (max_time / bin_size_seconds).ceil() as usize + 1;
        let mut bins: Vec<RiskTimelineEntry> = (0..num_bins)
            .map(|i| RiskTimelineEntry {
                timestamp_seconds: i as f64 * bin_size_seconds,
                aggregated_risk: 0.0,
                event_count: 0,
                labels: Vec::new(),
            })
            .collect();

        for event in &all_events {
            let bin_idx = (event.timestamp / bin_size_seconds) as usize;
            if bin_idx < bins.len() {
                bins[bin_idx].aggregated_risk += event.risk_weight;
                bins[bin_idx].event_count += 1;
                bins[bin_idx].labels.push(event.label.clone());
            }
        }

        // Cap risk at 100.
        for bin in &mut bins {
            bin.aggregated_risk = bin.aggregated_risk.min(100.0);
        }

        bins
    }

    // ------------------------------------------------------------------
    // Event extraction
    // ------------------------------------------------------------------

    fn extract_visual_events(
        &self,
        visual: &sentinel_core::proto::VisualAnalysis,
    ) -> Vec<TimedEvent> {
        let mut events = Vec::new();

        for obj in &visual.objects {
            let risk = obj.confidence * obj.category.risk_modifier().abs();
            if risk >= self.risk_threshold {
                events.push(TimedEvent {
                    timestamp: obj.timestamp.seconds,
                    label: obj.label.clone(),
                    modality: Modality::Visual,
                    risk_weight: risk * 10.0, // Scale to 0..100 range
                });
            }
        }

        for text in &visual.texts {
            let risk = if text.is_profanity {
                text.confidence * 0.5
            } else {
                0.0
            };
            if risk >= self.risk_threshold {
                events.push(TimedEvent {
                    timestamp: text.timestamp.seconds,
                    label: format!("text:{}", text.text),
                    modality: Modality::Visual,
                    risk_weight: risk * 10.0,
                });
            }
        }

        for flash in &visual.flashing {
            let risk = flash.severity.weight() * 0.1;
            events.push(TimedEvent {
                timestamp: flash.start.seconds,
                label: "flashing".into(),
                modality: Modality::Visual,
                risk_weight: risk,
            });
        }

        events
    }

    fn extract_audio_events(
        &self,
        audio: &sentinel_core::proto::AudioAnalysis,
    ) -> Vec<TimedEvent> {
        let mut events = Vec::new();

        for evt in &audio.events {
            let risk = evt.confidence;
            if risk >= self.risk_threshold {
                events.push(TimedEvent {
                    timestamp: evt.timestamp.seconds,
                    label: evt.event_type.clone(),
                    modality: Modality::Audio,
                    risk_weight: risk * 10.0,
                });
            }
        }

        for seg in &audio.transcript {
            // Flag transcript segments with negative sentiment or sarcasm.
            let sentiment_risk = if seg.sentiment < -0.3 {
                seg.confidence * seg.sentiment.abs()
            } else {
                0.0
            };
            let sarcasm_risk = if seg.is_sarcasm {
                seg.confidence * 0.3
            } else {
                0.0
            };
            let risk = sentiment_risk + sarcasm_risk;
            if risk >= self.risk_threshold {
                events.push(TimedEvent {
                    timestamp: seg.start.seconds,
                    label: format!("transcript:{}", seg.text),
                    modality: Modality::Audio,
                    risk_weight: risk * 10.0,
                });
            }
        }

        events
    }

    // ------------------------------------------------------------------
    // Pattern detection — Escalation
    // ------------------------------------------------------------------

    /// Detect monotonically increasing risk over a time window.
    fn detect_escalation(&self, events: &[TimedEvent]) -> Vec<TemporalCorrelation> {
        let mut correlations = Vec::new();
        if events.len() < 3 {
            return correlations;
        }

        // Sliding window of 5 events.
        for window in events.windows(5) {
            let risks: Vec<f64> = window.iter().map(|e| e.risk_weight).collect();

            // Check if risk is monotonically increasing (with small tolerance).
            let mut increasing = true;
            for i in 1..risks.len() {
                if risks[i] < risks[i - 1] * 0.95 {
                    // Allow 5% jitter
                    increasing = false;
                    break;
                }
            }

            if increasing && risks.last().unwrap_or(&0.0) - &risks[0] > 1.0 {
                let start = window[0].timestamp;
                let end = window[window.len() - 1].timestamp;
                let duration = end - start;

                if duration > 0.5 {
                    let confidence = ((risks.last().unwrap() - risks[0]) / 10.0)
                        .clamp(0.0, 1.0);
                    let risk_score = risks.iter().sum::<f64>() * TemporalPattern::Escalation.risk_multiplier();

                    let vis_labels: Vec<String> = window
                        .iter()
                        .filter(|e| e.modality == Modality::Visual)
                        .map(|e| e.label.clone())
                        .collect();
                    let aud_labels: Vec<String> = window
                        .iter()
                        .filter(|e| e.modality == Modality::Audio)
                        .map(|e| e.label.clone())
                        .collect();

                    correlations.push(
                        TemporalCorrelation::new(
                            TemporalPattern::Escalation,
                            format!(
                                "Risk escalation from {:.1} to {:.1} over {:.1}s",
                                risks[0],
                                risks.last().unwrap(),
                                duration
                            ),
                            start,
                            end,
                            confidence,
                            risk_score.min(100.0),
                        )
                        .with_visual_events(vis_labels)
                        .with_audio_events(aud_labels),
                    );
                }
            }
        }

        // Deduplicate overlapping escalations (keep highest confidence).
        correlations.sort_by(|a, b| {
            let cmp = a
                .start_seconds
                .partial_cmp(&b.start_seconds)
                .unwrap_or(std::cmp::Ordering::Equal);
            if cmp == std::cmp::Ordering::Equal {
                b.confidence
                    .partial_cmp(&a.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            } else {
                cmp
            }
        });

        let mut deduped = Vec::new();
        for corr in correlations {
            if let Some(last) = deduped.last() {
                if (corr.start_seconds - last.start_seconds).abs() < self.min_gap_seconds {
                    continue;
                }
            }
            deduped.push(corr);
        }

        deduped
    }

    // ------------------------------------------------------------------
    // Pattern detection — Repetition
    // ------------------------------------------------------------------

    /// Detect repeated occurrences of the same event label.
    fn detect_repetition(&self, events: &[TimedEvent]) -> Vec<TemporalCorrelation> {
        let mut correlations = Vec::new();
        if events.len() < self.repetition_threshold {
            return correlations;
        }

        // Group events by label.
        let mut label_groups: std::collections::HashMap<String, Vec<&TimedEvent>> =
            std::collections::HashMap::new();

        for event in events {
            label_groups
                .entry(event.label.clone())
                .or_default()
                .push(event);
        }

        for (label, occurrences) in label_groups {
            if occurrences.len() >= self.repetition_threshold {
                let timestamps: Vec<f64> = occurrences.iter().map(|e| e.timestamp).collect();
                let start = *timestamps.first().unwrap_or(&0.0);
                let end = *timestamps.last().unwrap_or(&0.0);
                let total_risk: f64 = occurrences.iter().map(|e| e.risk_weight).sum();

                let confidence = (occurrences.len() as f64 / 10.0).min(1.0);
                let risk_score = total_risk * TemporalPattern::Repetition.risk_multiplier();

                let vis_labels: Vec<String> = occurrences
                    .iter()
                    .filter(|e| e.modality == Modality::Visual)
                    .map(|e| e.label.clone())
                    .collect();
                let aud_labels: Vec<String> = occurrences
                    .iter()
                    .filter(|e| e.modality == Modality::Audio)
                    .map(|e| e.label.clone())
                    .collect();

                correlations.push(
                    TemporalCorrelation::new(
                        TemporalPattern::Repetition,
                        format!(
                            "'{}' repeated {} times between {:.1}s and {:.1}s",
                            label,
                            occurrences.len(),
                            start,
                            end
                        ),
                        start,
                        end,
                        confidence,
                        risk_score.min(100.0),
                    )
                    .with_visual_events(vis_labels)
                    .with_audio_events(aud_labels),
                );
            }
        }

        correlations
    }

    // ------------------------------------------------------------------
    // Pattern detection — Coincidence
    // ------------------------------------------------------------------

    /// Detect when visual and audio events occur at the same timestamp.
    fn detect_coincidence(&self, events: &[TimedEvent]) -> Vec<TemporalCorrelation> {
        let mut correlations = Vec::new();

        // Group events by timestamp (binned to 0.5s resolution).
        let bin_size = self.min_gap_seconds;
        let mut time_bins: std::collections::BTreeMap<u64, Vec<&TimedEvent>> =
            std::collections::BTreeMap::new();

        for event in events {
            let bin = (event.timestamp / bin_size) as u64;
            time_bins.entry(bin).or_default().push(event);
        }

        for (_bin, bin_events) in time_bins {
            let has_visual = bin_events.iter().any(|e| e.modality == Modality::Visual);
            let has_audio = bin_events.iter().any(|e| e.modality == Modality::Audio);

            if has_visual && has_audio {
                let timestamps: Vec<f64> =
                    bin_events.iter().map(|e| e.timestamp).collect();
                let start = timestamps.iter().fold(f64::INFINITY, |a, &b| a.min(b));
                let end = timestamps.iter().fold(0f64, |a, &b| a.max(b));
                let total_risk: f64 = bin_events.iter().map(|e| e.risk_weight).sum();

                let vis_labels: Vec<String> = bin_events
                    .iter()
                    .filter(|e| e.modality == Modality::Visual)
                    .map(|e| e.label.clone())
                    .collect();
                let aud_labels: Vec<String> = bin_events
                    .iter()
                    .filter(|e| e.modality == Modality::Audio)
                    .map(|e| e.label.clone())
                    .collect();

                correlations.push(
                    TemporalCorrelation::new(
                        TemporalPattern::Coincidence,
                        format!(
                            "Visual+audio coincidence at {:.1}s: {} visual, {} audio events",
                            start,
                            vis_labels.len(),
                            aud_labels.len()
                        ),
                        start,
                        end.max(start + 0.1),
                        0.8f64.min(1.0),
                        (total_risk * TemporalPattern::Coincidence.risk_multiplier()).min(100.0),
                    )
                    .with_visual_events(vis_labels)
                    .with_audio_events(aud_labels),
                );
            }
        }

        // Deduplicate.
        let mut deduped = Vec::new();
        for corr in correlations {
            if let Some(last) = deduped.last() {
                if (corr.start_seconds - last.start_seconds).abs() < self.min_gap_seconds {
                    continue;
                }
            }
            deduped.push(corr);
        }

        deduped
    }

    // ------------------------------------------------------------------
    // Pattern detection — Cluster
    // ------------------------------------------------------------------

    /// Detect dense groups of events within a short time window.
    fn detect_cluster(&self, events: &[TimedEvent]) -> Vec<TemporalCorrelation> {
        let mut correlations = Vec::new();
        if events.len() < 3 {
            return correlations;
        }

        // Sliding window.
        for i in 0..events.len() {
            let window_start = events[i].timestamp;
            let window_end = window_start + self.cluster_window_seconds;

            let cluster_events: Vec<&TimedEvent> = events
                .iter()
                .filter(|e| e.timestamp >= window_start && e.timestamp <= window_end)
                .collect();

            // A cluster needs at least 3 events within the window.
            if cluster_events.len() >= 3 {
                let total_risk: f64 = cluster_events.iter().map(|e| e.risk_weight).sum();
                let end_ts = cluster_events
                    .iter()
                    .map(|e| e.timestamp)
                    .fold(0f64, |a, b| a.max(b));

                let vis_labels: Vec<String> = cluster_events
                    .iter()
                    .filter(|e| e.modality == Modality::Visual)
                    .map(|e| e.label.clone())
                    .collect();
                let aud_labels: Vec<String> = cluster_events
                    .iter()
                    .filter(|e| e.modality == Modality::Audio)
                    .map(|e| e.label.clone())
                    .collect();

                // Density-based confidence.
                let density = cluster_events.len() as f64 / self.cluster_window_seconds;
                let confidence = (density / 5.0).min(1.0);

                correlations.push(
                    TemporalCorrelation::new(
                        TemporalPattern::Cluster,
                        format!(
                            "Event cluster: {} events in {:.1}s window at {:.1}s",
                            cluster_events.len(),
                            self.cluster_window_seconds,
                            window_start
                        ),
                        window_start,
                        end_ts,
                        confidence,
                        (total_risk * TemporalPattern::Cluster.risk_multiplier()).min(100.0),
                    )
                    .with_visual_events(vis_labels)
                    .with_audio_events(aud_labels),
                );
            }
        }

        // Deduplicate overlapping clusters.
        correlations.sort_by(|a, b| {
            a.start_seconds
                .partial_cmp(&b.start_seconds)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut deduped = Vec::new();
        for corr in correlations {
            if let Some(last) = deduped.last() {
                if (corr.start_seconds - last.start_seconds).abs() < self.cluster_window_seconds {
                    continue;
                }
            }
            deduped.push(corr);
        }

        deduped
    }

    // ------------------------------------------------------------------
    // Pattern detection — Intermittent
    // ------------------------------------------------------------------

    /// Detect sporadic events with irregular gaps.
    fn detect_intermittent(&self, events: &[TimedEvent]) -> Vec<TemporalCorrelation> {
        let mut correlations = Vec::new();
        if events.len() < 4 {
            return correlations;
        }

        // Find windows where events are spaced irregularly.
        for window in events.windows(4) {
            let gaps: Vec<f64> = window.windows(2).map(|w| w[1].timestamp - w[0].timestamp).collect();

            if gaps.len() >= 3 {
                // Calculate gap variance.
                let mean_gap = gaps.iter().sum::<f64>() / gaps.len() as f64;
                if mean_gap > 0.0 {
                    let variance: f64 = gaps
                        .iter()
                        .map(|g| (g - mean_gap).powi(2))
                        .sum::<f64>()
                        / gaps.len() as f64;
                    let cv = (variance.sqrt()) / mean_gap; // coefficient of variation

                    // High CV means irregular gaps.
                    if cv > 0.5 && gaps.iter().all(|g| *g <= self.intermittent_max_gap) {
                        let start = window[0].timestamp;
                        let end = window[window.len() - 1].timestamp;
                        let total_risk: f64 = window.iter().map(|e| e.risk_weight).sum();

                        let vis_labels: Vec<String> = window
                            .iter()
                            .filter(|e| e.modality == Modality::Visual)
                            .map(|e| e.label.clone())
                            .collect();
                        let aud_labels: Vec<String> = window
                            .iter()
                            .filter(|e| e.modality == Modality::Audio)
                            .map(|e| e.label.clone())
                            .collect();

                        correlations.push(
                            TemporalCorrelation::new(
                                TemporalPattern::Intermittent,
                                format!(
                                    "Intermittent events (CV={:.2}) between {:.1}s and {:.1}s",
                                    cv, start, end
                                ),
                                start,
                                end,
                                cv.min(1.0),
                                (total_risk * TemporalPattern::Intermittent.risk_multiplier())
                                    .min(100.0),
                            )
                            .with_visual_events(vis_labels)
                            .with_audio_events(aud_labels),
                        );
                    }
                }
            }
        }

        // Deduplicate.
        let mut deduped = Vec::new();
        for corr in correlations {
            if let Some(last) = deduped.last() {
                if (corr.start_seconds - last.start_seconds).abs() < self.min_gap_seconds {
                    continue;
                }
            }
            deduped.push(corr);
        }

        deduped
    }

    // ------------------------------------------------------------------
    // Pattern detection — Sustained
    // ------------------------------------------------------------------

    /// Detect events that persist continuously over a long duration.
    fn detect_sustained(&self, events: &[TimedEvent]) -> Vec<TemporalCorrelation> {
        let mut correlations = Vec::new();
        if events.len() < 2 {
            return correlations;
        }

        // Find the longest continuous span of events.
        let mut current_start = events[0].timestamp;
        let mut current_end = events[0].timestamp;
        let mut current_events: Vec<&TimedEvent> = vec![&events[0]];

        for i in 1..events.len() {
            let gap = events[i].timestamp - events[i - 1].timestamp;

            if gap <= self.min_gap_seconds * 2.0 {
                // Continue the current span.
                current_end = events[i].timestamp;
                current_events.push(&events[i]);
            } else {
                // End current span, check if sustained.
                let duration = current_end - current_start;
                if duration >= self.sustained_threshold_seconds && current_events.len() >= 3 {
                    let total_risk: f64 = current_events.iter().map(|e| e.risk_weight).sum();

                    let vis_labels: Vec<String> = current_events
                        .iter()
                        .filter(|e| e.modality == Modality::Visual)
                        .map(|e| e.label.clone())
                        .collect();
                    let aud_labels: Vec<String> = current_events
                        .iter()
                        .filter(|e| e.modality == Modality::Audio)
                        .map(|e| e.label.clone())
                        .collect();

                    correlations.push(
                        TemporalCorrelation::new(
                            TemporalPattern::Sustained,
                            format!(
                                "Sustained activity: {} events over {:.1}s from {:.1}s to {:.1}s",
                                current_events.len(),
                                duration,
                                current_start,
                                current_end
                            ),
                            current_start,
                            current_end,
                            (duration / 30.0).min(1.0),
                            (total_risk * TemporalPattern::Sustained.risk_multiplier()).min(100.0),
                        )
                        .with_visual_events(vis_labels)
                        .with_audio_events(aud_labels),
                    );
                }

                // Start new span.
                current_start = events[i].timestamp;
                current_end = events[i].timestamp;
                current_events = vec![&events[i]];
            }
        }

        // Check final span.
        let duration = current_end - current_start;
        if duration >= self.sustained_threshold_seconds && current_events.len() >= 3 {
            let total_risk: f64 = current_events.iter().map(|e| e.risk_weight).sum();

            let vis_labels: Vec<String> = current_events
                .iter()
                .filter(|e| e.modality == Modality::Visual)
                .map(|e| e.label.clone())
                .collect();
            let aud_labels: Vec<String> = current_events
                .iter()
                .filter(|e| e.modality == Modality::Audio)
                .map(|e| e.label.clone())
                .collect();

            correlations.push(
                TemporalCorrelation::new(
                    TemporalPattern::Sustained,
                    format!(
                        "Sustained activity: {} events over {:.1}s from {:.1}s to {:.1}s",
                        current_events.len(),
                        duration,
                        current_start,
                        current_end
                    ),
                    current_start,
                    current_end,
                    (duration / 30.0).min(1.0),
                    (total_risk * TemporalPattern::Sustained.risk_multiplier()).min(100.0),
                )
                .with_visual_events(vis_labels)
                .with_audio_events(aud_labels),
            );
        }

        correlations
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_visual() -> sentinel_core::proto::VisualAnalysis {
        sentinel_core::proto::VisualAnalysis::empty()
    }

    fn empty_audio() -> sentinel_core::proto::AudioAnalysis {
        sentinel_core::proto::AudioAnalysis::empty()
    }

    #[test]
    fn correlator_new_has_defaults() {
        let c = TemporalCorrelator::new();
        assert_eq!(c.min_gap_seconds, 0.5);
        assert_eq!(c.cluster_window_seconds, 3.0);
        assert_eq!(c.sustained_threshold_seconds, 10.0);
        assert_eq!(c.repetition_threshold, 3);
    }

    #[test]
    fn correlate_empty_returns_empty() {
        let c = TemporalCorrelator::new();
        let result = c.correlate_events(&empty_visual(), &empty_audio());
        assert!(result.is_empty());
    }

    #[test]
    fn risk_timeline_empty_returns_empty() {
        let c = TemporalCorrelator::new();
        let result = c.build_risk_timeline(&empty_visual(), &empty_audio(), 1.0);
        assert!(result.is_empty());
    }

    #[test]
    fn temporal_pattern_labels() {
        assert_eq!(TemporalPattern::Escalation.label(), "escalation");
        assert_eq!(TemporalPattern::Repetition.label(), "repetition");
        assert_eq!(TemporalPattern::Coincidence.label(), "coincidence");
        assert_eq!(TemporalPattern::Cluster.label(), "cluster");
        assert_eq!(TemporalPattern::Intermittent.label(), "intermittent");
        assert_eq!(TemporalPattern::Sustained.label(), "sustained");
    }

    #[test]
    fn temporal_pattern_risk_multipliers() {
        assert!(TemporalPattern::Escalation.risk_multiplier() > TemporalPattern::Intermittent.risk_multiplier());
        assert!(TemporalPattern::Coincidence.risk_multiplier() > 1.0);
    }

    #[test]
    fn correlation_duration() {
        let corr = TemporalCorrelation::new(
            TemporalPattern::Escalation,
            "test",
            10.0,
            25.0,
            0.8,
            50.0,
        );
        assert!((corr.duration_seconds() - 15.0).abs() < 0.001);
    }

    #[test]
    fn correlation_builder_methods() {
        let corr = TemporalCorrelation::new(
            TemporalPattern::Cluster,
            "test",
            0.0,
            5.0,
            0.9,
            30.0,
        )
        .with_visual_events(vec!["weapon".into()])
        .with_audio_events(vec!["scream".into()]);

        assert_eq!(corr.visual_events.len(), 1);
        assert_eq!(corr.audio_events.len(), 1);
        assert_eq!(corr.visual_events[0], "weapon");
    }

    #[test]
    fn correlate_with_visual_events() {
        use sentinel_core::proto::{BoundingBox, DetectedObject, Timestamp, ViolationCategory};

        let mut visual = empty_visual();
        visual.objects.push(
            DetectedObject::new("weapon", 0.9)
                .with_category(ViolationCategory::Violence)
                .at_timestamp(Timestamp::new(5.0, 150)),
        );
        visual.objects.push(
            DetectedObject::new("weapon", 0.85)
                .with_category(ViolationCategory::Violence)
                .at_timestamp(Timestamp::new(6.0, 180)),
        );
        visual.objects.push(
            DetectedObject::new("weapon", 0.88)
                .with_category(ViolationCategory::Violence)
                .at_timestamp(Timestamp::new(7.0, 210)),
        );

        let audio = empty_audio();

        let c = TemporalCorrelator::new();
        let result = c.correlate_events(&visual, &audio);
        // Should detect repetition of "weapon" and possibly escalation.
        assert!(!result.is_empty());
    }

    #[test]
    fn correlate_coincidence() {
        use sentinel_core::proto::{AudioEvent, BoundingBox, DetectedObject, Timestamp, ViolationCategory};

        let mut visual = empty_visual();
        visual.objects.push(
            DetectedObject::new("weapon", 0.9)
                .with_category(ViolationCategory::Violence)
                .at_timestamp(Timestamp::new(5.0, 150)),
        );

        let mut audio = empty_audio();
        audio.events.push(AudioEvent::new("gunshot", 0.9, Timestamp::new(5.1, 153)));

        let c = TemporalCorrelator::new();
        let result = c.correlate_events(&visual, &audio);

        let has_coincidence = result.iter().any(|r| r.pattern == TemporalPattern::Coincidence);
        assert!(has_coincidence, "Should detect a coincidence pattern");
    }
}

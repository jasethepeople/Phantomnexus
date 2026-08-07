//! Metadata analyzer: clickbait detection, profanity filtering, and keyword analysis.
//!
//! Uses rule-based heuristics and a multi-language profanity Trie to analyze
//! video titles, descriptions, and tags for policy compliance.

use sentinel_core::error::SentinelError;
use sentinel_core::proto::{MetadataAnalysis, VideoSource};
use std::collections::HashMap;
use tracing::{debug, info, instrument, warn};

// ===========================================================================
// MetadataAnalyzer
// ===========================================================================

/// Configuration for the metadata analysis pipeline.
#[derive(Debug, Clone, PartialEq)]
pub struct MetadataAnalyzerConfig {
    /// Threshold for flagging clickbait (0.0..1.0).
    pub clickbait_threshold: f64,
    /// Threshold for flagging keyword stuffing (0.0..1.0).
    pub keyword_stuffing_threshold: f64,
    /// Maximum allowed ALL CAPS ratio before flagging.
    pub max_all_caps_ratio: f64,
    /// Maximum allowed exclamation count before flagging.
    pub max_exclamation_count: usize,
    /// Maximum tag count before flagging keyword stuffing.
    pub max_reasonable_tags: usize,
    /// Whether to enable profanity checking.
    pub enable_profanity: bool,
    /// Minimum profanity match confidence to flag.
    pub profanity_confidence: f64,
}

impl Default for MetadataAnalyzerConfig {
    fn default() -> Self {
        MetadataAnalyzerConfig {
            clickbait_threshold: 0.5,
            keyword_stuffing_threshold: 0.6,
            max_all_caps_ratio: 0.5,
            max_exclamation_count: 3,
            max_reasonable_tags: 20,
            enable_profanity: true,
            profanity_confidence: 0.5,
        }
    }
}

impl MetadataAnalyzerConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_clickbait_threshold(mut self, t: f64) -> Self {
        self.clickbait_threshold = t.clamp(0.0, 1.0);
        self
    }

    pub fn with_keyword_stuffing_threshold(mut self, t: f64) -> Self {
        self.keyword_stuffing_threshold = t.clamp(0.0, 1.0);
        self
    }

    pub fn with_profanity(mut self, enable: bool) -> Self {
        self.enable_profanity = enable;
        self
    }
}

/// Result of analyzing a video title.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TitleAnalysis {
    pub clickbait_score: f64,
    pub all_caps_ratio: f64,
    pub exclamation_count: usize,
    pub flagged_keywords: Vec<String>,
    pub profanity_matches: Vec<ProfanityMatch>,
    pub suggestions: Vec<String>,
}

/// Result of analyzing a video description.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DescriptionAnalysis {
    pub clickbait_score: f64,
    pub profanity_matches: Vec<ProfanityMatch>,
    pub flagged_keywords: Vec<String>,
    pub has_external_links: bool,
    pub has_excessive_formatting: bool,
    pub suggestions: Vec<String>,
}

/// Result of analyzing video tags.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TagsAnalysis {
    pub keyword_stuffing_score: f64,
    pub tag_count: usize,
    pub duplicate_count: usize,
    pub flagged_tags: Vec<String>,
    pub suggestions: Vec<String>,
}

/// A matched profanity term with its position and severity.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfanityMatch {
    pub term: String,
    pub start: usize,
    pub end: usize,
    pub language: String,
    pub severity: ProfanitySeverity,
    pub context: String,
}

/// Severity of a profanity match.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProfanitySeverity {
    Mild,
    Moderate,
    Severe,
}

impl ProfanitySeverity {
    pub fn weight(&self) -> f64 {
        match self {
            ProfanitySeverity::Mild => 1.0,
            ProfanitySeverity::Moderate => 3.0,
            ProfanitySeverity::Severe => 6.0,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ProfanitySeverity::Mild => "mild",
            ProfanitySeverity::Moderate => "moderate",
            ProfanitySeverity::Severe => "severe",
        }
    }
}

// ===========================================================================
// Trie-based Profanity Filter
// ===========================================================================

/// A Trie node for efficient multi-pattern string matching.
#[derive(Debug, Clone, Default)]
struct TrieNode {
    children: HashMap<char, TrieNode>,
    is_end: bool,
    severity: Option<ProfanitySeverity>,
    language: Option<String>,
}

impl TrieNode {
    fn new() -> Self {
        TrieNode {
            children: HashMap::new(),
            is_end: false,
            severity: None,
            language: None,
        }
    }
}

/// Multi-language profanity filter using a Trie data structure.
///
/// Supports case-insensitive matching and tracks match positions
/// for highlighting / reporting.
pub struct ProfanityFilter {
    root: TrieNode,
}

impl ProfanityFilter {
    /// Create a profanity filter with a default word list.
    pub fn new() -> Self {
        let mut filter = ProfanityFilter {
            root: TrieNode::new(),
        };
        filter.load_default_words();
        filter
    }

    /// Create an empty profanity filter (no words loaded).
    pub fn empty() -> Self {
        ProfanityFilter {
            root: TrieNode::new(),
        }
    }

    /// Insert a word into the Trie.
    pub fn insert<S: Into<String>>(
        &mut self,
        word: S,
        severity: ProfanitySeverity,
        language: S,
    ) {
        let word = word.into().to_lowercase();
        let language = language.into();
        let mut node = &mut self.root;
        for ch in word.chars() {
            node = node.children.entry(ch).or_insert_with(TrieNode::new);
        }
        node.is_end = true;
        node.severity = Some(severity);
        node.language = Some(language);
    }

    /// Check if text contains any profanity matches.
    pub fn check(&self, text: &str) -> Vec<ProfanityMatch> {
        let lower = text.to_lowercase();
        let chars: Vec<char> = lower.chars().collect();
        let mut matches = Vec::new();
        let len = chars.len();

        for start in 0..len {
            let mut node = &self.root;
            for end in start..len {
                let ch = chars[end];
                match node.children.get(&ch) {
                    Some(child) => {
                        node = child;
                        if node.is_end {
                            // Found a match — extract original-case context.
                            let match_end = end + 1;
                            let original_text: String = text.chars().skip(start).take(match_end - start).collect();
                            let context_start = start.saturating_sub(10);
                            let context_end = (match_end + 10).min(len);
                            let context: String = text
                                .chars()
                                .skip(context_start)
                                .take(context_end - context_start)
                                .collect();

                            matches.push(ProfanityMatch {
                                term: original_text,
                                start,
                                end: match_end,
                                language: node.language.clone().unwrap_or_default(),
                                severity: node.severity.unwrap_or(ProfanitySeverity::Moderate),
                                context,
                            });
                        }
                    }
                    None => break,
                }
            }
        }

        matches
    }

    /// Load default profanity word lists for multiple languages.
    fn load_default_words(&mut self) {
        // English mild
        for word in &["darn", "heck", "shoot", "crap", "stupid", "idiot", "dumb", "lame"] {
            self.insert(*word, ProfanitySeverity::Mild, "en");
        }
        // English moderate
        for word in &[
            "damn", "hell", "crap", "piss", "bastard", "frick", "freaking",
        ] {
            self.insert(*word, ProfanitySeverity::Moderate, "en");
        }
        // English severe
        for word in &[
            "shit", "fuck", "bitch", "asshole", "cunt", "nigger", "faggot",
        ] {
            self.insert(*word, ProfanitySeverity::Severe, "en");
        }
        // Spanish
        for word in &["mierda", "puta", "carajo", "pendejo", "cojones", "joder"] {
            self.insert(*word, ProfanitySeverity::Severe, "es");
        }
        // French
        for word in &["merde", "putain", "con", "connard", "salope"] {
            self.insert(*word, ProfanitySeverity::Severe, "fr");
        }
        // German
        for word in &["scheisse", "arschloch", "verdammt", "hurensohn"] {
            self.insert(*word, ProfanitySeverity::Severe, "de");
        }
        // Russian (transliterated)
        for word in &["blyat", "suka", "pizdec", "ebat"] {
            self.insert(*word, ProfanitySeverity::Severe, "ru");
        }
        // Chinese (pinyin)
        for word in &["cao", "ta ma de", "sha bi", "wang ba"] {
            self.insert(*word, ProfanitySeverity::Severe, "zh");
        }
    }
}

// ===========================================================================
// Clickbait Detection Patterns
// ===========================================================================

/// Known clickbait patterns and their weights.
fn clickbait_patterns() -> Vec<(&'static str, f64)> {
    vec![
        ("shocking", 0.25),
        ("won't believe", 0.30),
        ("you won't believe", 0.35),
        ("unbelievable", 0.20),
        ("incredible", 0.15),
        ("amazing", 0.15),
        ("mind-blowing", 0.30),
        ("mind blowing", 0.30),
        ("insane", 0.20),
        ("crazy", 0.15),
        ("impossible", 0.20),
        ("miracle", 0.25),
        ("doctors hate", 0.35),
        ("one weird trick", 0.40),
        ("they don't want", 0.30),
        ("secret", 0.15),
        ("exposed", 0.20),
        ("revealed", 0.15),
        ("truth about", 0.20),
        ("what happens next", 0.30),
        ("gone wrong", 0.25),
        ("gone sexual", 0.35),
        ("at 3am", 0.30),
        ("in the hood", 0.25),
        ("caught on camera", 0.20),
        ("must watch", 0.25),
        ("don't watch", 0.30),
        ("warning", 0.15),
        ("urgent", 0.20),
        ("breaking", 0.15),
        ("prank", 0.10),
        ("challenge", 0.10),
        ("extreme", 0.15),
        ("destroy", 0.15),
        ("killed", 0.20),
        ("dies", 0.20),
        ("dead", 0.15),
        ("everyone is talking", 0.25),
        ("number \d+ will shock", 0.35),
        ("this is why", 0.15),
        ("the real reason", 0.20),
        ("what they don't tell", 0.30),
    ]
}

// ===========================================================================
// MetadataAnalyzer
// ===========================================================================

/// Metadata analysis pipeline for titles, descriptions, and tags.
pub struct MetadataAnalyzer {
    config: MetadataAnalyzerConfig,
    profanity_filter: ProfanityFilter,
}

impl MetadataAnalyzer {
    /// Create a new metadata analyzer.
    pub fn new(config: MetadataAnalyzerConfig) -> Self {
        let filter = if config.enable_profanity {
            ProfanityFilter::new()
        } else {
            ProfanityFilter::empty()
        };
        MetadataAnalyzer {
            config,
            profanity_filter: filter,
        }
    }

    // ------------------------------------------------------------------
    // Title Analysis
    // ------------------------------------------------------------------

    /// Analyze a video title for clickbait, profanity, and policy issues.
    ///
    /// Detection rules:
    /// - **ALL CAPS ratio**: proportion of alphabetic characters that are uppercase.
    /// - **Exclamation count**: number of `!` characters.
    /// - **Clickbait keywords**: weighted score based on known clickbait patterns.
    /// - **Profanity**: Trie-based multi-language profanity scan.
    #[instrument(skip(self, title))]
    pub fn analyze_title(&self, title: &str) -> Result<TitleAnalysis, SentinelError> {
        let clickbait_score = self.detect_clickbait(title);
        let all_caps_ratio = Self::compute_all_caps_ratio(title);
        let exclamation_count = title.chars().filter(|&c| c == '!').count();

        // Collect flagged clickbait keywords.
        let flagged_keywords = Self::extract_flagged_keywords(title);

        // Profanity check.
        let profanity_matches = if self.config.enable_profanity {
            self.check_profanity(title)
        } else {
            Vec::new()
        };

        // Generate suggestions.
        let mut suggestions = Vec::new();
        if clickbait_score > self.config.clickbait_threshold {
            suggestions.push(format!(
                "Consider revising title to reduce clickbait indicators (score: {:.2})",
                clickbait_score
            ));
        }
        if all_caps_ratio > self.config.max_all_caps_ratio {
            suggestions.push("Reduce ALL CAPS usage in title".into());
        }
        if exclamation_count > self.config.max_exclamation_count {
            suggestions.push("Reduce exclamation marks in title".into());
        }
        if !profanity_matches.is_empty() {
            let severe: Vec<_> = profanity_matches
                .iter()
                .filter(|m| matches!(m.severity, ProfanitySeverity::Severe))
                .collect();
            if !severe.is_empty() {
                suggestions.push("Remove severe profanity from title".into());
            } else {
                suggestions.push("Consider removing mild profanity from title".into());
            }
        }

        info!(
            "title analysis: clickbait={:.2}, caps={:.2}, excl={}, profanity={}",
            clickbait_score,
            all_caps_ratio,
            exclamation_count,
            profanity_matches.len()
        );

        Ok(TitleAnalysis {
            clickbait_score,
            all_caps_ratio,
            exclamation_count,
            flagged_keywords,
            profanity_matches,
            suggestions,
        })
    }

    // ------------------------------------------------------------------
    // Description Analysis
    // ------------------------------------------------------------------

    /// Analyze a video description for clickbait, profanity, and spam indicators.
    #[instrument(skip(self, description))]
    pub fn analyze_description(
        &self,
        description: &str,
    ) -> Result<DescriptionAnalysis, SentinelError> {
        let clickbait_score = self.detect_clickbait(description);
        let profanity_matches = if self.config.enable_profanity {
            self.check_profanity(description)
        } else {
            Vec::new()
        };

        let flagged_keywords = Self::extract_flagged_keywords(description);

        // Detect external links (http:// or https://).
        let has_external_links = description.contains("http://") || description.contains("https://");

        // Detect excessive formatting (multiple consecutive newlines, excessive symbols).
        let excessive_newlines = description.matches("\n\n\n").count();
        let excessive_symbols = description.chars().filter(|&c| c == '|' || c == '~' || c == '*').count();
        let has_excessive_formatting = excessive_newlines > 2 || excessive_symbols > 20;

        let mut suggestions = Vec::new();
        if clickbait_score > self.config.clickbait_threshold {
            suggestions.push(format!(
                "Consider revising description to reduce clickbait language (score: {:.2})",
                clickbait_score
            ));
        }
        if !profanity_matches.is_empty() {
            suggestions.push("Remove profanity from description".into());
        }
        if has_external_links {
            suggestions.push("Verify all external links are safe and relevant".into());
        }
        if has_excessive_formatting {
            suggestions.push("Reduce excessive formatting in description".into());
        }

        Ok(DescriptionAnalysis {
            clickbait_score,
            profanity_matches,
            flagged_keywords,
            has_external_links,
            has_excessive_formatting,
            suggestions,
        })
    }

    // ------------------------------------------------------------------
    // Tags Analysis
    // ------------------------------------------------------------------

    /// Analyze video tags for keyword stuffing and duplicates.
    #[instrument(skip(self, tags))]
    pub fn analyze_tags(&self, tags: &[String]) -> Result<TagsAnalysis, SentinelError> {
        let tag_count = tags.len();

        // Count duplicates (case-insensitive).
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut duplicates = 0usize;
        for tag in tags {
            let lower = tag.to_lowercase().trim().to_string();
            let count = seen.entry(lower).or_insert(0);
            *count += 1;
            if *count > 1 {
                duplicates += 1;
            }
        }

        // Check for suspicious patterns: very long tags, excessive tag count.
        let mut flagged_tags = Vec::new();
        for tag in tags {
            if tag.len() > 50 {
                flagged_tags.push(format!("{} (too long)", tag));
            }
            if tag.chars().filter(|&c| c == ',').count() > 2 {
                flagged_tags.push(format!("{} (contains commas - possible multi-tag)", tag));
            }
        }

        // Keyword stuffing score based on tag count and duplicates.
        let stuffing_score = if tag_count > self.config.max_reasonable_tags {
            ((tag_count - self.config.max_reasonable_tags) as f64 / self.config.max_reasonable_tags as f64)
                .min(1.0)
                + (duplicates as f64 * 0.1)
        } else {
            duplicates as f64 * 0.05
        };
        let stuffing_score = stuffing_score.clamp(0.0, 1.0);

        let mut suggestions = Vec::new();
        if tag_count > self.config.max_reasonable_tags {
            suggestions.push(format!(
                "Reduce tag count from {} to under {}",
                tag_count, self.config.max_reasonable_tags
            ));
        }
        if duplicates > 0 {
            suggestions.push(format!("Remove {} duplicate tag(s)", duplicates));
        }
        if !flagged_tags.is_empty() {
            suggestions.push("Review flagged tags for compliance".into());
        }

        Ok(TagsAnalysis {
            keyword_stuffing_score: stuffing_score,
            tag_count,
            duplicate_count: duplicates,
            flagged_tags,
            suggestions,
        })
    }

    // ------------------------------------------------------------------
    // Clickbait Detection
    // ------------------------------------------------------------------

    /// Detect clickbait patterns in text and return a score from 0.0 to 1.0.
    ///
    /// Higher scores indicate stronger clickbait characteristics.  The score
    /// is based on:
    /// - Keyword pattern matches (weighted).
    /// - ALL CAPS ratio.
    /// - Excessive punctuation (!, ?, ...).
    /// - Number presence (listicles).
    /// - Sentence ending with ellipsis.
    pub fn detect_clickbait(&self, text: &str) -> f64 {
        let lower = text.to_lowercase();
        let mut score = 0.0f64;

        // Keyword pattern matching.
        for (pattern, weight) in clickbait_patterns() {
            if lower.contains(pattern) {
                score += weight;
            }
        }

        // ALL CAPS ratio.
        let caps_ratio = Self::compute_all_caps_ratio(text);
        if caps_ratio > 0.3 {
            score += (caps_ratio - 0.3) * 0.5;
        }

        // Excessive exclamation marks.
        let excl_count = text.chars().filter(|&c| c == '!').count();
        if excl_count >= 2 {
            score += (excl_count as f64 - 1.0) * 0.05;
        }

        // Multiple question marks.
        let q_count = text.chars().filter(|&c| c == '?').count();
        if q_count >= 2 {
            score += (q_count as f64 - 1.0) * 0.05;
        }

        // Presence of numbers (listicles, "top 10", etc.).
        let has_number = text.chars().any(|c| c.is_ascii_digit());
        if has_number {
            // Check for listicle patterns.
            let listicle_patterns = ["top ", "best ", "worst ", "ways to ", "reasons "];
            for pat in &listicle_patterns {
                if lower.contains(pat) {
                    score += 0.1;
                    break;
                }
            }
        }

        // Sentence ending with ellipsis (teaser).
        if text.trim_end().ends_with("...") {
            score += 0.1;
        }

        // Emoji presence (common in clickbait).
        let emoji_count = text.chars().filter(|&c| {
            (0x1F600..=0x1F64F).contains(&(c as u32)) // emoticons
                || (0x1F300..=0x1F5FF).contains(&(c as u32)) // symbols
                || (0x1F680..=0x1F6FF).contains(&(c as u32)) // transport
        }).count();
        score += emoji_count as f64 * 0.03;

        score.clamp(0.0, 1.0)
    }

    // ------------------------------------------------------------------
    // Profanity Check
    // ------------------------------------------------------------------

    /// Check text for profanity using the Trie-based multi-language filter.
    pub fn check_profanity(&self, text: &str) -> Vec<ProfanityMatch> {
        self.profanity_filter.check(text)
    }

    // ------------------------------------------------------------------
    // Full metadata pipeline
    // ------------------------------------------------------------------

    /// Run the complete metadata analysis pipeline on a video source.
    #[instrument(skip(self, source))]
    pub fn analyze(&self, source: &VideoSource) -> Result<MetadataAnalysis, SentinelError> {
        info!("starting metadata analysis for video '{}'", source.video_id);

        let title_analysis = self.analyze_title(&source.title)?;
        let desc_analysis = self.analyze_description(&source.description)?;
        let tags_analysis = self.analyze_tags(&source.tags)?;

        // Aggregate flagged keywords.
        let mut all_flagged = title_analysis.flagged_keywords.clone();
        all_flagged.extend(desc_analysis.flagged_keywords.clone());
        all_flagged.extend(tags_analysis.flagged_tags.clone());
        all_flagged.sort();
        all_flagged.dedup();

        // Aggregate suggestions.
        let mut all_suggestions = title_analysis.suggestions.clone();
        all_suggestions.extend(desc_analysis.suggestions.clone());
        all_suggestions.extend(tags_analysis.suggestions.clone());
        all_suggestions.sort();
        all_suggestions.dedup();

        // Compute misleading score from profanity severity.
        let misleading_score: f64 = title_analysis
            .profanity_matches
            .iter()
            .chain(&desc_analysis.profanity_matches)
            .map(|m| m.severity.weight() * 0.5)
            .sum::<f64>()
            .min(50.0);

        let mut analysis = MetadataAnalysis {
            clickbait_score: title_analysis.clickbait_score.max(desc_analysis.clickbait_score),
            keyword_stuffing_score: tags_analysis.keyword_stuffing_score,
            misleading_score,
            flagged_keywords: all_flagged,
            suggestions: all_suggestions,
            overall_risk_score: 0.0,
        };

        analysis.compute_risk_score();

        info!(
            "metadata analysis complete: clickbait={:.2}, stuffing={:.2}, misleading={:.2}, score={:.2}",
            analysis.clickbait_score,
            analysis.keyword_stuffing_score,
            analysis.misleading_score,
            analysis.overall_risk_score
        );

        Ok(analysis)
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    /// Compute the ratio of uppercase alphabetic characters to total alphabetic characters.
    fn compute_all_caps_ratio(text: &str) -> f64 {
        let mut alpha_count = 0u64;
        let mut caps_count = 0u64;
        for ch in text.chars() {
            if ch.is_alphabetic() {
                alpha_count += 1;
                if ch.is_uppercase() {
                    caps_count += 1;
                }
            }
        }
        if alpha_count == 0 {
            0.0
        } else {
            caps_count as f64 / alpha_count as f64
        }
    }

    /// Extract flagged clickbait keywords found in the text.
    fn extract_flagged_keywords(text: &str) -> Vec<String> {
        let lower = text.to_lowercase();
        let mut found = Vec::new();
        for (pattern, _) in clickbait_patterns() {
            if lower.contains(pattern) {
                found.push(pattern.to_string());
            }
        }
        found.sort();
        found.dedup();
        found
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- Config ---

    #[test]
    fn test_config_default() {
        let cfg = MetadataAnalyzerConfig::default();
        assert_eq!(cfg.clickbait_threshold, 0.5);
        assert!(cfg.enable_profanity);
    }

    #[test]
    fn test_config_builder() {
        let cfg = MetadataAnalyzerConfig::new()
            .with_clickbait_threshold(0.8)
            .with_profanity(false);
        assert_eq!(cfg.clickbait_threshold, 0.8);
        assert!(!cfg.enable_profanity);
    }

    // --- Clickbait Detection ---

    #[test]
    fn test_clickbait_score_zero() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let score = analyzer.detect_clickbait("A normal video title about gardening");
        assert!(score < 0.3, "score={}", score);
    }

    #[test]
    fn test_clickbait_score_shocking() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let score = analyzer.detect_clickbait("SHOCKING: You Won't Believe What Happens Next!");
        assert!(score > 0.4, "score={}", score);
    }

    #[test]
    fn test_clickbait_score_caps() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let score = analyzer.detect_clickbait("THIS IS IN ALL CAPS AND VERY LOUD!!!");
        assert!(score > 0.2, "score={}", score);
    }

    #[test]
    fn test_clickbait_score_listicle() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let score = analyzer.detect_clickbait("Top 10 Amazing Facts You Never Knew");
        assert!(score > 0.1, "score={}", score);
    }

    #[test]
    fn test_clickbait_score_ellipsis() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let score = analyzer.detect_clickbait("Wait until you see what happens...");
        assert!(score > 0.05, "score={}", score);
    }

    #[test]
    fn test_clickbait_clamped() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        // Extremely clickbait title should still be max 1.0.
        let score = analyzer.detect_clickbait(
            "SHOCKING!!! You Won't Believe This One Weird Trick! Doctors HATE It!!! Must Watch!!!",
        );
        assert!(score <= 1.0, "score={}", score);
        assert!(score > 0.5, "score={}", score);
    }

    // --- Profanity Filter ---

    #[test]
    fn test_profanity_filter_empty() {
        let filter = ProfanityFilter::empty();
        let matches = filter.check("this is clean text");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_profanity_filter_match() {
        let filter = ProfanityFilter::new();
        let matches = filter.check("this contains the word shit in it");
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.term == "shit"));
    }

    #[test]
    fn test_profanity_filter_no_match() {
        let filter = ProfanityFilter::new();
        let matches = filter.check("this is a completely clean and friendly title");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_profanity_filter_case_insensitive() {
        let filter = ProfanityFilter::new();
        let matches = filter.check("This Has DAMN in it");
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.term.to_lowercase() == "damn"));
    }

    #[test]
    fn test_profanity_filter_spanish() {
        let filter = ProfanityFilter::new();
        let matches = filter.check("esto es mierda total");
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.language == "es"));
    }

    #[test]
    fn test_profanity_filter_french() {
        let filter = ProfanityFilter::new();
        let matches = filter.check("c'est une putain de situation");
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.language == "fr"));
    }

    #[test]
    fn test_profanity_filter_context() {
        let filter = ProfanityFilter::new();
        let matches = filter.check("the word bastard appears here");
        assert!(!matches.is_empty());
        assert!(matches[0].context.contains("bastard"));
        assert!(matches[0].start < matches[0].end);
    }

    #[test]
    fn test_profanity_filter_multiple_matches() {
        let filter = ProfanityFilter::new();
        let matches = filter.check("shit and fuck are both here");
        assert!(matches.len() >= 2);
    }

    // --- Title Analysis ---

    #[test]
    fn test_analyze_title_clean() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let analysis = analyzer.analyze_title("How to Grow Tomatoes in Your Garden").unwrap();
        assert!(analysis.clickbait_score < 0.3, "clickbait={}", analysis.clickbait_score);
        assert!(analysis.profanity_matches.is_empty());
        assert!(analysis.suggestions.is_empty() || analysis.suggestions.len() < 3);
    }

    #[test]
    fn test_analyze_title_clickbait() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let analysis = analyzer
            .analyze_title("SHOCKING!!! You Won't Believe What Happens!!!")
            .unwrap();
        assert!(analysis.clickbait_score > 0.3, "clickbait={}", analysis.clickbait_score);
        assert!(analysis.exclamation_count > 2);
        assert!(!analysis.suggestions.is_empty());
    }

    #[test]
    fn test_analyze_title_profanity() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let analysis = analyzer.analyze_title("This video is damn good").unwrap();
        assert!(!analysis.profanity_matches.is_empty());
    }

    #[test]
    fn test_all_caps_ratio() {
        assert_eq!(MetadataAnalyzer::compute_all_caps_ratio("ABC"), 1.0);
        assert_eq!(MetadataAnalyzer::compute_all_caps_ratio("abc"), 0.0);
        assert_eq!(MetadataAnalyzer::compute_all_caps_ratio("AbC"), 2.0 / 3.0);
        assert_eq!(MetadataAnalyzer::compute_all_caps_ratio("123"), 0.0);
    }

    // --- Description Analysis ---

    #[test]
    fn test_analyze_description_clean() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let analysis = analyzer
            .analyze_description("This is a normal description about cooking pasta.")
            .unwrap();
        assert!(analysis.clickbait_score < 0.3);
        assert!(!analysis.has_external_links);
    }

    #[test]
    fn test_analyze_description_with_links() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let analysis = analyzer
            .analyze_description("Check out https://example.com for more info.")
            .unwrap();
        assert!(analysis.has_external_links);
    }

    #[test]
    fn test_analyze_description_excessive_formatting() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let analysis = analyzer
            .analyze_description("Line 1\n\n\nLine 2\n\n\nLine 3\n\n\nLine 4")
            .unwrap();
        assert!(analysis.has_excessive_formatting);
    }

    // --- Tags Analysis ---

    #[test]
    fn test_analyze_tags_normal() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let tags: Vec<String> = vec![
            "cooking", "pasta", "recipe", "italian", "food",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let analysis = analyzer.analyze_tags(&tags).unwrap();
        assert!(analysis.keyword_stuffing_score < 0.3, "stuffing={}", analysis.keyword_stuffing_score);
        assert_eq!(analysis.duplicate_count, 0);
    }

    #[test]
    fn test_analyze_tags_stuffing() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let tags: Vec<String> = (0..50)
            .map(|i| format!("tag{}", i))
            .collect();
        let analysis = analyzer.analyze_tags(&tags).unwrap();
        assert!(analysis.keyword_stuffing_score > 0.3, "stuffing={}", analysis.keyword_stuffing_score);
        assert!(!analysis.suggestions.is_empty());
    }

    #[test]
    fn test_analyze_tags_duplicates() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let tags: Vec<String> = vec!["food", "FOOD", "Food", "cooking", "cooking"]
            .into_iter()
            .map(String::from)
            .collect();
        let analysis = analyzer.analyze_tags(&tags).unwrap();
        assert!(analysis.duplicate_count > 0, "duplicates={}", analysis.duplicate_count);
    }

    #[test]
    fn test_analyze_tags_long_tag() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let tags: Vec<String> = vec!["a".repeat(60)];
        let analysis = analyzer.analyze_tags(&tags).unwrap();
        assert!(!analysis.flagged_tags.is_empty());
    }

    // --- Full Pipeline ---

    #[test]
    fn test_analyze_full_pipeline() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let source = VideoSource::new("vid-1", "/tmp/test.mp4")
            .with_title("Amazing Cooking Tips You Won't Believe!");
        let analysis = analyzer.analyze(&source).unwrap();
        assert!(analysis.clickbait_score > 0.0);
        assert!(analysis.overall_risk_score >= 0.0 && analysis.overall_risk_score <= 100.0);
    }

    #[test]
    fn test_analyze_full_pipeline_clean() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let source = VideoSource::new("vid-2", "/tmp/test.mp4")
            .with_title("How to Make Fresh Pasta at Home");
        let analysis = analyzer.analyze(&source).unwrap();
        assert!(analysis.clickbait_score < 0.3, "clickbait={}", analysis.clickbait_score);
    }

    #[test]
    fn test_profanity_severity_weights() {
        assert!(ProfanitySeverity::Severe.weight() > ProfanitySeverity::Moderate.weight());
        assert!(ProfanitySeverity::Moderate.weight() > ProfanitySeverity::Mild.weight());
    }

    #[test]
    fn test_empty_text() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let score = analyzer.detect_clickbait("");
        assert_eq!(score, 0.0);

        let matches = analyzer.check_profanity("");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_profanity_disabled() {
        let analyzer = MetadataAnalyzer::new(
            MetadataAnalyzerConfig::new().with_profanity(false),
        );
        let analysis = analyzer.analyze_title("this has shit in it").unwrap();
        assert!(analysis.profanity_matches.is_empty());
    }

    #[test]
    fn test_extract_flagged_keywords() {
        let keywords = MetadataAnalyzer::extract_flagged_keywords(
            "This is SHOCKING and you won't believe it",
        );
        assert!(!keywords.is_empty());
        assert!(keywords.iter().any(|k| k.contains("shocking")));
    }

    #[test]
    fn test_no_false_positives_clean_text() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let text = "The quick brown fox jumps over the lazy dog. This is a completely innocent title about nature and wildlife photography.";
        let score = analyzer.detect_clickbait(text);
        assert!(score < 0.2, "clean text should have low score, got {}", score);
    }

    #[test]
    fn test_clickbait_one_weird_trick() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let score = analyzer.detect_clickbait("Doctors HATE This One Weird Trick for Weight Loss");
        assert!(score > 0.3, "score={}", score);
    }

    #[test]
    fn test_clickbait_numbered_list() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let score = analyzer.detect_clickbait("Top 10 Secret Places You Must Visit Before You Die");
        assert!(score > 0.15, "score={}", score);
    }

    #[test]
    fn test_metadata_analysis_bounds() {
        let analyzer = MetadataAnalyzer::new(MetadataAnalyzerConfig::default());
        let source = VideoSource::new("vid-3", "/tmp/test.mp4")
            .with_title("!!! EXTREME SHOCKING VIDEO !!! YOU WON'T BELIEVE !!!");
        let analysis = analyzer.analyze(&source).unwrap();
        assert!(analysis.clickbait_score >= 0.0 && analysis.clickbait_score <= 1.0);
        assert!(analysis.keyword_stuffing_score >= 0.0 && analysis.keyword_stuffing_score <= 1.0);
        assert!(analysis.misleading_score >= 0.0 && analysis.misleading_score <= 100.0);
        assert!(analysis.overall_risk_score >= 0.0 && analysis.overall_risk_score <= 100.0);
    }
}

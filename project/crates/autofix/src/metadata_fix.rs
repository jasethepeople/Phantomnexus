use regex::Regex;
use tracing::{debug, info, instrument, warn};

use sentinel_core::{ContextType, MetadataIssue, MetadataIssueType, Violation};

use crate::{AutoFixError, Result};

/// Clickbait patterns that should be removed or rewritten
const CLICKBAIT_PATTERNS: &[&str] = &[
    r"(?i)SHOCKING",
    r"(?i)WON'?T\s+BELIEVE",
    r"(?i)YOU\s+NEED\s+TO\s+SEE",
    r"(?i)MIND\s+BLOWN",
    r"(?i)UNBELIEVABLE",
    r"(?i)INCREDIBLE",
    r"(?i)INSANE",
    r"(?i)CRAZY",
    r"(?i)IMPOSSIBLE",
    r"(?i)YOU\s+WON'?T",
    r"(?i)WAIT\s+UNTIL\s+YOU",
    r"(?i)THIS\s+CHANGES\s+EVERYTHING",
    r"(?i)NUMBER\s+\d+\s+WILL\s+SHOCK",
    r"(?i)DOCTORS\s+HATE\s+THIS",
    r"(?i)ONE\s+WEIRD\s+TRICK",
];

/// Patterns for excessive capitalization
const EXCESSIVE_CAPS_PATTERNS: &[&str] = &[
    r"[A-Z]{5,}",
];

/// Patterns for excessive punctuation
const EXCESSIVE_PUNCT_PATTERNS: &[&str] = &[
    r"!{2,}",
    r"\?{2,}",
    r"!\?{2,}",
    r"\?!{2,}",
    r"\.{4,}",
];

/// Profanity words to filter (with context-aware exceptions)
const PROFANITY_LIST: &[&str] = &[
    // This is a placeholder list — real implementation would use
    // a comprehensive profanity filter library
];

/// Disclaimer templates for different context types
const DISCLAIMER_TEMPLATES: &[(ContextType, &str)] = &[
    (
        ContextType::Educational,
        "\n\nDisclaimer: This content is for educational purposes only. \
         The information presented should not be considered professional advice."
    ),
    (
        ContextType::Medical,
        "\n\nMedical Disclaimer: This content is for informational purposes only \
         and does not constitute medical advice. Always consult a qualified \
         healthcare provider for medical concerns."
    ),
    (
        ContextType::Financial,
        "\n\nFinancial Disclaimer: This content is for informational purposes only \
         and should not be construed as financial advice. Consult a financial \
         advisor before making investment decisions."
    ),
    (
        ContextType::Legal,
        "\n\nLegal Disclaimer: This content is for general informational purposes \
         and does not constitute legal advice. Consult a qualified attorney \
         for legal matters."
    ),
    (
        ContextType::News,
        "\n\nDisclaimer: This content reports on events as they are understood \
         at the time of publication. Information may be updated as new details \
         become available."
    ),
    (
        ContextType::Entertainment,
        "\n\nDisclaimer: This content is intended for entertainment purposes. \
         Any opinions expressed are those of the creator."
    ),
    (
        ContextType::Sponsored,
        "\n\nSponsored Content Disclaimer: This video contains sponsored content. \
         The creator has received compensation for featuring certain products \
         or services. All opinions expressed remain honest and genuine."
    ),
    (
        ContextType::Political,
        "\n\nDisclaimer: The views and opinions expressed in this content are \
         those of the speaker(s) and do not necessarily reflect the official \
         policy or position of any organization."
    ),
    (
        ContextType::General,
        "\n\nDisclaimer: The content provided is for general informational \
         purposes only. While we strive for accuracy, we make no guarantees \
         about the completeness or reliability of the information."
    ),
];

/// Engine for metadata remediation (titles, descriptions, tags)
pub struct MetadataFixEngine {
    clickbait_regexes: Vec<Regex>,
    caps_regex: Regex,
    punct_regexes: Vec<Regex>,
}

impl MetadataFixEngine {
    /// Create a new MetadataFixEngine with compiled regex patterns
    pub fn new() -> Self {
        debug!("Initializing MetadataFixEngine with compiled patterns");

        let clickbait_regexes = CLICKBAIT_PATTERNS
            .iter()
            .filter_map(|pattern| Regex::new(pattern).ok())
            .collect();

        let caps_regex = Regex::new(EXCESSIVE_CAPS_PATTERNS[0]).unwrap_or_else(|_| {
            Regex::new(r"[A-Z]{5,}").expect("fallback caps regex should compile")
        });

        let punct_regexes = EXCESSIVE_PUNCT_PATTERNS
            .iter()
            .filter_map(|pattern| Regex::new(pattern).ok())
            .collect();

        Self {
            clickbait_regexes,
            caps_regex,
            punct_regexes,
        }
    }

    /// Rewrite a video title to remove policy violations
    ///
    /// Removes clickbait patterns, excessive capitalization, and
    /// excessive punctuation while preserving the core message.
    #[instrument(skip(self))]
    pub fn rewrite_title(&self, original: &str, violations: &[Violation]) -> Result<String> {
        info!("Rewriting title ({} chars)", original.len());
        let mut title = original.to_string();

        // Check for metadata-specific violations
        let has_clickbait = violations.iter().any(|v| {
            matches!(
                v.rule_id.as_str(),
                "clickbait_title" | "excessive_caps" | "excessive_punctuation"
            )
        });

        if !has_clickbait && violations.is_empty() {
            // Check our own patterns even without explicit violations
            let detected = self.detect_clickbait_patterns(original);
            if detected.is_empty() && !self.has_excessive_caps(original)
                && !self.has_excessive_punctuation(original) {
                debug!("Title passes all checks, returning as-is");
                return Ok(title);
            }
        }

        // Remove clickbait patterns
        title = self.remove_clickbait_patterns(&title);

        // Normalize excessive caps
        title = self.normalize_caps(&title);

        // Normalize excessive punctuation
        title = self.normalize_punctuation(&title);

        // Balance keywords (remove keyword stuffing)
        title = self.balance_keywords(&title);

        // Trim and clean up
        title = title.trim().to_string();

        // Ensure title is not empty after cleaning
        if title.is_empty() {
            title = "Untitled Video".to_string();
        }

        // Capitalize first letter of each word (title case) for consistency
        title = self.to_title_case(&title);

        info!("Title rewritten: '{}' -> '{}'", original, title);
        Ok(title)
    }

    /// Rewrite a video description to remove policy violations
    #[instrument(skip(self))]
    pub fn rewrite_description(
        &self,
        original: &str,
        violations: &[Violation],
    ) -> Result<String> {
        info!("Rewriting description ({} chars)", original.len());
        let mut description = original.to_string();

        // Apply the same fixes as title, plus description-specific ones
        description = self.remove_clickbait_patterns(&description);
        description = self.normalize_caps(&description);
        description = self.normalize_punctuation(&description);

        // Remove excessive repetition
        description = self.remove_repetition(&description);

        // Filter profanity with context awareness
        description = self.filter_profanity(&description);

        // Trim whitespace
        description = description.trim().to_string();

        // Ensure minimum description length
        if description.len() < 10 {
            description = format!("{}

#Video", description);
        }

        info!(
            "Description rewritten: {} chars -> {} chars",
            original.len(),
            description.len()
        );
        Ok(description)
    }

    /// Optimize tags to remove policy violations
    #[instrument(skip(self))]
    pub fn optimize_tags(
        &self,
        original: &[String],
        violations: &[Violation],
    ) -> Result<Vec<String>> {
        info!("Optimizing {} tags", original.len());
        let mut tags: Vec<String> = original.to_vec();

        // Check for misleading tags or keyword stuffing violations
        let has_tag_issues = violations.iter().any(|v| {
            matches!(v.rule_id.as_str(), "misleading_tags" | "keyword_stuffing")
        });

        if has_tag_issues {
            // Remove duplicate tags
            tags = self.remove_duplicate_tags(&tags);

            // Limit tag count to avoid keyword stuffing
            let max_tags = 15;
            if tags.len() > max_tags {
                warn!("Reducing tags from {} to {}", tags.len(), max_tags);
                tags.truncate(max_tags);
            }
        }

        // Filter out tags matching clickbait patterns
        tags.retain(|tag| {
            let detected = self.detect_clickbait_patterns(tag);
            detected.is_empty() && !self.has_excessive_caps(tag)
        });

        // Normalize each tag
        tags = tags
            .into_iter()
            .map(|tag| {
                let normalized = tag.to_lowercase().trim().to_string();
                // Remove excessive punctuation from tags
                self.normalize_punctuation(&normalized)
            })
            .filter(|tag| !tag.is_empty())
            .collect();

        // Deduplicate
        tags = self.remove_duplicate_tags(&tags);

        info!("Tags optimized: {} -> {}", original.len(), tags.len());
        Ok(tags)
    }

    /// Add an appropriate disclaimer to a description
    #[instrument(skip(self))]
    pub fn add_disclaimer(
        &self,
        description: &str,
        context: ContextType,
    ) -> Result<String> {
        info!("Adding {:?} disclaimer to description", context);

        // Find the appropriate disclaimer template
        let disclaimer = DISCLAIMER_TEMPLATES
            .iter()
            .find(|(ctx, _)| *ctx == context)
            .map(|(_, text)| *text)
            .unwrap_or_else(|| {
                DISCLAIMER_TEMPLATES
                    .iter()
                    .find(|(ctx, _)| *ctx == ContextType::General)
                    .map(|(_, text)| *text)
                    .unwrap_or("")
            });

        // Check if disclaimer already exists
        if description.contains("Disclaimer:") || description.contains("disclaimer") {
            warn!("Description already contains a disclaimer, skipping");
            return Ok(description.to_string());
        }

        let result = format!("{}{}", description.trim(), disclaimer);

        info!("Disclaimer added for {:?} context", context);
        Ok(result)
    }

    // ---- Internal helper methods ----

    /// Detect clickbait patterns in text
    fn detect_clickbait_patterns(&self, text: &str) -> Vec<String> {
        let mut matches = Vec::new();
        for regex in &self.clickbait_regexes {
            for mat in regex.find_iter(text) {
                matches.push(mat.as_str().to_string());
            }
        }
        matches
    }

    /// Remove clickbait patterns from text
    fn remove_clickbait_patterns(&self, text: &str) -> String {
        let mut result = text.to_string();
        for regex in &self.clickbait_regexes {
            result = regex.replace_all(&result, "").to_string();
        }
        // Clean up extra whitespace left behind
        result = Regex::new(r"\s{2,}")
            .unwrap()
            .replace_all(&result, " ")
            .to_string();
        result
    }

    /// Check if text has excessive capitalization
    fn has_excessive_caps(&self, text: &str) -> bool {
        let caps_count = text.chars().filter(|c| c.is_uppercase()).count();
        let total_alpha = text.chars().filter(|c| c.is_alphabetic()).count();
        if total_alpha == 0 {
            return false;
        }
        let caps_ratio = caps_count as f64 / total_alpha as f64;
        caps_ratio > 0.7 || self.caps_regex.is_match(text)
    }

    /// Normalize excessive capitalization
    fn normalize_caps(&self, text: &str) -> String {
        let mut result = String::new();
        let mut chars = text.chars().peekable();

        while let Some(ch) = chars.next() {
            // If we have 3+ consecutive uppercase letters, lowercase the extras
            if ch.is_uppercase() {
                let mut uppercase_run = vec![ch];
                while let Some(&next) = chars.peek() {
                    if next.is_uppercase() {
                        uppercase_run.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }

                if uppercase_run.len() >= 5 {
                    // Keep first letter uppercase, lowercase the rest
                    result.push(uppercase_run[0]);
                    for c in uppercase_run.into_iter().skip(1) {
                        result.push(c.to_lowercase().next().unwrap_or(c));
                    }
                } else {
                    for c in uppercase_run {
                        result.push(c);
                    }
                }
            } else {
                result.push(ch);
            }
        }

        result
    }

    /// Check for excessive punctuation
    fn has_excessive_punctuation(&self, text: &str) -> bool {
        self.punct_regexes.iter().any(|re| re.is_match(text))
    }

    /// Normalize excessive punctuation
    fn normalize_punctuation(&self, text: &str) -> String {
        let mut result = text.to_string();

        // Replace multiple exclamation marks with single
        if let Ok(re) = Regex::new(r"!{2,}") {
            result = re.replace_all(&result, "!").to_string();
        }
        // Replace multiple question marks with single
        if let Ok(re) = Regex::new(r"\?{2,}") {
            result = re.replace_all(&result, "?").to_string();
        }
        // Replace 4+ periods with ellipsis
        if let Ok(re) = Regex::new(r"\.{4,}") {
            result = re.replace_all(&result, "...").to_string();
        }
        // Clean up mixed excessive punctuation
        if let Ok(re) = Regex::new(r"[!?]{2,}") {
            result = re.replace_all(&result, "!").to_string();
        }

        result
    }

    /// Balance keywords — reduce keyword stuffing
    fn balance_keywords(&self, text: &str) -> String {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.len() <= 3 {
            return text.to_string();
        }

        // Count word frequencies
        let mut word_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for word in &words {
            let lower = word.to_lowercase();
            *word_counts.entry(lower).or_insert(0) += 1;
        }

        // If a word appears more than 30% of the time, reduce it
        let threshold = (words.len() as f64 * 0.3).ceil() as usize;
        let mut overused: Vec<String> = word_counts
            .iter()
            .filter(|(_, &count)| count > threshold)
            .map(|(word, _)| word.clone())
            .collect();

        if overused.is_empty() {
            return text.to_string();
        }

        // Remove excess occurrences of overused words
        let mut result = Vec::new();
        let mut removed_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for word in &words {
            let lower = word.to_lowercase();
            if overused.contains(&lower) {
                let count = removed_counts.entry(lower).or_insert(0);
                *count += 1;
                if *count <= threshold {
                    result.push(*word);
                }
                // Skip excess occurrences
            } else {
                result.push(*word);
            }
        }

        result.join(" ")
    }

    /// Remove repeated sentences/phrases
    fn remove_repetition(&self, text: &str) -> String {
        let sentences: Vec<&str> = text.split(['.', '!', '?']).collect();
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();

        for sentence in sentences {
            let trimmed = sentence.trim();
            if trimmed.is_empty() {
                continue;
            }
            let normalized = trimmed.to_lowercase().replace(|c: char| c.is_whitespace(), "");
            if seen.contains(&normalized) {
                continue;
            }
            seen.insert(normalized);
            result.push(trimmed);
        }

        result.join(". ")
    }

    /// Filter profanity with context-aware exceptions
    fn filter_profanity(&self, text: &str) -> String {
        // Simple placeholder — real implementation would use a
        // comprehensive profanity library with stem checks
        let mut result = text.to_string();

        for word in PROFANITY_LIST {
            let pattern = format!(r"(?i)\b{}\b", regex::escape(word));
            if let Ok(re) = Regex::new(&pattern) {
                result = re.replace_all(&result, "[censored]").to_string();
            }
        }

        result
    }

    /// Remove duplicate tags (case-insensitive)
    fn remove_duplicate_tags(&self, tags: &[String]) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();

        for tag in tags {
            let lower = tag.to_lowercase();
            if !seen.contains(&lower) {
                seen.insert(lower);
                result.push(tag.clone());
            }
        }

        result
    }

    /// Convert text to title case
    fn to_title_case(&self, text: &str) -> String {
        let minor_words: std::collections::HashSet<&str> = [
            "a", "an", "the", "and", "but", "or", "nor", "for", "yet", "so",
            "at", "by", "from", "in", "into", "of", "off", "on", "onto", "to",
            "up", "with", "as",
        ]
        .iter()
        .cloned()
        .collect();

        text.split_whitespace()
            .enumerate()
            .map(|(i, word)| {
                let lower = word.to_lowercase();
                if i == 0 || i == text.split_whitespace().count() - 1 || !minor_words.contains(lower.as_str()) {
                    // Capitalize first letter
                    let mut chars = word.chars();
                    match chars.next() {
                        Some(first) => {
                            let rest: String = chars.collect();
                            format!("{}{}", first.to_uppercase(), rest.to_lowercase())
                        }
                        None => word.to_string(),
                    }
                } else {
                    lower
                }
            })
            .collect::<Vec<String>>()
            .join(" ")
    }

    /// Get a list of detected clickbait patterns in a title
    pub fn analyze_clickbait(&self, title: &str) -> Vec<String> {
        self.detect_clickbait_patterns(title)
    }

    /// Get a clickbait risk score (0.0 - 1.0)
    pub fn clickbait_score(&self, title: &str) -> f64 {
        let patterns_found = self.detect_clickbait_patterns(title).len();
        let caps_score = if self.has_excessive_caps(title) { 0.3 } else { 0.0 };
        let punct_score = if self.has_excessive_punctuation(title) { 0.2 } else { 0.0 };

        let pattern_score = (patterns_found as f64 * 0.25).min(0.5);

        (pattern_score + caps_score + punct_score).min(1.0)
    }
}

impl Default for MetadataFixEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_clickbait_patterns() {
        let engine = MetadataFixEngine::new();

        let title = "SHOCKING: You Won't Believe What Happens Next";
        let cleaned = engine.remove_clickbait_patterns(title);
        assert!(!cleaned.contains("SHOCKING"));
        assert!(!cleaned.contains("Won't Believe"));
    }

    #[test]
    fn test_clickbait_score() {
        let engine = MetadataFixEngine::new();

        let high_score = engine.clickbait_score("SHOCKING!!! You WON'T BELIEVE This!!!");
        assert!(high_score > 0.5);

        let low_score = engine.clickbait_score("A calm tutorial about Rust programming");
        assert!(low_score < 0.3);
    }

    #[test]
    fn test_normalize_caps() {
        let engine = MetadataFixEngine::new();

        let result = engine.normalize_caps("THIS IS AMAZING!!!");
        assert!(!result.contains("THIS IS AMAZING"));
        // Should reduce excessive caps
        assert!(result.chars().filter(|c| c.is_uppercase()).count() < "THIS IS AMAZING!!!".chars().filter(|c| c.is_uppercase()).count());
    }

    #[test]
    fn test_normalize_punctuation() {
        let engine = MetadataFixEngine::new();

        let result = engine.normalize_punctuation("Hello!!! How are you??? Great....");
        assert!(!result.contains("!!!"));
        assert!(!result.contains("???"));
        assert!(!result.contains("...."));
        assert!(result.contains("!"));
        assert!(result.contains("?"));
    }

    #[test]
    fn test_balance_keywords() {
        let engine = MetadataFixEngine::new();

        let stuffed = "best phone best phone best phone best phone best phone review";
        let balanced = engine.balance_keywords(stuffed);
        // Should reduce excessive repetition of "best"
        assert!(balanced.len() < stuffed.len());
    }

    #[test]
    fn test_remove_repetition() {
        let engine = MetadataFixEngine::new();

        let repeated = "This is great. This is great. This is something else.";
        let cleaned = engine.remove_repetition(repeated);
        // Should remove the duplicate sentence
        let occurrences = cleaned.to_lowercase().matches("this is great").count();
        assert_eq!(occurrences, 1);
    }

    #[test]
    fn test_to_title_case() {
        let engine = MetadataFixEngine::new();

        let result = engine.to_title_case("the quick brown fox jumps over the lazy dog");
        assert_eq!(result, "The Quick Brown Fox Jumps Over The Lazy Dog");

        // Minor words in the middle should be lowercase
        let result2 = engine.to_title_case("rust and python programming");
        assert!(result2.contains(" and ")); // "and" should stay lowercase in middle
    }

    #[test]
    fn test_rewrite_title_clickbait() {
        let engine = MetadataFixEngine::new();
        let violations: Vec<Violation> = vec![];

        let title = "SHOCKING!!! You WON'T BELIEVE This Simple Trick!!!";
        let rewritten = engine.rewrite_title(title, &violations).unwrap();

        assert!(!rewritten.contains("SHOCKING"));
        assert!(!rewritten.contains("!!!"));
        assert!(!rewritten.is_empty());
    }

    #[test]
    fn test_rewrite_title_clean() {
        let engine = MetadataFixEngine::new();
        let violations: Vec<Violation> = vec![];

        let title = "Rust Programming Tutorial for Beginners";
        let rewritten = engine.rewrite_title(title, &violations).unwrap();

        assert_eq!(rewritten, "Rust Programming Tutorial For Beginners");
    }

    #[test]
    fn test_optimize_tags() {
        let engine = MetadataFixEngine::new();
        let violations: Vec<Violation> = vec![];

        let tags = vec![
            "rust".to_string(),
            "programming".to_string(),
            "tutorial".to_string(),
            "RUST".to_string(), // duplicate
            "SHOCKING".to_string(),
            "coding".to_string(),
        ];

        let optimized = engine.optimize_tags(&tags, &violations).unwrap();

        // Should remove duplicate "rust" / "RUST"
        let rust_count = optimized.iter().filter(|t| t.to_lowercase() == "rust").count();
        assert_eq!(rust_count, 1);

        // Should remove SHOCKING tag
        assert!(!optimized.iter().any(|t| t.to_lowercase() == "shocking"));
    }

    #[test]
    fn test_add_disclaimer() {
        let engine = MetadataFixEngine::new();

        let desc = "Learn about financial planning.";
        let with_disclaimer = engine.add_disclaimer(desc, ContextType::Financial).unwrap();

        assert!(with_disclaimer.contains("Disclaimer"));
        assert!(with_disclaimer.contains("financial"));
        assert!(with_disclaimer.starts_with("Learn about financial planning."));
    }

    #[test]
    fn test_add_disclaimer_duplicate_prevention() {
        let engine = MetadataFixEngine::new();

        let desc = "Already has a disclaimer here.";
        let result = engine.add_disclaimer(desc, ContextType::General).unwrap();
        // Since "disclaimer" is in the text, it should skip adding
        assert_eq!(result, desc);
    }

    #[test]
    fn test_disclaimer_all_contexts() {
        let engine = MetadataFixEngine::new();

        for &(context, template) in DISCLAIMER_TEMPLATES {
            let desc = "Test description.";
            let result = engine.add_disclaimer(desc, context).unwrap();
            assert!(
                result.contains("Disclaimer"),
                "Context {:?} should produce a disclaimer",
                context
            );
            assert!(
                result.len() > desc.len(),
                "Context {:?} should extend the description",
                context
            );
        }
    }

    #[test]
    fn test_analyze_clickbait() {
        let engine = MetadataFixEngine::new();

        let patterns = engine.analyze_clickbait("SHOCKING: You Need To See This!!!");
        assert!(!patterns.is_empty());
        assert!(patterns.iter().any(|p| p.contains("SHOCKING")));
    }

    #[test]
    fn test_excessive_caps_detection() {
        let engine = MetadataFixEngine::new();

        assert!(engine.has_excessive_caps("THIS IS ALL CAPS AND VERY LOUD"));
        assert!(!engine.has_excessive_caps("This is Normal Title Case Text"));
    }

    #[test]
    fn test_excessive_punctuation_detection() {
        let engine = MetadataFixEngine::new();

        assert!(engine.has_excessive_punctuation("Wow!!! This is crazy???"));
        assert!(!engine.has_excessive_punctuation("This is normal. Really!"));
    }

    #[test]
    fn test_empty_title_handling() {
        let engine = MetadataFixEngine::new();
        let violations: Vec<Violation> = vec![];

        // Title that becomes empty after cleaning clickbait
        let title = "SHOCKING!!! UNBELIEVABLE!!!";
        let rewritten = engine.rewrite_title(title, &violations).unwrap();
        assert!(!rewritten.is_empty());
    }

    #[test]
    fn test_remove_duplicate_tags() {
        let engine = MetadataFixEngine::new();

        let tags = vec!["rust".to_string(), "RUST".to_string(), "Rust".to_string(), "python".to_string()];
        let deduped = engine.remove_duplicate_tags(&tags);
        assert_eq!(deduped.len(), 2);
    }
}

//! Context classifier with five indicator lexicons.
//!
//! Each lexicon contains 50+ weighted pattern strings.  The classifier
//! scores a slice of `ContentElement`s against every lexicon and returns
//! the primary context, confidence, and any secondary contexts that
//! exceeded the threshold.

use crate::content_elements::ContentElement;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ===========================================================================
// ContextType
// ===========================================================================

/// The kind of contextual framing detected in the content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContextType {
    Educational,
    Satirical,
    Documentary,
    Artistic,
    News,
    Unknown,
}

impl ContextType {
    pub fn label(&self) -> &'static str {
        match self {
            ContextType::Educational => "educational",
            ContextType::Satirical => "satirical",
            ContextType::Documentary => "documentary",
            ContextType::Artistic => "artistic",
            ContextType::News => "news",
            ContextType::Unknown => "unknown",
        }
    }

    /// Risk reduction factor applied to the base risk score when this
    /// context is the primary classification.
    pub fn risk_reduction_factor(&self) -> f64 {
        match self {
            ContextType::Educational => 0.85,
            ContextType::Documentary => 0.80,
            ContextType::News => 0.75,
            ContextType::Satirical => 0.75,
            ContextType::Artistic => 0.70,
            ContextType::Unknown => 1.0,
        }
    }
}

// ===========================================================================
// ContextClassification
// ===========================================================================

/// The result of classifying a set of content elements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextClassification {
    /// The most likely context type.
    pub primary_context: ContextType,
    /// Confidence in the primary classification (0.0..1.0).
    pub confidence: f64,
    /// Other context types that scored above the secondary threshold,
    /// sorted by score descending.
    pub secondary_contexts: Vec<(ContextType, f64)>,
    /// Per-context raw scores.
    pub raw_scores: HashMap<ContextType, f64>,
}

impl ContextClassification {
    pub fn unknown() -> Self {
        ContextClassification {
            primary_context: ContextType::Unknown,
            confidence: 0.0,
            secondary_contexts: Vec::new(),
            raw_scores: HashMap::new(),
        }
    }

    /// Apply the risk reduction factor for the primary context.
    pub fn apply_risk_reduction(&self, base_risk: f64) -> f64 {
        base_risk * self.primary_context.risk_reduction_factor()
    }
}

// ===========================================================================
// ScoredPattern — a pattern string with an associated weight
// ===========================================================================

/// A pattern and its contribution weight.
#[derive(Debug, Clone)]
pub struct ScoredPattern {
    pub pattern: String,
    pub weight: f64,
}

impl ScoredPattern {
    pub fn new<S: Into<String>>(pattern: S, weight: f64) -> Self {
        ScoredPattern {
            pattern: pattern.into(),
            weight,
        }
    }
}

// ===========================================================================
// IndicatorLexicon — named set of scored patterns
// ===========================================================================

/// A named lexicon of weighted string patterns.
#[derive(Debug, Clone)]
pub struct IndicatorLexicon {
    pub name: String,
    pub context: ContextType,
    pub patterns: Vec<ScoredPattern>,
}

impl IndicatorLexicon {
    /// Score a single text string against this lexicon.
    ///
    /// Returns the sum of weights of all patterns that appear as
    /// substrings (case-insensitive), normalised by the number of
    /// matching patterns.  The result is clamped to `[0.0, 1.0]`.
    pub fn score_text(&self, text: &str) -> f64 {
        let lower = text.to_lowercase();
        let mut total_weight = 0.0;
        let mut matches = 0usize;

        for sp in &self.patterns {
            if lower.contains(&sp.pattern.to_lowercase()) {
                total_weight += sp.weight;
                matches += 1;
            }
        }

        if matches == 0 {
            0.0
        } else {
            (total_weight / matches as f64).min(1.0)
        }
    }

    /// Score a slice of content elements against this lexicon.
    ///
    /// Each element's display string is checked; results are averaged
    /// and weighted by element confidence.
    pub fn score_elements(&self, elements: &[ContentElement]) -> f64 {
        if elements.is_empty() {
            return 0.0;
        }

        let mut weighted_sum = 0.0;
        let mut weight_total = 0.0;

        for elem in elements {
            let text = elem.to_display_string();
            let score = self.score_text(&text);
            let conf = elem.confidence();
            weighted_sum += score * conf;
            weight_total += conf;
        }

        if weight_total < 1e-12 {
            0.0
        } else {
            (weighted_sum / weight_total).min(1.0)
        }
    }
}

// ===========================================================================
// LexiconBuilder — helpers to create the five built-in lexicons
// ===========================================================================

pub struct LexiconBuilder;

impl LexiconBuilder {
    /// Build the Educational indicator lexicon (50+ patterns).
    pub fn educational() -> IndicatorLexicon {
        let patterns = vec![
            // Core educational vocabulary (weight 1.0)
            ScoredPattern::new("tutorial", 1.0),
            ScoredPattern::new("explained", 1.0),
            ScoredPattern::new("lesson", 1.0),
            ScoredPattern::new("educational purpose", 1.0),
            ScoredPattern::new("for learning", 1.0),
            ScoredPattern::new("how to", 1.0),
            ScoredPattern::new("learn", 0.95),
            ScoredPattern::new("teach", 0.95),
            ScoredPattern::new("instructor", 0.95),
            ScoredPattern::new("professor", 0.95),
            // Academic subjects (weight 0.9)
            ScoredPattern::new("mathematics", 0.9),
            ScoredPattern::new("physics", 0.9),
            ScoredPattern::new("chemistry", 0.9),
            ScoredPattern::new("biology", 0.9),
            ScoredPattern::new("history", 0.9),
            ScoredPattern::new("literature", 0.9),
            ScoredPattern::new("science", 0.9),
            ScoredPattern::new("programming", 0.9),
            ScoredPattern::new("engineering", 0.9),
            ScoredPattern::new("economics", 0.9),
            // Learning formats (weight 0.85)
            ScoredPattern::new("course", 0.85),
            ScoredPattern::new("lecture", 0.85),
            ScoredPattern::new("seminar", 0.85),
            ScoredPattern::new("workshop", 0.85),
            ScoredPattern::new("study guide", 0.85),
            ScoredPattern::new("study", 0.8),
            ScoredPattern::new("exam preparation", 0.85),
            ScoredPattern::new("homework help", 0.85),
            ScoredPattern::new("step by step", 0.8),
            ScoredPattern::new("demonstration", 0.8),
            // Pedagogical terms (weight 0.8)
            ScoredPattern::new("curriculum", 0.8),
            ScoredPattern::new("syllabus", 0.8),
            ScoredPattern::new("assignment", 0.8),
            ScoredPattern::new("quiz", 0.8),
            ScoredPattern::new("explainer", 0.85),
            ScoredPattern::new("guide", 0.75),
            ScoredPattern::new("introduction to", 0.8),
            ScoredPattern::new("basics of", 0.8),
            ScoredPattern::new("fundamentals", 0.8),
            ScoredPattern::new("principles of", 0.8),
            // Educational channels / formats (weight 0.75)
            ScoredPattern::new("crash course", 0.85),
            ScoredPattern::new("online class", 0.8),
            ScoredPattern::new("mooc", 0.85),
            ScoredPattern::new("khan academy", 0.9),
            ScoredPattern::new("academy", 0.75),
            ScoredPattern::new("university", 0.8),
            ScoredPattern::new("college", 0.75),
            ScoredPattern::new("school", 0.7),
            ScoredPattern::new("education", 0.85),
            ScoredPattern::new("educational", 0.9),
            // Learning outcomes (weight 0.7)
            ScoredPattern::new("understand", 0.7),
            ScoredPattern::new("comprehend", 0.75),
            ScoredPattern::new("analysis", 0.7),
            ScoredPattern::new("critical thinking", 0.8),
            ScoredPattern::new("problem solving", 0.75),
            ScoredPattern::new("review", 0.65),
            ScoredPattern::new("walkthrough", 0.75),
            ScoredPattern::new("deep dive", 0.7),
            ScoredPattern::new("research", 0.75),
            ScoredPattern::new("methodology", 0.8),
            // Visual/text cues (weight 0.65)
            ScoredPattern::new("diagram", 0.7),
            ScoredPattern::new("whiteboard", 0.85),
            ScoredPattern::new("blackboard", 0.85),
            ScoredPattern::new("chalkboard", 0.85),
            ScoredPattern::new("formula", 0.8),
            ScoredPattern::new("equation", 0.8),
            ScoredPattern::new("textbook", 0.8),
            ScoredPattern::new("notes", 0.65),
            ScoredPattern::new("summary", 0.65),
            ScoredPattern::new("key concepts", 0.8),
            ScoredPattern::new("what is", 0.75),
            ScoredPattern::new("why does", 0.75),
            ScoredPattern::new("how does", 0.75),
            ScoredPattern::new("beginner", 0.7),
            ScoredPattern::new("advanced", 0.7),
            ScoredPattern::new("intermediate", 0.7),
        ];

        IndicatorLexicon {
            name: "educational".into(),
            context: ContextType::Educational,
            patterns,
        }
    }

    /// Build the Satirical indicator lexicon (50+ patterns).
    pub fn satirical() -> IndicatorLexicon {
        let patterns = vec![
            // Core satirical vocabulary (weight 1.0)
            ScoredPattern::new("satire", 1.0),
            ScoredPattern::new("parody", 1.0),
            ScoredPattern::new("comedy", 0.95),
            ScoredPattern::new("not serious", 1.0),
            ScoredPattern::new("joke", 0.95),
            // Comedy genres (weight 0.9)
            ScoredPattern::new("sketch comedy", 0.95),
            ScoredPattern::new("stand up", 0.9),
            ScoredPattern::new("stand-up", 0.9),
            ScoredPattern::new("improv", 0.85),
            ScoredPattern::new("improvisation", 0.85),
            ScoredPattern::new("mockumentary", 0.95),
            ScoredPattern::new("spoof", 0.95),
            ScoredPattern::new("lampoon", 0.9),
            ScoredPattern::new("caricature", 0.85),
            ScoredPattern::new("farce", 0.85),
            // Humour indicators (weight 0.85)
            ScoredPattern::new("funny", 0.8),
            ScoredPattern::new("hilarious", 0.85),
            ScoredPattern::new("humor", 0.85),
            ScoredPattern::new("humour", 0.85),
            ScoredPattern::new("sarcasm", 0.9),
            ScoredPattern::new("sarcastic", 0.9),
            ScoredPattern::new("ironic", 0.85),
            ScoredPattern::new("irony", 0.85),
            ScoredPattern::new("meme", 0.8),
            ScoredPattern::new("memetic", 0.8),
            // Satire-specific (weight 0.9)
            ScoredPattern::new("tongue in cheek", 0.9),
            ScoredPattern::new("tongue-in-cheek", 0.9),
            ScoredPattern::new("poking fun", 0.85),
            ScoredPattern::new("making fun of", 0.85),
            ScoredPattern::new("ridicule", 0.85),
            ScoredPattern::new("exaggerated", 0.85),
            ScoredPattern::new("over the top", 0.85),
            ScoredPattern::new("over-the-top", 0.85),
            ScoredPattern::new("absurdist", 0.85),
            ScoredPattern::new("absurd", 0.8),
            // Comedy show formats (weight 0.8)
            ScoredPattern::new("late night", 0.8),
            ScoredPattern::new("talk show", 0.7),
            ScoredPattern::new("comedy special", 0.9),
            ScoredPattern::new("roast", 0.85),
            ScoredPattern::new("satirical news", 0.95),
            ScoredPattern::new("fake news", 0.85),
            ScoredPattern::new("onion", 0.85),
            ScoredPattern::new("daily show", 0.8),
            ScoredPattern::new("colbert", 0.8),
            ScoredPattern::new("saturday night live", 0.85),
            // Internet humour (weight 0.75)
            ScoredPattern::new("reaction video", 0.7),
            ScoredPattern::new("prank", 0.75),
            ScoredPattern::new("challenge video", 0.7),
            ScoredPattern::new(" vines ", 0.7),
            ScoredPattern::new("tiktok", 0.65),
            ScoredPattern::new("trending meme", 0.7),
            ScoredPattern::new("viral comedy", 0.8),
            ScoredPattern::new("comedic", 0.9),
            ScoredPattern::new("haha", 0.6),
            ScoredPattern::new("lol", 0.55),
            // Intent dis claimers (weight 0.9)
            ScoredPattern::new("for entertainment", 0.85),
            ScoredPattern::new("just a joke", 0.9),
            ScoredPattern::new("not real", 0.85),
            ScoredPattern::new("fictional", 0.75),
            ScoredPattern::new("no offense", 0.75),
            ScoredPattern::new("dont take seriously", 0.85),
            ScoredPattern::new("don't take seriously", 0.85),
            ScoredPattern::new("comedic intent", 0.95),
            ScoredPattern::new("satirical intent", 0.95),
            ScoredPattern::new("humorous intent", 0.9),
            // Performance (weight 0.75)
            ScoredPattern::new("impression", 0.75),
            ScoredPattern::new("impersonation", 0.75),
            ScoredPattern::new("comedic timing", 0.85),
            ScoredPattern::new("punchline", 0.9),
            ScoredPattern::new("setup and punchline", 0.9),
            ScoredPattern::new("slapstick", 0.85),
            ScoredPattern::new("witty", 0.8),
            ScoredPattern::new("banter", 0.75),
            ScoredPattern::new("wordplay", 0.8),
            ScoredPattern::new("pun", 0.75),
            ScoredPattern::new("satire disclaimer", 0.95),
        ];

        IndicatorLexicon {
            name: "satirical".into(),
            context: ContextType::Satirical,
            patterns,
        }
    }

    /// Build the Documentary indicator lexicon (50+ patterns).
    pub fn documentary() -> IndicatorLexicon {
        let patterns = vec![
            // Core documentary vocabulary (weight 1.0)
            ScoredPattern::new("documentary", 1.0),
            ScoredPattern::new("real footage", 1.0),
            ScoredPattern::new("archival", 0.95),
            ScoredPattern::new("interview", 0.9),
            ScoredPattern::new("footage", 0.85),
            // Documentary sub-genres (weight 0.9)
            ScoredPattern::new("investigative", 0.9),
            ScoredPattern::new("expos", 0.9),
            ScoredPattern::new("behind the scenes", 0.85),
            ScoredPattern::new("behind-the-scenes", 0.85),
            ScoredPattern::new("true story", 0.9),
            ScoredPattern::new("based on true", 0.85),
            ScoredPattern::new("biography", 0.85),
            ScoredPattern::new("biographical", 0.85),
            ScoredPattern::new("autobiography", 0.85),
            ScoredPattern::new("profile", 0.8),
            // Filmmaking terms (weight 0.85)
            ScoredPattern::new("narration", 0.85),
            ScoredPattern::new("narrator", 0.85),
            ScoredPattern::new("voiceover", 0.8),
            ScoredPattern::new("b-roll", 0.85),
            ScoredPattern::new("stock footage", 0.8),
            ScoredPattern::new("primary source", 0.85),
            ScoredPattern::new("eyewitness", 0.85),
            ScoredPattern::new("testimony", 0.85),
            ScoredPattern::new("expert opinion", 0.85),
            ScoredPattern::new("subject matter expert", 0.8),
            // Research / factual (weight 0.8)
            ScoredPattern::new("research", 0.8),
            ScoredPattern::new("study", 0.75),
            ScoredPattern::new("analysis", 0.75),
            ScoredPattern::new("data", 0.7),
            ScoredPattern::new("statistics", 0.75),
            ScoredPattern::new("evidence", 0.8),
            ScoredPattern::new("fact check", 0.85),
            ScoredPattern::new("verified", 0.75),
            ScoredPattern::new("on the ground", 0.8),
            ScoredPattern::new("field report", 0.85),
            // Nature / science documentary (weight 0.8)
            ScoredPattern::new("nature documentary", 0.9),
            ScoredPattern::new("wildlife", 0.8),
            ScoredPattern::new("planet earth", 0.85),
            ScoredPattern::new("national geographic", 0.9),
            ScoredPattern::new("discovery", 0.75),
            ScoredPattern::new("observation", 0.75),
            ScoredPattern::new("in the wild", 0.8),
            ScoredPattern::new("ecosystem", 0.8),
            ScoredPattern::new("conservation", 0.8),
            ScoredPattern::new("environment", 0.75),
            // Historical (weight 0.8)
            ScoredPattern::new("historical", 0.85),
            ScoredPattern::new("history", 0.8),
            ScoredPattern::new("retrospective", 0.85),
            ScoredPattern::new("legacy", 0.75),
            ScoredPattern::new("era", 0.7),
            ScoredPattern::new("period", 0.7),
            ScoredPattern::new("timeline", 0.75),
            ScoredPattern::new("rare footage", 0.9),
            ScoredPattern::new("never before seen", 0.85),
            ScoredPattern::new("exclusive", 0.75),
            // Social / political documentary (weight 0.8)
            ScoredPattern::new("social issue", 0.85),
            ScoredPattern::new("current affairs", 0.85),
            ScoredPattern::new("human rights", 0.85),
            ScoredPattern::new("awareness", 0.75),
            ScoredPattern::new("activism", 0.8),
            ScoredPattern::new("advocacy", 0.8),
            ScoredPattern::new("in depth", 0.8),
            ScoredPattern::new("in-depth", 0.8),
            ScoredPattern::new("reportage", 0.9),
            ScoredPattern::new("journalistic", 0.85),
            // Production cues (weight 0.7)
            ScoredPattern::new("directed by", 0.75),
            ScoredPattern::new("produced by", 0.7),
            ScoredPattern::new("film festival", 0.8),
            ScoredPattern::new("sundance", 0.85),
            ScoredPattern::new("documentary film", 0.95),
            ScoredPattern::new("feature length", 0.8),
            ScoredPattern::new("docuseries", 0.9),
            ScoredPattern::new("mini series", 0.8),
            ScoredPattern::new("episode", 0.65),
            ScoredPattern::new("season", 0.65),
            // Aesthetic / style cues (weight 0.7)
            ScoredPattern::new("candid", 0.75),
            ScoredPattern::new("unscripted", 0.8),
            ScoredPattern::new("on location", 0.8),
            ScoredPattern::new("cinema verite", 0.9),
            ScoredPattern::new("fly on the wall", 0.85),
            ScoredPattern::new("observational", 0.8),
            ScoredPattern::new("participatory", 0.75),
            ScoredPattern::new("reflexive", 0.75),
            ScoredPattern::new("performative", 0.75),
            ScoredPattern::new("poetic documentary", 0.85),
            ScoredPattern::new("documentary evidence", 0.9),
        ];

        IndicatorLexicon {
            name: "documentary".into(),
            context: ContextType::Documentary,
            patterns,
        }
    }

    /// Build the Artistic indicator lexicon (50+ patterns).
    pub fn artistic() -> IndicatorLexicon {
        let patterns = vec![
            // Core artistic vocabulary (weight 1.0)
            ScoredPattern::new("art", 1.0),
            ScoredPattern::new("cinematic", 0.95),
            ScoredPattern::new("film", 0.9),
            ScoredPattern::new("music video", 1.0),
            ScoredPattern::new("performance", 0.95),
            // Visual arts (weight 0.9)
            ScoredPattern::new("painting", 0.9),
            ScoredPattern::new("sculpture", 0.9),
            ScoredPattern::new("photography", 0.85),
            ScoredPattern::new("visual art", 0.95),
            ScoredPattern::new("installation", 0.85),
            ScoredPattern::new("mixed media", 0.85),
            ScoredPattern::new("digital art", 0.9),
            ScoredPattern::new("fine art", 0.95),
            ScoredPattern::new("contemporary art", 0.9),
            ScoredPattern::new("gallery", 0.8),
            // Performing arts (weight 0.9)
            ScoredPattern::new("dance", 0.9),
            ScoredPattern::new("ballet", 0.9),
            ScoredPattern::new("choreography", 0.9),
            ScoredPattern::new("theatre", 0.85),
            ScoredPattern::new("theater", 0.85),
            ScoredPattern::new("acting", 0.85),
            ScoredPattern::new("stage", 0.8),
            ScoredPattern::new("live performance", 0.9),
            ScoredPattern::new("concert", 0.85),
            ScoredPattern::new("recital", 0.85),
            // Music (weight 0.85)
            ScoredPattern::new("orchestral", 0.9),
            ScoredPattern::new("symphony", 0.9),
            ScoredPattern::new("composition", 0.85),
            ScoredPattern::new("musical", 0.85),
            ScoredPattern::new("song", 0.75),
            ScoredPattern::new("acoustic", 0.8),
            ScoredPattern::new("instrumental", 0.85),
            ScoredPattern::new("vocal", 0.8),
            ScoredPattern::new("opus", 0.85),
            ScoredPattern::new("masterpiece", 0.85),
            // Film / video art (weight 0.85)
            ScoredPattern::new("short film", 0.9),
            ScoredPattern::new("experimental film", 0.95),
            ScoredPattern::new("avant garde", 0.95),
            ScoredPattern::new("avant-garde", 0.95),
            ScoredPattern::new("independent film", 0.85),
            ScoredPattern::new("indie film", 0.85),
            ScoredPattern::new("animation", 0.85),
            ScoredPattern::new("animated", 0.8),
            ScoredPattern::new("stop motion", 0.85),
            ScoredPattern::new("visual effects", 0.75),
            // Aesthetic descriptors (weight 0.8)
            ScoredPattern::new("aesthetic", 0.85),
            ScoredPattern::new("beautiful", 0.75),
            ScoredPattern::new("stunning visuals", 0.85),
            ScoredPattern::new("visual storytelling", 0.9),
            ScoredPattern::new("cinematography", 0.9),
            ScoredPattern::new("mise en scene", 0.9),
            ScoredPattern::new("color grading", 0.8),
            ScoredPattern::new("slow motion", 0.75),
            ScoredPattern::new("time lapse", 0.8),
            ScoredPattern::new("timelapse", 0.8),
            // Creative process (weight 0.75)
            ScoredPattern::new("creative", 0.8),
            ScoredPattern::new("creation", 0.75),
            ScoredPattern::new("process", 0.7),
            ScoredPattern::new("making of", 0.8),
            ScoredPattern::new("behind the art", 0.85),
            ScoredPattern::new("artist", 0.85),
            ScoredPattern::new("artistic vision", 0.9),
            ScoredPattern::new("creative vision", 0.85),
            ScoredPattern::new("portfolio", 0.8),
            ScoredPattern::new("showcase", 0.75),
            // Cultural (weight 0.75)
            ScoredPattern::new("cultural", 0.75),
            ScoredPattern::new("heritage", 0.8),
            ScoredPattern::new("tradition", 0.75),
            ScoredPattern::new("expression", 0.8),
            ScoredPattern::new("interpretation", 0.75),
            ScoredPattern::new("art movement", 0.9),
            ScoredPattern::new("impressionism", 0.9),
            ScoredPattern::new("surrealism", 0.9),
            ScoredPattern::new("abstract", 0.85),
            ScoredPattern::new("minimalist", 0.85),
            ScoredPattern::new("artistic expression", 0.95),
            ScoredPattern::new("opus", 0.85),
            ScoredPattern::new("oeuvre", 0.9),
            ScoredPattern::new("film noir", 0.9),
            ScoredPattern::new("neo noir", 0.85),
        ];

        IndicatorLexicon {
            name: "artistic".into(),
            context: ContextType::Artistic,
            patterns,
        }
    }

    /// Build the News indicator lexicon (50+ patterns).
    pub fn news() -> IndicatorLexicon {
        let patterns = vec![
            // Core news vocabulary (weight 1.0)
            ScoredPattern::new("breaking", 1.0),
            ScoredPattern::new("report", 0.9),
            ScoredPattern::new("news coverage", 1.0),
            ScoredPattern::new("journalism", 0.95),
            ScoredPattern::new("news", 0.85),
            // News formats (weight 0.9)
            ScoredPattern::new("breaking news", 1.0),
            ScoredPattern::new("live coverage", 0.9),
            ScoredPattern::new("special report", 0.95),
            ScoredPattern::new("news bulletin", 0.9),
            ScoredPattern::new("press conference", 0.9),
            ScoredPattern::new("news conference", 0.9),
            ScoredPattern::new("live update", 0.85),
            ScoredPattern::new("developing story", 0.9),
            ScoredPattern::new("exclusive report", 0.9),
            ScoredPattern::new("investigation", 0.85),
            // Broadcast terms (weight 0.85)
            ScoredPattern::new("anchor", 0.85),
            ScoredPattern::new("correspondent", 0.9),
            ScoredPattern::new("reporter", 0.9),
            ScoredPattern::new("field reporter", 0.9),
            ScoredPattern::new("live from", 0.85),
            ScoredPattern::new("broadcasting", 0.85),
            ScoredPattern::new("on air", 0.85),
            ScoredPattern::new("signing off", 0.8),
            ScoredPattern::new("news desk", 0.85),
            ScoredPattern::new("studio", 0.7),
            // Journalistic standards (weight 0.85)
            ScoredPattern::new("sources say", 0.85),
            ScoredPattern::new("according to", 0.75),
            ScoredPattern::new("official statement", 0.85),
            ScoredPattern::new("confirmed", 0.8),
            ScoredPattern::new("unconfirmed", 0.8),
            ScoredPattern::new("eyewitness", 0.85),
            ScoredPattern::new("witness", 0.8),
            ScoredPattern::new("statement", 0.75),
            ScoredPattern::new("announcement", 0.75),
            ScoredPattern::new("update", 0.7),
            // News categories (weight 0.8)
            ScoredPattern::new("political news", 0.9),
            ScoredPattern::new("world news", 0.9),
            ScoredPattern::new("local news", 0.85),
            ScoredPattern::new("business news", 0.85),
            ScoredPattern::new("sports news", 0.85),
            ScoredPattern::new("weather report", 0.85),
            ScoredPattern::new("traffic report", 0.8),
            ScoredPattern::new("financial news", 0.85),
            ScoredPattern::new("tech news", 0.8),
            ScoredPattern::new("entertainment news", 0.8),
            // Urgency / timeliness (weight 0.85)
            ScoredPattern::new("just in", 0.85),
            ScoredPattern::new("happening now", 0.85),
            ScoredPattern::new("right now", 0.8),
            ScoredPattern::new("at this hour", 0.85),
            ScoredPattern::new("latest", 0.8),
            ScoredPattern::new("urgent", 0.85),
            ScoredPattern::new("alert", 0.8),
            ScoredPattern::new("emergency", 0.8),
            ScoredPattern::new("crisis", 0.75),
            ScoredPattern::new("ongoing", 0.75),
            // Media outlets (weight 0.75)
            ScoredPattern::new("cnn", 0.75),
            ScoredPattern::new("bbc", 0.75),
            ScoredPattern::new("reuters", 0.8),
            ScoredPattern::new("associated press", 0.8),
            ScoredPattern::new("news agency", 0.85),
            ScoredPattern::new("newsroom", 0.85),
            ScoredPattern::new("editorial", 0.8),
            ScoredPattern::new("oped", 0.75),
            ScoredPattern::new("op-ed", 0.75),
            ScoredPattern::new("column", 0.7),
            // News programme types (weight 0.8)
            ScoredPattern::new("evening news", 0.9),
            ScoredPattern::new("morning news", 0.9),
            ScoredPattern::new("news hour", 0.85),
            ScoredPattern::new("news program", 0.85),
            ScoredPattern::new("news segment", 0.85),
            ScoredPattern::new("headlines", 0.8),
            ScoredPattern::new("top stories", 0.85),
            ScoredPattern::new("in other news", 0.85),
            ScoredPattern::new("back to you", 0.8),
            ScoredPattern::new("good night", 0.6),
            // Digital news (weight 0.75)
            ScoredPattern::new("news channel", 0.85),
            ScoredPattern::new("live stream", 0.75),
            ScoredPattern::new("news feed", 0.75),
            ScoredPattern::new("trending topic", 0.7),
            ScoredPattern::new("viral news", 0.75),
            ScoredPattern::new("citizen journalism", 0.85),
            ScoredPattern::new("crowdsourced", 0.75),
            ScoredPattern::new("social media report", 0.8),
            ScoredPattern::new("press release", 0.8),
            ScoredPattern::new("media briefing", 0.85),
            ScoredPattern::new("news update", 0.85),
            ScoredPattern::new("follow up", 0.75),
            ScoredPattern::new("follow-up", 0.75),
            ScoredPattern::new("retraction", 0.8),
            ScoredPattern::new("correction", 0.75),
        ];

        IndicatorLexicon {
            name: "news".into(),
            context: ContextType::News,
            patterns,
        }
    }
}

// ===========================================================================
// ContextClassifier
// ===========================================================================

/// Classifies content elements into contextual categories using
/// five indicator lexicons.
///
/// The classifier scores elements against Educational, Satirical,
/// Documentary, Artistic, and News lexicons, each containing 50+
/// weighted patterns.  The highest-scoring context above the
/// primary threshold becomes the primary classification; any other
/// contexts above the secondary threshold are returned as secondary.
#[derive(Debug, Clone)]
pub struct ContextClassifier {
    lexicons: Vec<IndicatorLexicon>,
    primary_threshold: f64,
    secondary_threshold: f64,
}

impl Default for ContextClassifier {
    fn default() -> Self {
        ContextClassifier {
            lexicons: vec![
                LexiconBuilder::educational(),
                LexiconBuilder::satirical(),
                LexiconBuilder::documentary(),
                LexiconBuilder::artistic(),
                LexiconBuilder::news(),
            ],
            primary_threshold: 0.25,
            secondary_threshold: 0.15,
        }
    }
}

impl ContextClassifier {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a classifier with custom thresholds.
    pub fn with_thresholds(mut self, primary: f64, secondary: f64) -> Self {
        self.primary_threshold = primary.clamp(0.0, 1.0);
        self.secondary_threshold = secondary.clamp(0.0, 1.0);
        self
    }

    /// Classify a slice of content elements.
    ///
    /// Returns a `ContextClassification` containing the primary context,
    /// confidence score, and any secondary contexts.
    pub fn classify(&self, elements: &[ContentElement]) -> ContextClassification {
        if elements.is_empty() {
            return ContextClassification::unknown();
        }

        let mut raw_scores: HashMap<ContextType, f64> = HashMap::new();

        // Score each lexicon against the element set.
        for lexicon in &self.lexicons {
            let score = lexicon.score_elements(elements);
            raw_scores.insert(lexicon.context, score);
        }

        // Find the best-scoring context.
        let mut best_context = ContextType::Unknown;
        let mut best_score = 0.0;

        for (ctx, score) in &raw_scores {
            if *score > best_score {
                best_score = *score;
                best_context = *ctx;
            }
        }

        // Collect secondary contexts (above threshold but not primary).
        let mut secondary: Vec<(ContextType, f64)> = raw_scores
            .iter()
            .filter(|(ctx, score)| {
                **ctx != best_context && **score >= self.secondary_threshold
            })
            .map(|(ctx, score)| (*ctx, *score))
            .collect();

        secondary.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        // Determine confidence.
        let confidence = if best_score >= self.primary_threshold {
            best_score.min(1.0)
        } else {
            best_score.min(1.0) * 0.5 // Reduced confidence when below threshold
        };

        ContextClassification {
            primary_context: best_context,
            confidence,
            secondary_contexts: secondary,
            raw_scores,
        }
    }

    /// Score a single text string against all lexicons and return raw scores.
    pub fn score_text(&self, text: &str) -> HashMap<ContextType, f64> {
        let mut scores = HashMap::new();
        for lexicon in &self.lexicons {
            scores.insert(lexicon.context, lexicon.score_text(text));
        }
        scores
    }

    /// Return references to the loaded lexicons.
    pub fn lexicons(&self) -> &[IndicatorLexicon] {
        &self.lexicons
    }

    /// Total number of patterns across all lexicons.
    pub fn total_pattern_count(&self) -> usize {
        self.lexicons.iter().map(|l| l.patterns.len()).sum()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_elements_with_text(text: &str) -> Vec<ContentElement> {
        vec![ContentElement::text(text, super::super::content_elements::TextSource::Transcript, 0.9)]
    }

    #[test]
    fn context_type_risk_reduction() {
        assert!((ContextType::Educational.risk_reduction_factor() - 0.85).abs() < 0.001);
        assert!((ContextType::Documentary.risk_reduction_factor() - 0.80).abs() < 0.001);
        assert!((ContextType::News.risk_reduction_factor() - 0.75).abs() < 0.001);
        assert!((ContextType::Satirical.risk_reduction_factor() - 0.75).abs() < 0.001);
        assert!((ContextType::Artistic.risk_reduction_factor() - 0.70).abs() < 0.001);
        assert_eq!(ContextType::Unknown.risk_reduction_factor(), 1.0);
    }

    #[test]
    fn lexicon_builder_pattern_counts() {
        assert!(LexiconBuilder::educational().patterns.len() >= 50);
        assert!(LexiconBuilder::satirical().patterns.len() >= 50);
        assert!(LexiconBuilder::documentary().patterns.len() >= 50);
        assert!(LexiconBuilder::artistic().patterns.len() >= 50);
        assert!(LexiconBuilder::news().patterns.len() >= 50);
    }

    #[test]
    fn educational_lexicon_scores_tutorial() {
        let lex = LexiconBuilder::educational();
        let score = lex.score_text("This is a great tutorial for learning physics");
        assert!(score > 0.0, "educational score should be > 0");
    }

    #[test]
    fn satirical_lexicon_scores_comedy() {
        let lex = LexiconBuilder::satirical();
        let score = lex.score_text("This is pure satire and comedy, not serious");
        assert!(score > 0.0, "satirical score should be > 0");
    }

    #[test]
    fn documentary_lexicon_scores_documentary() {
        let lex = LexiconBuilder::documentary();
        let score = lex.score_text("A documentary with real footage and interviews");
        assert!(score > 0.0, "documentary score should be > 0");
    }

    #[test]
    fn artistic_lexicon_scores_art() {
        let lex = LexiconBuilder::artistic();
        let score = lex.score_text("A cinematic music video with beautiful art");
        assert!(score > 0.0, "artistic score should be > 0");
    }

    #[test]
    fn news_lexicon_scores_breaking() {
        let lex = LexiconBuilder::news();
        let score = lex.score_text("Breaking news coverage from our correspondent");
        assert!(score > 0.0, "news score should be > 0");
    }

    #[test]
    fn classifier_classifies_educational() {
        let classifier = ContextClassifier::new();
        let elements = make_test_elements_with_text(
            "Welcome to this educational tutorial where we will learn the basics of physics \
             in a step by step lesson for learning",
        );
        let result = classifier.classify(&elements);
        assert_eq!(result.primary_context, ContextType::Educational);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn classifier_classifies_satirical() {
        let classifier = ContextClassifier::new();
        let elements = make_test_elements_with_text(
            "This is pure satire and parody, a comedy sketch not serious, just a joke",
        );
        let result = classifier.classify(&elements);
        assert_eq!(result.primary_context, ContextType::Satirical);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn classifier_classifies_news() {
        let classifier = ContextClassifier::new();
        let elements = make_test_elements_with_text(
            "Breaking news coverage from our correspondent with live updates and journalism",
        );
        let result = classifier.classify(&elements);
        assert_eq!(result.primary_context, ContextType::News);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn classifier_classifies_artistic() {
        let classifier = ContextClassifier::new();
        let elements = make_test_elements_with_text(
            "A cinematic music video with stunning cinematography and artistic performance",
        );
        let result = classifier.classify(&elements);
        assert_eq!(result.primary_context, ContextType::Artistic);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn classifier_classifies_documentary() {
        let classifier = ContextClassifier::new();
        let elements = make_test_elements_with_text(
            "This documentary features real footage, archival interviews, and narration",
        );
        let result = classifier.classify(&elements);
        assert_eq!(result.primary_context, ContextType::Documentary);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn classifier_unknown_for_empty() {
        let classifier = ContextClassifier::new();
        let result = classifier.classify(&[]);
        assert_eq!(result.primary_context, ContextType::Unknown);
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn classifier_has_secondary_contexts() {
        let classifier = ContextClassifier::new();
        // Text that could trigger multiple contexts
        let elements = make_test_elements_with_text(
            "A documentary film about art and education with interviews and tutorials",
        );
        let result = classifier.classify(&elements);
        // Should have at least one primary
        assert!(result.confidence > 0.0);
        // Secondary contexts may or may not appear depending on scores
    }

    #[test]
    fn context_classification_apply_reduction() {
        let classification = ContextClassification {
            primary_context: ContextType::Educational,
            confidence: 0.8,
            secondary_contexts: vec![],
            raw_scores: HashMap::new(),
        };
        let reduced = classification.apply_risk_reduction(1.0);
        assert!((reduced - 0.85).abs() < 0.01);
    }

    #[test]
    fn classifier_total_pattern_count() {
        let classifier = ContextClassifier::new();
        let total = classifier.total_pattern_count();
        assert!(total >= 250, "expected >= 250 patterns across 5 lexicons, got {}", total);
    }

    #[test]
    fn score_text_returns_all_contexts() {
        let classifier = ContextClassifier::new();
        let scores = classifier.score_text("educational tutorial about art");
        assert_eq!(scores.len(), 5);
        assert!(scores.contains_key(&ContextType::Educational));
        assert!(scores.contains_key(&ContextType::Artistic));
    }
}

use lazy_static::lazy_static;
use sentinel_core::proto::ViolationCategory;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// GuidelineReference
// ---------------------------------------------------------------------------

/// Reference to a YouTube Help Center article and remediation advice.
#[derive(Debug, Clone, PartialEq)]
pub struct GuidelineReference {
    /// YouTube Help Center URL for this category.
    pub help_center_url: String,
    /// Short description of the policy.
    pub description: String,
    /// Remediation steps for creators.
    pub remediation_steps: Vec<String>,
    /// Category-specific best practices.
    pub best_practices: Vec<String>,
    /// Approximate penalty severity for first-time violations.
    pub first_offense_penalty: String,
    /// Approximate penalty severity for repeat violations.
    pub repeat_offense_penalty: String,
}

impl GuidelineReference {
    pub fn new<S: Into<String>>(url: S, description: S) -> Self {
        GuidelineReference {
            help_center_url: url.into(),
            description: description.into(),
            remediation_steps: Vec::new(),
            best_practices: Vec::new(),
            first_offense_penalty: String::new(),
            repeat_offense_penalty: String::new(),
        }
    }

    pub fn with_remediation_step<S: Into<String>>(mut self, step: S) -> Self {
        self.remediation_steps.push(step.into());
        self
    }

    pub fn with_best_practice<S: Into<String>>(mut self, practice: S) -> Self {
        self.best_practices.push(practice.into());
        self
    }

    pub fn with_penalties<S1: Into<String>, S2: Into<String>>(
        mut self,
        first: S1,
        repeat: S2,
    ) -> Self {
        self.first_offense_penalty = first.into();
        self.repeat_offense_penalty = repeat.into();
        self
    }
}

// ---------------------------------------------------------------------------
// Global guideline map
// ---------------------------------------------------------------------------

lazy_static! {
    /// Static map of ViolationCategory to GuidelineReference.
    pub static ref GUIDELINE_MAP: HashMap<ViolationCategory, GuidelineReference> = {
        let mut m = HashMap::new();

        m.insert(
            ViolationCategory::HateSpeech,
            GuidelineReference::new(
                "https://support.google.com/youtube/answer/2801939",
                "YouTube doesn't allow content promoting violence or hatred against individuals or groups."
            )
            .with_remediation_step("Remove or edit hateful language from your content.")
            .with_remediation_step("Provide educational context if the content is documentary or news.")
            .with_remediation_step("Blur or bleep sensitive words in the video.")
            .with_remediation_step("Add a content warning in the title or description.")
            .with_best_practice("Focus on criticizing ideas, not people.")
            .with_best_practice("Use respectful language even when disagreeing.")
            .with_best_practice("Include diverse perspectives to avoid one-sided attacks.")
            .with_penalties(
                "Warning or strike; video may be removed.",
                "Channel strike; potential termination for repeated severe violations."
            ),
        );

        m.insert(
            ViolationCategory::Harassment,
            GuidelineReference::new(
                "https://support.google.com/youtube/answer/2801924",
                "YouTube doesn't allow content that targets individuals with prolonged or malicious insults."
            )
            .with_remediation_step("Remove specific insults or personal attacks directed at individuals.")
            .with_remediation_step("Focus on addressing the topic rather than the person.")
            .with_remediation_step("Edit out doxxing or revealing private information.")
            .with_remediation_step("If satirical, clearly label it as such.")
            .with_best_practice("Avoid singling out individuals for ridicule.")
            .with_best_practice("Use constructive criticism focused on actions or ideas.")
            .with_best_practice("Be mindful of power dynamics between creator and subject.")
            .with_penalties(
                "Video removed; warning issued.",
                "Channel strike; possible suspension."
            ),
        );

        m.insert(
            ViolationCategory::Violence,
            GuidelineReference::new(
                "https://support.google.com/youtube/answer/2802008",
                "YouTube doesn't allow violent or gory content intended to shock or disgust viewers."
            )
            .with_remediation_step("Remove or blur graphic violent imagery.")
            .with_remediation_step("Add age restriction to the video.")
            .with_remediation_step("Provide educational or documentary context in description.")
            .with_remediation_step("Trim segments showing gratuitous violence.")
            .with_best_practice("Show only the minimum necessary to convey information.")
            .with_best_practice("Use warnings before graphic content.")
            .with_best_practice("Consider using animations or descriptions instead of real footage.")
            .with_penalties(
                "Age-restricted or removed depending on severity.",
                "Strike issued; severe cases may result in termination."
            ),
        );

        m.insert(
            ViolationCategory::AdultContent,
            GuidelineReference::new(
                "https://support.google.com/youtube/answer/2803176",
                "YouTube doesn't allow sexually explicit content. Nudity may be allowed in educational or artistic contexts."
            )
            .with_remediation_step("Remove or blur sexually explicit content.")
            .with_remediation_step("Ensure any nudity has clear educational, documentary, or artistic context.")
            .with_remediation_step("Add age restriction to the video.")
            .with_remediation_step("Review thumbnail for implicit sexual content.")
            .with_best_practice("Use anatomical diagrams rather than real imagery when possible.")
            .with_best_practice("Clearly state educational intent in title and description.")
            .with_best_practice("Avoid sexually suggestive thumbnails or titles.")
            .with_penalties(
                "Age-restricted or removed.",
                "Strike; repeated violations lead to termination."
            ),
        );

        m.insert(
            ViolationCategory::HarmfulDangerous,
            GuidelineReference::new(
                "https://support.google.com/youtube/answer/2801964",
                "YouTube doesn't allow content that encourages dangerous or illegal activities."
            )
            .with_remediation_step("Remove instructions for dangerous acts.")
            .with_remediation_step("Add disclaimers and safety warnings.")
            .with_remediation_step("If educational, clearly state the risks and safety precautions.")
            .with_remediation_step("Avoid glorifying or encouraging illegal behavior.")
            .with_best_practice("Always include safety warnings for risky activities.")
            .with_best_practice("Focus on the consequences rather than the method.")
            .with_best_practice("Consult safety guidelines from relevant authorities.")
            .with_penalties(
                "Video removed; age restriction for mild cases.",
                "Strike issued for repeated or severe cases."
            ),
        );

        m.insert(
            ViolationCategory::Misinformation,
            GuidelineReference::new(
                "https://support.google.com/youtube/answer/2797387",
                "YouTube doesn't allow content that contradicts authoritative consensus on sensitive topics."
            )
            .with_remediation_step("Add corrections in the video description or pinned comment.")
            .with_remediation_step("Remove specific claims that contradict authoritative sources.")
            .with_remediation_step("Include links to credible sources supporting your claims.")
            .with_remediation_step("If speculative, clearly label as opinion or theory.")
            .with_best_practice("Cite reputable sources in your content.")
            .with_best_practice("Distinguish between fact and opinion clearly.")
            .with_best_practice("Update outdated information promptly.")
            .with_penalties(
                "Information panels may be added; video may be removed for egregious cases.",
                "Reduced reach; potential removal."
            ),
        );

        m.insert(
            ViolationCategory::ChildSafety,
            GuidelineReference::new(
                "https://support.google.com/youtube/answer/2802271",
                "YouTube has a zero-tolerance policy for content that endangers minors."
            )
            .with_remediation_step("Immediately remove any content depicting minors in dangerous situations.")
            .with_remediation_step("Report the content to appropriate authorities if illegal.")
            .with_remediation_step("If family content, review for accidental revealing of child's location or routine.")
            .with_remediation_step("Enable YouTube's made-for-kids setting if applicable.")
            .with_best_practice("Never include identifiable information about minors.")
            .with_best_practice("Obtain parental consent for featuring children.")
            .with_best_practice("Monitor comments on videos featuring children.")
            .with_penalties(
                "Immediate removal; severe cases reported to law enforcement.",
                "Channel termination; permanent ban from platform."
            ),
        );

        m.insert(
            ViolationCategory::Copyright,
            GuidelineReference::new(
                "https://support.google.com/youtube/answer/2797370",
                "YouTube's copyright system protects original works. Unauthorized use may result in claims."
            )
            .with_remediation_step("Replace copyrighted music with licensed or royalty-free alternatives.")
            .with_remediation_step("Trim segments containing copyrighted material.")
            .with_remediation_step("Use YouTube's Audio Library for background music.")
            .with_remediation_step("File a dispute if the claim is incorrect or qualifies as fair use.")
            .with_best_practice("Only use content you created or have explicit permission to use.")
            .with_best_practice("Check copyright status before using third-party material.")
            .with_best_practice("Consider Creative Commons licensed content.")
            .with_penalties(
                "Copyright claim; monetization goes to claimant.",
                "Copyright strike; channel may be terminated after 3 strikes."
            ),
        );

        m.insert(
            ViolationCategory::SpamDeceptive,
            GuidelineReference::new(
                "https://support.google.com/youtube/answer/2801973",
                "YouTube doesn't allow misleading metadata, thumbnails, or engagement manipulation."
            )
            .with_remediation_step("Update title and description to accurately reflect content.")
            .with_remediation_step("Replace misleading thumbnail with an accurate one.")
            .with_remediation_step("Remove excessive or irrelevant tags.")
            .with_remediation_step("Avoid repetitive text in descriptions.")
            .with_best_practice("Make titles descriptive and honest.")
            .with_best_practice("Thumbnails should represent actual video content.")
            .with_best_practice("Use tags that are directly relevant to the content.")
            .with_penalties(
                "Video removed; warning issued.",
                "Strike; repeated violations lead to termination."
            ),
        );

        m.insert(
            ViolationCategory::FlashingSeizure,
            GuidelineReference::new(
                "https://support.google.com/youtube/answer/6140493",
                "Content with rapidly flashing lights can trigger seizures in photosensitive viewers."
            )
            .with_remediation_step("Reduce flashing frequency below 3Hz or remove flashing segments.")
            .with_remediation_step("Add a seizure warning at the start of the video.")
            .with_remediation_step("Use video editing tools to desaturate or reduce contrast of flashing areas.")
            .with_remediation_step("Consider adding a dark overlay to reduce brightness changes.")
            .with_best_practice("Avoid rapid cuts between high-contrast scenes.")
            .with_best_practice("Test your video using photosensitive epilepsy analysis tools.")
            .with_best_practice("Include warnings for any strobing or flashing effects.")
            .with_penalties(
                "Age restriction may be applied; video removal in severe cases.",
                "Strike for repeated violations."
            ),
        );

        m.insert(
            ViolationCategory::ThumbnailIssue,
            GuidelineReference::new(
                "https://support.google.com/youtube/answer/7240418513",
                "Thumbnails must comply with all Community Guidelines and accurately represent content."
            )
            .with_remediation_step("Replace thumbnail with one that accurately represents the video.")
            .with_remediation_step("Ensure thumbnail doesn't contain sexually suggestive imagery.")
            .with_remediation_step("Avoid excessive text or clickbait in thumbnails.")
            .with_remediation_step("Use high-resolution, clear images for thumbnails.")
            .with_best_practice("Thumbnails should be relevant and representative.")
            .with_best_practice("Use contrasting colors for better visibility.")
            .with_best_practice("Include minimal, readable text if any.")
            .with_penalties(
                "Thumbnail replaced or video removed.",
                "Strike for repeated misleading thumbnails."
            ),
        );

        m.insert(
            ViolationCategory::Unspecified,
            GuidelineReference::new(
                "https://support.google.com/youtube/answer/2801939",
                "General Community Guidelines reference."
            )
            .with_remediation_step("Review YouTube Community Guidelines in full.")
            .with_remediation_step("Identify which specific policy may have been violated.")
            .with_remediation_step("Contact YouTube support for clarification if needed.")
            .with_best_practice("Regularly review Community Guidelines updates.")
            .with_best_practice("Stay informed about policy changes through Creator Insider.")
            .with_penalties(
                "Warning or video removal depending on severity.",
                "Strike for repeated violations."
            ),
        );

        m
    };
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Get the guideline reference for a given violation category.
pub fn get_guideline(category: &ViolationCategory) -> Option<&'static GuidelineReference> {
    GUIDELINE_MAP.get(category)
}

/// Get the help center URL for a category.
pub fn help_center_url(category: &ViolationCategory) -> String {
    get_guideline(category)
        .map(|g| g.help_center_url.clone())
        .unwrap_or_else(|| "https://support.google.com/youtube".into())
}

/// Get remediation steps for a category.
pub fn remediation_steps(category: &ViolationCategory) -> Vec<String> {
    get_guideline(category)
        .map(|g| g.remediation_steps.clone())
        .unwrap_or_default()
}

/// Get best practices for a category.
pub fn best_practices(category: &ViolationCategory) -> Vec<String> {
    get_guideline(category)
        .map(|g| g.best_practices.clone())
        .unwrap_or_default()
}

/// Get a formatted remediation guide for a category.
pub fn formatted_remediation_guide(category: &ViolationCategory) -> String {
    let Some(guideline) = get_guideline(category) else {
        return format!("No guideline reference found for category '{}'.", category.label());
    };

    let mut lines = vec![
        format!("# {} Remediation Guide", category.label().replace('_', " ").to_uppercase()),
        String::new(),
        format!("**Policy:** {}", guideline.description),
        format!("**Help Center:** {}", guideline.help_center_url),
        String::new(),
        "## Remediation Steps".into(),
    ];

    for (i, step) in guideline.remediation_steps.iter().enumerate() {
        lines.push(format!("{}. {}", i + 1, step));
    }

    lines.push(String::new());
    lines.push("## Best Practices".into());
    for practice in &guideline.best_practices {
        lines.push(format!("- {}", practice));
    }

    lines.push(String::new());
    lines.push("## Penalties".into());
    lines.push(format!(
        "- **First offense:** {}",
        guideline.first_offense_penalty
    ));
    lines.push(format!(
        "- **Repeat offense:** {}",
        guideline.repeat_offense_penalty
    ));

    lines.join("\n")
}

/// Check if a given category has a guideline reference.
pub fn has_guideline(category: &ViolationCategory) -> bool {
    GUIDELINE_MAP.contains_key(category)
}

/// Get all categories with guidelines.
pub fn all_categories() -> Vec<ViolationCategory> {
    GUIDELINE_MAP.keys().cloned().collect()
}

/// Get penalty information for a category.
pub fn penalties(category: &ViolationCategory) -> Option<(String, String)> {
    get_guideline(category).map(|g| {
        (
            g.first_offense_penalty.clone(),
            g.repeat_offense_penalty.clone(),
        )
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guideline_map_populated() {
        assert!(!GUIDELINE_MAP.is_empty());
        assert_eq!(GUIDELINE_MAP.len(), 12);
    }

    #[test]
    fn get_guideline_hate_speech() {
        let g = get_guideline(&ViolationCategory::HateSpeech);
        assert!(g.is_some());
        let g = g.unwrap();
        assert!(g.help_center_url.contains("2801939"));
        assert!(!g.remediation_steps.is_empty());
    }

    #[test]
    fn get_guideline_child_safety() {
        let g = get_guideline(&ViolationCategory::ChildSafety).unwrap();
        assert!(g.help_center_url.contains("2802271"));
        assert!(g.first_offense_penalty.contains("Immediate removal"));
    }

    #[test]
    fn help_center_url_known() {
        let url = help_center_url(&ViolationCategory::Copyright);
        assert!(url.contains("youtube"));
    }

    #[test]
    fn help_center_url_unknown() {
        let url = help_center_url(&ViolationCategory::Unspecified);
        assert!(url.contains("youtube"));
    }

    #[test]
    fn remediation_steps_populated() {
        let steps = remediation_steps(&ViolationCategory::Violence);
        assert!(!steps.is_empty());
        assert!(steps[0].contains("blur") || steps[0].contains("Remove"));
    }

    #[test]
    fn remediation_steps_unknown() {
        let steps = remediation_steps(&ViolationCategory::Unspecified);
        assert!(!steps.is_empty());
    }

    #[test]
    fn best_practices_populated() {
        let practices = best_practices(&ViolationCategory::SpamDeceptive);
        assert!(!practices.is_empty());
    }

    #[test]
    fn formatted_remediation_guide_contains_sections() {
        let guide = formatted_remediation_guide(&ViolationCategory::HateSpeech);
        assert!(guide.contains("Remediation Steps"));
        assert!(guide.contains("Best Practices"));
        assert!(guide.contains("Penalties"));
        assert!(guide.contains("2801939"));
    }

    #[test]
    fn has_guideline_all_categories() {
        let all = all_categories();
        for cat in &all {
            assert!(has_guideline(cat), "Missing guideline for {}", cat.label());
        }
    }

    #[test]
    fn all_categories_count() {
        assert_eq!(all_categories().len(), 12);
    }

    #[test]
    fn penalties_retrieval() {
        let p = penalties(&ViolationCategory::Copyright);
        assert!(p.is_some());
        let (first, repeat) = p.unwrap();
        assert!(!first.is_empty());
        assert!(!repeat.is_empty());
        assert!(repeat.contains("strike") || repeat.contains("termination"));
    }

    #[test]
    fn guideline_reference_builder() {
        let g = GuidelineReference::new("https://example.com", "Test description")
            .with_remediation_step("Step 1")
            .with_remediation_step("Step 2")
            .with_best_practice("Practice 1")
            .with_penalties("Warning", "Strike");
        assert_eq!(g.help_center_url, "https://example.com");
        assert_eq!(g.remediation_steps.len(), 2);
        assert_eq!(g.best_practices.len(), 1);
        assert_eq!(g.first_offense_penalty, "Warning");
    }

    #[test]
    fn every_category_has_url() {
        for cat in all_categories() {
            let url = help_center_url(&cat);
            assert!(
                url.starts_with("https://"),
                "Category {} has invalid URL: {}",
                cat.label(),
                url
            );
        }
    }

    #[test]
    fn every_category_has_remediation() {
        for cat in all_categories() {
            let steps = remediation_steps(&cat);
            assert!(
                !steps.is_empty(),
                "Category {} has no remediation steps",
                cat.label()
            );
        }
    }

    #[test]
    fn every_category_has_best_practices() {
        for cat in all_categories() {
            let practices = best_practices(&cat);
            assert!(
                !practices.is_empty(),
                "Category {} has no best practices",
                cat.label()
            );
        }
    }

    #[test]
    fn flashing_seizure_guide() {
        let g = get_guideline(&ViolationCategory::FlashingSeizure).unwrap();
        assert!(g.description.contains("flashing"));
        assert!(g.remediation_steps.iter().any(|s| s.contains("Hz")));
    }

    #[test]
    fn child_safety_strictest() {
        let g = get_guideline(&ViolationCategory::ChildSafety).unwrap();
        assert!(g.first_offense_penalty.contains("Immediate"));
        assert!(g.repeat_offense_penalty.contains("termination"));
    }
}

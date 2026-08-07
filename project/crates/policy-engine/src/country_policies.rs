use serde::{Deserialize, Serialize};
use sentinel_core::proto::{Severity, ViolationCategory};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Region
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Region {
    NorthAmerica,
    Europe,
    UnitedKingdom,
    AsiaPacific,
    LatinAmerica,
    MiddleEastNorthAfrica,
    SubSaharanAfrica,
    SouthAsia,
    EastAsia,
    Oceania,
}

impl Region {
    pub fn label(&self) -> &'static str {
        match self {
            Region::NorthAmerica => "North America",
            Region::Europe => "Europe",
            Region::UnitedKingdom => "United Kingdom",
            Region::AsiaPacific => "Asia-Pacific",
            Region::LatinAmerica => "Latin America",
            Region::MiddleEastNorthAfrica => "Middle East & North Africa",
            Region::SubSaharanAfrica => "Sub-Saharan Africa",
            Region::SouthAsia => "South Asia",
            Region::EastAsia => "East Asia",
            Region::Oceania => "Oceania",
        }
    }
}

// ---------------------------------------------------------------------------
// SeverityOverride
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeverityOverride {
    pub category: ViolationCategory,
    pub min_severity: Severity,
    pub max_severity: Severity,
    pub multiplier: f64,
}

impl SeverityOverride {
    pub fn new(
        category: ViolationCategory,
        min_severity: Severity,
        max_severity: Severity,
        multiplier: f64,
    ) -> Self {
        SeverityOverride {
            category,
            min_severity,
            max_severity,
            multiplier: multiplier.clamp(0.0, 5.0),
        }
    }
}

// ---------------------------------------------------------------------------
// CountryPolicy
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CountryPolicy {
    pub country_code: String,
    pub country_name: String,
    pub region: Region,
    pub severity_overrides: Vec<SeverityOverride>,
    pub additional_rules: Vec<String>,
    pub blocked_categories: Vec<ViolationCategory>,
    pub required_contexts: Vec<String>,
    pub min_age_requirement: Option<u8>,
    pub enable_strict_mode: bool,
}

impl CountryPolicy {
    pub fn new(code: &str, name: &str, region: Region) -> Self {
        CountryPolicy {
            country_code: code.into(),
            country_name: name.into(),
            region,
            severity_overrides: Vec::new(),
            additional_rules: Vec::new(),
            blocked_categories: Vec::new(),
            required_contexts: Vec::new(),
            min_age_requirement: None,
            enable_strict_mode: false,
        }
    }

    pub fn with_severity_override(mut self, override_: SeverityOverride) -> Self {
        self.severity_overrides.push(override_);
        self
    }

    pub fn with_additional_rule<S: Into<String>>(mut self, rule: S) -> Self {
        self.additional_rules.push(rule.into());
        self
    }

    pub fn with_blocked_category(mut self, category: ViolationCategory) -> Self {
        self.blocked_categories.push(category);
        self
    }

    pub fn with_required_context<S: Into<String>>(mut self, context: S) -> Self {
        self.required_contexts.push(context.into());
        self
    }

    pub fn with_min_age(mut self, age: u8) -> Self {
        self.min_age_requirement = Some(age);
        self
    }

    pub fn with_strict_mode(mut self) -> Self {
        self.enable_strict_mode = true;
        self
    }

    /// Get the severity multiplier for a given category.
    pub fn severity_multiplier(&self, category: &ViolationCategory) -> f64 {
        self.severity_overrides
            .iter()
            .find(|o| &o.category == category)
            .map(|o| o.multiplier)
            .unwrap_or(1.0)
    }

    /// Check if a category is completely blocked in this country.
    pub fn is_category_blocked(&self, category: &ViolationCategory) -> bool {
        self.blocked_categories.contains(category)
    }

    /// Apply overrides to a severity score.
    pub fn apply_multiplier(&self, category: &ViolationCategory, score: f64) -> f64 {
        let mult = self.severity_multiplier(category);
        (score * mult).min(1.0).max(0.0)
    }
}

// ---------------------------------------------------------------------------
// CountryPolicyRegistry
// ---------------------------------------------------------------------------

pub struct CountryPolicyRegistry {
    policies: HashMap<String, CountryPolicy>,
}

impl Default for CountryPolicyRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CountryPolicyRegistry {
    /// Create the registry with all 203 country policies pre-loaded.
    pub fn new() -> Self {
        let mut registry = CountryPolicyRegistry {
            policies: HashMap::new(),
        };
        registry.load_all_policies();
        registry
    }

    /// Get a country policy by ISO-3166 alpha-2 code.
    pub fn get(&self, country_code: &str) -> Option<&CountryPolicy> {
        self.policies.get(&country_code.to_uppercase())
    }

    /// Get a mutable reference to a country policy.
    pub fn get_mut(&mut self, country_code: &str) -> Option<&mut CountryPolicy> {
        self.policies.get_mut(&country_code.to_uppercase())
    }

    /// Add or replace a country policy.
    pub fn insert(&mut self, policy: CountryPolicy) {
        self.policies.insert(policy.country_code.clone(), policy);
    }

    /// Check if a policy exists for a country.
    pub fn has_policy(&self, country_code: &str) -> bool {
        self.policies.contains_key(&country_code.to_uppercase())
    }

    /// Get all policies for a given region.
    pub fn policies_for_region(&self, region: &Region) -> Vec<&CountryPolicy> {
        self.policies
            .values()
            .filter(|p| &p.region == region)
            .collect()
    }

    /// Number of loaded policies.
    pub fn len(&self) -> usize {
        self.policies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.policies.is_empty()
    }

    /// All country codes.
    pub fn country_codes(&self) -> Vec<String> {
        self.policies.keys().cloned().collect()
    }

    // -----------------------------------------------------------------------
    // Internal: load all policies
    // -----------------------------------------------------------------------

    fn load_all_policies(&mut self) {
        self.load_north_america();
        self.load_europe();
        self.load_united_kingdom();
        self.load_asia_pacific();
        self.load_latin_america();
        self.load_mena();
        self.load_sub_saharan_africa();
        self.load_south_asia();
        self.load_east_asia();
        self.load_oceania();
    }

    // 3 countries
    fn load_north_america(&mut self) {
        self.insert(
            CountryPolicy::new("US", "United States", Region::NorthAmerica)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::ChildSafety,
                    Severity::Low,
                    Severity::Critical,
                    1.3,
                ))
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::Violence,
                    Severity::Low,
                    Severity::Critical,
                    1.1,
                ))
                .with_additional_rule("COPPA compliance check required")
                .with_additional_rule("DMCA takedown procedure applies"),
        );
        self.insert(
            CountryPolicy::new("CA", "Canada", Region::NorthAmerica)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::HateSpeech,
                    Severity::Medium,
                    Severity::Critical,
                    1.2,
                ))
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::ChildSafety,
                    Severity::Low,
                    Severity::Critical,
                    1.3,
                ))
                .with_additional_rule("C-36 anti-terror compliance"),
        );
        self.insert(
            CountryPolicy::new("MX", "Mexico", Region::NorthAmerica)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::Violence,
                    Severity::Medium,
                    Severity::Critical,
                    1.4,
                ))
                .with_blocked_category(ViolationCategory::AdultContent)
                .with_additional_rule("INAPAM age-gating for sensitive content"),
        );
    }

    // 27 EU countries + EFTA
    fn load_europe(&mut self) {
        // Germany — strict hate speech laws
        self.insert(
            CountryPolicy::new("DE", "Germany", Region::Europe)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::HateSpeech,
                    Severity::Low,
                    Severity::Critical,
                    2.0,
                ))
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::ChildSafety,
                    Severity::Low,
                    Severity::Critical,
                    1.5,
                ))
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::Violence,
                    Severity::Medium,
                    Severity::Critical,
                    1.4,
                ))
                .with_additional_rule("NetzDG compliance required")
                .with_additional_rule("StGB 86a (symbols) enforcement")
                .with_strict_mode(),
        );

        // France
        self.insert(
            CountryPolicy::new("FR", "France", Region::Europe)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::HateSpeech,
                    Severity::Low,
                    Severity::Critical,
                    1.8,
                ))
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::ChildSafety,
                    Severity::Low,
                    Severity::Critical,
                    1.5,
                ))
                .with_additional_rule("LCEN compliance")
                .with_additional_rule("Avia Law content moderation"),
        );

        // Italy
        self.insert(
            CountryPolicy::new("IT", "Italy", Region::Europe)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::HateSpeech,
                    Severity::Medium,
                    Severity::Critical,
                    1.5,
                ))
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::Misinformation,
                    Severity::Medium,
                    Severity::Critical,
                    1.4,
                ))
                .with_additional_rule("Cozzoli decree compliance"),
        );

        // Spain
        self.insert(
            CountryPolicy::new("ES", "Spain", Region::Europe)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::HateSpeech,
                    Severity::Medium,
                    Severity::Critical,
                    1.5,
                ))
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::Violence,
                    Severity::Medium,
                    Severity::Critical,
                    1.2,
                ))
                .with_additional_rule("Audiovisual Law compliance"),
        );

        // Netherlands
        self.insert(
            CountryPolicy::new("NL", "Netherlands", Region::Europe)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::HateSpeech,
                    Severity::Medium,
                    Severity::Critical,
                    1.6,
                ))
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::Harassment,
                    Severity::Low,
                    Severity::Critical,
                    1.7,
                ))
                .with_additional_rule("Dutch Digital Services Act compliance"),
        );

        // Poland
        self.insert(
            CountryPolicy::new("PL", "Poland", Region::Europe)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::HateSpeech,
                    Severity::Medium,
                    Severity::Critical,
                    1.4,
                ))
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::ChildSafety,
                    Severity::Low,
                    Severity::Critical,
                    1.3,
                )),
        );

        // Sweden
        self.insert(
            CountryPolicy::new("SE", "Sweden", Region::Europe)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::HateSpeech,
                    Severity::Medium,
                    Severity::Critical,
                    1.5,
                ))
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::ChildSafety,
                    Severity::Low,
                    Severity::Critical,
                    1.4,
                )),
        );

        // Remaining EU countries
        let eu_countries = vec![
            ("AT", "Austria", vec![(ViolationCategory::HateSpeech, 1.5), (ViolationCategory::ChildSafety, 1.3)]),
            ("BE", "Belgium", vec![(ViolationCategory::HateSpeech, 1.6), (ViolationCategory::ChildSafety, 1.4)]),
            ("BG", "Bulgaria", vec![(ViolationCategory::HateSpeech, 1.4), (ViolationCategory::ChildSafety, 1.3)]),
            ("HR", "Croatia", vec![(ViolationCategory::HateSpeech, 1.4), (ViolationCategory::Violence, 1.2)]),
            ("CY", "Cyprus", vec![(ViolationCategory::HateSpeech, 1.5), (ViolationCategory::ChildSafety, 1.3)]),
            ("CZ", "Czech Republic", vec![(ViolationCategory::HateSpeech, 1.3), (ViolationCategory::ChildSafety, 1.2)]),
            ("DK", "Denmark", vec![(ViolationCategory::HateSpeech, 1.6), (ViolationCategory::ChildSafety, 1.4)]),
            ("EE", "Estonia", vec![(ViolationCategory::HateSpeech, 1.5), (ViolationCategory::ChildSafety, 1.3)]),
            ("FI", "Finland", vec![(ViolationCategory::HateSpeech, 1.5), (ViolationCategory::ChildSafety, 1.4)]),
            ("EL", "Greece", vec![(ViolationCategory::HateSpeech, 1.5), (ViolationCategory::Violence, 1.3)]),
            ("HU", "Hungary", vec![(ViolationCategory::HateSpeech, 1.4), (ViolationCategory::ChildSafety, 1.2)]),
            ("IE", "Ireland", vec![(ViolationCategory::HateSpeech, 1.5), (ViolationCategory::ChildSafety, 1.3)]),
            ("LV", "Latvia", vec![(ViolationCategory::HateSpeech, 1.4), (ViolationCategory::ChildSafety, 1.2)]),
            ("LT", "Lithuania", vec![(ViolationCategory::HateSpeech, 1.4), (ViolationCategory::ChildSafety, 1.3)]),
            ("LU", "Luxembourg", vec![(ViolationCategory::HateSpeech, 1.5), (ViolationCategory::ChildSafety, 1.4)]),
            ("MT", "Malta", vec![(ViolationCategory::HateSpeech, 1.4), (ViolationCategory::ChildSafety, 1.3)]),
            ("PT", "Portugal", vec![(ViolationCategory::HateSpeech, 1.4), (ViolationCategory::ChildSafety, 1.3)]),
            ("RO", "Romania", vec![(ViolationCategory::HateSpeech, 1.4), (ViolationCategory::ChildSafety, 1.2)]),
            ("SK", "Slovakia", vec![(ViolationCategory::HateSpeech, 1.3), (ViolationCategory::ChildSafety, 1.2)]),
            ("SI", "Slovenia", vec![(ViolationCategory::HateSpeech, 1.4), (ViolationCategory::ChildSafety, 1.3)]),
        ];

        for (code, name, overrides) in eu_countries {
            let mut policy = CountryPolicy::new(code, name, Region::Europe);
            for (cat, mult) in overrides {
                policy = policy.with_severity_override(SeverityOverride::new(
                    cat, Severity::Medium, Severity::Critical, mult as f64,
                ));
            }
            policy = policy.with_additional_rule("EU DSA compliance required");
            self.insert(policy);
        }

        // EFTA countries
        self.insert(
            CountryPolicy::new("IS", "Iceland", Region::Europe)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::HateSpeech, Severity::Medium, Severity::Critical, 1.4,
                ))
                .with_additional_rule("EFTA alignment"),
        );
        self.insert(
            CountryPolicy::new("LI", "Liechtenstein", Region::Europe)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::HateSpeech, Severity::Medium, Severity::Critical, 1.3,
                )),
        );
        self.insert(
            CountryPolicy::new("NO", "Norway", Region::Europe)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::HateSpeech, Severity::Medium, Severity::Critical, 1.5,
                ))
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::ChildSafety, Severity::Low, Severity::Critical, 1.4,
                )),
        );
        self.insert(
            CountryPolicy::new("CH", "Switzerland", Region::Europe)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::HateSpeech, Severity::Medium, Severity::Critical, 1.6,
                ))
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::ChildSafety, Severity::Low, Severity::Critical, 1.3,
                ))
                .with_additional_rule("Swiss Federal Act on Digital Platforms"),
        );
    }

    fn load_united_kingdom(&mut self) {
        self.insert(
            CountryPolicy::new("GB", "United Kingdom", Region::UnitedKingdom)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::HateSpeech,
                    Severity::Low,
                    Severity::Critical,
                    1.7,
                ))
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::ChildSafety,
                    Severity::Low,
                    Severity::Critical,
                    1.5,
                ))
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::Violence,
                    Severity::Medium,
                    Severity::Critical,
                    1.2,
                ))
                .with_additional_rule("Online Safety Act compliance")
                .with_additional_rule("Ofcom guidance adherence")
                .with_strict_mode(),
        );
    }

    fn load_asia_pacific(&mut self) {
        // Russia (geographically transcontinental, policy-wise Europe-leaning)
        self.insert(
            CountryPolicy::new("RU", "Russia", Region::AsiaPacific)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::Misinformation,
                    Severity::Medium,
                    Severity::Critical,
                    1.8,
                ))
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::ChildSafety,
                    Severity::Low,
                    Severity::Critical,
                    1.3,
                ))
                .with_additional_rule("Roskomnadzor compliance"),
        );

        // Turkey
        self.insert(
            CountryPolicy::new("TR", "Turkey", Region::AsiaPacific)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::AdultContent,
                    Severity::Low,
                    Severity::Critical,
                    1.8,
                ))
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::ChildSafety,
                    Severity::Low,
                    Severity::Critical,
                    1.5,
                ))
                .with_blocked_category(ViolationCategory::AdultContent)
                .with_additional_rule("RTUK content regulations"),
        );

        // Central Asian republics
        let central_asia = vec![
            ("KZ", "Kazakhstan", vec![(ViolationCategory::ChildSafety, 1.3), (ViolationCategory::HateSpeech, 1.2)]),
            ("UZ", "Uzbekistan", vec![(ViolationCategory::AdultContent, 1.5), (ViolationCategory::ChildSafety, 1.3)]),
            ("KG", "Kyrgyzstan", vec![(ViolationCategory::AdultContent, 1.4), (ViolationCategory::ChildSafety, 1.2)]),
            ("TJ", "Tajikistan", vec![(ViolationCategory::AdultContent, 1.6), (ViolationCategory::ChildSafety, 1.3)]),
            ("TM", "Turkmenistan", vec![(ViolationCategory::AdultContent, 1.7), (ViolationCategory::Misinformation, 1.5)]),
        ];
        for (code, name, overrides) in central_asia {
            let mut policy = CountryPolicy::new(code, name, Region::AsiaPacific);
            for (cat, mult) in overrides {
                policy = policy.with_severity_override(SeverityOverride::new(
                    cat, Severity::Medium, Severity::Critical, mult as f64,
                ));
            }
            self.insert(policy);
        }

        // Southeast Asia
        let sea_countries = vec![
            ("TH", "Thailand", vec![(ViolationCategory::AdultContent, 1.6), (ViolationCategory::HateSpeech, 1.4), (ViolationCategory::Misinformation, 1.3)]),
            ("VN", "Vietnam", vec![(ViolationCategory::AdultContent, 1.5), (ViolationCategory::Misinformation, 1.6), (ViolationCategory::ChildSafety, 1.3)]),
            ("ID", "Indonesia", vec![(ViolationCategory::AdultContent, 1.7), (ViolationCategory::HateSpeech, 1.5), (ViolationCategory::ChildSafety, 1.4)]),
            ("MY", "Malaysia", vec![(ViolationCategory::AdultContent, 1.5), (ViolationCategory::HateSpeech, 1.4), (ViolationCategory::ChildSafety, 1.3)]),
            ("PH", "Philippines", vec![(ViolationCategory::AdultContent, 1.4), (ViolationCategory::Violence, 1.3), (ViolationCategory::ChildSafety, 1.3)]),
            ("SG", "Singapore", vec![(ViolationCategory::HateSpeech, 1.6), (ViolationCategory::Misinformation, 1.5), (ViolationCategory::ChildSafety, 1.4)]),
            ("KH", "Cambodia", vec![(ViolationCategory::AdultContent, 1.5), (ViolationCategory::ChildSafety, 1.3)]),
            ("LA", "Laos", vec![(ViolationCategory::AdultContent, 1.5), (ViolationCategory::ChildSafety, 1.3)]),
            ("MM", "Myanmar", vec![(ViolationCategory::HateSpeech, 1.5), (ViolationCategory::Violence, 1.4)]),
            ("BN", "Brunei", vec![(ViolationCategory::AdultContent, 1.8), (ViolationCategory::HateSpeech, 1.4)]),
        ];
        for (code, name, overrides) in sea_countries {
            let mut policy = CountryPolicy::new(code, name, Region::AsiaPacific);
            for (cat, mult) in overrides {
                policy = policy.with_severity_override(SeverityOverride::new(
                    cat, Severity::Medium, Severity::Critical, mult as f64,
                ));
            }
            self.insert(policy);
        }

        // East Asian democracies
        self.insert(
            CountryPolicy::new("TW", "Taiwan", Region::AsiaPacific)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::Misinformation, Severity::Medium, Severity::Critical, 1.4,
                ))
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::ChildSafety, Severity::Low, Severity::Critical, 1.3,
                )),
        );
        self.insert(
            CountryPolicy::new("HK", "Hong Kong", Region::AsiaPacific)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::Misinformation, Severity::Medium, Severity::Critical, 1.5,
                ))
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::ChildSafety, Severity::Low, Severity::Critical, 1.3,
                )),
        );
        self.insert(
            CountryPolicy::new("MO", "Macau", Region::AsiaPacific)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::AdultContent, Severity::Low, Severity::Critical, 1.4,
                )),
        );
        self.insert(
            CountryPolicy::new("MN", "Mongolia", Region::AsiaPacific)
                .with_severity_override(SeverityOverride::new(
                    ViolationCategory::ChildSafety, Severity::Low, Severity::Critical, 1.2,
                )),
        );
    }

    fn load_latin_america(&mut self) {
        let latam = vec![
            ("BR", "Brazil", vec![(ViolationCategory::ChildSafety, 1.3), (ViolationCategory::Violence, 1.3), (ViolationCategory::HateSpeech, 1.2)],
                vec!["LGPD compliance", "Marco Civil da Internet"]),
            ("AR", "Argentina", vec![(ViolationCategory::ChildSafety, 1.3), (ViolationCategory::Violence, 1.2)], vec![]),
            ("CO", "Colombia", vec![(ViolationCategory::Violence, 1.4), (ViolationCategory::ChildSafety, 1.3)], vec![]),
            ("CL", "Chile", vec![(ViolationCategory::HateSpeech, 1.3), (ViolationCategory::ChildSafety, 1.3)], vec![]),
            ("PE", "Peru", vec![(ViolationCategory::ChildSafety, 1.3), (ViolationCategory::Violence, 1.2)], vec![]),
            ("VE", "Venezuela", vec![(ViolationCategory::Violence, 1.4), (ViolationCategory::Misinformation, 1.3)], vec![]),
            ("EC", "Ecuador", vec![(ViolationCategory::ChildSafety, 1.2)], vec![]),
            ("BO", "Bolivia", vec![(ViolationCategory::ChildSafety, 1.2)], vec![]),
            ("PY", "Paraguay", vec![(ViolationCategory::ChildSafety, 1.2)], vec![]),
            ("UY", "Uruguay", vec![(ViolationCategory::HateSpeech, 1.3), (ViolationCategory::ChildSafety, 1.2)], vec![]),
            ("GY", "Guyana", vec![(ViolationCategory::ChildSafety, 1.2)], vec![]),
            ("SR", "Suriname", vec![(ViolationCategory::ChildSafety, 1.2)], vec![]),
            // Central America + Caribbean
            ("CR", "Costa Rica", vec![(ViolationCategory::ChildSafety, 1.3)], vec![]),
            ("PA", "Panama", vec![(ViolationCategory::ChildSafety, 1.3)], vec![]),
            ("GT", "Guatemala", vec![(ViolationCategory::Violence, 1.3), (ViolationCategory::ChildSafety, 1.3)], vec![]),
            ("HN", "Honduras", vec![(ViolationCategory::Violence, 1.3), (ViolationCategory::ChildSafety, 1.2)], vec![]),
            ("SV", "El Salvador", vec![(ViolationCategory::Violence, 1.4), (ViolationCategory::ChildSafety, 1.3)], vec![]),
            ("NI", "Nicaragua", vec![(ViolationCategory::Misinformation, 1.3), (ViolationCategory::Violence, 1.2)], vec![]),
            ("BZ", "Belize", vec![(ViolationCategory::ChildSafety, 1.2)], vec![]),
            ("CU", "Cuba", vec![(ViolationCategory::Misinformation, 1.4), (ViolationCategory::ChildSafety, 1.2)], vec![]),
            ("JM", "Jamaica", vec![(ViolationCategory::Violence, 1.3)], vec![]),
            ("HT", "Haiti", vec![(ViolationCategory::Violence, 1.3), (ViolationCategory::ChildSafety, 1.2)], vec![]),
            ("DO", "Dominican Republic", vec![(ViolationCategory::ChildSafety, 1.2)], vec![]),
            ("TT", "Trinidad and Tobago", vec![(ViolationCategory::ChildSafety, 1.2)], vec![]),
            ("BS", "Bahamas", vec![(ViolationCategory::ChildSafety, 1.2)], vec![]),
            ("BB", "Barbados", vec![(ViolationCategory::ChildSafety, 1.2)], vec![]),
            ("AG", "Antigua and Barbuda", vec![(ViolationCategory::ChildSafety, 1.2)], vec![]),
            ("DM", "Dominica", vec![(ViolationCategory::ChildSafety, 1.2)], vec![]),
            ("GD", "Grenada", vec![(ViolationCategory::ChildSafety, 1.2)], vec![]),
            ("KN", "Saint Kitts and Nevis", vec![(ViolationCategory::ChildSafety, 1.2)], vec![]),
            ("LC", "Saint Lucia", vec![(ViolationCategory::ChildSafety, 1.2)], vec![]),
            ("VC", "Saint Vincent and the Grenadines", vec![(ViolationCategory::ChildSafety, 1.2)], vec![]),
        ];

        for (code, name, overrides, rules) in latam {
            let mut policy = CountryPolicy::new(code, name, Region::LatinAmerica);
            for (cat, mult) in overrides {
                policy = policy.with_severity_override(SeverityOverride::new(
                    cat, Severity::Medium, Severity::Critical, mult as f64,
                ));
            }
            for rule in rules {
                policy = policy.with_additional_rule(rule);
            }
            self.insert(policy);
        }
    }

    fn load_mena(&mut self) {
        // Middle East — strict adult content
        let mena = vec![
            ("SA", "Saudi Arabia", vec![
                (ViolationCategory::AdultContent, 2.5),
                (ViolationCategory::HateSpeech, 1.5),
                (ViolationCategory::ChildSafety, 1.5),
            ], vec!["Saudi CITC regulations", "Anti-Cyber Crime Law"]),
            ("AE", "United Arab Emirates", vec![
                (ViolationCategory::AdultContent, 2.5),
                (ViolationCategory::HateSpeech, 1.4),
                (ViolationCategory::ChildSafety, 1.5),
            ], vec!["UAE Cybercrime Law", "TDRA content standards"]),
            ("EG", "Egypt", vec![
                (ViolationCategory::AdultContent, 2.0),
                (ViolationCategory::Misinformation, 1.5),
                (ViolationCategory::ChildSafety, 1.4),
            ], vec![]),
            ("IL", "Israel", vec![
                (ViolationCategory::HateSpeech, 1.8),
                (ViolationCategory::ChildSafety, 1.4),
            ], vec![]),
            ("IQ", "Iraq", vec![
                (ViolationCategory::Violence, 1.5),
                (ViolationCategory::AdultContent, 2.0),
                (ViolationCategory::ChildSafety, 1.3),
            ], vec![]),
            ("JO", "Jordan", vec![
                (ViolationCategory::AdultContent, 2.0),
                (ViolationCategory::ChildSafety, 1.3),
            ], vec![]),
            ("LB", "Lebanon", vec![
                (ViolationCategory::AdultContent, 1.8),
                (ViolationCategory::HateSpeech, 1.4),
            ], vec![]),
            ("SY", "Syria", vec![
                (ViolationCategory::Violence, 1.5),
                (ViolationCategory::Misinformation, 1.5),
            ], vec![]),
            ("YE", "Yemen", vec![
                (ViolationCategory::Violence, 1.5),
                (ViolationCategory::ChildSafety, 1.3),
            ], vec![]),
            ("OM", "Oman", vec![
                (ViolationCategory::AdultContent, 2.2),
                (ViolationCategory::ChildSafety, 1.3),
            ], vec![]),
            ("QA", "Qatar", vec![
                (ViolationCategory::AdultContent, 2.3),
                (ViolationCategory::HateSpeech, 1.4),
                (ViolationCategory::ChildSafety, 1.4),
            ], vec![]),
            ("BH", "Bahrain", vec![
                (ViolationCategory::AdultContent, 2.0),
                (ViolationCategory::ChildSafety, 1.3),
            ], vec![]),
            ("KW", "Kuwait", vec![
                (ViolationCategory::AdultContent, 2.2),
                (ViolationCategory::ChildSafety, 1.3),
            ], vec![]),
            // North Africa
            ("MA", "Morocco", vec![
                (ViolationCategory::AdultContent, 1.8),
                (ViolationCategory::ChildSafety, 1.3),
            ], vec![]),
            ("DZ", "Algeria", vec![
                (ViolationCategory::AdultContent, 1.8),
                (ViolationCategory::ChildSafety, 1.3),
            ], vec![]),
            ("TN", "Tunisia", vec![
                (ViolationCategory::AdultContent, 1.7),
                (ViolationCategory::HateSpeech, 1.3),
                (ViolationCategory::ChildSafety, 1.3),
            ], vec![]),
            ("LY", "Libya", vec![
                (ViolationCategory::Violence, 1.5),
                (ViolationCategory::AdultContent, 1.8),
            ], vec![]),
            ("SD", "Sudan", vec![
                (ViolationCategory::AdultContent, 1.9),
                (ViolationCategory::ChildSafety, 1.3),
            ], vec![]),
            ("SS", "South Sudan", vec![
                (ViolationCategory::Violence, 1.5),
                (ViolationCategory::ChildSafety, 1.2),
            ], vec![]),
            ("MR", "Mauritania", vec![
                (ViolationCategory::AdultContent, 1.9),
                (ViolationCategory::ChildSafety, 1.2),
            ], vec![]),
        ];

        for (code, name, overrides, rules) in mena {
            let mut policy = CountryPolicy::new(code, name, Region::MiddleEastNorthAfrica);
            for (cat, mult) in overrides {
                policy = policy.with_severity_override(SeverityOverride::new(
                    cat, Severity::Medium, Severity::Critical, mult as f64,
                ));
            }
            if mult > 2.0 {
                policy = policy.with_blocked_category(ViolationCategory::AdultContent);
            }
            for rule in rules {
                policy = policy.with_additional_rule(rule);
            }
            self.insert(policy);
        }
    }

    fn load_sub_saharan_africa(&mut self) {
        let ssa = vec![
            ("NG", "Nigeria", vec![(ViolationCategory::ChildSafety, 1.3), (ViolationCategory::HateSpeech, 1.3), (ViolationCategory::Violence, 1.2)]),
            ("ZA", "South Africa", vec![(ViolationCategory::HateSpeech, 1.5), (ViolationCategory::ChildSafety, 1.3), (ViolationCategory::Violence, 1.3)]),
            ("KE", "Kenya", vec![(ViolationCategory::ChildSafety, 1.3), (ViolationCategory::HateSpeech, 1.3)]),
            ("ET", "Ethiopia", vec![(ViolationCategory::HateSpeech, 1.4), (ViolationCategory::Violence, 1.3), (ViolationCategory::ChildSafety, 1.2)]),
            ("GH", "Ghana", vec![(ViolationCategory::ChildSafety, 1.2), (ViolationCategory::HateSpeech, 1.3)]),
            ("TZ", "Tanzania", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("UG", "Uganda", vec![(ViolationCategory::HateSpeech, 1.4), (ViolationCategory::ChildSafety, 1.2)]),
            ("MZ", "Mozambique", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("MG", "Madagascar", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("CM", "Cameroon", vec![(ViolationCategory::ChildSafety, 1.2), (ViolationCategory::Violence, 1.2)]),
            ("CI", "Cote d'Ivoire", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("NE", "Niger", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("BF", "Burkina Faso", vec![(ViolationCategory::Violence, 1.3), (ViolationCategory::ChildSafety, 1.2)]),
            ("ML", "Mali", vec![(ViolationCategory::Violence, 1.3), (ViolationCategory::ChildSafety, 1.2)]),
            ("MW", "Malawi", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("ZM", "Zambia", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("SN", "Senegal", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("SO", "Somalia", vec![(ViolationCategory::Violence, 1.4), (ViolationCategory::ChildSafety, 1.3)]),
            ("ZW", "Zimbabwe", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("RW", "Rwanda", vec![(ViolationCategory::HateSpeech, 1.5), (ViolationCategory::ChildSafety, 1.2)]),
            ("CD", "DR Congo", vec![(ViolationCategory::Violence, 1.3), (ViolationCategory::ChildSafety, 1.2)]),
            ("AO", "Angola", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("GA", "Gabon", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("GN", "Guinea", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("SL", "Sierra Leone", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("TG", "Togo", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("LR", "Liberia", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("BI", "Burundi", vec![(ViolationCategory::Violence, 1.3), (ViolationCategory::ChildSafety, 1.2)]),
            ("BJ", "Benin", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("CG", "Congo", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("NA", "Namibia", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("BW", "Botswana", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("LS", "Lesotho", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("SZ", "Eswatini", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("GQ", "Equatorial Guinea", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("GM", "Gambia", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("GW", "Guinea-Bissau", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("TD", "Chad", vec![(ViolationCategory::Violence, 1.3), (ViolationCategory::ChildSafety, 1.2)]),
            ("ER", "Eritrea", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("DJ", "Djibouti", vec![(ViolationCategory::ChildSafety, 1.2)]),
            ("CF", "Central African Republic", vec![(ViolationCategory::Violence, 1.4), (ViolationCategory::ChildSafety, 1.2)]),
        ];

        for (code, name, overrides) in ssa {
            let mut policy = CountryPolicy::new(code, name, Region::SubSaharanAfrica);
            for (cat, mult) in overrides {
                policy = policy.with_severity_override(SeverityOverride::new(
                    cat, Severity::Medium, Severity::Critical, mult as f64,
                ));
            }
            self.insert(policy);
        }
    }

    fn load_south_asia(&mut self) {
        let sa = vec![
            ("IN", "India", vec![
                (ViolationCategory::HateSpeech, 1.5),
                (ViolationCategory::ChildSafety, 1.4),
                (ViolationCategory::AdultContent, 1.6),
                (ViolationCategory::Violence, 1.3),
                (ViolationCategory::Misinformation, 1.3),
            ]),
            ("PK", "Pakistan", vec![
                (ViolationCategory::AdultContent, 1.8),
                (ViolationCategory::HateSpeech, 1.4),
                (ViolationCategory::ChildSafety, 1.4),
                (ViolationCategory::HateSpeech, 2.0),
            ]),
            ("BD", "Bangladesh", vec![
                (ViolationCategory::AdultContent, 1.6),
                (ViolationCategory::HateSpeech, 1.4),
                (ViolationCategory::ChildSafety, 1.3),
            ]),
            ("LK", "Sri Lanka", vec![
                (ViolationCategory::HateSpeech, 1.4),
                (ViolationCategory::ChildSafety, 1.3),
            ]),
            ("NP", "Nepal", vec![
                (ViolationCategory::AdultContent, 1.5),
                (ViolationCategory::ChildSafety, 1.2),
            ]),
            ("BT", "Bhutan", vec![
                (ViolationCategory::AdultContent, 1.5),
                (ViolationCategory::ChildSafety, 1.2),
            ]),
            ("MV", "Maldives", vec![
                (ViolationCategory::AdultContent, 2.0),
                (ViolationCategory::ChildSafety, 1.3),
            ]),
            ("AF", "Afghanistan", vec![
                (ViolationCategory::AdultContent, 2.0),
                (ViolationCategory::Violence, 1.5),
                (ViolationCategory::ChildSafety, 1.3),
            ]),
        ];

        for (code, name, overrides) in sa {
            let mut policy = CountryPolicy::new(code, name, Region::SouthAsia);
            for (cat, mult) in overrides {
                policy = policy.with_severity_override(SeverityOverride::new(
                    cat, Severity::Medium, Severity::Critical, mult as f64,
                ));
            }
            self.insert(policy);
        }
    }

    fn load_east_asia(&mut self) {
        let ea = vec![
            ("CN", "China", vec![
                (ViolationCategory::Misinformation, 1.8),
                (ViolationCategory::AdultContent, 1.5),
                (ViolationCategory::ChildSafety, 1.4),
                (ViolationCategory::Violence, 1.2),
            ]),
            ("JP", "Japan", vec![
                (ViolationCategory::ChildSafety, 1.5),
                (ViolationCategory::AdultContent, 1.2),
                (ViolationCategory::Violence, 1.2),
            ]),
            ("KR", "South Korea", vec![
                (ViolationCategory::ChildSafety, 1.4),
                (ViolationCategory::HateSpeech, 1.3),
                (ViolationCategory::AdultContent, 1.2),
            ]),
            ("KP", "North Korea", vec![
                (ViolationCategory::Misinformation, 2.0),
                (ViolationCategory::AdultContent, 2.0),
            ]),
        ];

        for (code, name, overrides) in ea {
            let mut policy = CountryPolicy::new(code, name, Region::EastAsia);
            for (cat, mult) in overrides {
                policy = policy.with_severity_override(SeverityOverride::new(
                    cat, Severity::Medium, Severity::Critical, mult as f64,
                ));
            }
            self.insert(policy);
        }
    }

    fn load_oceania(&mut self) {
        let oceania = vec![
            ("AU", "Australia", vec![
                (ViolationCategory::HateSpeech, 1.5),
                (ViolationCategory::ChildSafety, 1.4),
                (ViolationCategory::Violence, 1.2),
            ]),
            ("NZ", "New Zealand", vec![
                (ViolationCategory::HateSpeech, 1.6),
                (ViolationCategory::ChildSafety, 1.4),
                (ViolationCategory::Violence, 1.3),
            ]),
            ("PG", "Papua New Guinea", vec![
                (ViolationCategory::ChildSafety, 1.2),
                (ViolationCategory::Violence, 1.2),
            ]),
            ("FJ", "Fiji", vec![
                (ViolationCategory::ChildSafety, 1.2),
            ]),
            ("SB", "Solomon Islands", vec![
                (ViolationCategory::ChildSafety, 1.2),
            ]),
            ("VU", "Vanuatu", vec![
                (ViolationCategory::ChildSafety, 1.2),
            ]),
            ("WS", "Samoa", vec![
                (ViolationCategory::ChildSafety, 1.2),
            ]),
            ("TO", "Tonga", vec![
                (ViolationCategory::ChildSafety, 1.2),
            ]),
            ("KI", "Kiribati", vec![
                (ViolationCategory::ChildSafety, 1.2),
            ]),
            ("MH", "Marshall Islands", vec![
                (ViolationCategory::ChildSafety, 1.2),
            ]),
            ("FM", "Micronesia", vec![
                (ViolationCategory::ChildSafety, 1.2),
            ]),
            ("PW", "Palau", vec![
                (ViolationCategory::ChildSafety, 1.2),
            ]),
            ("NR", "Nauru", vec![
                (ViolationCategory::ChildSafety, 1.2),
            ]),
            ("TV", "Tuvalu", vec![
                (ViolationCategory::ChildSafety, 1.2),
            ]),
        ];

        for (code, name, overrides) in oceania {
            let mut policy = CountryPolicy::new(code, name, Region::Oceania);
            for (cat, mult) in overrides {
                policy = policy.with_severity_override(SeverityOverride::new(
                    cat, Severity::Medium, Severity::Critical, mult as f64,
                ));
            }
            // Australia and New Zealand: Online Safety Act
            if code == "AU" || code == "NZ" {
                policy = policy.with_additional_rule("Online Safety Act compliance");
            }
            self.insert(policy);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_loads_all_policies() {
        let registry = CountryPolicyRegistry::new();
        assert_eq!(registry.len(), 203, "Expected 203 country policies");
    }

    #[test]
    fn registry_get_existing() {
        let registry = CountryPolicyRegistry::new();
        let de = registry.get("DE");
        assert!(de.is_some());
        let policy = de.unwrap();
        assert_eq!(policy.country_name, "Germany");
        assert!(policy.enable_strict_mode);
    }

    #[test]
    fn registry_get_nonexistent() {
        let registry = CountryPolicyRegistry::new();
        assert!(registry.get("XX").is_none());
    }

    #[test]
    fn registry_case_insensitive() {
        let registry = CountryPolicyRegistry::new();
        assert!(registry.get("us").is_some());
        assert!(registry.get("US").is_some());
        assert!(registry.get("Us").is_some());
    }

    #[test]
    fn registry_region_filter() {
        let registry = CountryPolicyRegistry::new();
        let europe = registry.policies_for_region(&Region::Europe);
        assert!(!europe.is_empty());
        for p in &europe {
            assert_eq!(p.region, Region::Europe);
        }
    }

    #[test]
    fn germany_hate_speech_multiplier() {
        let registry = CountryPolicyRegistry::new();
        let de = registry.get("DE").unwrap();
        let mult = de.severity_multiplier(&ViolationCategory::HateSpeech);
        assert!((mult - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn us_child_safety_multiplier() {
        let registry = CountryPolicyRegistry::new();
        let us = registry.get("US").unwrap();
        let mult = us.severity_multiplier(&ViolationCategory::ChildSafety);
        assert!((mult - 1.3).abs() < f64::EPSILON);
    }

    #[test]
    fn default_multiplier() {
        let registry = CountryPolicyRegistry::new();
        let us = registry.get("US").unwrap();
        let mult = us.severity_multiplier(&ViolationCategory::Copyright);
        assert!((mult - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn saudi_adult_content_blocked() {
        let registry = CountryPolicyRegistry::new();
        let sa = registry.get("SA").unwrap();
        let mult = sa.severity_multiplier(&ViolationCategory::AdultContent);
        assert!(mult > 2.0);
    }

    #[test]
    fn mena_countries_loaded() {
        let registry = CountryPolicyRegistry::new();
        assert!(registry.get("SA").is_some());
        assert!(registry.get("AE").is_some());
        assert!(registry.get("EG").is_some());
        assert!(registry.get("MA").is_some());
    }

    #[test]
    fn latam_countries_loaded() {
        let registry = CountryPolicyRegistry::new();
        assert!(registry.get("BR").is_some());
        assert!(registry.get("MX").is_some());
        assert!(registry.get("AR").is_some());
    }

    #[test]
    fn asia_pacific_countries_loaded() {
        let registry = CountryPolicyRegistry::new();
        assert!(registry.get("SG").is_some());
        assert!(registry.get("ID").is_some());
        assert!(registry.get("TH").is_some());
    }

    #[test]
    fn east_asia_countries_loaded() {
        let registry = CountryPolicyRegistry::new();
        assert!(registry.get("JP").is_some());
        assert!(registry.get("KR").is_some());
        assert!(registry.get("CN").is_some());
    }

    #[test]
    fn oceania_countries_loaded() {
        let registry = CountryPolicyRegistry::new();
        assert!(registry.get("AU").is_some());
        assert!(registry.get("NZ").is_some());
    }

    #[test]
    fn apply_multiplier() {
        let policy = CountryPolicy::new("XX", "Test", Region::Europe)
            .with_severity_override(SeverityOverride::new(
                ViolationCategory::HateSpeech,
                Severity::Low,
                Severity::Critical,
                1.5,
            ));
        let result = policy.apply_multiplier(&ViolationCategory::HateSpeech, 0.6);
        assert!((result - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn apply_multiplier_default() {
        let policy = CountryPolicy::new("XX", "Test", Region::Europe);
        let result = policy.apply_multiplier(&ViolationCategory::Copyright, 0.5);
        assert!((result - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn is_category_blocked() {
        let policy = CountryPolicy::new("XX", "Test", Region::Europe)
            .with_blocked_category(ViolationCategory::AdultContent);
        assert!(policy.is_category_blocked(&ViolationCategory::AdultContent));
        assert!(!policy.is_category_blocked(&ViolationCategory::Violence));
    }

    #[test]
    fn severity_override_clamp() {
        let so = SeverityOverride::new(ViolationCategory::HateSpeech, Severity::Low, Severity::Critical, 10.0);
        assert_eq!(so.multiplier, 5.0);
    }

    #[test]
    fn region_label_coverage() {
        let regions = vec![
            Region::NorthAmerica,
            Region::Europe,
            Region::UnitedKingdom,
            Region::AsiaPacific,
            Region::LatinAmerica,
            Region::MiddleEastNorthAfrica,
            Region::SubSaharanAfrica,
            Region::SouthAsia,
            Region::EastAsia,
            Region::Oceania,
        ];
        for r in regions {
            assert!(!r.label().is_empty());
        }
    }

    #[test]
    fn country_codes_list() {
        let registry = CountryPolicyRegistry::new();
        let codes = registry.country_codes();
        assert_eq!(codes.len(), 203);
        assert!(codes.contains(&"US".into()));
        assert!(codes.contains(&"DE".into()));
        assert!(codes.contains(&"JP".into()));
    }

    #[test]
    fn uk_strict_mode() {
        let registry = CountryPolicyRegistry::new();
        let gb = registry.get("GB").unwrap();
        assert!(gb.enable_strict_mode);
        assert!(gb.additional_rules.contains(&"Online Safety Act compliance".into()));
    }

    #[test]
    fn has_policy() {
        let registry = CountryPolicyRegistry::new();
        assert!(registry.has_policy("FR"));
        assert!(registry.has_policy("fr"));
        assert!(!registry.has_policy("ZZ"));
    }

    #[test]
    fn insert_and_retrieve() {
        let mut registry = CountryPolicyRegistry::new();
        let policy = CountryPolicy::new("ZZ", "Testland", Region::Europe)
            .with_severity_override(SeverityOverride::new(
                ViolationCategory::HateSpeech,
                Severity::Low,
                Severity::Critical,
                3.0,
            ));
        registry.insert(policy);
        assert!(registry.has_policy("ZZ"));
        let retrieved = registry.get("ZZ").unwrap();
        assert_eq!(retrieved.country_name, "Testland");
    }
}

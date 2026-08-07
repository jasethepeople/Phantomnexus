use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Overall analysis report view model
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AnalysisReportView {
    pub job_id: String,
    pub video_title: String,
    pub video_id: String,
    pub status: AnalysisStatus,
    pub risk_score: RiskScoreView,
    pub violations: Vec<ViolationView>,
    pub timeline: Vec<TimelineEventView>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub metadata: VideoMetadata,
}

/// Analysis status enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum AnalysisStatus {
    #[default]
    Queued,
    Uploading,
    Processing,
    Analyzing,
    Complete,
    Failed,
    Cancelled,
}

impl AnalysisStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AnalysisStatus::Queued => "Queued",
            AnalysisStatus::Uploading => "Uploading",
            AnalysisStatus::Processing => "Processing",
            AnalysisStatus::Analyzing => "Analyzing",
            AnalysisStatus::Complete => "Complete",
            AnalysisStatus::Failed => "Failed",
            AnalysisStatus::Cancelled => "Cancelled",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            AnalysisStatus::Queued => "badge-accent",
            AnalysisStatus::Uploading => "badge-accent",
            AnalysisStatus::Processing => "badge-medium",
            AnalysisStatus::Analyzing => "badge-medium",
            AnalysisStatus::Complete => "badge-low",
            AnalysisStatus::Failed => "badge-critical",
            AnalysisStatus::Cancelled => "badge-high",
        }
    }
}

/// Risk score view model with color-coded levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RiskScoreView {
    pub overall: f32,
    pub copyright: f32,
    pub community_guidelines: f32,
    pub monetization: f32,
    pub overall_label: String,
    pub overall_color: String,
}

impl RiskScoreView {
    pub fn new(overall: f32, copyright: f32, community_guidelines: f32, monetization: f32) -> Self {
        let overall = overall.clamp(0.0, 100.0);
        let (label, color) = Self::risk_level(overall);
        Self {
            overall,
            copyright: copyright.clamp(0.0, 100.0),
            community_guidelines: community_guidelines.clamp(0.0, 100.0),
            monetization: monetization.clamp(0.0, 100.0),
            overall_label: label,
            overall_color: color,
        }
    }

    pub fn risk_level(score: f32) -> (String, String) {
        match score {
            s if s < 25.0 => ("Low".to_string(), "#4ade80".to_string()),
            s if s < 50.0 => ("Medium".to_string(), "#facc15".to_string()),
            s if s < 75.0 => ("High".to_string(), "#fb923c".to_string()),
            _ => ("Critical".to_string(), "#ef4444".to_string()),
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self.overall {
            s if s < 25.0 => "badge-low",
            s if s < 50.0 => "badge-medium",
            s if s < 75.0 => "badge-high",
            _ => "badge-critical",
        }
    }

    pub fn css_color(&self) -> &'static str {
        match self.overall {
            s if s < 25.0 => "var(--risk-low)",
            s if s < 50.0 => "var(--risk-medium)",
            s if s < 75.0 => "var(--risk-high)",
            _ => "var(--risk-critical)",
        }
    }

    pub fn css_bg(&self) -> &'static str {
        match self.overall {
            s if s < 25.0 => "var(--risk-low-bg)",
            s if s < 50.0 => "var(--risk-medium-bg)",
            s if s < 75.0 => "var(--risk-high-bg)",
            _ => "var(--risk-critical-bg)",
        }
    }
}

/// Individual violation view model
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ViolationView {
    pub id: String,
    pub violation_type: ViolationType,
    pub severity: SeverityLevel,
    pub category: String,
    pub description: String,
    pub details: String,
    pub timestamp: Option<f64>,
    pub duration: Option<f64>,
    pub confidence: f32,
    pub suggested_fix: Option<FixSuggestionView>,
    pub created_at: DateTime<Utc>,
}

impl Default for ViolationView {
    fn default() -> Self {
        Self {
            id: "default".to_string(),
            violation_type: ViolationType::Copyright,
            severity: SeverityLevel::Low,
            category: "General".to_string(),
            description: "Default violation".to_string(),
            details: String::new(),
            timestamp: None,
            duration: None,
            confidence: 0.0,
            suggested_fix: None,
            created_at: Utc::now(),
        }
    }
}

/// Violation type categories
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ViolationType {
    Copyright,
    CommunityGuidelines,
    Monetization,
    ContentId,
    AgeRestriction,
    Misinformation,
    HateSpeech,
    Harassment,
    Spam,
    Other,
}

impl ViolationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ViolationType::Copyright => "Copyright",
            ViolationType::CommunityGuidelines => "Community Guidelines",
            ViolationType::Monetization => "Monetization",
            ViolationType::ContentId => "Content ID",
            ViolationType::AgeRestriction => "Age Restriction",
            ViolationType::Misinformation => "Misinformation",
            ViolationType::HateSpeech => "Hate Speech",
            ViolationType::Harassment => "Harassment",
            ViolationType::Spam => "Spam",
            ViolationType::Other => "Other",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            ViolationType::Copyright => "copyright",
            ViolationType::CommunityGuidelines => "shield",
            ViolationType::Monetization => "dollar",
            ViolationType::ContentId => "fingerprint",
            ViolationType::AgeRestriction => "age",
            ViolationType::Misinformation => "alert",
            ViolationType::HateSpeech => "warning",
            ViolationType::Harassment => "user-x",
            ViolationType::Spam => "spam",
            ViolationType::Other => "info",
        }
    }
}

/// Severity level for violations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SeverityLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl SeverityLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            SeverityLevel::Low => "Low",
            SeverityLevel::Medium => "Medium",
            SeverityLevel::High => "High",
            SeverityLevel::Critical => "Critical",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            SeverityLevel::Low => "badge-low",
            SeverityLevel::Medium => "badge-medium",
            SeverityLevel::High => "badge-high",
            SeverityLevel::Critical => "badge-critical",
        }
    }

    pub fn css_color(&self) -> &'static str {
        match self {
            SeverityLevel::Low => "var(--risk-low)",
            SeverityLevel::Medium => "var(--risk-medium)",
            SeverityLevel::High => "var(--risk-high)",
            SeverityLevel::Critical => "var(--risk-critical)",
        }
    }

    pub fn numeric(&self) -> u8 {
        match self {
            SeverityLevel::Low => 1,
            SeverityLevel::Medium => 2,
            SeverityLevel::High => 3,
            SeverityLevel::Critical => 4,
        }
    }
}

/// Fix suggestion view model
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FixSuggestionView {
    pub id: String,
    pub violation_id: String,
    pub fix_type: FixType,
    pub description: String,
    pub original_value: String,
    pub suggested_value: String,
    pub parameters: Vec<FixParameter>,
    pub preview: String,
    pub confidence: f32,
    pub auto_applicable: bool,
}

impl Default for FixSuggestionView {
    fn default() -> Self {
        Self {
            id: "fix_default".to_string(),
            violation_id: "default".to_string(),
            fix_type: FixType::Replace,
            description: "Default fix".to_string(),
            original_value: String::new(),
            suggested_value: String::new(),
            parameters: Vec::new(),
            preview: String::new(),
            confidence: 0.0,
            auto_applicable: false,
        }
    }
}

/// Fix type enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FixType {
    Replace,
    Remove,
    Mute,
    Blur,
    Trim,
    AddDisclaimer,
    ModifyTitle,
    ModifyDescription,
    ModifyTags,
    Other,
}

impl FixType {
    pub fn as_str(&self) -> &'static str {
        match self {
            FixType::Replace => "Replace",
            FixType::Remove => "Remove",
            FixType::Mute => "Mute",
            FixType::Blur => "Blur",
            FixType::Trim => "Trim",
            FixType::AddDisclaimer => "Add Disclaimer",
            FixType::ModifyTitle => "Modify Title",
            FixType::ModifyDescription => "Modify Description",
            FixType::ModifyTags => "Modify Tags",
            FixType::Other => "Other",
        }
    }
}

/// Adjustable fix parameter
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FixParameter {
    pub name: String,
    pub label: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub unit: String,
}

impl FixParameter {
    pub fn new(name: &str, label: &str, value: f32, min: f32, max: f32, step: f32, unit: &str) -> Self {
        Self {
            name: name.to_string(),
            label: label.to_string(),
            value,
            min,
            max,
            step,
            unit: unit.to_string(),
        }
    }
}

/// Timeline event view model
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineEventView {
    pub id: String,
    pub event_type: TimelineEventType,
    pub label: String,
    pub description: String,
    pub timestamp: f64,
    pub severity: Option<SeverityLevel>,
    pub metadata: serde_json::Value,
}

impl Default for TimelineEventView {
    fn default() -> Self {
        Self {
            id: "default".to_string(),
            event_type: TimelineEventType::Info,
            label: "Default".to_string(),
            description: String::new(),
            timestamp: 0.0,
            severity: None,
            metadata: serde_json::Value::Null,
        }
    }
}

/// Timeline event type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TimelineEventType {
    Info,
    Warning,
    Violation,
    Fix,
    Milestone,
}

impl TimelineEventType {
    pub fn css_color(&self) -> &'static str {
        match self {
            TimelineEventType::Info => "var(--accent)",
            TimelineEventType::Warning => "var(--risk-medium)",
            TimelineEventType::Violation => "var(--risk-critical)",
            TimelineEventType::Fix => "var(--risk-low)",
            TimelineEventType::Milestone => "var(--info)",
        }
    }

    pub fn css_bg(&self) -> &'static str {
        match self {
            TimelineEventType::Info => "var(--accent-dim)",
            TimelineEventType::Warning => "var(--risk-medium-bg)",
            TimelineEventType::Violation => "var(--risk-critical-bg)",
            TimelineEventType::Fix => "var(--risk-low-bg)",
            TimelineEventType::Milestone => "var(--accent-dim)",
        }
    }
}

/// Progress message for real-time updates
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProgressMessage {
    pub job_id: String,
    pub stage: String,
    pub stage_label: String,
    pub progress: f32,
    pub message: String,
    pub log_line: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

impl ProgressMessage {
    pub fn new(job_id: &str, stage: &str, stage_label: &str, progress: f32, message: &str) -> Self {
        Self {
            job_id: job_id.to_string(),
            stage: stage.to_string(),
            stage_label: stage_label.to_string(),
            progress: progress.clamp(0.0, 100.0),
            message: message.to_string(),
            log_line: None,
            timestamp: Utc::now(),
            metadata: None,
        }
    }

    pub fn with_log(mut self, log: &str) -> Self {
        self.log_line = Some(log.to_string());
        self
    }

    pub fn with_metadata(mut self, meta: serde_json::Value) -> Self {
        self.metadata = Some(meta);
        self
    }
}

/// Video metadata
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct VideoMetadata {
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub category: String,
    pub privacy: PrivacyStatus,
    pub language: String,
    pub made_for_kids: bool,
    pub license: String,
    pub embeddable: bool,
    pub public_stats_viewable: bool,
}

/// Privacy status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum PrivacyStatus {
    #[default]
    Public,
    Unlisted,
    Private,
}

impl PrivacyStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            PrivacyStatus::Public => "public",
            PrivacyStatus::Unlisted => "unlisted",
            PrivacyStatus::Private => "private",
        }
    }
}

/// Job list item for job list page
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobListItem {
    pub id: String,
    pub video_title: String,
    pub status: AnalysisStatus,
    pub progress: f32,
    pub risk_score: f32,
    pub violation_count: usize,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for JobListItem {
    fn default() -> Self {
        Self {
            id: "default".to_string(),
            video_title: "Untitled".to_string(),
            status: AnalysisStatus::Queued,
            progress: 0.0,
            risk_score: 0.0,
            violation_count: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

/// Submit analysis request
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SubmitAnalysisRequest {
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub video_data: Option<Vec<u8>>,
    pub video_url: Option<String>,
    pub privacy: PrivacyStatus,
    pub language: String,
    pub category: String,
}

/// Submit analysis response
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SubmitAnalysisResponse {
    pub job_id: String,
    pub status: String,
    pub message: String,
}

/// Job status response
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JobStatus {
    pub job_id: String,
    pub status: AnalysisStatus,
    pub progress: f32,
    pub current_stage: String,
    pub message: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Fix response
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FixResponse {
    pub success: bool,
    pub applied_fixes: Vec<String>,
    pub failed_fixes: Vec<String>,
    pub new_job_id: Option<String>,
    pub message: String,
}

/// App settings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppSettings {
    pub api_base_url: String,
    pub ws_url: String,
    pub theme: Theme,
    pub auto_refresh: bool,
    pub show_preview: bool,
    pub confidence_threshold: f32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            api_base_url: "http://localhost:8080/api".to_string(),
            ws_url: "ws://localhost:8080/ws".to_string(),
            theme: Theme::Dark,
            auto_refresh: true,
            show_preview: true,
            confidence_threshold: 0.7,
        }
    }
}

/// Theme enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Theme {
    Dark,
    Light,
    System,
}

impl Theme {
    pub fn as_str(&self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
            Theme::System => "system",
        }
    }
}

/// WebSocket connection state
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

impl ConnectionState {
    pub fn css_class(&self) -> &'static str {
        match self {
            ConnectionState::Disconnected => "status-dot",
            ConnectionState::Connecting => "status-dot connecting",
            ConnectionState::Connected => "status-dot active",
            ConnectionState::Error(_) => "status-dot error",
        }
    }

    pub fn label(&self) -> String {
        match self {
            ConnectionState::Disconnected => "Disconnected".to_string(),
            ConnectionState::Connecting => "Connecting...".to_string(),
            ConnectionState::Connected => "Connected".to_string(),
            ConnectionState::Error(e) => format!("Error: {}", e),
        }
    }
}

/// Pipeline stage for job detail tracking
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PipelineStage {
    pub id: String,
    pub label: String,
    pub status: StageStatus,
    pub progress: f32,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

impl PipelineStage {
    pub fn new(id: &str, label: &str) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            status: StageStatus::Pending,
            progress: 0.0,
            started_at: None,
            completed_at: None,
            error: None,
        }
    }
}

/// Stage status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StageStatus {
    Pending,
    InProgress,
    Complete,
    Failed,
    Skipped,
}

impl StageStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            StageStatus::Pending => "Pending",
            StageStatus::InProgress => "In Progress",
            StageStatus::Complete => "Complete",
            StageStatus::Failed => "Failed",
            StageStatus::Skipped => "Skipped",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            StageStatus::Pending => "badge-accent",
            StageStatus::InProgress => "badge-medium",
            StageStatus::Complete => "badge-low",
            StageStatus::Failed => "badge-critical",
            StageStatus::Skipped => "badge-high",
        }
    }
}

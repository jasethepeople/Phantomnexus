// Package models defines all request/response DTOs used by the API handlers.
// Each struct is annotated with JSON tags for consistent serialization.
package models

import (
	"time"
)

// ---------------------------------------------------------------------------
// Submit (upload + analysis)
// ---------------------------------------------------------------------------

// SubmitAnalysisRequest represents the multipart form data for starting
// a new video analysis job.
type SubmitAnalysisRequest struct {
	// VideoFile is populated by the handler from the multipart form
	// field named "video".
	VideoFile string `json:"-" form:"video"`

	// CallbackURL is an optional webhook invoked when analysis completes.
	CallbackURL string `json:"callback_url,omitempty" form:"callback_url"`

	// AnalysisOptions allows the caller to enable/disable specific checks.
	AnalysisOptions *AnalysisOptions `json:"analysis_options,omitempty" form:"analysis_options"`
}

// AnalysisOptions controls which analysis stages are executed.
type AnalysisOptions struct {
	CheckCopyright    bool `json:"check_copyright,omitempty" form:"check_copyright"`
	CheckVisibility   bool `json:"check_visibility,omitempty" form:"check_visibility"`
	CheckMonetization bool `json:"check_monetization,omitempty" form:"check_monetization"`
	CheckAgeGate      bool `json:"check_age_gate,omitempty" form:"check_age_gate"`
	CheckRegionBlock  bool `json:"check_region_block,omitempty" form:"check_region_block"`
}

// SubmitAnalysisResponse is returned after a successful job submission.
type SubmitAnalysisResponse struct {
	JobID       string    `json:"job_id"`
	Status      string    `json:"status"`
	SubmittedAt time.Time `json:"submitted_at"`
	Message     string    `json:"message,omitempty"`
}

// ---------------------------------------------------------------------------
// Job status
// ---------------------------------------------------------------------------

// JobStatusResponse represents the current state of an analysis job.
type JobStatusResponse struct {
	JobID        string    `json:"job_id"`
	Status       string    `json:"status"`        // pending | processing | completed | failed
	Progress     float64   `json:"progress"`      // 0.0 – 1.0
	CurrentStage string    `json:"current_stage"` // e.g. "uploading", "probing", "analysing", "generating_report"
	Error        string    `json:"error,omitempty"`
	CreatedAt    time.Time `json:"created_at"`
	UpdatedAt    time.Time `json:"updated_at"`
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

// AnalysisReport is the full report returned for a completed job.
type AnalysisReport struct {
	JobID     string         `json:"job_id"`
	CreatedAt time.Time      `json:"created_at"`
	Video     VideoInfo      `json:"video"`
	Issues    []Issue        `json:"issues"`
	Summary   ReportSummary  `json:"summary"`
}

// VideoInfo holds metadata extracted from the uploaded file.
type VideoInfo struct {
	FileName   string  `json:"file_name"`
	FileSize   int64   `json:"file_size"`
	Duration   float64 `json:"duration"`   // seconds
	Width      int     `json:"width"`
	Height     int     `json:"height"`
	Codec      string  `json:"codec"`
	Bitrate    int64   `json:"bitrate"`
	FrameRate  float64 `json:"frame_rate"`
	Container  string  `json:"container"`
}

// Issue represents a single detected problem in the video.
type Issue struct {
	ID           string   `json:"id"`
	Type         string   `json:"type"`          // copyright | visibility | monetization | age_gate | region_block
	Severity     string   `json:"severity"`      // low | medium | high | critical
	Description  string   `json:"description"`
	Timestamp    float64  `json:"timestamp"`     // seconds, 0 if not time-specific
	Recommendations []string `json:"recommendations,omitempty"`
	Fixable      bool     `json:"fixable"`
}

// ReportSummary provides a high-level overview of the analysis.
type ReportSummary struct {
	TotalIssues    int    `json:"total_issues"`
	CriticalCount  int    `json:"critical_count"`
	HighCount      int    `json:"high_count"`
	MediumCount    int    `json:"medium_count"`
	LowCount       int    `json:"low_count"`
	OverallHealth  string `json:"overall_health"` // healthy | needs_attention | critical
	Score          int    `json:"score"`          // 0-100
}

// ---------------------------------------------------------------------------
// Fix
// ---------------------------------------------------------------------------

// ApplyFixRequest carries the list of issue IDs the user wants to fix.
type ApplyFixRequest struct {
	FixIDs []string `json:"fix_ids" binding:"required,min=1"`
}

// ApplyFixResponse reports the result of an attempted fix operation.
type ApplyFixResponse struct {
	JobID        string        `json:"job_id"`
	Status       string        `json:"status"` // fixing | completed | failed
	AppliedCount int           `json:"applied_count"`
	FailedFixes  []FailedFix   `json:"failed_fixes,omitempty"`
	FixedVideoURL string       `json:"fixed_video_url,omitempty"`
	Message      string        `json:"message,omitempty"`
}

// FailedFix details a single fix that could not be applied.
type FailedFix struct {
	FixID   string `json:"fix_id"`
	Reason  string `json:"reason"`
}

// ---------------------------------------------------------------------------
// Download
// ---------------------------------------------------------------------------

// DownloadResponse is returned when a fixed video is ready for download.
type DownloadResponse struct {
	JobID        string `json:"job_id"`
	FileName     string `json:"file_name"`
	ContentType  string `json:"content_type"`
	FileSize     int64  `json:"file_size"`
	DownloadURL  string `json:"download_url,omitempty"`
}

// ---------------------------------------------------------------------------
// Upload (internal)
// ---------------------------------------------------------------------------

// UploadResult carries the outcome of the multipart upload handler.
type UploadResult struct {
	TempPath   string            `json:"temp_path"`
	FileName   string            `json:"file_name"`
	FileSize   int64             `json:"file_size"`
	ContentType string           `json:"content_type"`
	Metadata   map[string]string `json:"metadata,omitempty"`
}

// ---------------------------------------------------------------------------
// Progress (WebSocket + gRPC streaming)
// ---------------------------------------------------------------------------

// ProgressUpdate is emitted over the WebSocket to inform the client about
// the current stage of processing.
type ProgressUpdate struct {
	JobID        string  `json:"job_id"`
	Status       string  `json:"status"`        // pending | processing | completed | failed
	Progress     float64 `json:"progress"`      // 0.0 – 1.0
	CurrentStage string  `json:"current_stage"`
	Message      string  `json:"message,omitempty"`
	Timestamp    int64   `json:"timestamp"`     // Unix milliseconds
}

// ProgressMessage is the envelope sent over the WebSocket.
type ProgressMessage struct {
	Type    string         `json:"type"`    // "progress" | "error" | "complete" | "ping"
	Payload ProgressUpdate `json:"payload,omitempty"`
	Error   string         `json:"error,omitempty"`
}

// ---------------------------------------------------------------------------
// Error responses
// ---------------------------------------------------------------------------

// ErrorResponse is the standard error envelope returned by all handlers.
type ErrorResponse struct {
	Code      int       `json:"code"`
	Message   string    `json:"message"`
	RequestID string    `json:"request_id,omitempty"`
	Timestamp time.Time `json:"timestamp"`
	Details   string    `json:"details,omitempty"`
}

// ValidationErrorResponse is returned when request validation fails.
type ValidationErrorResponse struct {
	ErrorResponse
	Fields map[string]string `json:"fields"`
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

// HealthResponse represents the health status of the API service and its
// downstream dependencies.
type HealthResponse struct {
	Status    string            `json:"status"` // ok | degraded | unhealthy
	Version   string            `json:"version"`
	Uptime    string            `json:"uptime"`
	Timestamp time.Time         `json:"timestamp"`
	Services  map[string]string `json:"services"` // e.g. "grpc": "ok", "redis": "ok"
}

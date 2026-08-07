// Package handlers contains HTTP handlers for the API service.
package handlers

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/gin-gonic/gin"

	"github.com/youtube-sentinel/api-service/config"
	"github.com/youtube-sentinel/api-service/grpc"
	"github.com/youtube-sentinel/api-service/models"
)

// AnalysisHandler holds dependencies for analysis endpoints.
type AnalysisHandler struct {
	cfg        *config.Config
	grpcClient grpc.AnalysisClientInterface
}

// NewAnalysisHandler creates a new AnalysisHandler with the given dependencies.
func NewAnalysisHandler(cfg *config.Config, grpcClient grpc.AnalysisClientInterface) *AnalysisHandler {
	return &AnalysisHandler{
		cfg:        cfg,
		grpcClient: grpcClient,
	}
}

// ---------------------------------------------------------------------------
// POST /api/v1/analyze
// ---------------------------------------------------------------------------

// SubmitAnalysis handles video upload and initiates analysis via gRPC.
// It accepts a multipart form with a "video" file field and optional
// analysis_options and callback_url fields.
func (h *AnalysisHandler) SubmitAnalysis(c *gin.Context) {
	requestID := c.GetString("request_id")

	// Handle multipart upload
	result, videoInfo, err := handleUpload(c, h.cfg)
	if err != nil {
		c.JSON(http.StatusBadRequest, models.ErrorResponse{
			Code:      http.StatusBadRequest,
			Message:   "Failed to process upload",
			RequestID: requestID,
			Timestamp: time.Now().UTC(),
			Details:   err.Error(),
		})
		return
	}

	// Schedule cleanup of temp file
	defer DeleteTempFile(result.TempPath)

	// Parse optional analysis options from form
	var options *models.AnalysisOptions
	if optStr := c.PostForm("analysis_options"); optStr != "" {
		// Simple form-based options - in production, parse JSON
		options = &models.AnalysisOptions{
			CheckCopyright:    c.PostForm("check_copyright") == "true",
			CheckVisibility:   c.PostForm("check_visibility") == "true",
			CheckMonetization: c.PostForm("check_monetization") == "true",
			CheckAgeGate:      c.PostForm("check_age_gate") == "true",
			CheckRegionBlock:  c.PostForm("check_region_block") == "true",
		}
	} else {
		// Default: run all checks
		options = &models.AnalysisOptions{
			CheckCopyright:    true,
			CheckVisibility:   true,
			CheckMonetization: true,
			CheckAgeGate:      true,
			CheckRegionBlock:  true,
		}
	}

	// Submit to gRPC analysis service
	submitReq := &grpc.SubmitVideoRequest{
		FileName:        result.FileName,
		FilePath:        result.TempPath,
		ContentType:     result.ContentType,
		FileSize:        result.FileSize,
		Metadata:        videoInfo,
		AnalysisOptions: options,
	}

	ctx, cancel := contextWithTimeout(c, 30*time.Second)
	defer cancel()

	submitResp, err := h.grpcClient.SubmitVideo(ctx, submitReq)
	if err != nil {
		c.JSON(http.StatusInternalServerError, models.ErrorResponse{
			Code:      http.StatusInternalServerError,
			Message:   "Failed to submit analysis job",
			RequestID: requestID,
			Timestamp: time.Now().UTC(),
			Details:   err.Error(),
		})
		return
	}

	c.JSON(http.StatusAccepted, models.SubmitAnalysisResponse{
		JobID:       submitResp.JobID,
		Status:      submitResp.Status,
		SubmittedAt: time.Now().UTC(),
		Message:     submitResp.Message,
	})
}

// ---------------------------------------------------------------------------
// GET /api/v1/jobs/:id
// ---------------------------------------------------------------------------

// GetJobStatus returns the current status, progress, and stage of a job.
func (h *AnalysisHandler) GetJobStatus(c *gin.Context) {
	requestID := c.GetString("request_id")
	jobID := c.Param("id")

	if err := validateJobID(jobID); err != nil {
		c.JSON(http.StatusBadRequest, models.ErrorResponse{
			Code:      http.StatusBadRequest,
			Message:   "Invalid job ID",
			RequestID: requestID,
			Timestamp: time.Now().UTC(),
			Details:   err.Error(),
		})
		return
	}

	// Fetch report to derive status (in production, use a lighter status endpoint)
	ctx, cancel := contextWithTimeout(c, 10*time.Second)
	defer cancel()

	report, err := h.grpcClient.GetReport(ctx, jobID)
	if err != nil {
		// If report not found, job might still be pending
		if strings.Contains(err.Error(), "not found") {
			c.JSON(http.StatusOK, models.JobStatusResponse{
				JobID:        jobID,
				Status:       "pending",
				Progress:     0.0,
				CurrentStage: "queued",
				CreatedAt:    time.Now().UTC(),
				UpdatedAt:    time.Now().UTC(),
			})
			return
		}

		c.JSON(http.StatusInternalServerError, models.ErrorResponse{
			Code:      http.StatusInternalServerError,
			Message:   "Failed to fetch job status",
			RequestID: requestID,
			Timestamp: time.Now().UTC(),
			Details:   err.Error(),
		})
		return
	}

	// Derive status from the report
	status := "completed"
	progress := 1.0
	stage := "completed"
	if report.Summary.TotalIssues > 0 {
		stage = "report_generated"
	}

	c.JSON(http.StatusOK, models.JobStatusResponse{
		JobID:        jobID,
		Status:       status,
		Progress:     progress,
		CurrentStage: stage,
		CreatedAt:    report.CreatedAt,
		UpdatedAt:    time.Now().UTC(),
	})
}

// ---------------------------------------------------------------------------
// GET /api/v1/jobs/:id/report
// ---------------------------------------------------------------------------

// GetReport returns the full AnalysisReport for a completed job.
func (h *AnalysisHandler) GetReport(c *gin.Context) {
	requestID := c.GetString("request_id")
	jobID := c.Param("id")

	if err := validateJobID(jobID); err != nil {
		c.JSON(http.StatusBadRequest, models.ErrorResponse{
			Code:      http.StatusBadRequest,
			Message:   "Invalid job ID",
			RequestID: requestID,
			Timestamp: time.Now().UTC(),
			Details:   err.Error(),
		})
		return
	}

	ctx, cancel := contextWithTimeout(c, 10*time.Second)
	defer cancel()

	report, err := h.grpcClient.GetReport(ctx, jobID)
	if err != nil {
		if strings.Contains(err.Error(), "not found") {
			c.JSON(http.StatusNotFound, models.ErrorResponse{
				Code:      http.StatusNotFound,
				Message:   "Report not found for this job",
				RequestID: requestID,
				Timestamp: time.Now().UTC(),
				Details:   err.Error(),
			})
			return
		}

		c.JSON(http.StatusInternalServerError, models.ErrorResponse{
			Code:      http.StatusInternalServerError,
			Message:   "Failed to fetch analysis report",
			RequestID: requestID,
			Timestamp: time.Now().UTC(),
			Details:   err.Error(),
		})
		return
	}

	c.JSON(http.StatusOK, report)
}

// ---------------------------------------------------------------------------
// POST /api/v1/jobs/:id/fix
// ---------------------------------------------------------------------------

// ApplyFix handles requests to apply automatic fixes to detected issues.
func (h *AnalysisHandler) ApplyFix(c *gin.Context) {
	requestID := c.GetString("request_id")
	jobID := c.Param("id")

	if err := validateJobID(jobID); err != nil {
		c.JSON(http.StatusBadRequest, models.ErrorResponse{
			Code:      http.StatusBadRequest,
			Message:   "Invalid job ID",
			RequestID: requestID,
			Timestamp: time.Now().UTC(),
			Details:   err.Error(),
		})
		return
	}

	// Parse request body
	var req models.ApplyFixRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, models.ValidationErrorResponse{
			ErrorResponse: models.ErrorResponse{
				Code:      http.StatusBadRequest,
				Message:   "Invalid request body",
				RequestID: requestID,
				Timestamp: time.Now().UTC(),
			},
			Fields: map[string]string{
				"fix_ids": "fix_ids array is required with at least one element",
			},
		})
		return
	}

	// Validate fix_ids
	if len(req.FixIDs) == 0 {
		c.JSON(http.StatusBadRequest, models.ValidationErrorResponse{
			ErrorResponse: models.ErrorResponse{
				Code:      http.StatusBadRequest,
				Message:   "Validation failed",
				RequestID: requestID,
				Timestamp: time.Now().UTC(),
			},
			Fields: map[string]string{
				"fix_ids": "At least one fix_id is required",
			},
		})
		return
	}

	ctx, cancel := contextWithTimeout(c, 60*time.Second)
	defer cancel()

	resp, err := h.grpcClient.ApplyFix(ctx, jobID, req.FixIDs)
	if err != nil {
		c.JSON(http.StatusInternalServerError, models.ErrorResponse{
			Code:      http.StatusInternalServerError,
			Message:   "Failed to apply fixes",
			RequestID: requestID,
			Timestamp: time.Now().UTC(),
			Details:   err.Error(),
		})
		return
	}

	c.JSON(http.StatusOK, models.ApplyFixResponse{
		JobID:         jobID,
		Status:        resp.Status,
		AppliedCount:  resp.AppliedCount,
		FailedFixes:   resp.FailedFixes,
		FixedVideoURL: resp.FixedVideoURL,
		Message:       resp.Message,
	})
}

// ---------------------------------------------------------------------------
// GET /api/v1/jobs/:id/download
// ---------------------------------------------------------------------------

// DownloadFixed streams the fixed video file to the client.
func (h *AnalysisHandler) DownloadFixed(c *gin.Context) {
	requestID := c.GetString("request_id")
	jobID := c.Param("id")

	if err := validateJobID(jobID); err != nil {
		c.JSON(http.StatusBadRequest, models.ErrorResponse{
			Code:      http.StatusBadRequest,
			Message:   "Invalid job ID",
			RequestID: requestID,
			Timestamp: time.Now().UTC(),
			Details:   err.Error(),
		})
		return
	}

	// In production, fetch the fixed file path from gRPC or storage
	// For now, we look for a fixed file in the temp directory
	fixedFilePath := findFixedFile(h.cfg.TempDir, jobID)
	if fixedFilePath == "" {
		c.JSON(http.StatusNotFound, models.ErrorResponse{
			Code:      http.StatusNotFound,
			Message:   "Fixed video file not found",
			RequestID: requestID,
			Timestamp: time.Now().UTC(),
			Details:   "The fixed video may still be processing or the job does not exist",
		})
		return
	}

	// Open the file
	file, err := os.Open(fixedFilePath)
	if err != nil {
		c.JSON(http.StatusInternalServerError, models.ErrorResponse{
			Code:      http.StatusInternalServerError,
			Message:   "Failed to open fixed video file",
			RequestID: requestID,
			Timestamp: time.Now().UTC(),
			Details:   err.Error(),
		})
		return
	}
	defer file.Close()

	// Get file info
	stat, err := file.Stat()
	if err != nil {
		c.JSON(http.StatusInternalServerError, models.ErrorResponse{
			Code:      http.StatusInternalServerError,
			Message:   "Failed to stat fixed video file",
			RequestID: requestID,
			Timestamp: time.Now().UTC(),
			Details:   err.Error(),
		})
		return
	}

	// Set headers for file download
	contentType := "video/mp4"
	fileName := fmt.Sprintf("fixed_%s.mp4", jobID)
	c.Header("Content-Type", contentType)
	c.Header("Content-Disposition", fmt.Sprintf("attachment; filename=%q", fileName))
	c.Header("Content-Length", fmt.Sprintf("%d", stat.Size()))
	c.Header("X-Content-Type-Options", "nosniff")
	c.Header("X-Request-ID", requestID)

	// Stream the file
	c.Status(http.StatusOK)
	written, err := io.Copy(c.Writer, file)
	if err != nil {
		// Error during streaming - log but headers already sent
		fmt.Fprintf(c.Writer, "\n[error: stream interrupted after %d bytes]", written)
		return
	}
}

// ---------------------------------------------------------------------------
// GET /health
// ---------------------------------------------------------------------------

// HealthCheck returns the health status of the API service and its
// downstream dependencies (gRPC, Redis).
func (h *AnalysisHandler) HealthCheck(c *gin.Context) {
	requestID := c.GetString("request_id")
	services := make(map[string]string)

	// Check gRPC connectivity
	if h.grpcClient != nil {
		ctx, cancel := context.WithTimeout(c.Request.Context(), 5*time.Second)
		defer cancel()
		if err := h.grpcClient.HealthCheck(ctx); err != nil {
			services["grpc"] = "unhealthy: " + err.Error()
		} else {
			services["grpc"] = "ok"
		}
	} else {
		services["grpc"] = "not_configured"
	}

	// Overall status
	status := "ok"
	for _, s := range services {
		if s != "ok" {
			status = "degraded"
			break
		}
	}

	c.JSON(http.StatusOK, models.HealthResponse{
		Status:    status,
		Version:   "1.0.0",
		Uptime:    time.Since(serverStartTime).String(),
		Timestamp: time.Now().UTC(),
		Services:  services,
	})
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

var serverStartTime = time.Now().UTC()

// validateJobID checks that a job ID is non-empty and well-formed.
func validateJobID(id string) error {
	if id == "" {
		return fmt.Errorf("job ID is required")
	}
	if len(id) > 256 {
		return fmt.Errorf("job ID exceeds maximum length of 256 characters")
	}
	// Check for path traversal attempts
	if strings.Contains(id, "..") || strings.Contains(id, "/") || strings.Contains(id, "\\") {
		return fmt.Errorf("job ID contains invalid characters")
	}
	return nil
}

// contextWithTimeout creates a context with timeout from a gin context.
// It prefers the gin request context as parent.
func contextWithTimeout(c *gin.Context, timeout time.Duration) (context.Context, context.CancelFunc) {
	return context.WithTimeout(c.Request.Context(), timeout)
}

// findFixedFile searches the temp directory for a fixed file associated
// with the given job ID.
func findFixedFile(tempDir, jobID string) string {
	fixedDir := filepath.Join(tempDir, "youtube-sentinel", "fixed", jobID)
	entries, err := os.ReadDir(fixedDir)
	if err != nil {
		return ""
	}
	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}
		name := entry.Name()
		ext := strings.ToLower(filepath.Ext(name))
		if ext == ".mp4" || ext == ".mov" || ext == ".avi" || ext == ".mkv" || ext == ".webm" {
			return filepath.Join(fixedDir, name)
		}
	}
	return ""
}

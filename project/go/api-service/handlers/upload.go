// Package handlers contains HTTP handlers for the API service.
package handlers

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/google/uuid"
	"github.com/youtube-sentinel/api-service/config"
	"github.com/youtube-sentinel/api-service/models"
)

// ---------------------------------------------------------------------------
// Upload handler
// ---------------------------------------------------------------------------

// UploadResult carries the outcome of the multipart upload handler.
type UploadResult struct {
	TempPath    string            `json:"temp_path"`
	FileName    string            `json:"file_name"`
	FileSize    int64             `json:"file_size"`
	ContentType string            `json:"content_type"`
	Metadata    map[string]string `json:"metadata,omitempty"`
}

// handleUpload processes a multipart file upload, validates it, saves to a
// temporary directory, and probes the video with ffprobe for metadata.
// It returns an UploadResult on success or an error describing what went wrong.
func handleUpload(c *gin.Context, cfg *config.Config) (*UploadResult, *models.VideoInfo, error) {
	// Parse multipart form with max memory
	if err := c.Request.ParseMultipartForm(cfg.MaxMultipartMemory); err != nil {
		return nil, nil, fmt.Errorf("failed to parse multipart form: %w", err)
	}

	// Extract the file from the form
	fileHeader, err := c.FormFile("video")
	if err != nil {
		return nil, nil, fmt.Errorf("video file is required: %w", err)
	}

	// Validate file size
	if fileHeader.Size > cfg.UploadMaxSize {
		return nil, nil, fmt.Errorf("file size %d exceeds maximum allowed %d bytes",
			fileHeader.Size, cfg.UploadMaxSize)
	}

	// Validate content type
	contentType := fileHeader.Header.Get("Content-Type")
	if contentType == "" {
		contentType = "application/octet-stream"
	}
	if !isAllowedContentType(contentType, cfg.AllowedVideoTypes) {
		return nil, nil, fmt.Errorf("content type %q is not an allowed video format (allowed: %s)",
			contentType, strings.Join(cfg.AllowedVideoTypes, ", "))
	}

	// Open uploaded file
	file, err := fileHeader.Open()
	if err != nil {
		return nil, nil, fmt.Errorf("failed to open uploaded file: %w", err)
	}
	defer file.Close()

	// Generate safe filename
	safeName := sanitizeFilename(fileHeader.Filename)
	uniqueName := fmt.Sprintf("%s_%s", uuid.New().String()[:8], safeName)
	tempPath := filepath.Join(cfg.TempDir, "youtube-sentinel", "uploads", uniqueName)

	// Ensure upload directory exists
	if err := os.MkdirAll(filepath.Dir(tempPath), 0755); err != nil {
		return nil, nil, fmt.Errorf("failed to create upload directory: %w", err)
	}

	// Create temp file
	tempFile, err := os.Create(tempPath)
	if err != nil {
		return nil, nil, fmt.Errorf("failed to create temp file: %w", err)
	}

	// Copy uploaded data to temp file with size tracking
	written, err := io.Copy(tempFile, file)
	if err != nil {
		tempFile.Close()
		os.Remove(tempPath)
		return nil, nil, fmt.Errorf("failed to save uploaded file: %w", err)
	}
	tempFile.Close()

	// Verify file size matches
	if written != fileHeader.Size {
		os.Remove(tempPath)
		return nil, nil, fmt.Errorf("uploaded file size mismatch: expected %d, got %d", fileHeader.Size, written)
	}

	// Probe video metadata with ffprobe
	videoInfo, err := probeVideo(tempPath)
	if err != nil {
		// Non-fatal: log but don't fail the upload
		videoInfo = &models.VideoInfo{
			FileName:  safeName,
			FileSize:  fileHeader.Size,
			Container: strings.TrimPrefix(filepath.Ext(safeName), "."),
		}
	} else {
		videoInfo.FileName = safeName
		videoInfo.FileSize = fileHeader.Size
	}

	result := &UploadResult{
		TempPath:    tempPath,
		FileName:    safeName,
		FileSize:    fileHeader.Size,
		ContentType: contentType,
		Metadata: map[string]string{
			"upload_time":   time.Now().UTC().Format(time.RFC3339),
			"original_name": fileHeader.Filename,
		},
	}

	return result, videoInfo, nil
}

// isAllowedContentType checks whether the given content type is in the
// allowed list. It also handles common MIME type variations.
func isAllowedContentType(ct string, allowed []string) bool {
	ct = strings.ToLower(strings.TrimSpace(ct))
	// Handle empty or generic types by extension check
	if ct == "" || ct == "application/octet-stream" {
		return true // Let extension-based checks handle these
	}
	for _, a := range allowed {
		if strings.EqualFold(ct, a) {
			return true
		}
	}
	// Also allow common extensions as MIME types
	extToMIME := map[string]string{
		".mp4":  "video/mp4",
		".mov":  "video/quicktime",
		".avi":  "video/x-msvideo",
		".mkv":  "video/x-matroska",
		".webm": "video/webm",
	}
	for ext, mime := range extToMIME {
		if strings.EqualFold(ct, mime) {
			return true
		}
		_ = ext
	}
	// Also check by extension mapping from MIME
	for _, allowedType := range allowed {
		if strings.EqualFold(ct, allowedType) {
			return true
		}
	}
	return false
}

// sanitizeFilename removes potentially dangerous characters from a filename
// and ensures it has a safe extension.
func sanitizeFilename(name string) string {
	// Remove path components
	name = filepath.Base(name)

	// Replace potentially dangerous characters
	replacer := strings.NewReplacer(
		"..", "_",
		"/", "_",
		"\\", "_",
		"\x00", "",
	)
	name = replacer.Replace(name)

	// Ensure valid extension
	ext := strings.ToLower(filepath.Ext(name))
	validExts := map[string]bool{
		".mp4": true, ".mov": true, ".avi": true,
		".mkv": true, ".webm": true,
	}
	if !validExts[ext] {
		name = name + ".mp4"
	}

	return name
}

// probeVideo runs ffprobe on the given file path and extracts video metadata.
func probeVideo(filePath string) (*models.VideoInfo, error) {
	// Run ffprobe with JSON output
	cmd := exec.Command("ffprobe",
		"-v", "quiet",
		"-print_format", "json",
		"-show_format",
		"-show_streams",
		filePath,
	)

	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	if err := cmd.Run(); err != nil {
		// ffprobe not available or file not valid - return basic info
		return nil, fmt.Errorf("ffprobe failed: %v (stderr: %s)", err, stderr.String())
	}

	// Parse ffprobe JSON output
	var ffprobeOut struct {
		Format struct {
			Duration string `json:"duration"`
			BitRate  string `json:"bit_rate"`
			Format   string `json:"format_name"`
		} `json:"format"`
		Streams []struct {
			CodecType  string `json:"codec_type"`
			CodecName  string `json:"codec_name"`
			Width      int    `json:"width"`
			Height     int    `json:"height"`
			AvgFrameRate string `json:"avg_frame_rate"`
		} `json:"streams"`
	}

	if err := json.Unmarshal(stdout.Bytes(), &ffprobeOut); err != nil {
		return nil, fmt.Errorf("failed to parse ffprobe output: %w", err)
	}

	info := &models.VideoInfo{
		Container: ffprobeOut.Format.Format,
	}

	// Parse duration
	if ffprobeOut.Format.Duration != "" {
		if d, err := strconv.ParseFloat(ffprobeOut.Format.Duration, 64); err == nil {
			info.Duration = d
		}
	}

	// Parse bitrate
	if ffprobeOut.Format.BitRate != "" {
		if b, err := strconv.ParseInt(ffprobeOut.Format.BitRate, 10, 64); err == nil {
			info.Bitrate = b
		}
	}

	// Extract video stream info
	for _, s := range ffprobeOut.Streams {
		if s.CodecType == "video" {
			info.Codec = s.CodecName
			info.Width = s.Width
			info.Height = s.Height

			// Parse frame rate (e.g. "30/1" or "30000/1001")
			if s.AvgFrameRate != "" {
				parts := strings.Split(s.AvgFrameRate, "/")
				if len(parts) == 2 {
					numerator, _ := strconv.ParseFloat(parts[0], 64)
					denominator, _ := strconv.ParseFloat(parts[1], 64)
					if denominator > 0 {
						info.FrameRate = numerator / denominator
					}
				} else {
					info.FrameRate, _ = strconv.ParseFloat(s.AvgFrameRate, 64)
				}
			}
			break
		}
	}

	return info, nil
}

// DeleteTempFile removes a temporary uploaded file. It should be called
// after the file has been processed or on error cleanup.
func DeleteTempFile(path string) {
	if path != "" {
		_ = os.Remove(path)
	}
}

// ---------------------------------------------------------------------------
// Upload handler (HTTP endpoint)
// ---------------------------------------------------------------------------

// UploadHandler handles direct upload requests (for testing or pre-upload).
func UploadHandler(cfg *config.Config) gin.HandlerFunc {
	return func(c *gin.Context) {
		result, videoInfo, err := handleUpload(c, cfg)
		if err != nil {
			c.JSON(http.StatusBadRequest, models.ErrorResponse{
				Code:      http.StatusBadRequest,
				Message:   "Upload failed",
				RequestID: c.GetString("request_id"),
				Timestamp: time.Now().UTC(),
				Details:   err.Error(),
			})
			return
		}

		c.JSON(http.StatusOK, gin.H{
			"upload": result,
			"video":  videoInfo,
		})
	}
}

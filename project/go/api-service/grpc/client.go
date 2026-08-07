// Package grpc provides a client for communicating with the analysis gRPC service.
package grpc

import (
	"context"
	"fmt"
	"io"
	"time"

	"google.golang.org/grpc"
	"google.golang.org/grpc/connectivity"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/health/grpc_health_v1"
	"google.golang.org/grpc/keepalive"
	"google.golang.org/grpc/status"

	"github.com/youtube-sentinel/api-service/models"
)

// AnalysisClient wraps a gRPC connection to the analysis backend service.
type AnalysisClient struct {
	// conn is the underlying gRPC connection.
	conn *grpc.ClientConn

	// address of the gRPC server.
	address string

	// timeout for individual RPC calls.
	timeout time.Duration

	// retry configuration.
	retryAttempts int
	retryBackoff  time.Duration
}

// SubmitResponse is returned by SubmitVideo.
type SubmitResponse struct {
	JobID     string `json:"job_id"`
	Status    string `json:"status"`
	Message   string `json:"message,omitempty"`
}

// ProgressUpdate is streamed from the gRPC server during analysis.
type ProgressUpdate struct {
	JobID        string  `json:"job_id"`
	Status       string  `json:"status"`
	Progress     float64 `json:"progress"`
	CurrentStage string  `json:"current_stage"`
	Message      string  `json:"message,omitempty"`
	Timestamp    int64   `json:"timestamp"`
}

// FixResponse is returned by ApplyFix.
type FixResponse struct {
	Status       string        `json:"status"`
	AppliedCount int           `json:"applied_count"`
	FailedFixes  []models.FailedFix `json:"failed_fixes,omitempty"`
	FixedVideoURL string       `json:"fixed_video_url,omitempty"`
	Message      string        `json:"message,omitempty"`
}

// SubmitVideoRequest carries the video data for submission.
type SubmitVideoRequest struct {
	FileName        string                `json:"file_name"`
	FilePath        string                `json:"file_path"`
	ContentType     string                `json:"content_type"`
	FileSize        int64                 `json:"file_size"`
	Metadata        *models.VideoInfo     `json:"metadata,omitempty"`
	AnalysisOptions *models.AnalysisOptions `json:"analysis_options,omitempty"`
}

// AnalysisClientInterface defines the methods for easier mocking in tests.
type AnalysisClientInterface interface {
	SubmitVideo(ctx context.Context, req *SubmitVideoRequest) (*SubmitResponse, error)
	StreamProgress(ctx context.Context, jobID string) (<-chan *ProgressUpdate, error)
	GetReport(ctx context.Context, jobID string) (*models.AnalysisReport, error)
	ApplyFix(ctx context.Context, jobID string, fixIDs []string) (*FixResponse, error)
	Close() error
	HealthCheck(ctx context.Context) error
}

// NewAnalysisClient creates a gRPC client with connection retry, keepalive,
// and health-check support.
func NewAnalysisClient(addr string, timeout time.Duration, retryAttempts int, retryBackoff time.Duration) (*AnalysisClient, error) {
	if timeout == 0 {
		timeout = 10 * time.Second
	}
	if retryAttempts <= 0 {
		retryAttempts = 3
	}
	if retryBackoff == 0 {
		retryBackoff = 500 * time.Millisecond
	}

	kacp := keepalive.ClientParameters{
		Time:                10 * time.Second,
		Timeout:             time.Second,
		PermitWithoutStream: true,
	}

	var conn *grpc.ClientConn
	var err error

	// Retry connection with exponential backoff
	for i := 0; i < retryAttempts; i++ {
		ctx, cancel := context.WithTimeout(context.Background(), timeout)
		conn, err = grpc.DialContext(ctx, addr,
			grpc.WithTransportCredentials(insecure.NewCredentials()),
			grpc.WithKeepaliveParams(kacp),
			grpc.WithBlock(),
		)
		cancel()

		if err == nil {
			break
		}

		if i < retryAttempts-1 {
			time.Sleep(retryBackoff * time.Duration(i+1))
		}
	}

	if err != nil {
		return nil, fmt.Errorf("failed to connect to gRPC server at %s after %d attempts: %w", addr, retryAttempts, err)
	}

	// Perform initial health check
	healthClient := grpc_health_v1.NewHealthClient(conn)
	healthCtx, healthCancel := context.WithTimeout(context.Background(), timeout)
	defer healthCancel()

	healthResp, err := healthClient.Check(healthCtx, &grpc_health_v1.HealthCheckRequest{})
	if err != nil {
		conn.Close()
		return nil, fmt.Errorf("gRPC health check failed: %w", err)
	}
	if healthResp.Status != grpc_health_v1.HealthCheckResponse_SERVING {
		conn.Close()
		return nil, fmt.Errorf("gRPC server is not serving (status: %v)", healthResp.Status)
	}

	return &AnalysisClient{
		conn:          conn,
		address:       addr,
		timeout:       timeout,
		retryAttempts: retryAttempts,
		retryBackoff:  retryBackoff,
	}, nil
}

// HealthCheck performs a health check against the gRPC server.
func (c *AnalysisClient) HealthCheck(ctx context.Context) error {
	healthClient := grpc_health_v1.NewHealthClient(c.conn)

	ctx, cancel := context.WithTimeout(ctx, c.timeout)
	defer cancel()

	resp, err := healthClient.Check(ctx, &grpc_health_v1.HealthCheckRequest{})
	if err != nil {
		return fmt.Errorf("health check RPC failed: %w", err)
	}
	if resp.Status != grpc_health_v1.HealthCheckResponse_SERVING {
		return fmt.Errorf("gRPC server status: %v", resp.Status)
	}
	return nil
}

// isReady returns true if the connection is in a ready state.
func (c *AnalysisClient) isReady() bool {
	state := c.conn.GetState()
	return state == connectivity.Ready
}

// reconnect attempts to re-establish the gRPC connection.
func (c *AnalysisClient) reconnect() error {
	if c.conn != nil {
		c.conn.Close()
	}

	ctx, cancel := context.WithTimeout(context.Background(), c.timeout)
	defer cancel()

	conn, err := grpc.DialContext(ctx, c.address,
		grpc.WithTransportCredentials(insecure.NewCredentials()),
		grpc.WithBlock(),
	)
	if err != nil {
		return fmt.Errorf("reconnection failed: %w", err)
	}

	c.conn = conn
	return nil
}

// withRetry executes the given function with automatic reconnection on failure.
func (c *AnalysisClient) withRetry(ctx context.Context, fn func() error) error {
	var lastErr error

	for i := 0; i < c.retryAttempts; i++ {
		if !c.isReady() {
			if err := c.reconnect(); err != nil {
				lastErr = err
				time.Sleep(c.retryBackoff * time.Duration(i+1))
				continue
			}
		}

		if err := fn(); err != nil {
			s, ok := status.FromError(err)
			if ok && (s.Code().String() == "Unavailable" || s.Code().String() == "Internal") {
				lastErr = err
				time.Sleep(c.retryBackoff * time.Duration(i+1))
				continue
			}
			return err
		}
		return nil
	}

	return fmt.Errorf("all %d attempts failed: %w", c.retryAttempts, lastErr)
}

// SubmitVideo sends a video file to the gRPC analysis service for processing.
func (c *AnalysisClient) SubmitVideo(ctx context.Context, req *SubmitVideoRequest) (*SubmitResponse, error) {
	// Build the gRPC request using the protobuf-generated types.
	// Since we don't have the actual protobuf definitions, we use a generic approach
	// that would be replaced by generated code in production.
	grpcReq := &SubmitVideoGRPCRequest{
		FileName:        req.FileName,
		FilePath:        req.FilePath,
		ContentType:     req.ContentType,
		FileSize:        req.FileSize,
		Metadata:        req.Metadata,
		AnalysisOptions: req.AnalysisOptions,
	}

	var resp *SubmitResponse
	err := c.withRetry(ctx, func() error {
		ctx, cancel := context.WithTimeout(ctx, c.timeout)
		defer cancel()

		// In a real implementation, this would call the generated gRPC stub:
		// r, err := c.grpcClient.SubmitVideo(ctx, grpcReq)
		// For now, we simulate the response structure
		_ = ctx
		_ = grpcReq

		// Simulated response - in production, this comes from gRPC
		resp = &SubmitResponse{
			JobID:   fmt.Sprintf("job-%d", time.Now().UnixNano()),
			Status:  "pending",
			Message: "Video submitted for analysis",
		}
		return nil
	})

	if err != nil {
		return nil, fmt.Errorf("submit video failed: %w", err)
	}

	return resp, nil
}

// SubmitVideoGRPCRequest is a placeholder for the generated protobuf type.
type SubmitVideoGRPCRequest struct {
	FileName        string
	FilePath        string
	ContentType     string
	FileSize        int64
	Metadata        *models.VideoInfo
	AnalysisOptions *models.AnalysisOptions
}

// StreamProgress opens a server-side streaming RPC that yields progress
// updates for the given job ID.
func (c *AnalysisClient) StreamProgress(ctx context.Context, jobID string) (<-chan *ProgressUpdate, error) {
	if !c.isReady() {
		if err := c.reconnect(); err != nil {
			return nil, fmt.Errorf("cannot stream progress: %w", err)
		}
	}

	updates := make(chan *ProgressUpdate, 10)

	go func() {
		defer close(updates)

		// In a real implementation, this would open a gRPC streaming call:
		// stream, err := c.grpcClient.StreamProgress(ctx, &pb.ProgressRequest{JobId: jobID})
		// For now, we simulate the stream with periodic updates.

		stages := []struct {
			stage    string
			progress float64
			message  string
		}{
			{"uploading", 0.10, "Uploading video file"},
			{"probing", 0.20, "Extracting video metadata"},
			{"analysing", 0.50, "Running analysis pipeline"},
			{"processing", 0.80, "Processing analysis results"},
			{"generating_report", 0.95, "Generating final report"},
			{"completed", 1.00, "Analysis complete"},
		}

		ticker := time.NewTicker(2 * time.Second)
		defer ticker.Stop()

		for i, stage := range stages {
			select {
			case <-ctx.Done():
				updates <- &ProgressUpdate{
					JobID:        jobID,
					Status:       "cancelled",
					Progress:     stages[max(0, i-1)].progress,
					CurrentStage: stages[max(0, i-1)].stage,
					Message:      "Stream cancelled by client",
					Timestamp:    time.Now().UnixMilli(),
				}
				return
			case <-ticker.C:
				updates <- &ProgressUpdate{
					JobID:        jobID,
					Status:       "processing",
					Progress:     stage.progress,
					CurrentStage: stage.stage,
					Message:      stage.message,
					Timestamp:    time.Now().UnixMilli(),
				}
			}
		}

		updates <- &ProgressUpdate{
			JobID:        jobID,
			Status:       "completed",
			Progress:     1.0,
			CurrentStage: "completed",
			Message:      "Analysis complete",
			Timestamp:    time.Now().UnixMilli(),
		}
	}()

	return updates, nil
}

// max returns the larger of a and b.
func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}

// min returns the smaller of a and b.
func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}

// GetReport fetches the full analysis report for a completed job.
func (c *AnalysisClient) GetReport(ctx context.Context, jobID string) (*models.AnalysisReport, error) {
	var report *models.AnalysisReport

	err := c.withRetry(ctx, func() error {
		ctx, cancel := context.WithTimeout(ctx, c.timeout)
		defer cancel()

		// In a real implementation:
		// resp, err := c.grpcClient.GetReport(ctx, &pb.ReportRequest{JobId: jobID})
		_ = ctx

		// Simulated response
		report = &models.AnalysisReport{
			JobID:     jobID,
			CreatedAt: time.Now().UTC(),
			Video: models.VideoInfo{
				FileName:  "sample.mp4",
				FileSize:  1024000,
				Duration:  120.5,
				Width:     1920,
				Height:    1080,
				Codec:     "h264",
				Bitrate:   5000000,
				FrameRate: 30.0,
				Container: "mp4",
			},
			Issues: []models.Issue{
				{
					ID:          "issue-001",
					Type:        "copyright",
					Severity:    "high",
					Description: "Potential copyrighted audio detected at 0:45",
					Timestamp:   45.0,
					Recommendations: []string{
						"Replace audio track with royalty-free alternative",
						"Apply audio filter to modify the detected segment",
					},
					Fixable: true,
				},
			},
			Summary: models.ReportSummary{
				TotalIssues:   1,
				CriticalCount: 0,
				HighCount:     1,
				MediumCount:   0,
				LowCount:      0,
				OverallHealth: "needs_attention",
				Score:         75,
			},
		}
		return nil
	})

	if err != nil {
		return nil, fmt.Errorf("get report failed: %w", err)
	}

	return report, nil
}

// ApplyFix sends a fix request to the gRPC service.
func (c *AnalysisClient) ApplyFix(ctx context.Context, jobID string, fixIDs []string) (*FixResponse, error) {
	if len(fixIDs) == 0 {
		return nil, fmt.Errorf("no fix IDs provided")
	}

	var resp *FixResponse

	err := c.withRetry(ctx, func() error {
		ctx, cancel := context.WithTimeout(ctx, c.timeout)
		defer cancel()

		// In a real implementation:
		// r, err := c.grpcClient.ApplyFix(ctx, &pb.FixRequest{JobId: jobID, FixIds: fixIDs})
		_ = ctx
		_ = jobID

		resp = &FixResponse{
			Status:        "completed",
			AppliedCount:  len(fixIDs),
			FailedFixes:   []models.FailedFix{},
			FixedVideoURL: fmt.Sprintf("/api/v1/jobs/%s/download", jobID),
			Message:       fmt.Sprintf("Successfully applied %d fix(es)", len(fixIDs)),
		}
		return nil
	})

	if err != nil {
		return nil, fmt.Errorf("apply fix failed: %w", err)
	}

	return resp, nil
}

// Close closes the gRPC connection.
func (c *AnalysisClient) Close() error {
	if c.conn != nil {
		return c.conn.Close()
	}
	return nil
}

// Connection returns the underlying gRPC connection for health checks.
func (c *AnalysisClient) Connection() *grpc.ClientConn {
	return c.conn
}

// ProgressStreamReader abstracts reading from a gRPC progress stream.
// In production this would be the generated protobuf client stream type.
type ProgressStreamReader interface {
	Recv() (*ProgressUpdate, error)
}

// ReadAllProgress drains a progress stream into a slice.
func ReadAllProgress(stream ProgressStreamReader) ([]*ProgressUpdate, error) {
	var updates []*ProgressUpdate
	for {
		u, err := stream.Recv()
		if err == io.EOF {
			break
		}
		if err != nil {
			return updates, err
		}
		updates = append(updates, u)
	}
	return updates, nil
}

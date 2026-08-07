// Package grpc provides a gRPC client for the video analysis service.
package grpc

import (
	"context"
	"fmt"
	"log/slog"
	"sync"
	"sync/atomic"
	"time"

	"google.golang.org/grpc"
	"google.golang.org/grpc/connectivity"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/status"

	"github.com/youtube-sentinel/job-worker/pb"
	"github.com/youtube-sentinel/job-worker/queue"
)

const (
	defaultDialTimeout    = 5 * time.Second
	defaultRequestTimeout = 60 * time.Second
	maxReconnectBackoff   = 30 * time.Second
)

// AnalysisClient wraps the gRPC analysis service client with health monitoring and retries.
type AnalysisClient struct {
	addr       string
	conn       *grpc.ClientConn
	client     pb.AnalysisServiceClient
	logger     *slog.Logger
	healthy    atomic.Bool
	mu         sync.RWMutex
	stopCh     chan struct{}
	wg         sync.WaitGroup
}

// AnalysisResult wraps the analysis outcome.
type AnalysisResult struct {
	JobID          string
	Status         string
	Violations     int
	AutoFixApplied bool
	FixDetails     string
	ProcessedAt    time.Time
}

// NewAnalysisClient creates a new analysis gRPC client with auto-reconnection.
func NewAnalysisClient(addr string, logger *slog.Logger) (*AnalysisClient, error) {
	if logger == nil {
		logger = slog.Default()
	}

	ctx, cancel := context.WithTimeout(context.Background(), defaultDialTimeout)
	defer cancel()

	conn, err := grpc.DialContext(ctx, addr,
		grpc.WithTransportCredentials(insecure.NewCredentials()),
		grpc.WithBlock(),
		grpc.WithDefaultServiceConfig(`{"healthCheckConfig":{"serviceName":""}}`),
	)
	if err != nil {
		// Try non-blocking fallback
		conn, err = grpc.Dial(addr,
			grpc.WithTransportCredentials(insecure.NewCredentials()),
		)
		if err != nil {
			return nil, fmt.Errorf("dial grpc server at %s: %w", addr, err)
		}
	}

	c := &AnalysisClient{
		addr:   addr,
		conn:   conn,
		client: pb.NewAnalysisServiceClient(conn),
		logger: logger,
		stopCh: make(chan struct{}),
	}
	c.healthy.Store(conn.GetState() == connectivity.Ready)

	// Start health monitor
	c.wg.Add(1)
	go c.healthMonitor()

	return c, nil
}

// AnalyzeVideo sends a video analysis request to the gRPC server.
func (c *AnalysisClient) AnalyzeVideo(ctx context.Context, job *queue.Job) (*AnalysisResult, error) {
	c.mu.RLock()
	client := c.client
	c.mu.RUnlock()

	if client == nil {
		return nil, fmt.Errorf("grpc client not initialized")
	}

	req := &pb.AnalysisRequest{
		JobId:         job.ID,
		VideoPath:     job.VideoPath,
		Title:         job.Title,
		Description:   job.Description,
		Tags:          job.Tags,
		EnableAutoFix: job.EnableAutoFix,
	}

	ctx, cancel := context.WithTimeout(ctx, defaultRequestTimeout)
	defer cancel()

	resp, err := client.AnalyzeVideo(ctx, req)
	if err != nil {
		st, _ := status.FromError(err)
		c.logger.Error("analyze video failed",
			"job_id", job.ID,
			"code", st.Code(),
			"message", st.Message(),
		)
		return nil, classifyGRPCError(st.Code(), st.Message())
	}

	if resp == nil {
		return nil, fmt.Errorf("empty response from analysis server")
	}

	result := &AnalysisResult{
		JobID:          resp.JobId,
		Status:         resp.Status,
		ProcessedAt:    time.Now().UTC(),
	}

	if len(resp.Violations) > 0 {
		result.Violations = len(resp.Violations)
		for _, v := range resp.Violations {
			if result.FixDetails != "" {
				result.FixDetails += "; "
			}
			result.FixDetails += fmt.Sprintf("[%s] %s: %s", v.Severity, v.Type, v.Description)
		}
	}
	result.AutoFixApplied = resp.AutoFixApplied

	c.logger.Info("video analysis completed",
		"job_id", job.ID,
		"violations", result.Violations,
		"auto_fix", result.AutoFixApplied,
	)
	return result, nil
}

// AnalyzeVideoWithRetry sends a video analysis request with retry logic.
func (c *AnalysisClient) AnalyzeVideoWithRetry(
	ctx context.Context,
	job *queue.Job,
	maxRetries int,
) (*AnalysisResult, error) {
	var lastErr error
	for attempt := 0; attempt <= maxRetries; attempt++ {
		result, err := c.AnalyzeVideo(ctx, job)
		if err == nil {
			return result, nil
		}
		lastErr = err

		// Check if it's a permanent error
		var permErr *queue.PermanentError
		if _, ok := err.(*queue.PermanentError); ok {
			return nil, err
		}

		if attempt < maxRetries {
			backoff := time.Second * time.Duration(1<<attempt)
			if backoff > maxReconnectBackoff {
				backoff = maxReconnectBackoff
			}
			c.logger.Warn("analysis retry",
				"job_id", job.ID,
				"attempt", attempt+1,
				"max", maxRetries,
				"backoff", backoff,
			)
			select {
			case <-time.After(backoff):
				continue
			case <-ctx.Done():
				return nil, fmt.Errorf("analysis cancelled: %w", ctx.Err())
			}
		}
	}
	return nil, fmt.Errorf("analysis failed after %d attempts: %w", maxRetries+1, lastErr)
}

// IsHealthy returns whether the gRPC connection is healthy.
func (c *AnalysisClient) IsHealthy() bool {
	return c.healthy.Load()
}

// Close closes the gRPC connection and stops the health monitor.
func (c *AnalysisClient) Close() error {
	close(c.stopCh)
	c.wg.Wait()
	if c.conn != nil {
		return c.conn.Close()
	}
	return nil
}

// ============================================================================
// Internal
// ============================================================================

func (c *AnalysisClient) healthMonitor() {
	defer c.wg.Done()
	ticker := time.NewTicker(10 * time.Second)
	defer ticker.Stop()

	for {
		select {
		case <-ticker.C:
			c.checkHealth()
		case <-c.stopCh:
			return
		}
	}
}

func (c *AnalysisClient) checkHealth() {
	state := c.conn.GetState()
	wasHealthy := c.healthy.Load()
	isHealthy := state == connectivity.Ready

	if isHealthy != wasHealthy {
		c.healthy.Store(isHealthy)
		if isHealthy {
			c.logger.Info("grpc connection healthy", "addr", c.addr)
		} else {
			c.logger.Warn("grpc connection unhealthy", "addr", c.addr, "state", state.String())
		}
	}

	if !isHealthy {
		c.attemptReconnect()
	}
}

func (c *AnalysisClient) attemptReconnect() {
	c.logger.Info("attempting gRPC reconnection", "addr", c.addr)

	ctx, cancel := context.WithTimeout(context.Background(), defaultDialTimeout)
	defer cancel()

	conn, err := grpc.DialContext(ctx, c.addr,
		grpc.WithTransportCredentials(insecure.NewCredentials()),
		grpc.WithBlock(),
	)
	if err != nil {
		c.logger.Error("grpc reconnection failed", "addr", c.addr, "error", err)
		return
	}

	c.mu.Lock()
	oldConn := c.conn
	c.conn = conn
	c.client = pb.NewAnalysisServiceClient(conn)
	c.mu.Unlock()

	if oldConn != nil {
		oldConn.Close()
	}

	c.healthy.Store(true)
	c.logger.Info("grpc reconnection successful", "addr", c.addr)
}

func classifyGRPCError(code interface{}, message string) error {
	// The code comes from status.Code() which is a codes.Code (uint32)
	// Permanent errors that should NOT be retried
	permanentCodes := map[uint32]bool{
		3:  true, // InvalidArgument
		5:  true, // NotFound
		7:  true, // PermissionDenied
		11: true, // OutOfRange
	}

	codeVal, ok := code.(interface{ String() string })
	_ = codeVal

	// Use numeric check for permanent errors
	codeNum, ok := code.(uint32)
	if !ok {
		// Try conversion via fmt
		fmt.Sscanf(fmt.Sprintf("%v", code), "%d", &codeNum)
	}

	if permanentCodes[codeNum] {
		return &queue.PermanentError{
			Message: message,
			Cause:   fmt.Errorf("gRPC code %v", code),
		}
	}
	return fmt.Errorf("gRPC error %v: %s", code, message)
}

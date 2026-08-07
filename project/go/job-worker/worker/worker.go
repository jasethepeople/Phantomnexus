// Package worker provides the worker pool and individual worker implementations.
package worker

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"sync"
	"time"

	"github.com/youtube-sentinel/job-worker/grpc"
	"github.com/youtube-sentinel/job-worker/metrics"
	"github.com/youtube-sentinel/job-worker/queue"
)

const (
	defaultDequeueTimeout = 5 * time.Second
	heartbeatInterval     = 30 * time.Second
	progressUpdateInterval = 10 * time.Second
)

// Worker processes jobs from the queue.
type Worker struct {
	id       int
	pool     *Pool
	queue    queue.JobQueue
	grpc     *grpc.AnalysisClient
	metrics  *metrics.WorkerMetrics
	logger   *slog.Logger
	ctx      context.Context
	cancel   context.CancelFunc
	wg       sync.WaitGroup
	stopCh   chan struct{}
}

// newWorker creates a new worker instance.
func newWorker(id int, pool *Pool) *Worker {
	ctx, cancel := context.WithCancel(context.Background())
	return &Worker{
		id:     id,
		pool:   pool,
		queue:  pool.queue,
		grpc:   pool.grpc,
		metrics: pool.metrics,
		logger: pool.logger.With("worker_id", id),
		ctx:    ctx,
		cancel: cancel,
		stopCh: make(chan struct{}),
	}
}

// Run starts the worker's main processing loop.
func (w *Worker) Run() {
	w.wg.Add(1)
	defer w.wg.Done()

	w.logger.Info("worker started")
	for {
		select {
		case <-w.ctx.Done():
			w.logger.Info("worker shutting down")
			return
		case <-w.stopCh:
			w.logger.Info("worker stopped")
			return
		default:
		}

		// Dequeue next job with blocking pop
		job, err := w.queue.Dequeue(w.ctx, w.pool.config.QueueName, defaultDequeueTimeout)
		if err != nil {
			w.logger.Error("dequeue failed", "error", err)
			select {
			case <-w.ctx.Done():
				return
			case <-time.After(time.Second):
				continue
			}
		}
		if job == nil {
			// Timeout, no jobs available
			continue
		}

		// Process the dequeued job
		if err := w.processJob(job); err != nil {
			w.logger.Error("job processing failed",
				"job_id", job.ID,
				"error", err,
			)
			w.handleFailure(job, err)
		}
	}
}

// Stop gracefully stops the worker.
func (w *Worker) Stop() {
	close(w.stopCh)
	w.cancel()
	w.wg.Wait()
}

// processJob handles the full processing pipeline for a single job.
func (w *Worker) processJob(job *queue.Job) error {
	w.metrics.RecordJobStarted()
	startTime := time.Now()

	w.logger.Info("processing job",
		"job_id", job.ID,
		"video", job.VideoPath,
		"attempt", job.RetryCount+1,
	)

	// Start heartbeat goroutine
	hbCtx, hbCancel := context.WithCancel(w.ctx)
	hbDone := make(chan struct{})
	go w.heartbeatLoop(hbCtx, job.ID, hbDone)

	// Start progress update goroutine
	progCtx, progCancel := context.WithCancel(w.ctx)
	progDone := make(chan struct{})
	go w.progressLoop(progCtx, job.ID, progDone)

	// Validate job
	if err := w.validateJob(job); err != nil {
		hbCancel()
		progCancel()
		<-hbDone
		<-progDone
		return fmt.Errorf("validation: %w", err)
	}

	// Verify video file exists
	if _, err := os.Stat(job.VideoPath); err != nil {
		if os.IsNotExist(err) {
			hbCancel()
			progCancel()
			<-hbDone
			<-progDone
			return &queue.PermanentError{
				Message: fmt.Sprintf("video file not found: %s", job.VideoPath),
				Cause:   err,
			}
		}
	}

	// Call gRPC analysis service
	result, err := w.grpc.AnalyzeVideoWithRetry(w.ctx, job, w.pool.config.MaxRetries)
	if err != nil {
		hbCancel()
		progCancel()
		<-hbDone
		<-progDone
		return fmt.Errorf("analysis: %w", err)
	}

	duration := time.Since(startTime)

	// Cancel background goroutines
	hbCancel()
	progCancel()
	<-hbDone
	<-progDone

	// Handle success
	w.handleSuccess(job, result)
	w.metrics.RecordJobProcessed(duration)

	w.logger.Info("job completed",
		"job_id", job.ID,
		"duration", duration,
		"violations", result.Violations,
		"auto_fix", result.AutoFixApplied,
	)
	return nil
}

// validateJob checks that a job has all required fields.
func (w *Worker) validateJob(job *queue.Job) error {
	if job.ID == "" {
		return &queue.PermanentError{Message: "job ID is empty"}
	}
	if job.VideoPath == "" {
		return &queue.PermanentError{Message: "video path is empty"}
	}
	if job.Title == "" {
		return &queue.PermanentError{Message: "title is empty"}
	}
	return nil
}

// handleSuccess acknowledges a successfully completed job.
func (w *Worker) handleSuccess(job *queue.Job, result *grpc.AnalysisResult) {
	analysisResult := &queue.AnalysisResult{
		JobID:          result.JobID,
		Status:         result.Status,
		Violations:     result.Violations,
		AutoFixApplied: result.AutoFixApplied,
		FixDetails:     result.FixDetails,
		ProcessedAt:    result.ProcessedAt,
	}

	if err := w.queue.Ack(w.ctx, job, analysisResult); err != nil {
		w.logger.Error("failed to ack job", "job_id", job.ID, "error", err)
	}
}

// handleFailure handles a failed job with appropriate retry classification.
func (w *Worker) handleFailure(job *queue.Job, jobErr error) {
	w.metrics.RecordJobFailed()

	// Classify error
	var permErr *queue.PermanentError
	isPermanent := false
	if _, ok := jobErr.(*queue.PermanentError); ok {
		isPermanent = true
	}

	// Permanent errors go directly to DLQ without retry
	if isPermanent {
		w.logger.Warn("permanent error, moving to DLQ",
			"job_id", job.ID,
			"error", jobErr.Error(),
		)
		if err := w.queue.MoveToDLQ(w.ctx, job, jobErr.Error()); err != nil {
			w.logger.Error("failed to move job to DLQ", "job_id", job.ID, "error", err)
		}
		return
	}

	// Retryable errors
	if job.RetryCount < w.pool.config.MaxRetries {
		w.metrics.RecordJobRetried()
		w.logger.Warn("retryable error, scheduling retry",
			"job_id", job.ID,
			"retry", job.RetryCount+1,
			"max", w.pool.config.MaxRetries,
			"error", jobErr.Error(),
		)
		if err := w.queue.Nack(w.ctx, job, jobErr); err != nil {
			w.logger.Error("failed to nack job", "job_id", job.ID, "error", err)
		}
		return
	}

	// Max retries exceeded
	w.logger.Error("max retries exceeded, moving to DLQ",
		"job_id", job.ID,
		"retries", job.RetryCount,
		"error", jobErr.Error(),
	)
	if err := w.queue.MoveToDLQ(w.ctx, job,
		fmt.Sprintf("max retries exceeded: %v", jobErr)); err != nil {
		w.logger.Error("failed to move job to DLQ", "job_id", job.ID, "error", err)
	}
}

// heartbeatLoop sends periodic heartbeats for a processing job.
func (w *Worker) heartbeatLoop(ctx context.Context, jobID string, done chan<- struct{}) {
	defer close(done)
	ticker := time.NewTicker(heartbeatInterval)
	defer ticker.Stop()

	// Try to get heartbeat manager from queue
	hbMgr, _ := w.queue.(queue.HeartbeatManager)

	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			if hbMgr != nil {
				if err := hbMgr.TouchHeartbeat(ctx, jobID); err != nil {
					w.logger.Warn("heartbeat failed", "job_id", jobID, "error", err)
				}
			}
		}
	}
}

// progressLoop sends periodic progress updates for a processing job.
func (w *Worker) progressLoop(ctx context.Context, jobID string, done chan<- struct{}) {
	defer close(done)
	ticker := time.NewTicker(progressUpdateInterval)
	defer ticker.Stop()

	progress := 0.0
	messages := []string{
		"downloading video metadata",
		"analyzing content",
		"checking violations",
		"applying auto-fixes",
		"generating report",
	}
	msgIdx := 0

	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			progress += 0.2
			if progress > 0.95 {
				progress = 0.95
			}
			msg := messages[msgIdx]
			if msgIdx < len(messages)-1 {
				msgIdx++
			}
			if err := w.queue.UpdateProgress(ctx, jobID, progress, msg); err != nil {
				w.logger.Warn("progress update failed", "job_id", jobID, "error", err)
			}
		}
	}
}

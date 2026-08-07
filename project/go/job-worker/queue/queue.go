// Package queue provides the JobQueue interface and related types for the YouTube Sentinel job worker.
package queue

import (
	"context"
	"encoding/json"
	"fmt"
	"time"
)

// JobStatus represents the current state of a job.
type JobStatus string

const (
	StatusPending    JobStatus = "pending"
	StatusProcessing JobStatus = "processing"
	StatusCompleted  JobStatus = "completed"
	StatusFailed     JobStatus = "failed"
	StatusRetrying   JobStatus = "retrying"
	StatusStalled    JobStatus = "stalled"
)

// Job represents a video analysis job.
type Job struct {
	ID            string    `json:"id"`
	VideoPath     string    `json:"video_path"`
	Title         string    `json:"title"`
	Description   string    `json:"description"`
	Tags          []string  `json:"tags"`
	EnableAutoFix bool      `json:"enable_auto_fix"`
	Status        JobStatus `json:"status"`
	RetryCount    int       `json:"retry_count"`
	CreatedAt     time.Time `json:"created_at"`
	StartedAt     *time.Time `json:"started_at,omitempty"`
	CompletedAt   *time.Time `json:"completed_at,omitempty"`
	Error         string    `json:"error,omitempty"`
	Result        string    `json:"result,omitempty"`
}

// AnalysisResult represents the result of a video analysis.
type AnalysisResult struct {
	JobID          string    `json:"job_id"`
	Status         string    `json:"status"`
	Violations     int       `json:"violations"`
	AutoFixApplied bool      `json:"auto_fix_applied"`
	FixDetails     string    `json:"fix_details"`
	ProcessedAt    time.Time `json:"processed_at"`
}

// String returns a string representation of the job.
func (j *Job) String() string {
	return fmt.Sprintf("Job{ID:%s Status:%s Video:%s Retries:%d}", j.ID, j.Status, j.VideoPath, j.RetryCount)
}

// ToJSON serializes the job to JSON.
func (j *Job) ToJSON() ([]byte, error) {
	return json.Marshal(j)
}

// FromJSON deserializes a job from JSON.
func JobFromJSON(data []byte) (*Job, error) {
	var j Job
	if err := json.Unmarshal(data, &j); err != nil {
		return nil, fmt.Errorf("unmarshal job: %w", err)
	}
	return &j, nil
}

// Duration returns how long the job has been running.
func (j *Job) Duration() time.Duration {
	if j.StartedAt == nil {
		return 0
	}
	if j.CompletedAt != nil {
		return j.CompletedAt.Sub(*j.StartedAt)
	}
	return time.Since(*j.StartedAt)
}

// JobQueue defines the interface for job queue operations.
type JobQueue interface {
	// Enqueue adds a new job to the pending queue.
	Enqueue(ctx context.Context, job *Job) error

	// Dequeue retrieves and locks the next available job.
	Dequeue(ctx context.Context, queueName string, timeout time.Duration) (*Job, error)

	// Ack acknowledges successful completion of a job.
	Ack(ctx context.Context, job *Job, result *AnalysisResult) error

	// Nack handles job failure with retry or DLQ routing.
	Nack(ctx context.Context, job *Job, jobErr error) error

	// GetJob retrieves a job by its ID.
	GetJob(ctx context.Context, jobID string) (*Job, error)

	// UpdateProgress updates the processing progress of a job.
	UpdateProgress(ctx context.Context, jobID string, progress float64, message string) error

	// ListPending returns all jobs in pending status.
	ListPending(ctx context.Context, queueName string) ([]*Job, error)

	// ListProcessing returns all jobs currently being processed.
	ListProcessing(ctx context.Context) ([]*Job, error)

	// ListFailed returns all jobs in the dead letter queue.
	ListFailed(ctx context.Context, limit int) ([]*Job, error)

	// RequeueStalled finds stalled jobs and re-queues them.
	RequeueStalled(ctx context.Context, stalledTimeout time.Duration) (int, error)

	// MoveToDLQ moves a failed job to the dead letter queue.
	MoveToDLQ(ctx context.Context, job *Job, reason string) error

	// GetDLQ retrieves failed jobs from the dead letter queue.
	GetDLQ(ctx context.Context, limit, offset int) ([]*Job, int64, error)

	// PurgeDLQ removes old entries from the dead letter queue.
	PurgeDLQ(ctx context.Context, maxAge time.Duration) (int64, error)
}

// QueueError represents queue-related errors.
type QueueError struct {
	Op  string
	Err error
}

func (e *QueueError) Error() string {
	return fmt.Sprintf("queue %s: %v", e.Op, e.Err)
}

func (e *QueueError) Unwrap() error {
	return e.Err
}

// PermanentError indicates a non-retryable error.
type PermanentError struct {
	Message string
	Cause   error
}

func (e *PermanentError) Error() string {
	return fmt.Sprintf("permanent error: %s: %v", e.Message, e.Cause)
}

func (e *PermanentError) Unwrap() error {
	return e.Cause
}

// HeartbeatManager allows workers to manage heartbeat keys for processing jobs.
type HeartbeatManager interface {
	TouchHeartbeat(ctx context.Context, jobID string) error
}

// DelayedJobProcessor handles moving delayed jobs back to the pending queue.
type DelayedJobProcessor interface {
	ProcessDelayedJobs(ctx context.Context) (int, error)
}

// IsRetryable determines if an error is retryable.
func IsRetryable(err error) bool {
	if err == nil {
		return false
	}
	var perm *PermanentError
	if _, ok := err.(*PermanentError); ok {
		return false
	}
	if _, ok := err.(interface{ IsRetryable() bool }); ok {
		return true
	}
	// By default, assume retryable
	return true
}

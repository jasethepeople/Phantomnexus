// Package queue provides Redis-backed implementations of the JobQueue interface.
package queue

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/redis/go-redis/v9"
)

const (
	keyPendingQueue    = "youtube_sentinel:queue:pending"
	keyProcessingSet   = "youtube_sentinel:queue:processing"
	keyDLQSet          = "youtube_sentinel:queue:dlq"
	keyJobHash         = "youtube_sentinel:jobs"
	keyProgressStream  = "youtube_sentinel:progress"
	keyHeartbeatPrefix = "youtube_sentinel:heartbeat"
	keyDLQIndex        = "youtube_sentinel:dlq:index"
)

// RedisQueue implements JobQueue using Redis data structures.
type RedisQueue struct {
	client *redis.Client
	logger *slog.Logger
	mu     sync.RWMutex
}

// NewRedisQueue creates a new Redis-backed job queue.
func NewRedisQueue(addr string, logger *slog.Logger) (*RedisQueue, error) {
	client := redis.NewClient(&redis.Options{
		Addr:         addr,
		PoolSize:     20,
		MinIdleConns: 5,
		MaxRetries:   3,
		ReadTimeout:  30 * time.Second,
		WriteTimeout: 30 * time.Second,
		DialTimeout:  5 * time.Second,
	})

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	if err := client.Ping(ctx).Err(); err != nil {
		return nil, fmt.Errorf("redis ping: %w", err)
	}

	if logger == nil {
		logger = slog.Default()
	}

	return &RedisQueue{
		client: client,
		logger: logger,
	}, nil
}

// Close closes the Redis connection.
func (q *RedisQueue) Close() error {
	return q.client.Close()
}

// RedisClient returns the underlying Redis client for direct access.
func (q *RedisQueue) RedisClient() *redis.Client {
	return q.client
}

// ============================================================================
// Core Queue Operations
// ============================================================================

// Enqueue adds a new job to the pending queue.
func (q *RedisQueue) Enqueue(ctx context.Context, job *Job) error {
	if job.ID == "" {
		job.ID = generateJobID()
	}
	if job.CreatedAt.IsZero() {
		job.CreatedAt = time.Now().UTC()
	}
	if job.Status == "" {
		job.Status = StatusPending
	}

	data, err := job.ToJSON()
	if err != nil {
		return &QueueError{Op: "enqueue/marshal", Err: err}
	}

	pipe := q.client.Pipeline()
	pipe.HSet(ctx, keyJobHash, job.ID, data)
	pipe.LPush(ctx, keyPendingQueue, job.ID)

	_, err = pipe.Exec(ctx)
	if err != nil {
		return &QueueError{Op: "enqueue/pipeline", Err: err}
	}

	q.logger.Info("job enqueued", "job_id", job.ID, "video", job.VideoPath)
	return nil
}

// Dequeue retrieves and locks the next available job.
func (q *RedisQueue) Dequeue(ctx context.Context, queueName string, timeout time.Duration) (*Job, error) {
	for {
		result, err := q.client.BRPop(ctx, timeout, keyPendingQueue).Result()
		if err == redis.Nil {
			return nil, nil
		}
		if err != nil {
			return nil, &QueueError{Op: "dequeue/brpop", Err: err}
		}

		if len(result) < 2 {
			continue
		}
		jobID := result[1]

		job, err := q.GetJob(ctx, jobID)
		if err != nil {
			q.logger.Warn("dequeue: failed to get job details", "job_id", jobID, "error", err)
			continue
		}
		if job == nil {
			continue
		}

		now := time.Now().UTC()
		job.Status = StatusProcessing
		job.StartedAt = &now

		data, _ := job.ToJSON()

		pipe := q.client.Pipeline()
		pipe.HSet(ctx, keyJobHash, job.ID, data)
		pipe.ZAdd(ctx, keyProcessingSet, redis.Z{
			Score:  float64(now.Unix()),
			Member: job.ID,
		})
		pipe.Set(ctx, fmt.Sprintf("%s:%s", keyHeartbeatPrefix, job.ID), "active", 2*time.Minute)

		_, err = pipe.Exec(ctx)
		if err != nil {
			return nil, &QueueError{Op: "dequeue/pipeline", Err: err}
		}

		q.logger.Info("job dequeued", "job_id", job.ID, "video", job.VideoPath)
		return job, nil
	}
}

// Ack acknowledges successful completion of a job.
func (q *RedisQueue) Ack(ctx context.Context, job *Job, result *AnalysisResult) error {
	now := time.Now().UTC()
	job.Status = StatusCompleted
	job.CompletedAt = &now

	if result != nil {
		resData, _ := json.Marshal(result)
		job.Result = string(resData)
	}

	data, err := job.ToJSON()
	if err != nil {
		return &QueueError{Op: "ack/marshal", Err: err}
	}

	pipe := q.client.Pipeline()
	pipe.HSet(ctx, keyJobHash, job.ID, data)
	pipe.ZRem(ctx, keyProcessingSet, job.ID)
	pipe.Del(ctx, fmt.Sprintf("%s:%s", keyHeartbeatPrefix, job.ID))

	_, err = pipe.Exec(ctx)
	if err != nil {
		return &QueueError{Op: "ack/pipeline", Err: err}
	}

	q.logger.Info("job acknowledged", "job_id", job.ID, "duration", job.Duration())
	return nil
}

// Nack handles job failure with retry or DLQ routing.
func (q *RedisQueue) Nack(ctx context.Context, job *Job, jobErr error) error {
	if !IsRetryable(jobErr) || job.RetryCount >= 3 {
		return q.MoveToDLQ(ctx, job, jobErr.Error())
	}

	job.RetryCount++
	job.Status = StatusRetrying
	if jobErr != nil {
		job.Error = jobErr.Error()
	}

	data, err := job.ToJSON()
	if err != nil {
		return &QueueError{Op: "nack/marshal", Err: err}
	}

	backoff := exponentialBackoff(job.RetryCount)

	pipe := q.client.Pipeline()
	pipe.HSet(ctx, keyJobHash, job.ID, data)
	pipe.ZRem(ctx, keyProcessingSet, job.ID)
	pipe.Del(ctx, fmt.Sprintf("%s:%s", keyHeartbeatPrefix, job.ID))
	// Use delayed requeue via ZSET with backoff
	pipe.ZAdd(ctx, keyPendingQueue+":delayed", redis.Z{
		Score:  float64(time.Now().Add(backoff).Unix()),
		Member: job.ID,
	})

	_, err = pipe.Exec(ctx)
	if err != nil {
		return &QueueError{Op: "nack/pipeline", Err: err}
	}

	q.logger.Info("job scheduled for retry",
		"job_id", job.ID,
		"retry_count", job.RetryCount,
		"backoff", backoff,
	)
	return nil
}

// GetJob retrieves a job by its ID.
func (q *RedisQueue) GetJob(ctx context.Context, jobID string) (*Job, error) {
	data, err := q.client.HGet(ctx, keyJobHash, jobID).Result()
	if err == redis.Nil {
		return nil, nil
	}
	if err != nil {
		return nil, &QueueError{Op: "getjob", Err: err}
	}
	return JobFromJSON([]byte(data))
}

// UpdateProgress updates the processing progress of a job.
func (q *RedisQueue) UpdateProgress(ctx context.Context, jobID string, progress float64, message string) error {
	event := map[string]interface{}{
		"job_id":    jobID,
		"progress":  progress,
		"message":   message,
		"timestamp": time.Now().UTC().Format(time.RFC3339),
	}
	data, err := json.Marshal(event)
	if err != nil {
		return &QueueError{Op: "progress/marshal", Err: err}
	}

	return q.client.XAdd(ctx, &redis.XAddArgs{
		Stream: keyProgressStream,
		MaxLen: 10000,
		Approx: true,
		Values: map[string]interface{}{
			"data": string(data),
		},
	}).Err()
}

// ============================================================================
// List Operations
// ============================================================================

// ListPending returns all jobs in pending status.
func (q *RedisQueue) ListPending(ctx context.Context, queueName string) ([]*Job, error) {
	jobIDs, err := q.client.LRange(ctx, keyPendingQueue, 0, -1).Result()
	if err != nil {
		return nil, &QueueError{Op: "listpending", Err: err}
	}

	jobs := make([]*Job, 0, len(jobIDs))
	for _, id := range jobIDs {
		job, err := q.GetJob(ctx, id)
		if err != nil || job == nil {
			continue
		}
		jobs = append(jobs, job)
	}
	return jobs, nil
}

// ListProcessing returns all jobs currently being processed.
func (q *RedisQueue) ListProcessing(ctx context.Context) ([]*Job, error) {
	jobIDs, err := q.client.ZRange(ctx, keyProcessingSet, 0, -1)
	if err != nil {
		return nil, &QueueError{Op: "listprocessing", Err: err}
	}

	jobs := make([]*Job, 0, len(jobIDs))
	for _, id := range jobIDs {
		job, err := q.GetJob(ctx, id)
		if err != nil || job == nil {
			continue
		}
		jobs = append(jobs, job)
	}
	return jobs, nil
}

// ListFailed returns all jobs in the dead letter queue.
func (q *RedisQueue) ListFailed(ctx context.Context, limit int) ([]*Job, error) {
	return q.getDLQJobs(ctx, limit, 0)
}

// ============================================================================
// Stalled Job Handling
// ============================================================================

// RequeueStalled finds stalled jobs and re-queues them.
func (q *RedisQueue) RequeueStalled(ctx context.Context, stalledTimeout time.Duration) (int, error) {
	cutoff := float64(time.Now().Add(-stalledTimeout).Unix())

	jobIDs, err := q.client.ZRangeByScore(ctx, keyProcessingSet, &redis.ZRangeBy{
		Min:   "-inf",
		Max:   fmt.Sprintf("%f", cutoff),
		Count: 100,
	}).Result()
	if err != nil {
		return 0, &QueueError{Op: "requeuestalled/zrange", Err: err}
	}

	requeued := 0
	for _, jobID := range jobIDs {
		heartbeatKey := fmt.Sprintf("%s:%s", keyHeartbeatPrefix, jobID)
		exists, _ := q.client.Exists(ctx, heartbeatKey).Result()
		if exists > 0 {
			continue // Worker is still alive
		}

		job, err := q.GetJob(ctx, jobID)
		if err != nil || job == nil {
			q.client.ZRem(ctx, keyProcessingSet, jobID)
			continue
		}

		job.Status = StatusStalled
		data, _ := job.ToJSON()

		pipe := q.client.Pipeline()
		pipe.HSet(ctx, keyJobHash, job.ID, data)
		pipe.ZRem(ctx, keyProcessingSet, jobID)

		if job.RetryCount < 3 {
			job.Status = StatusRetrying
			job.RetryCount++
			data2, _ := job.ToJSON()
			pipe.HSet(ctx, keyJobHash, job.ID, data2)
			pipe.RPush(ctx, keyPendingQueue, job.ID)
			q.logger.Info("stalled job requeued", "job_id", job.ID, "retry", job.RetryCount)
		} else {
			pipe.SAdd(ctx, keyDLQSet, job.ID)
			pipe.HSet(ctx, keyDLQIndex, job.ID, time.Now().UTC().Format(time.RFC3339))
			q.logger.Warn("stalled job moved to DLQ", "job_id", job.ID)
		}

		_, err = pipe.Exec(ctx)
		if err != nil {
			q.logger.Error("failed to requeue stalled job", "job_id", job.ID, "error", err)
			continue
		}
		requeued++
	}

	return requeued, nil
}

// ============================================================================
// Dead Letter Queue
// ============================================================================

// MoveToDLQ moves a failed job to the dead letter queue.
func (q *RedisQueue) MoveToDLQ(ctx context.Context, job *Job, reason string) error {
	now := time.Now().UTC()
	job.Status = StatusFailed
	job.CompletedAt = &now
	job.Error = reason

	data, err := job.ToJSON()
	if err != nil {
		return &QueueError{Op: "movetodlq/marshal", Err: err}
	}

	pipe := q.client.Pipeline()
	pipe.HSet(ctx, keyJobHash, job.ID, data)
	pipe.SAdd(ctx, keyDLQSet, job.ID)
	pipe.HSet(ctx, keyDLQIndex, job.ID, now.Format(time.RFC3339))
	pipe.ZRem(ctx, keyProcessingSet, job.ID)
	pipe.Del(ctx, fmt.Sprintf("%s:%s", keyHeartbeatPrefix, job.ID))

	_, err = pipe.Exec(ctx)
	if err != nil {
		return &QueueError{Op: "movetodlq/pipeline", Err: err}
	}

	q.logger.Error("job moved to DLQ", "job_id", job.ID, "reason", reason)
	return nil
}

// GetDLQ retrieves failed jobs from the dead letter queue with pagination.
func (q *RedisQueue) GetDLQ(ctx context.Context, limit, offset int) ([]*Job, int64, error) {
	total, err := q.client.SCard(ctx, keyDLQSet).Result()
	if err != nil {
		return nil, 0, &QueueError{Op: "getdlq/count", Err: err}
	}

	jobs, err := q.getDLQJobs(ctx, limit, offset)
	if err != nil {
		return nil, 0, err
	}
	return jobs, total, nil
}

// PurgeDLQ removes old entries from the dead letter queue.
func (q *RedisQueue) PurgeDLQ(ctx context.Context, maxAge time.Duration) (int64, error) {
	cutoff := time.Now().Add(-maxAge)
	entries, err := q.client.HGetAll(ctx, keyDLQIndex).Result()
	if err != nil {
		return 0, &QueueError{Op: "purgedlq/hgetall", Err: err}
	}

	var purged int64
	pipe := q.client.Pipeline()
	for jobID, tsStr := range entries {
		ts, err := time.Parse(time.RFC3339, tsStr)
		if err != nil {
			continue
		}
		if ts.Before(cutoff) {
			pipe.SRem(ctx, keyDLQSet, jobID)
			pipe.HDel(ctx, keyDLQIndex, jobID)
			pipe.HDel(ctx, keyJobHash, jobID)
			purged++
		}
	}

	_, err = pipe.Exec(ctx)
	if err != nil {
		return 0, &QueueError{Op: "purgedlq/pipeline", Err: err}
	}

	q.logger.Info("DLQ purged", "purged", purged, "max_age", maxAge)
	return purged, nil
}

// ============================================================================
// Heartbeat
// ============================================================================

// TouchHeartbeat updates the heartbeat TTL for a processing job.
func (q *RedisQueue) TouchHeartbeat(ctx context.Context, jobID string) error {
	key := fmt.Sprintf("%s:%s", keyHeartbeatPrefix, jobID)
	return q.client.Set(ctx, key, "active", 2*time.Minute).Err()
}

// ProcessDelayedJobs moves jobs from the delayed set back to the pending queue
// when their backoff timer expires.
func (q *RedisQueue) ProcessDelayedJobs(ctx context.Context) (int, error) {
	now := float64(time.Now().Unix())
	jobIDs, err := q.client.ZRangeByScore(ctx, keyPendingQueue+":delayed", &redis.ZRangeBy{
		Min:   "0",
		Max:   fmt.Sprintf("%f", now),
		Count: 100,
	}).Result()
	if err != nil {
		return 0, err
	}

	moved := 0
	for _, jobID := range jobIDs {
		pipe := q.client.Pipeline()
		pipe.ZRem(ctx, keyPendingQueue+":delayed", jobID)
		pipe.RPush(ctx, keyPendingQueue, jobID)
		_, err := pipe.Exec(ctx)
		if err != nil {
			q.logger.Warn("failed to move delayed job", "job_id", jobID, "error", err)
			continue
		}
		moved++
	}
	return moved, nil
}

// ============================================================================
// Internal Helpers
// ============================================================================

func (q *RedisQueue) getDLQJobs(ctx context.Context, limit, offset int) ([]*Job, error) {
	jobIDs, err := q.client.SMembers(ctx, keyDLQSet).Result()
	if err != nil {
		return nil, &QueueError{Op: "getdlq/smembers", Err: err}
	}

	if offset >= len(jobIDs) {
		return []*Job{}, nil
	}
	end := offset + limit
	if end > len(jobIDs) || limit <= 0 {
		end = len(jobIDs)
	}
	pageIDs := jobIDs[offset:end]

	jobs := make([]*Job, 0, len(pageIDs))
	for _, id := range pageIDs {
		job, err := q.GetJob(ctx, id)
		if err != nil || job == nil {
			continue
		}
		jobs = append(jobs, job)
	}
	return jobs, nil
}

func generateJobID() string {
	return fmt.Sprintf("job_%d_%d", time.Now().UnixNano(), time.Now().Unix())
}

func exponentialBackoff(retryCount int) time.Duration {
	base := time.Second * 2
	maxDelay := time.Minute * 10
	delay := base * (1 << (retryCount - 1))
	if delay > maxDelay {
		delay = maxDelay
	}
	jitter := time.Duration(float64(delay) * 0.2)
	return delay + jitter
}

// RedisKey helpers for external use.
func PendingQueueKey() string   { return keyPendingQueue }
func ProcessingSetKey() string  { return keyProcessingSet }
func DLQSetKey() string         { return keyDLQSet }
func JobHashKey() string        { return keyJobHash }
func HeartbeatKey(jobID string) string {
	return fmt.Sprintf("%s:%s", keyHeartbeatPrefix, jobID)
}

// Package scheduler provides cron-based cleanup and maintenance tasks.
package scheduler

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/robfig/cron/v3"
	"github.com/youtube-sentinel/job-worker/queue"
)

// CleanupScheduler manages periodic cleanup tasks using cron.
type CleanupScheduler struct {
	queue       queue.JobQueue
	logger      *slog.Logger
	cron        *cron.Cron
	videoDir    string           // Directory where video files are stored
	mu          sync.Mutex
	running     bool
	tasks       map[string]cron.EntryID
}

// CleanupConfig holds configuration for cleanup tasks.
type CleanupConfig struct {
	StalledCheckInterval  time.Duration // How often to check for stalled jobs (default: 2m)
	DLQPurgeInterval      time.Duration // How often to purge old DLQ entries (default: 24h)
	OrphanCleanupInterval time.Duration // How often to clean orphaned files (default: 12h)
	DLQMaxAge             time.Duration // Max age of DLQ entries before purge (default: 7d)
	OrphanMaxAge          time.Duration // Max age of orphaned files (default: 24h)
	VideoDir              string        // Directory to scan for orphaned files
}

// SetDefaults fills in default values.
func (c *CleanupConfig) SetDefaults() {
	if c.StalledCheckInterval <= 0 {
		c.StalledCheckInterval = 2 * time.Minute
	}
	if c.DLQPurgeInterval <= 0 {
		c.DLQPurgeInterval = 24 * time.Hour
	}
	if c.OrphanCleanupInterval <= 0 {
		c.OrphanCleanupInterval = 12 * time.Hour
	}
	if c.DLQMaxAge <= 0 {
		c.DLQMaxAge = 7 * 24 * time.Hour
	}
	if c.OrphanMaxAge <= 0 {
		c.OrphanMaxAge = 24 * time.Hour
	}
	if c.VideoDir == "" {
		c.VideoDir = "/tmp/youtube-sentinel/videos"
	}
}

// NewCleanupScheduler creates a new cleanup scheduler.
func NewCleanupScheduler(q queue.JobQueue, cfg CleanupConfig, logger *slog.Logger) *CleanupScheduler {
	cfg.SetDefaults()
	if logger == nil {
		logger = slog.Default()
	}

	return &CleanupScheduler{
		queue:    q,
		logger:   logger,
		videoDir: cfg.VideoDir,
		cron:     cron.New(cron.WithSeconds()),
		tasks:    make(map[string]cron.EntryID),
	}
}

// Start begins all scheduled cleanup tasks.
func (s *CleanupScheduler) Start() error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.running {
		return fmt.Errorf("scheduler already running")
	}

	// Task 1: Requeue stalled jobs every 2 minutes
	stalledID, err := s.cron.AddFunc("0 */2 * * * *", func() {
		s.requeueStalled()
	})
	if err != nil {
		return fmt.Errorf("schedule stalled check: %w", err)
	}
	s.tasks["stalled"] = stalledID
	s.logger.Info("scheduled stalled job requeue", "interval", "2m")

	// Task 2: Purge DLQ entries older than 7 days every 24 hours
	dlqID, err := s.cron.AddFunc("0 0 3 * * *", func() { // 3 AM daily
		s.purgeDLQ()
	})
	if err != nil {
		return fmt.Errorf("schedule DLQ purge: %w", err)
	}
	s.tasks["dlq"] = dlqID
	s.logger.Info("scheduled DLQ purge", "interval", "24h")

	// Task 3: Orphaned file cleanup every 12 hours
	orphanID, err := s.cron.AddFunc("0 0 */12 * * *", func() {
		cleanupOrphanedFiles(s.videoDir, 24*time.Hour, s.logger)
	})
	if err != nil {
		return fmt.Errorf("schedule orphan cleanup: %w", err)
	}
	s.tasks["orphan"] = orphanID
	s.logger.Info("scheduled orphan cleanup", "interval", "12h")

	s.cron.Start()
	s.running = true

	s.logger.Info("cleanup scheduler started",
		"tasks", len(s.tasks),
		"video_dir", s.videoDir,
	)
	return nil
}

// Stop halts all scheduled tasks.
func (s *CleanupScheduler) Stop() {
	s.mu.Lock()
	defer s.mu.Unlock()

	if !s.running {
		return
	}

	ctx := s.cron.Stop()
	<-ctx.Done()
	s.running = false
	s.logger.Info("cleanup scheduler stopped")
}

// IsRunning returns whether the scheduler is running.
func (s *CleanupScheduler) IsRunning() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.running
}

// ============================================================================
// Task Implementations
// ============================================================================

func (s *CleanupScheduler) requeueStalled() {
	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()

	count, err := s.queue.RequeueStalled(ctx, 5*time.Minute)
	if err != nil {
		s.logger.Error("stalled job requeue failed", "error", err)
		return
	}
	if count > 0 {
		s.logger.Info("requeued stalled jobs", "count", count)
	}
}

func (s *CleanupScheduler) purgeDLQ() {
	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()

	purged, err := s.queue.PurgeDLQ(ctx, 7*24*time.Hour)
	if err != nil {
		s.logger.Error("DLQ purge failed", "error", err)
		return
	}
	if purged > 0 {
		s.logger.Info("purged DLQ entries", "count", purged)
	} else {
		s.logger.Debug("DLQ purge: no entries to purge")
	}
}

// cleanupOrphanedFiles removes video files not referenced by any job.
func cleanupOrphanedFiles(dir string, maxAge time.Duration, logger *slog.Logger) {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
	defer cancel()

	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return
		}
		logger.Error("failed to read video directory", "dir", dir, "error", err)
		return
	}

	cutoff := time.Now().Add(-maxAge)
	removed := 0
	skipped := 0

	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}

		info, err := entry.Info()
		if err != nil {
			continue
		}

		// Skip recently modified files
		if info.ModTime().After(cutoff) {
			skipped++
			continue
		}

		// Check file extension
	ext := strings.ToLower(filepath.Ext(entry.Name()))
		if ext != ".mp4" && ext != ".mkv" && ext != ".avi" && ext != ".mov" && ext != ".webm" {
			skipped++
			continue
		}

		path := filepath.Join(dir, entry.Name())
		select {
		case <-ctx.Done():
			logger.Warn("orphan cleanup timed out", "removed", removed)
			return
		default:
		}

		if err := os.Remove(path); err != nil {
			logger.Warn("failed to remove orphaned file", "path", path, "error", err)
		} else {
			removed++
			logger.Debug("removed orphaned file", "path", path, "age", time.Since(info.ModTime()))
		}
	}

	if removed > 0 {
		logger.Info("orphan cleanup complete", "removed", removed, "skipped", skipped)
	}
}

// TriggerStalledCheck manually triggers a stalled job check.
func (s *CleanupScheduler) TriggerStalledCheck() {
	s.requeueStalled()
}

// TriggerDLQPurge manually triggers a DLQ purge.
func (s *CleanupScheduler) TriggerDLQPurge() {
	s.purgeDLQ()
}

// TriggerOrphanCleanup manually triggers orphaned file cleanup.
func (s *CleanupScheduler) TriggerOrphanCleanup() {
	cleanupOrphanedFiles(s.videoDir, 24*time.Hour, s.logger)
}

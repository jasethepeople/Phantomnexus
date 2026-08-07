// Package metrics provides operational metrics and health endpoints for the job worker.
package metrics

import (
	"encoding/json"
	"fmt"
	"log/slog"
	"net/http"
	"sync/atomic"
	"time"
)

// WorkerMetrics tracks worker performance using atomic counters.
type WorkerMetrics struct {
	jobsProcessed   atomic.Uint64
	jobsFailed      atomic.Uint64
	jobsRetried     atomic.Uint64
	jobsStalled     atomic.Uint64
	currentJobs     atomic.Int64
	totalProcessMs  atomic.Uint64 // sum of processing time in ms for average calculation

	startedAt time.Time
	logger    *slog.Logger
}

// Stats represents a snapshot of worker metrics.
type Stats struct {
	JobsProcessed  uint64  `json:"jobs_processed"`
	JobsFailed     uint64  `json:"jobs_failed"`
	JobsRetried    uint64  `json:"jobs_retried"`
	JobsStalled    uint64  `json:"jobs_stalled"`
	CurrentJobs    int64   `json:"current_jobs"`
	AvgProcessTime float64 `json:"avg_process_time_ms"`
	UptimeSeconds  float64 `json:"uptime_seconds"`
}

// NewWorkerMetrics creates a new metrics tracker.
func NewWorkerMetrics(logger *slog.Logger) *WorkerMetrics {
	if logger == nil {
		logger = slog.Default()
	}
	return &WorkerMetrics{
		startedAt: time.Now().UTC(),
		logger:    logger,
	}
}

// RecordJobProcessed increments the jobs processed counter.
func (m *WorkerMetrics) RecordJobProcessed(duration time.Duration) {
	m.jobsProcessed.Add(1)
	m.currentJobs.Add(-1)
	m.totalProcessMs.Add(uint64(duration.Milliseconds()))
}

// RecordJobFailed increments the jobs failed counter.
func (m *WorkerMetrics) RecordJobFailed() {
	m.jobsFailed.Add(1)
	m.currentJobs.Add(-1)
}

// RecordJobRetried increments the jobs retried counter.
func (m *WorkerMetrics) RecordJobRetried() {
	m.jobsRetried.Add(1)
}

// RecordJobStalled increments the jobs stalled counter.
func (m *WorkerMetrics) RecordJobStalled() {
	m.jobsStalled.Add(1)
}

// RecordJobStarted increments the current jobs counter.
func (m *WorkerMetrics) RecordJobStarted() {
	m.currentJobs.Add(1)
}

// Snapshot returns a point-in-time copy of all metrics.
func (m *WorkerMetrics) Snapshot() Stats {
	processed := m.jobsProcessed.Load()
	var avgMs float64
	if processed > 0 {
		avgMs = float64(m.totalProcessMs.Load()) / float64(processed)
	}
	return Stats{
		JobsProcessed:  processed,
		JobsFailed:     m.jobsFailed.Load(),
		JobsRetried:    m.jobsRetried.Load(),
		JobsStalled:    m.jobsStalled.Load(),
		CurrentJobs:    m.currentJobs.Load(),
		AvgProcessTime: avgMs,
		UptimeSeconds:  time.Since(m.startedAt).Seconds(),
	}
}

// JobsProcessed returns the total number of processed jobs.
func (m *WorkerMetrics) JobsProcessed() uint64 { return m.jobsProcessed.Load() }

// JobsFailed returns the total number of failed jobs.
func (m *WorkerMetrics) JobsFailed() uint64 { return m.jobsFailed.Load() }

// JobsRetried returns the total number of retried jobs.
func (m *WorkerMetrics) JobsRetried() uint64 { return m.jobsRetried.Load() }

// JobsStalled returns the total number of stalled jobs.
func (m *WorkerMetrics) JobsStalled() uint64 { return m.jobsStalled.Load() }

// AvgProcessTime returns the average processing time in milliseconds.
func (m *WorkerMetrics) AvgProcessTime() float64 {
	processed := m.jobsProcessed.Load()
	if processed == 0 {
		return 0
	}
	return float64(m.totalProcessMs.Load()) / float64(processed)
}

// CurrentJobs returns the number of currently processing jobs.
func (m *WorkerMetrics) CurrentJobs() int64 { return m.currentJobs.Load() }

// ============================================================================
// HTTP Server
// ============================================================================

// HTTPServer exposes metrics and health endpoints.
type HTTPServer struct {
	metrics *WorkerMetrics
	addr    string
	server  *http.Server
	logger  *slog.Logger
	ready   atomic.Bool
}

// NewHTTPServer creates a metrics HTTP server.
func NewHTTPServer(addr string, m *WorkerMetrics, logger *slog.Logger) *HTTPServer {
	if logger == nil {
		logger = slog.Default()
	}

	h := &HTTPServer{
		metrics: m,
		addr:    addr,
		logger:  logger,
	}

	mux := http.NewServeMux()
	mux.HandleFunc("/metrics", h.handleMetrics)
	mux.HandleFunc("/health", h.handleHealth)
	mux.HandleFunc("/ready", h.handleReady)

	h.server = &http.Server{
		Addr:         addr,
		Handler:      mux,
		ReadTimeout:  5 * time.Second,
		WriteTimeout: 10 * time.Second,
		IdleTimeout:  120 * time.Second,
	}

	return h
}

// SetReady sets the readiness state.
func (h *HTTPServer) SetReady(ready bool) {
	h.ready.Store(ready)
}

// Start starts the HTTP server in a goroutine.
func (h *HTTPServer) Start() {
	go func() {
		h.logger.Info("metrics server starting", "addr", h.addr)
		if err := h.server.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			h.logger.Error("metrics server failed", "error", err)
		}
	}()
}

// Stop gracefully shuts down the HTTP server.
func (h *HTTPServer) Stop(ctx context.Context) error {
	return h.server.Shutdown(ctx)
}

// handleMetrics returns current metrics as JSON.
func (h *HTTPServer) handleMetrics(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	stats := h.metrics.Snapshot()
	if err := json.NewEncoder(w).Encode(stats); err != nil {
		h.logger.Error("failed to encode metrics", "error", err)
		http.Error(w, "internal error", http.StatusInternalServerError)
	}
}

// handleHealth returns health status.
func (h *HTTPServer) handleHealth(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	resp := map[string]interface{}{
		"status":    "healthy",
		"timestamp": time.Now().UTC().Format(time.RFC3339),
	}
	w.WriteHeader(http.StatusOK)
	_ = json.NewEncoder(w).Encode(resp)
}

// handleReady returns readiness status.
func (h *HTTPServer) handleReady(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	if !h.ready.Load() {
		resp := map[string]interface{}{
			"status":    "not ready",
			"timestamp": time.Now().UTC().Format(time.RFC3339),
		}
		w.WriteHeader(http.StatusServiceUnavailable)
		_ = json.NewEncoder(w).Encode(resp)
		return
	}
	resp := map[string]interface{}{
		"status":    "ready",
		"timestamp": time.Now().UTC().Format(time.RFC3339),
	}
	w.WriteHeader(http.StatusOK)
	_ = json.NewEncoder(w).Encode(resp)
}

// prometheusFormat returns metrics in Prometheus exposition format.
func (h *HTTPServer) prometheusFormat() string {
	s := h.metrics.Snapshot()
	return fmt.Sprintf(`# HELP youtube_sentinel_jobs_processed_total Total jobs processed
# TYPE youtube_sentinel_jobs_processed_total counter
youtube_sentinel_jobs_processed_total %d
# HELP youtube_sentinel_jobs_failed_total Total jobs failed
# TYPE youtube_sentinel_jobs_failed_total counter
youtube_sentinel_jobs_failed_total %d
# HELP youtube_sentinel_jobs_retried_total Total jobs retried
# TYPE youtube_sentinel_jobs_retried_total counter
youtube_sentinel_jobs_retried_total %d
# HELP youtube_sentinel_jobs_stalled_total Total jobs stalled
# TYPE youtube_sentinel_jobs_stalled_total counter
youtube_sentinel_jobs_stalled_total %d
# HELP youtube_sentinel_jobs_current Currently processing jobs
# TYPE youtube_sentinel_jobs_current gauge
youtube_sentinel_jobs_current %d
# HELP youtube_sentinel_avg_process_time_ms Average processing time
# TYPE youtube_sentinel_avg_process_time_ms gauge
youtube_sentinel_avg_process_time_ms %f
`, s.JobsProcessed, s.JobsFailed, s.JobsRetried, s.JobsStalled, s.CurrentJobs, s.AvgProcessTime)
}

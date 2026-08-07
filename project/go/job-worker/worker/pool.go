// Package worker provides the worker pool implementation.
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

// PoolConfig holds configuration for the worker pool.
type PoolConfig struct {
	WorkerCount    int           // Number of concurrent workers (default: 5)
	QueueName      string        // Redis queue name
	GRPCServerAddr string        // gRPC analysis server address
	RedisAddr      string        // Redis server address
	MaxRetries     int           // Max retry attempts per job (default: 3)
	RetryDelay     time.Duration // Delay between retries (default: 5s)
	StalledTimeout time.Duration // Timeout to consider a job stalled (default: 5m)
	MetricsAddr    string        // HTTP metrics server address (default: :8080)
}

// SetDefaults fills in default values for missing configuration.
func (c *PoolConfig) SetDefaults() {
	if c.WorkerCount <= 0 {
		c.WorkerCount = 5
	}
	if c.QueueName == "" {
		c.QueueName = "youtube_sentinel:queue:pending"
	}
	if c.GRPCServerAddr == "" {
		c.GRPCServerAddr = "localhost:50051"
	}
	if c.RedisAddr == "" {
		c.RedisAddr = "localhost:6379"
	}
	if c.MaxRetries <= 0 {
		c.MaxRetries = 3
	}
	if c.RetryDelay <= 0 {
		c.RetryDelay = 5 * time.Second
	}
	if c.StalledTimeout <= 0 {
		c.StalledTimeout = 5 * time.Minute
	}
	if c.MetricsAddr == "" {
		c.MetricsAddr = ":8080"
	}
}

// Pool manages a pool of workers that process jobs from Redis.
type Pool struct {
	config   PoolConfig
	queue    queue.JobQueue
	grpc     *grpc.AnalysisClient
	metrics  *metrics.WorkerMetrics
	http     *metrics.HTTPServer
	logger   *slog.Logger
	workers  []*Worker
	mu       sync.RWMutex
	ctx      context.Context
	cancel   context.CancelFunc
	wg       sync.WaitGroup
	stopCh   chan struct{}
	running  bool
}

// NewPool creates a new worker pool with the given configuration.
func NewPool(config PoolConfig) (*Pool, error) {
	config.SetDefaults()

	logger := slog.New(slog.NewJSONHandler(os.Stderr, &slog.HandlerOptions{
		Level: slog.LevelInfo,
	}))

	// Create Redis queue
	q, err := queue.NewRedisQueue(config.RedisAddr, logger)
	if err != nil {
		return nil, fmt.Errorf("create redis queue: %w", err)
	}

	// Create gRPC analysis client
	grpcClient, err := grpc.NewAnalysisClient(config.GRPCServerAddr, logger)
	if err != nil {
		logger.Warn("grpc client creation failed, will retry", "error", err)
		// Create a nil client that will attempt reconnection
		grpcClient = nil
	}

	m := metrics.NewWorkerMetrics(logger)

	ctx, cancel := context.WithCancel(context.Background())

	pool := &Pool{
		config:  config,
		queue:   q,
		grpc:    grpcClient,
		metrics: m,
		logger:  logger,
		ctx:     ctx,
		cancel:  cancel,
		stopCh:  make(chan struct{}),
	}

	// Create HTTP metrics server
	pool.http = metrics.NewHTTPServer(config.MetricsAddr, m, logger)

	return pool, nil
}

// Start launches the worker pool.
func (p *Pool) Start() error {
	p.mu.Lock()
	defer p.mu.Unlock()

	if p.running {
		return fmt.Errorf("pool already running")
	}

	// Ensure gRPC client is available
	if p.grpc == nil {
		grpcClient, err := grpc.NewAnalysisClient(p.config.GRPCServerAddr, p.logger)
		if err != nil {
			return fmt.Errorf("grpc client: %w", err)
		}
		p.grpc = grpcClient
	}

	// Start metrics HTTP server
	p.http.SetReady(false)
	p.http.Start()

	// Start heartbeat monitor
	p.wg.Add(1)
	go p.heartbeatMonitor()

	// Start delayed job processor
	p.wg.Add(1)
	go p.delayedJobProcessor()

	// Create and start workers
	p.workers = make([]*Worker, p.config.WorkerCount)
	for i := 0; i < p.config.WorkerCount; i++ {
		w := newWorker(i+1, p)
		p.workers[i] = w
		p.wg.Add(1)
		go func(worker *Worker) {
			defer p.wg.Done()
			worker.Run()
		}(w)
	}

	p.running = true
	p.http.SetReady(true)
	p.logger.Info("worker pool started",
		"workers", p.config.WorkerCount,
		"grpc", p.config.GRPCServerAddr,
		"redis", p.config.RedisAddr,
	)

	return nil
}

// Stop gracefully shuts down the worker pool with a timeout.
func (p *Pool) Stop() error {
	return p.StopWithTimeout(30 * time.Second)
}

// StopWithTimeout gracefully shuts down with the specified timeout.
func (p *Pool) StopWithTimeout(timeout time.Duration) error {
	p.mu.Lock()
	if !p.running {
		p.mu.Unlock()
		return nil
	}
	p.running = false
	p.mu.Unlock()

	p.logger.Info("pool shutdown initiated", "timeout", timeout)

	// Signal all workers to stop
	close(p.stopCh)
	p.cancel()

	// Stop each worker
	for _, w := range p.workers {
		w.Stop()
	}

	// Wait for all goroutines with timeout
	done := make(chan struct{})
	go func() {
		p.wg.Wait()
		close(done)
	}()

	select {
	case <-done:
		p.logger.Info("pool shutdown complete")
	case <-time.After(timeout):
		p.logger.Warn("pool shutdown timed out")
	}

	// Shutdown metrics server
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	if err := p.http.Stop(ctx); err != nil {
		p.logger.Warn("metrics server shutdown error", "error", err)
	}

	// Cleanup resources
	if p.grpc != nil {
		if err := p.grpc.Close(); err != nil {
			p.logger.Warn("grpc client close error", "error", err)
		}
	}

	type closer interface{ Close() error }
	if c, ok := p.queue.(closer); ok {
		if err := c.Close(); err != nil {
			p.logger.Warn("queue close error", "error", err)
		}
	}

	return nil
}

// IsRunning returns whether the pool is running.
func (p *Pool) IsRunning() bool {
	p.mu.RLock()
	defer p.mu.RUnlock()
	return p.running
}

// Metrics returns the worker metrics.
func (p *Pool) Metrics() *metrics.WorkerMetrics {
	return p.metrics
}

// Queue returns the job queue.
func (p *Pool) Queue() queue.JobQueue {
	return p.queue
}

// ============================================================================
// Background Monitors
// ============================================================================

// heartbeatMonitor periodically checks for stalled jobs.
func (p *Pool) heartbeatMonitor() {
	defer p.wg.Done()
	ticker := time.NewTicker(30 * time.Second)
	defer ticker.Stop()

	for {
		select {
		case <-p.ctx.Done():
			return
		case <-p.stopCh:
			return
		case <-ticker.C:
			count, err := p.queue.RequeueStalled(p.ctx, p.config.StalledTimeout)
			if err != nil {
				p.logger.Error("stalled job check failed", "error", err)
			} else if count > 0 {
				p.metrics.RecordJobStalled()
				p.logger.Info("requeued stalled jobs", "count", count)
			}
		}
	}
}

// delayedJobProcessor moves delayed jobs back to the pending queue.
func (p *Pool) delayedJobProcessor() {
	defer p.wg.Done()
	ticker := time.NewTicker(30 * time.Second)
	defer ticker.Stop()

	for {
		select {
		case <-p.ctx.Done():
			return
		case <-p.stopCh:
			return
		case <-ticker.C:
			if djp, ok := p.queue.(queue.DelayedJobProcessor); ok {
				count, err := djp.ProcessDelayedJobs(p.ctx)
				if err != nil {
					p.logger.Error("delayed job processing failed", "error", err)
				} else if count > 0 {
					p.logger.Info("processed delayed jobs", "count", count)
				}
			}
		}
	}
}

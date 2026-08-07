// Command job-worker is the background job worker for YouTube Sentinel.
package main

import (
	"flag"
	"fmt"
	"log/slog"
	"os"
	"os/signal"
	"strconv"
	"syscall"
	"time"

	"github.com/youtube-sentinel/job-worker/scheduler"
	"github.com/youtube-sentinel/job-worker/worker"
)

func main() {
	logger := slog.New(slog.NewJSONHandler(os.Stderr, &slog.HandlerOptions{
		Level:     getLogLevel(),
		AddSource: true,
	}))
	slog.SetDefault(logger)

	logger.Info("starting YouTube Sentinel job worker")

	// Parse configuration
	cfg := parseConfig()
	logger.Info("configuration loaded",
		"workers", cfg.WorkerCount,
		"grpc", cfg.GRPCServerAddr,
		"redis", cfg.RedisAddr,
		"retries", cfg.MaxRetries,
		"stalled_timeout", cfg.StalledTimeout,
	)

	// Create and start worker pool
	pool, err := worker.NewPool(cfg)
	if err != nil {
		logger.Error("failed to create worker pool", "error", err)
		os.Exit(1)
	}

	if err := pool.Start(); err != nil {
		logger.Error("failed to start worker pool", "error", err)
		os.Exit(1)
	}

	// Start cleanup scheduler
	cleanupCfg := scheduler.CleanupConfig{
		VideoDir: getEnv("VIDEO_DIR", "/tmp/youtube-sentinel/videos"),
	}
	cleanup := scheduler.NewCleanupScheduler(pool.Queue(), cleanupCfg, logger)
	if err := cleanup.Start(); err != nil {
		logger.Error("failed to start cleanup scheduler", "error", err)
		// Non-fatal: worker can run without cleanup
	}

	logger.Info("all services started successfully",
		"metrics_addr", cfg.MetricsAddr,
	)

	// Wait for shutdown signal
	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)

	sig := <-sigCh
	logger.Info("shutdown signal received", "signal", sig.String())

	// Graceful shutdown with 30 second timeout
	shutdown(pool, cleanup, logger)

	logger.Info("goodbye")
}

// shutdown performs graceful shutdown of all services.
func shutdown(pool *worker.Pool, cleanup *scheduler.CleanupScheduler, logger *slog.Logger) {
	done := make(chan struct{})

	go func() {
		defer close(done)

		// Stop cleanup scheduler
		if cleanup != nil && cleanup.IsRunning() {
			logger.Info("stopping cleanup scheduler")
			cleanup.Stop()
		}

		// Stop worker pool
		logger.Info("stopping worker pool")
		if err := pool.Stop(); err != nil {
			logger.Error("pool stop error", "error", err)
		}
	}()

	select {
	case <-done:
		logger.Info("graceful shutdown complete")
	case <-time.After(30 * time.Second):
		logger.Error("graceful shutdown timed out after 30s")
	}
}

// parseConfig parses configuration from flags and environment variables.
func parseConfig() worker.PoolConfig {
	var cfg worker.PoolConfig

	// Command line flags
	flag.IntVar(&cfg.WorkerCount, "workers",
		getEnvInt("WORKER_COUNT", 5),
		"Number of concurrent workers")
	flag.StringVar(&cfg.QueueName, "queue",
		getEnv("QUEUE_NAME", "youtube_sentinel:queue:pending"),
		"Redis queue name")
	flag.StringVar(&cfg.GRPCServerAddr, "grpc-addr",
		getEnv("GRPC_SERVER_ADDR", "localhost:50051"),
		"gRPC analysis server address")
	flag.StringVar(&cfg.RedisAddr, "redis-addr",
		getEnv("REDIS_ADDR", "localhost:6379"),
		"Redis server address")
	flag.IntVar(&cfg.MaxRetries, "max-retries",
		getEnvInt("MAX_RETRIES", 3),
		"Maximum retry attempts per job")
	flag.DurationVar(&cfg.RetryDelay, "retry-delay",
		getEnvDuration("RETRY_DELAY", 5*time.Second),
		"Delay between retries")
	flag.DurationVar(&cfg.StalledTimeout, "stalled-timeout",
		getEnvDuration("STALLED_TIMEOUT", 5*time.Minute),
		"Timeout to consider a job stalled")
	flag.StringVar(&cfg.MetricsAddr, "metrics-addr",
		getEnv("METRICS_ADDR", ":8080"),
		"HTTP metrics server address")
	flag.Parse()

	return cfg
}

// ============================================================================
// Environment helpers
// ============================================================================

func getEnv(key, defaultVal string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return defaultVal
}

func getEnvInt(key string, defaultVal int) int {
	if v := os.Getenv(key); v != "" {
		if i, err := strconv.Atoi(v); err == nil {
			return i
		}
	}
	return defaultVal
}

func getEnvDuration(key string, defaultVal time.Duration) time.Duration {
	if v := os.Getenv(key); v != "" {
		if d, err := time.ParseDuration(v); err == nil {
			return d
		}
	}
	return defaultVal
}

func getLogLevel() slog.Level {
	lvl := getEnv("LOG_LEVEL", "info")
	switch lvl {
	case "debug":
		return slog.LevelDebug
	case "warn":
		return slog.LevelWarn
	case "error":
		return slog.LevelError
	default:
		return slog.LevelInfo
	}
}

// Package server provides the HTTP server setup and lifecycle management
// for the YouTube Sentinel API service.
package server

import (
	"context"
	"fmt"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/redis/go-redis/v9"

	"github.com/youtube-sentinel/api-service/config"
	"github.com/youtube-sentinel/api-service/handlers"
	"github.com/youtube-sentinel/api-service/middleware"
	ytsgrpc "github.com/youtube-sentinel/api-service/grpc"
)

// Server wraps the gin.Engine along with all external dependencies needed
// to serve the API.
type Server struct {
	// engine is the gin HTTP router.
	engine *gin.Engine

	// cfg holds the application configuration.
	cfg *config.Config

	// grpcClient communicates with the analysis backend.
	grpcClient *ytsgrpc.AnalysisClient

	// redisClient is used for rate limiting and caching.
	redisClient *redis.Client

	// wsHub manages WebSocket connections for progress streaming.
	wsHub *handlers.WebSocketHub

	// httpServer is the underlying HTTP server instance.
	httpServer *http.Server

	// analysisHandler handles analysis-related HTTP endpoints.
	analysisHandler *handlers.AnalysisHandler

	// rateLimiter enforces request rate limits.
	rateLimiter *middleware.RateLimiter
}

// New creates a new Server with all dependencies initialized.
func New(cfg *config.Config) (*Server, error) {
	// Set Gin mode based on log level
	if cfg.LogLevel == "debug" {
		gin.SetMode(gin.DebugMode)
	} else if cfg.LogLevel == "test" {
		gin.SetMode(gin.TestMode)
	} else {
		gin.SetMode(gin.ReleaseMode)
	}

	// Initialize gRPC client with retry
	grpcClient, err := ytsgrpc.NewAnalysisClient(
		cfg.GRPCAddress,
		cfg.GRPCTimeout,
		cfg.GRPCRetryAttempts,
		cfg.GRPCRetryBackoff,
	)
	if err != nil {
		return nil, fmt.Errorf("failed to initialize gRPC client: %w", err)
	}

	// Initialize Redis client
	redisClient := redis.NewClient(&redis.Options{
		Addr:     cfg.RedisAddress,
		Password: cfg.RedisPassword,
		DB:       cfg.RedisDB,
		PoolSize: cfg.RedisPoolSize,
	})

	// Verify Redis connectivity
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	if err := redisClient.Ping(ctx).Err(); err != nil {
		grpcClient.Close()
		return nil, fmt.Errorf("failed to connect to Redis at %s: %w", cfg.RedisAddress, err)
	}

	// Initialize WebSocket hub
	wsHub := handlers.NewWebSocketHub(cfg)

	// Initialize handlers
	analysisHandler := handlers.NewAnalysisHandler(cfg, grpcClient)

	// Create the gin engine
	engine := gin.New()

	// Create rate limiter
	rateLimiter := middleware.NewRateLimiter(redisClient, cfg.RateLimitRequests, cfg.RateLimitWindow)

	s := &Server{
		engine:          engine,
		cfg:             cfg,
		grpcClient:      grpcClient,
		redisClient:     redisClient,
		wsHub:           wsHub,
		analysisHandler: analysisHandler,
		rateLimiter:     rateLimiter,
	}

	// Setup middleware and routes
	s.SetupMiddleware()
	s.SetupRoutes()

	return s, nil
}

// SetupMiddleware configures all global middleware on the gin engine.
func (s *Server) SetupMiddleware() {
	// Recovery middleware (must be first to catch panics)
	s.engine.Use(middleware.RecoveryLogger())

	// Request logger with request ID
	s.engine.Use(middleware.RequestLogger())

	// CORS
	corsConfig := middleware.CORSConfig{
		AllowedOrigins:   s.cfg.CORSAllowedOrigins,
		AllowedMethods:   []string{"GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"},
		AllowedHeaders:   []string{"Origin", "Content-Type", "Accept", "Authorization", "X-Api-Key", "X-Request-ID"},
		ExposedHeaders:   []string{"Content-Length", "Content-Disposition", "X-Request-ID"},
		AllowCredentials: true,
		MaxAge:           86400,
	}
	s.engine.Use(middleware.CORS(corsConfig))

	// API key authentication (if configured)
	s.engine.Use(middleware.APIKeyAuth(s.cfg.APIKey, s.cfg.SkipAuthPaths))

	// Rate limiting
	s.engine.Use(s.rateLimiter.Middleware())
}

// SetupRoutes registers all API routes.
func (s *Server) SetupRoutes() {
	// Health check (no auth required)
	s.engine.GET("/health", s.analysisHandler.HealthCheck)

	// API v1 routes
	v1 := s.engine.Group("/api/v1")
	{
		// Analysis
		v1.POST("/analyze", s.analysisHandler.SubmitAnalysis)

		// Job management
		v1.GET("/jobs/:id", s.analysisHandler.GetJobStatus)
		v1.GET("/jobs/:id/report", s.analysisHandler.GetReport)
		v1.POST("/jobs/:id/fix", s.analysisHandler.ApplyFix)
		v1.GET("/jobs/:id/download", s.analysisHandler.DownloadFixed)

		// WebSocket progress streaming
		v1.GET("/jobs/:id/stream", s.wsHub.StreamProgress(s.cfg, s.grpcClient))
	}
}

// Run starts the HTTP server and blocks until a shutdown signal is received.
// It implements graceful shutdown with connection draining.
func (s *Server) Run(addr string) error {
	s.httpServer = &http.Server{
		Addr:         addr,
		Handler:      s.engine,
		ReadTimeout:  s.cfg.ReadTimeout,
		WriteTimeout: s.cfg.WriteTimeout,
	}

	// Channel to listen for shutdown signals
	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)

	// Channel to capture server errors
	serverErr := make(chan error, 1)

	// Start server in a goroutine
	go func() {
		fmt.Printf("[SERVER] Starting HTTP server on %s\n", addr)
		fmt.Printf("[SERVER] gRPC backend: %s\n", s.cfg.GRPCAddress)
		fmt.Printf("[SERVER] Redis: %s\n", s.cfg.RedisAddress)
		fmt.Printf("[SERVER] Rate limit: %d requests per %s\n", s.cfg.RateLimitRequests, s.cfg.RateLimitWindow)
		if s.cfg.APIKey != "" {
			fmt.Println("[SERVER] API key authentication: enabled")
		} else {
			fmt.Println("[SERVER] API key authentication: disabled")
		}
		fmt.Printf("[SERVER] Max upload size: %d GB\n", s.cfg.UploadMaxSize>>30)

		if err := s.httpServer.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			serverErr <- fmt.Errorf("server error: %w", err)
		}
	}()

	// Wait for shutdown signal or server error
	select {
	case err := <-serverErr:
		return err
	case sig := <-quit:
		fmt.Printf("\n[SERVER] Received signal %s, initiating graceful shutdown...\n", sig)
	}

	// Graceful shutdown
	return s.Shutdown()
}

// Shutdown gracefully shuts down the server, draining active connections.
func (s *Server) Shutdown() error {
	// Create a timeout context for shutdown
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	// Shutdown HTTP server
	if err := s.httpServer.Shutdown(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "[SERVER] HTTP shutdown error: %v\n", err)
	}

	// Close WebSocket hub
	s.wsHub.Close()

	// Close gRPC client
	if err := s.grpcClient.Close(); err != nil {
		fmt.Fprintf(os.Stderr, "[SERVER] gRPC client close error: %v\n", err)
	}

	// Close Redis client
	if err := s.redisClient.Close(); err != nil {
		fmt.Fprintf(os.Stderr, "[SERVER] Redis client close error: %v\n", err)
	}

	// Close rate limiter
	if err := s.rateLimiter.Close(); err != nil {
		fmt.Fprintf(os.Stderr, "[SERVER] Rate limiter close error: %v\n", err)
	}

	fmt.Println("[SERVER] Graceful shutdown complete")
	return nil
}

// Engine returns the underlying gin engine (useful for testing).
func (s *Server) Engine() *gin.Engine {
	return s.engine
}

// GRPCClient returns the gRPC client (useful for testing).
func (s *Server) GRPCClient() *ytsgrpc.AnalysisClient {
	return s.grpcClient
}

// RedisClient returns the Redis client (useful for testing).
func (s *Server) RedisClient() *redis.Client {
	return s.redisClient
}

// WSHub returns the WebSocket hub (useful for testing).
func (s *Server) WSHub() *handlers.WebSocketHub {
	return s.wsHub
}

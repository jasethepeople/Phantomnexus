// YouTube Sentinel API Service
//
// This is the main entry point for the HTTP REST API with WebSocket
// streaming. It loads configuration, initializes all dependencies
// (gRPC client, Redis, WebSocket hub), sets up routes, and starts
// the server with graceful shutdown handling.
package main

import (
	"fmt"
	"os"

	"github.com/youtube-sentinel/api-service/config"
	"github.com/youtube-sentinel/api-service/server"
)

// version is set at build time via ldflags.
var version = "dev"

// buildTime is set at build time via ldflags.
var buildTime = "unknown"

func main() {
	fmt.Printf("=== YouTube Sentinel API Service v%s (built %s) ===\n\n", version, buildTime)

	if err := run(); err != nil {
		fmt.Fprintf(os.Stderr, "[FATAL] %v\n", err)
		os.Exit(1)
	}
}

func run() error {
	// -------------------------------------------------------------------
	// 1. Load configuration from environment / .env file
	// -------------------------------------------------------------------
	cfg, err := config.Load()
	if err != nil {
		return fmt.Errorf("failed to load configuration: %w", err)
	}

	fmt.Printf("[MAIN] Configuration loaded successfully\n")
	fmt.Printf("[MAIN] Server will listen on %s\n", cfg.Addr())
	fmt.Printf("[MAIN] gRPC backend: %s\n", cfg.GRPCAddress)
	fmt.Printf("[MAIN] Redis: %s (DB: %d)\n", cfg.RedisAddress, cfg.RedisDB)
	fmt.Println()

	// -------------------------------------------------------------------
	// 2. Create server with all dependencies
	// -------------------------------------------------------------------
	srv, err := server.New(cfg)
	if err != nil {
		return fmt.Errorf("failed to create server: %w", err)
	}

	// -------------------------------------------------------------------
	// 3. Start server (blocks until shutdown signal)
	// -------------------------------------------------------------------
	return srv.Run(cfg.Addr())
}

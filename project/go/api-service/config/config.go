// Package config provides centralized configuration management for the API service.
// It loads configuration from environment variables with sensible defaults and
// validates required values at startup.
package config

import (
	"fmt"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/joho/godotenv"
)

// Config holds all application configuration loaded from environment variables.
type Config struct {
	// Server settings
	ServerPort         string        `json:"server_port"`
	ServerHost         string        `json:"server_host"`
	ReadTimeout        time.Duration `json:"read_timeout"`
	WriteTimeout       time.Duration `json:"write_timeout"`
	MaxMultipartMemory int64         `json:"max_multipart_memory"`

	// gRPC client settings
	GRPCAddress          string        `json:"grpc_address"`
	GRPCTimeout          time.Duration `json:"grpc_timeout"`
	GRPCRetryAttempts    int           `json:"grpc_retry_attempts"`
	GRPCRetryBackoff     time.Duration `json:"grpc_retry_backoff"`
	GRPCHealthCheckInterval time.Duration `json:"grpc_health_check_interval"`

	// Redis settings
	RedisAddress   string `json:"redis_address"`
	RedisPassword  string `json:"redis_password"`
	RedisDB        int    `json:"redis_db"`
	RedisPoolSize  int    `json:"redis_pool_size"`

	// Authentication
	APIKey        string   `json:"api_key"`
	SkipAuthPaths []string `json:"skip_auth_paths"`

	// CORS settings
	CORSAllowedOrigins []string `json:"cors_allowed_origins"`

	// Rate limiting
	RateLimitRequests  int           `json:"rate_limit_requests"`
	RateLimitWindow    time.Duration `json:"rate_limit_window"`

	// Upload settings
	UploadMaxSize     int64    `json:"upload_max_size"`
	AllowedVideoTypes []string `json:"allowed_video_types"`
	TempDir           string   `json:"temp_dir"`

	// Logging
	LogLevel string `json:"log_level"`

	// WebSocket settings
	WebSocketPingInterval time.Duration `json:"websocket_ping_interval"`
	WebSocketWriteTimeout time.Duration `json:"websocket_write_timeout"`
}

// DefaultConfig returns a Config populated with sensible defaults.
func DefaultConfig() *Config {
	return &Config{
		ServerPort:              "8080",
		ServerHost:              "0.0.0.0",
		ReadTimeout:             30 * time.Second,
		WriteTimeout:            30 * time.Second,
		MaxMultipartMemory:      32 << 20, // 32 MiB
		GRPCAddress:             "localhost:50051",
		GRPCTimeout:             10 * time.Second,
		GRPCRetryAttempts:       3,
		GRPCRetryBackoff:        500 * time.Millisecond,
		GRPCHealthCheckInterval: 30 * time.Second,
		RedisAddress:            "localhost:6379",
		RedisPassword:           "",
		RedisDB:                 0,
		RedisPoolSize:           10,
		APIKey:                  "",
		SkipAuthPaths:           []string{"/health", "/api/v1/jobs/"},
		CORSAllowedOrigins:      []string{"*"},
		RateLimitRequests:       60,
		RateLimitWindow:         1 * time.Minute,
		UploadMaxSize:           10 << 30, // 10 GiB
		AllowedVideoTypes:       []string{"video/mp4", "video/quicktime", "video/x-msvideo", "video/x-matroska", "video/webm"},
		TempDir:                 os.TempDir(),
		LogLevel:                "info",
		WebSocketPingInterval:   30 * time.Second,
		WebSocketWriteTimeout:   10 * time.Second,
	}
}

// Load reads the .env file and environment variables, returning a fully
// populated Config. It applies validation to ensure required fields are set.
func Load() (*Config, error) {
	// Attempt to load .env file; it's okay if it doesn't exist
	_ = godotenv.Load()

	cfg := DefaultConfig()

	// Server settings
	if v := os.Getenv("SERVER_PORT"); v != "" {
		cfg.ServerPort = v
	}
	if v := os.Getenv("SERVER_HOST"); v != "" {
		cfg.ServerHost = v
	}
	if v := os.Getenv("READ_TIMEOUT"); v != "" {
		if d, err := time.ParseDuration(v); err == nil {
			cfg.ReadTimeout = d
		}
	}
	if v := os.Getenv("WRITE_TIMEOUT"); v != "" {
		if d, err := time.ParseDuration(v); err == nil {
			cfg.WriteTimeout = d
		}
	}
	if v := os.Getenv("MAX_MULTIPART_MEMORY"); v != "" {
		if n, err := strconv.ParseInt(v, 10, 64); err == nil {
			cfg.MaxMultipartMemory = n
		}
	}

	// gRPC settings
	if v := os.Getenv("GRPC_ADDRESS"); v != "" {
		cfg.GRPCAddress = v
	}
	if v := os.Getenv("GRPC_TIMEOUT"); v != "" {
		if d, err := time.ParseDuration(v); err == nil {
			cfg.GRPCTimeout = d
		}
	}
	if v := os.Getenv("GRPC_RETRY_ATTEMPTS"); v != "" {
		if n, err := strconv.Atoi(v); err == nil {
			cfg.GRPCRetryAttempts = n
		}
	}
	if v := os.Getenv("GRPC_RETRY_BACKOFF"); v != "" {
		if d, err := time.ParseDuration(v); err == nil {
			cfg.GRPCRetryBackoff = d
		}
	}

	// Redis settings
	if v := os.Getenv("REDIS_ADDRESS"); v != "" {
		cfg.RedisAddress = v
	}
	if v := os.Getenv("REDIS_PASSWORD"); v != "" {
		cfg.RedisPassword = v
	}
	if v := os.Getenv("REDIS_DB"); v != "" {
		if n, err := strconv.Atoi(v); err == nil {
			cfg.RedisDB = n
		}
	}
	if v := os.Getenv("REDIS_POOL_SIZE"); v != "" {
		if n, err := strconv.Atoi(v); err == nil {
			cfg.RedisPoolSize = n
		}
	}

	// Authentication
	if v := os.Getenv("API_KEY"); v != "" {
		cfg.APIKey = v
	}
	if v := os.Getenv("SKIP_AUTH_PATHS"); v != "" {
		cfg.SkipAuthPaths = strings.Split(v, ",")
	}

	// CORS settings
	if v := os.Getenv("CORS_ALLOWED_ORIGINS"); v != "" {
		cfg.CORSAllowedOrigins = strings.Split(v, ",")
	}

	// Rate limiting
	if v := os.Getenv("RATE_LIMIT_REQUESTS"); v != "" {
		if n, err := strconv.Atoi(v); err == nil {
			cfg.RateLimitRequests = n
		}
	}
	if v := os.Getenv("RATE_LIMIT_WINDOW"); v != "" {
		if d, err := time.ParseDuration(v); err == nil {
			cfg.RateLimitWindow = d
		}
	}

	// Upload settings
	if v := os.Getenv("UPLOAD_MAX_SIZE"); v != "" {
		if n, err := strconv.ParseInt(v, 10, 64); err == nil {
			cfg.UploadMaxSize = n
		}
	}
	if v := os.Getenv("ALLOWED_VIDEO_TYPES"); v != "" {
		cfg.AllowedVideoTypes = strings.Split(v, ",")
	}
	if v := os.Getenv("TEMP_DIR"); v != "" {
		cfg.TempDir = v
	}

	// Logging
	if v := os.Getenv("LOG_LEVEL"); v != "" {
		cfg.LogLevel = v
	}

	// WebSocket settings
	if v := os.Getenv("WEBSOCKET_PING_INTERVAL"); v != "" {
		if d, err := time.ParseDuration(v); err == nil {
			cfg.WebSocketPingInterval = d
		}
	}
	if v := os.Getenv("WEBSOCKET_WRITE_TIMEOUT"); v != "" {
		if d, err := time.ParseDuration(v); err == nil {
			cfg.WebSocketWriteTimeout = d
		}
	}

	// Validate required fields
	if err := cfg.Validate(); err != nil {
		return nil, err
	}

	return cfg, nil
}

// Validate checks that all required configuration fields are properly set.
func (c *Config) Validate() error {
	if c.GRPCAddress == "" {
		return fmt.Errorf("grpc_address is required")
	}
	if c.RedisAddress == "" {
		return fmt.Errorf("redis_address is required")
	}
	if c.ServerPort == "" {
		return fmt.Errorf("server_port is required")
	}
	if c.RateLimitRequests <= 0 {
		return fmt.Errorf("rate_limit_requests must be positive")
	}
	if c.RateLimitWindow <= 0 {
		return fmt.Errorf("rate_limit_window must be positive")
	}
	if c.UploadMaxSize <= 0 {
		return fmt.Errorf("upload_max_size must be positive")
	}
	if len(c.AllowedVideoTypes) == 0 {
		return fmt.Errorf("at least one allowed_video_type is required")
	}
	return nil
}

// Addr returns the full server address (host:port).
func (c *Config) Addr() string {
	return fmt.Sprintf("%s:%s", c.ServerHost, c.ServerPort)
}

// ShouldSkipAuth returns true if the given request path should bypass
// API key authentication.
func (c *Config) ShouldSkipAuth(path string) bool {
	for _, p := range c.SkipAuthPaths {
		if strings.HasPrefix(path, p) {
			return true
		}
	}
	return false
}

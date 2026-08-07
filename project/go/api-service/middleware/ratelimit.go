package middleware

import (
	"fmt"
	"net/http"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/google/uuid"
	"github.com/redis/go-redis/v9"
	"github.com/youtube-sentinel/api-service/models"
)

// RateLimiter provides Redis-backed sliding-window rate limiting.
type RateLimiter struct {
	// Redis client used to store sliding-window counters.
	client *redis.Client

	// MaxRequests is the maximum number of requests allowed within the window.
	maxRequests int

	// Window is the duration of the sliding window.
	window time.Duration

	// KeyPrefix is prepended to all Redis keys used by this limiter.
	keyPrefix string
}

// NewRateLimiter creates a rate limiter using Redis for distributed state.
func NewRateLimiter(client *redis.Client, maxRequests int, window time.Duration) *RateLimiter {
	return &RateLimiter{
		client:      client,
		maxRequests: maxRequests,
		window:      window,
		keyPrefix:   "ratelimit:",
	}
}

// key generates a Redis key for the given client identifier.
func (rl *RateLimiter) key(clientID string) string {
	return rl.keyPrefix + clientID
}

// isAllowed checks whether the clientID has remaining quota in the current
// sliding window. It uses Redis sorted sets to track request timestamps.
func (rl *RateLimiter) isAllowed(c *gin.Context, clientID string) (bool, map[string]string, error) {
	ctx := c.Request.Context()
	key := rl.key(clientID)
	now := time.Now().UTC()
	windowStart := now.Add(-rl.window).UnixMilli()
	current := now.UnixMilli()

	pipe := rl.client.Pipeline()

	// Remove timestamps older than the window
	pipe.ZRemRangeByScore(ctx, key, "0", strconv.FormatInt(windowStart, 10))

	// Count remaining entries in the window
	countCmd := pipe.ZCard(ctx, key)

	// Add the current request timestamp
	pipe.ZAdd(ctx, key, redis.Z{Score: float64(current), Member: current})

	// Set expiry on the key
	pipe.Expire(ctx, key, rl.window)

	_, err := pipe.Exec(ctx)
	if err != nil {
		return false, nil, fmt.Errorf("redis pipeline failed: %w", err)
	}

	count := countCmd.Val()
	if count >= int64(rl.maxRequests) {
		retryAfter := rl.window - time.Since(time.UnixMilli(windowStart))
		headers := map[string]string{
			"X-RateLimit-Limit":     strconv.Itoa(rl.maxRequests),
			"X-RateLimit-Remaining": "0",
			"X-RateLimit-Reset":     strconv.FormatInt(now.Add(retryAfter).Unix(), 10),
			"Retry-After":           strconv.Itoa(int(retryAfter.Seconds())),
		}
		return false, headers, nil
	}

	remaining := rl.maxRequests - int(count) - 1
	if remaining < 0 {
		remaining = 0
	}

	headers := map[string]string{
		"X-RateLimit-Limit":     strconv.Itoa(rl.maxRequests),
		"X-RateLimit-Remaining": strconv.Itoa(remaining),
		"X-RateLimit-Window":    rl.window.String(),
	}
	return true, headers, nil
}

// Middleware returns a gin handler that enforces rate limiting per client.
// Client identification uses, in order of preference:
//  1. X-Client-ID header
//  2. X-Forwarded-For header
//  3. Remote address
func (rl *RateLimiter) Middleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		// Determine client identifier
		clientID := c.GetHeader("X-Client-ID")
		if clientID == "" {
			clientID = c.GetHeader("X-Forwarded-For")
		}
		if clientID == "" {
			clientID = c.ClientIP()
		}
		if clientID == "" {
			clientID = uuid.New().String()
		}

		allowed, headers, err := rl.isAllowed(c, clientID)
		if err != nil {
			// On Redis error, allow the request but log the failure
			fmt.Fprintf(c.Writer, "rate limiter error: %v", err)
			c.Next()
			return
		}

		// Write rate limit headers
		for k, v := range headers {
			c.Header(k, v)
		}

		if !allowed {
			c.AbortWithStatusJSON(http.StatusTooManyRequests, models.ErrorResponse{
				Code:      http.StatusTooManyRequests,
				Message:   "Rate limit exceeded. Please try again later.",
				RequestID: c.GetString("request_id"),
				Timestamp: time.Now().UTC(),
				Details:   fmt.Sprintf("Limit: %d requests per %s", rl.maxRequests, rl.window),
			})
			return
		}

		c.Next()
	}
}

// Close gracefully shuts down the rate limiter's Redis connection.
func (rl *RateLimiter) Close() error {
	return rl.client.Close()
}

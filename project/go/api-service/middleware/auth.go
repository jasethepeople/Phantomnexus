package middleware

import (
	"crypto/subtle"
	"net/http"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/youtube-sentinel/api-service/models"
)

// APIKeyAuth returns a middleware that validates the X-Api-Key header using
// constant-time comparison to prevent timing attacks.
// Requests to paths matching the skip list bypass this check.
func APIKeyAuth(apiKey string, skipPaths []string) gin.HandlerFunc {
	return func(c *gin.Context) {
		// Bypass auth for paths that don't require it
		for _, p := range skipPaths {
			if strings.HasPrefix(c.Request.URL.Path, p) {
				c.Next()
				return
			}
		}

		// If no API key is configured, allow all requests
		if apiKey == "" {
			c.Next()
			return
		}

		// Extract key from header
		headerKey := c.GetHeader("X-Api-Key")
		if headerKey == "" {
			// Also try Authorization: Bearer <key>
			authHeader := c.GetHeader("Authorization")
			const prefix = "Bearer "
			if strings.HasPrefix(authHeader, prefix) {
				headerKey = strings.TrimSpace(authHeader[len(prefix):])
			}
		}

		if headerKey == "" {
			c.AbortWithStatusJSON(http.StatusUnauthorized, models.ErrorResponse{
				Code:      http.StatusUnauthorized,
				Message:   "API key is required",
				RequestID: c.GetString("request_id"),
				Timestamp: time.Now().UTC(),
			})
			return
		}

		// Constant-time comparison to prevent timing attacks
		if subtle.ConstantTimeCompare([]byte(headerKey), []byte(apiKey)) != 1 {
			c.AbortWithStatusJSON(http.StatusUnauthorized, models.ErrorResponse{
				Code:      http.StatusUnauthorized,
				Message:   "Invalid API key",
				RequestID: c.GetString("request_id"),
				Timestamp: time.Now().UTC(),
			})
			return
		}

		c.Next()
	}
}

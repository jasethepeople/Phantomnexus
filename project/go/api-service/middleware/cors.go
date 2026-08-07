// Package middleware provides reusable HTTP middleware for the API service.
package middleware

import (
	"net/http"
	"strconv"
	"strings"

	"github.com/gin-gonic/gin"
)

// CORSConfig holds the configuration for the CORS middleware.
type CORSConfig struct {
	// AllowedOrigins is a list of origins that are permitted to make
	// cross-origin requests. Use ["*"] to allow any origin.
	AllowedOrigins []string

	// AllowedMethods specifies the HTTP methods allowed for CORS.
	AllowedMethods []string

	// AllowedHeaders specifies which headers can be used in requests.
	AllowedHeaders []string

	// ExposedHeaders lists headers that browsers are allowed to access.
	ExposedHeaders []string

	// AllowCredentials indicates whether the request can include credentials.
	AllowCredentials bool

	// MaxAge indicates how long (in seconds) browsers can cache
	// preflight response.
	MaxAge int
}

// DefaultCORSConfig returns a CORSConfig with sensible defaults.
func DefaultCORSConfig() CORSConfig {
	return CORSConfig{
		AllowedOrigins:   []string{"*"},
		AllowedMethods:   []string{"GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"},
		AllowedHeaders:   []string{"Origin", "Content-Type", "Accept", "Authorization", "X-Api-Key", "X-Request-ID"},
		ExposedHeaders:   []string{"Content-Length", "Content-Disposition", "X-Request-ID"},
		AllowCredentials: true,
		MaxAge:           86400,
	}
}

// CORS returns a gin middleware that handles Cross-Origin Resource Sharing.
// It processes preflight OPTIONS requests and adds appropriate headers to
// all responses.
func CORS(config CORSConfig) gin.HandlerFunc {
	allowedOrigins := make(map[string]bool, len(config.AllowedOrigins))
	allowAll := false
	for _, o := range config.AllowedOrigins {
		o = strings.TrimSpace(o)
		if o == "*" {
			allowAll = true
			break
		}
		allowedOrigins[o] = true
	}

	methods := strings.Join(config.AllowedMethods, ", ")
	headers := strings.Join(config.AllowedHeaders, ", ")
	exposed := strings.Join(config.ExposedHeaders, ", ")

	return func(c *gin.Context) {
		origin := c.Request.Header.Get("Origin")

		// Determine whether the origin is allowed
		if allowAll {
			c.Header("Access-Control-Allow-Origin", "*")
		} else if origin != "" && allowedOrigins[origin] {
			c.Header("Access-Control-Allow-Origin", origin)
		}

		if config.AllowCredentials {
			c.Header("Access-Control-Allow-Credentials", "true")
		}

		if exposed != "" {
			c.Header("Access-Control-Expose-Headers", exposed)
		}

		// Handle preflight (OPTIONS) request
		if c.Request.Method == http.MethodOptions {
			c.Header("Access-Control-Allow-Methods", methods)
			if headers != "" {
				c.Header("Access-Control-Allow-Headers", headers)
			}
			if config.MaxAge > 0 {
				c.Header("Access-Control-Max-Age", strconv.Itoa(config.MaxAge))
			}
			c.AbortWithStatus(http.StatusNoContent)
			return
		}

		c.Next()
	}
}

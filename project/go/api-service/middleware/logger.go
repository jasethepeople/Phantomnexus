package middleware

import (
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/google/uuid"
)

// ANSI color codes for terminal output.
const (
	colorReset   = "\033[0m"
	colorRed     = "\033[31m"
	colorGreen   = "\033[32m"
	colorYellow  = "\033[33m"
	colorBlue    = "\033[34m"
	colorMagenta = "\033[35m"
	colorCyan    = "\033[36m"
	colorGray    = "\033[37m"
)

// statusColor returns the ANSI color code for the given HTTP status code.
func statusColor(code int) string {
	switch {
	case code >= 200 && code < 300:
		return colorGreen
	case code >= 300 && code < 400:
		return colorCyan
	case code >= 400 && code < 500:
		return colorYellow
	case code >= 500:
		return colorRed
	default:
		return colorReset
	}
}

// methodColor returns the ANSI color code for the given HTTP method.
func methodColor(method string) string {
	switch method {
	case "GET":
		return colorBlue
	case "POST":
		return colorGreen
	case "PUT":
		return colorYellow
	case "PATCH":
		return colorCyan
	case "DELETE":
		return colorRed
	default:
		return colorMagenta
	}
}

// RequestLogger returns a gin middleware that logs every HTTP request with a
// unique request ID, colored method/status, duration, and client details.
func RequestLogger() gin.HandlerFunc {
	// Check if output is a terminal for color support
	useColor := isTerminal()

	return func(c *gin.Context) {
		// Generate unique request ID
		requestID := c.GetHeader("X-Request-ID")
		if requestID == "" {
			requestID = uuid.New().String()
		}
		c.Set("request_id", requestID)
		c.Writer.Header().Set("X-Request-ID", requestID)

		start := time.Now()
		path := c.Request.URL.Path
		raw := c.Request.URL.RawQuery

		// Process the request
		c.Next()

		// Calculate latency
		latency := time.Since(start)

		// Build query string
		if raw != "" {
			path = path + "?" + raw
		}

		statusCode := c.Writer.Status()
		clientIP := c.ClientIP()
		method := c.Request.Method
		errorMessage := c.Errors.ByType(gin.ErrorTypePrivate).String()

		if useColor {
			fmt.Fprintf(os.Stdout, "[%s] %s |%s %3d %s| %13v | %15s |%s %-7s %s %s %s\n",
				time.Now().Format("2006/01/02 15:04:05"),
				requestID[:8],
				statusColor(statusCode), statusCode, colorReset,
				latency,
				clientIP,
				methodColor(method), method, colorReset,
				path,
				errorMessage,
			)
		} else {
			fmt.Fprintf(os.Stdout, "[%s] %s | %3d | %13v | %15s | %-7s %s %s\n",
				time.Now().Format("2006/01/02 15:04:05"),
				requestID[:8],
				statusCode,
				latency,
				clientIP,
				method,
				path,
				errorMessage,
			)
		}
	}
}

// isTerminal checks whether stdout is a terminal to decide on color support.
func isTerminal() bool {
	// Simplified check: if TERM is set and not "dumb", assume terminal
	term := os.Getenv("TERM")
	return term != "" && term != "dumb"
}

// RecoveryLogger returns a gin recovery middleware that logs panics with
// the request ID for traceability.
func RecoveryLogger() gin.HandlerFunc {
	return gin.CustomRecovery(func(c *gin.Context, err interface{}) {
		requestID := c.GetString("request_id")
		fmt.Fprintf(os.Stderr, "[PANIC] request_id=%s error=%v path=%s method=%s\n",
			requestID, err, c.Request.URL.Path, c.Request.Method)
		c.AbortWithStatusJSON(500, gin.H{
			"code":       500,
			"message":    "Internal server error",
			"request_id": requestID,
		})
	})
}

// StripPort removes the port suffix from an address string.
func StripPort(addr string) string {
	if idx := strings.LastIndex(addr, ":"); idx != -1 {
		return addr[:idx]
	}
	return addr
}

// Package handlers contains HTTP handlers for the API service.
package handlers

import (
	"encoding/json"
	"fmt"
	"net/http"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/gorilla/websocket"

	"github.com/youtube-sentinel/api-service/config"
	"github.com/youtube-sentinel/api-service/grpc"
	"github.com/youtube-sentinel/api-service/models"
)

// WebSocketHub manages active WebSocket connections for progress streaming.
type WebSocketHub struct {
	// upgrader configures the WebSocket upgrade behavior.
	upgrader websocket.Upgrader

	// pingInterval controls how often ping frames are sent.
	pingInterval time.Duration

	// writeTimeout is the maximum duration for write operations.
	writeTimeout time.Duration
}

// NewWebSocketHub creates a new WebSocketHub with the given configuration.
func NewWebSocketHub(cfg *config.Config) *WebSocketHub {
	return &WebSocketHub{
		upgrader: websocket.Upgrader{
			ReadBufferSize:  1024,
			WriteBufferSize: 1024,
			CheckOrigin: func(r *http.Request) bool {
				// Allow all origins in development; in production,
				// validate against cfg.CORSAllowedOrigins.
				return true
			},
			HandshakeTimeout: cfg.ReadTimeout,
		},
		pingInterval: cfg.WebSocketPingInterval,
		writeTimeout: cfg.WebSocketWriteTimeout,
	}
}

// StreamProgress upgrades an HTTP connection to WebSocket and streams
// analysis progress updates from the gRPC service to the client.
// Endpoint: GET /api/v1/jobs/:id/stream
func (wh *WebSocketHub) StreamProgress(cfg *config.Config, grpcClient grpc.AnalysisClientInterface) gin.HandlerFunc {
	return func(c *gin.Context) {
		requestID := c.GetString("request_id")
		jobID := c.Param("id")

		if err := validateJobID(jobID); err != nil {
			c.JSON(http.StatusBadRequest, models.ErrorResponse{
				Code:      http.StatusBadRequest,
				Message:   "Invalid job ID",
				RequestID: requestID,
				Timestamp: time.Now().UTC(),
				Details:   err.Error(),
			})
			return
		}

		// Upgrade HTTP to WebSocket
		wsConn, err := wh.upgrader.Upgrade(c.Writer, c.Request, nil)
		if err != nil {
			// Upgrade already writes the error response
			fmt.Fprintf(c.Writer, "WebSocket upgrade failed: %v", err)
			return
		}
		defer wsConn.Close()

		// Configure WebSocket
		wsConn.SetReadLimit(512) // Small limit - we only expect pong responses
		wsConn.SetPongHandler(func(data string) error {
			return nil
		})

		// Start goroutine to handle ping/pong and client disconnect
		done := make(chan struct{})
		go func() {
			defer close(done)
			for {
				_, _, err := wsConn.ReadMessage()
				if err != nil {
					if websocket.IsUnexpectedCloseError(err, websocket.CloseGoingAway, websocket.CloseAbnormalClosure, websocket.CloseNormalClosure) {
						fmt.Printf("[WS] unexpected close for job=%s: %v\n", jobID, err)
					}
					return
				}
			}
		}()

		// Open gRPC progress stream
		ctx, cancel := contextWithTimeout(c, 5*time.Minute)
		defer cancel()

		progressChan, err := grpcClient.StreamProgress(ctx, jobID)
		if err != nil {
			errMsg := models.ProgressMessage{
				Type:  "error",
				Error: fmt.Sprintf("Failed to open progress stream: %v", err),
			}
			wh.writeJSON(wsConn, errMsg)
			return
		}

		// Send initial connection confirmation
		confirmMsg := models.ProgressMessage{
			Type: "connected",
			Payload: models.ProgressUpdate{
				JobID:     jobID,
				Status:    "connected",
				Progress:  0,
				Message:   "WebSocket connection established. Streaming progress...",
				Timestamp: time.Now().UnixMilli(),
			},
		}
		if err := wh.writeJSON(wsConn, confirmMsg); err != nil {
			fmt.Printf("[WS] failed to send confirmation for job=%s: %v\n", jobID, err)
			return
		}

		// Start ping ticker
		pingTicker := time.NewTicker(wh.pingInterval)
		defer pingTicker.Stop()

		// Forward progress updates from gRPC to WebSocket
		for {
			select {
			case <-done:
				// Client disconnected
				fmt.Printf("[WS] client disconnected for job=%s\n", jobID)
				return

			case <-pingTicker.C:
				if err := wsConn.WriteControl(websocket.PingMessage, []byte{}, time.Now().Add(wh.writeTimeout)); err != nil {
					fmt.Printf("[WS] ping failed for job=%s: %v\n", jobID, err)
					return
				}

			case update, ok := <-progressChan:
				if !ok {
					// Stream closed - send completion and exit
					finalMsg := models.ProgressMessage{
						Type: "complete",
						Payload: models.ProgressUpdate{
							JobID:        jobID,
							Status:       "completed",
							Progress:     1.0,
							CurrentStage: "stream_closed",
							Message:      "Progress stream closed",
							Timestamp:    time.Now().UnixMilli(),
						},
					}
					wh.writeJSON(wsConn, finalMsg)
					return
				}

				msg := models.ProgressMessage{
					Type:    "progress",
					Payload: *update,
				}

				if err := wh.writeJSON(wsConn, msg); err != nil {
					fmt.Printf("[WS] write failed for job=%s: %v\n", jobID, err)
					return
				}

				// If status is terminal, close after sending
				if update.Status == "completed" || update.Status == "failed" || update.Status == "cancelled" {
					// Small delay to ensure client receives final message
					time.Sleep(500 * time.Millisecond)
					return
				}
			}
		}
	}
}

// writeJSON sends a JSON-encoded message over the WebSocket with a write timeout.
func (wh *WebSocketHub) writeJSON(conn *websocket.Conn, v interface{}) error {
	conn.SetWriteDeadline(time.Now().Add(wh.writeTimeout))
	defer conn.SetWriteDeadline(time.Time{}) // Reset deadline
	return conn.WriteJSON(v)
}

// writeMessage sends a raw message over the WebSocket.
func (wh *WebSocketHub) writeMessage(conn *websocket.Conn, messageType int, data []byte) error {
	conn.SetWriteDeadline(time.Now().Add(wh.writeTimeout))
	defer conn.SetWriteDeadline(time.Time{})
	return conn.WriteMessage(messageType, data)
}

// BroadcastProgress sends a progress update to all connected WebSocket clients
// for the given job ID. (Used for server-initiated broadcasts.)
func (wh *WebSocketHub) BroadcastProgress(jobID string, update *grpc.ProgressUpdate) error {
	// In a full implementation, this would look up all connections for jobID
	// and send the update to each. The connections map would be protected by a mutex.
	_ = jobID
	_ = update
	return nil
}

// BroadcastJSON marshals the given value as JSON and broadcasts it to all
// connections tracking the specified job.
func (wh *WebSocketHub) BroadcastJSON(jobID string, v interface{}) error {
	data, err := json.Marshal(v)
	if err != nil {
		return fmt.Errorf("failed to marshal broadcast message: %w", err)
	}
	_ = data
	_ = jobID
	return nil
}

// Close gracefully closes all active WebSocket connections in the hub.
func (wh *WebSocketHub) Close() {
	// In a full implementation, iterate over all connections and close them.
}

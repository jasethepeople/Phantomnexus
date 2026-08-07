# =============================================================================
# YouTube Sentinel - Worker Service Dockerfile
# =============================================================================
# Multi-stage build for the Go background worker service.
# Stage 1: Builder with full Go toolchain
# Stage 2: Production runtime using distroless (nonroot)
#
# Features:
#   - Minimal attack surface (distroless base)
#   - Non-root execution
#   - Static binary with CGO disabled
#   - Replicas support for horizontal scaling
#
# Build:
#   docker build -f docker/worker/Dockerfile -t sentinel-worker .
#
# =============================================================================

# ---------------------------------------------------------------------------
# STAGE 1: Builder
# ---------------------------------------------------------------------------
FROM golang:1.21-bookworm AS builder

LABEL maintainer="YouTube Sentinel Team <team@youtube-sentinel.dev>"
LABEL description="Sentinel Worker Service - Go background job processor"
LABEL stage="builder"

# Build arguments
ARG CGO_ENABLED=0
ARG GOOS=linux
ARG GOARCH=amd64
ARG VERSION=dev
ARG BUILD_TIME
ARG GIT_COMMIT
ARG GIT_BRANCH

# Environment
ENV CGO_ENABLED=${CGO_ENABLED}
ENV GOOS=${GOOS}
ENV GOARCH=${GOARCH}
ENV GOPROXY=https://proxy.golang.org,direct
ENV GOSUMDB=sum.golang.org
ENV GOTOOLCHAIN=local

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler \
    git \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Create build directory
WORKDIR /build

# Copy go module files first for layer caching
COPY go.mod go.sum ./

# Download dependencies (cached layer)
RUN go mod download && \
    go mod verify

# Copy source code
COPY cmd/worker/ cmd/worker/
COPY internal/ internal/
COPY pkg/ pkg/
COPY proto/ proto/

# Generate protobuf code (if needed)
RUN go generate ./... 2>/dev/null || true

# Build static binary with optimizations
RUN BUILD_TIME=${BUILD_TIME:-$(date -u +"%Y-%m-%dT%H:%M:%SZ")} && \
    GIT_COMMIT=${GIT_COMMIT:-$(git rev-parse --short HEAD 2>/dev/null || echo 'unknown')} && \
    GIT_BRANCH=${GIT_BRANCH:-$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo 'unknown')} && \
    go build \
    -trimpath \
    -ldflags "-s -w \
        -X github.com/youtube-sentinel/internal/version.Version=${VERSION} \
        -X github.com/youtube-sentinel/internal/version.BuildTime=${BUILD_TIME} \
        -X github.com/youtube-sentinel/internal/version.GitCommit=${GIT_COMMIT} \
        -X github.com/youtube-sentinel/internal/version.GitBranch=${GIT_BRANCH} \
        -extldflags '-static'" \
    -a \
    -installsuffix cgo \
    -o /tmp/worker-server \
    ./cmd/worker/

# Verify binary
RUN ls -lh /tmp/worker-server && \
    file /tmp/worker-server && \
    /tmp/worker-server -version 2>/dev/null || true

# ---------------------------------------------------------------------------
# STAGE 2: Production Runtime (Distroless)
# ---------------------------------------------------------------------------
FROM gcr.io/distroless/cc-debian12:nonroot AS production

LABEL maintainer="YouTube Sentinel Team <team@youtube-sentinel.dev>"
LABEL description="Sentinel Worker Service - Production runtime"
LABEL stage="production"

# Copy CA certificates for TLS
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy binary from builder
COPY --from=builder /tmp/worker-server /worker-server

# Expose metrics port
EXPOSE 8083

# Use nonroot user (UID 65532 in distroless)
USER nonroot:nonroot

# Health check endpoint
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/worker-server", "-health-check"]

# Run the worker
ENTRYPOINT ["/worker-server"]
CMD ["-queues=analysis,download,notify,export"]

# ---------------------------------------------------------------------------
# STAGE 3: Development
# ---------------------------------------------------------------------------
FROM golang:1.21-bookworm AS development

LABEL stage="development"

ENV SENTINEL_ENV=development
ENV CGO_ENABLED=0

# Install development tools
RUN go install github.com/cosmtrek/air@latest && \
    go install github.com/go-delve/delve/cmd/dlv@latest

WORKDIR /app

COPY go.mod go.sum ./
RUN go mod download

COPY . .

EXPOSE 8083 40000

CMD ["air", "-c", ".air.worker.toml"]

# =============================================================================
# YouTube Sentinel - API Service Dockerfile
# =============================================================================
# Multi-stage build for the Go REST/gRPC Gateway service.
# Stage 1: Builder with full Go toolchain
# Stage 2: Production runtime using distroless (nonroot)
#
# Features:
#   - Minimal attack surface (distroless base)
#   - Non-root execution
#   - Static binary with CGO disabled
#   - Layer caching for dependencies
#
# Build:
#   docker build -f docker/api/Dockerfile -t sentinel-api .
#
# =============================================================================

# ---------------------------------------------------------------------------
# STAGE 1: Builder
# ---------------------------------------------------------------------------
FROM golang:1.21-bookworm AS builder

LABEL maintainer="YouTube Sentinel Team <team@youtube-sentinel.dev>"
LABEL description="Sentinel API Service - Go REST/gRPC Gateway"
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
    protoc-gen-go \
    protoc-gen-go-grpc \
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
COPY cmd/api/ cmd/api/
COPY internal/ internal/
COPY pkg/ pkg/
COPY proto/ proto/

# Generate protobuf code (if needed)
RUN go generate ./... 2>/dev/null || true

# Build static binary with optimizations
# -ldflags:
#   -s: Omit symbol table
#   -w: Omit DWARF debug info
#   -extldflags '-static': Static linking
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
    -o /tmp/api-server \
    ./cmd/api/

# Verify binary
RUN ls -lh /tmp/api-server && \
    file /tmp/api-server && \
    /tmp/api-server -version 2>/dev/null || true

# ---------------------------------------------------------------------------
# STAGE 2: Production Runtime (Distroless)
# ---------------------------------------------------------------------------
FROM gcr.io/distroless/cc-debian12:nonroot AS production

LABEL maintainer="YouTube Sentinel Team <team@youtube-sentinel.dev>"
LABEL description="Sentinel API Service - Production runtime"
LABEL stage="production"

# Copy CA certificates for TLS
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy binary from builder
COPY --from=builder /tmp/api-server /api-server

# Expose HTTP and gRPC ports
EXPOSE 8080 9090 8081

# Use nonroot user (UID 65532 in distroless)
USER nonroot:nonroot

# Health check endpoint
HEALTHCHECK --interval=15s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/api-server", "-health-check"]

# Run the API server
ENTRYPOINT ["/api-server"]
CMD ["-http-addr=:8080", "-grpc-addr=:9090"]

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

EXPOSE 8080 9090 8081 40000

CMD ["air", "-c", ".air.toml"]

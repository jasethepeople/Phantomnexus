# YouTube Sentinel

[![Build Status](https://img.shields.io/github/actions/workflow/status/youtube-sentinel/sentinel/ci.yml?branch=main)](https://github.com/youtube-sentinel/sentinel/actions)
[![Go Version](https://img.shields.io/badge/go-1.21-blue)](https://golang.org/)
[![Rust Version](https://img.shields.io/badge/rust-1.75-orange)](https://www.rust-lang.org/)
[![Zig Version](https://img.shields.io/badge/zig-0.11-yellow)](https://ziglang.org/)
[![License](https://img.shields.io/badge/license-Apache%202.0-green)](LICENSE)

> AI-powered YouTube content analysis platform with real-time video processing,
> GPU-accelerated ML inference, and a high-performance WebAssembly frontend.

## Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Quick Start](#quick-start)
- [Prerequisites](#prerequisites)
- [Build Instructions](#build-instructions)
- [API Documentation](#api-documentation)
- [Docker Deployment](#docker-deployment)
- [Configuration Reference](#configuration-reference)
- [Development Guide](#development-guide)
- [Monitoring](#monitoring)
- [Troubleshooting](#troubleshooting)
- [Contributing](#contributing)
- [License](#license)

---

## Overview

YouTube Sentinel is a polyglot microservices platform designed for high-performance
YouTube content analysis. It combines the strengths of multiple programming languages
to deliver optimal performance across different workloads:

- **Go** - Concurrent API gateway and background job processing
- **Rust** - GPU-accelerated video analysis with Vulkan compute shaders
- **Zig** - Low-level video codec encoding/decoding
- **WASM** - Browser-based real-time video preview and UI

### Key Features

- Real-time video upload and analysis pipeline
- GPU-accelerated ML inference (object detection, classification, transcription)
- Horizontal scaling with configurable worker replicas
- WebSocket-based live progress updates
- Comprehensive observability with Prometheus + Grafana
- gRPC inter-service communication with Protocol Buffers
- Rate limiting and request authentication
- Multi-format video support (MP4, WebM, MOV, AVI, MKV)

---

## Architecture

```
                          +-------------------+
                          |     Client        |
                          |  (Browser/WASM)   |
                          +--------+----------+
                                   |
                                   | HTTPS/WSS
                                   v
+------------------------------------------------------------------+
|                          Nginx (Port 80)                          |
|  +------------------+  +------------------+  +------------------+ |
|  |   Static Files   |  |  Reverse Proxy   |  |   WebSocket      | |
|  |   /usr/share/    |  |   /api -> API    |  |   /ws -> API     | |
|  |   nginx/html     |  |   :8080          |  |   :8080          | |
|  +------------------+  +--------+---------+  +------------------+ |
+-----------------------------------+--------------------------------+
                                    |
              +---------------------+---------------------+
              |                     |                     |
              v                     v                     v
  +-----------+---------+ +-------+-------+ +-----------+---------+
  |   API Service       | |   Analysis    | |   Worker Service    |
  |   (Go, Port 8080)   | |   (Rust,      | |   (Go, Replicas: 2) |
  |                     | |   Port 50051) | |                     |
  | - REST API          | | - gRPC        | | - Job Processing    |
  | - Auth/JWT          | | - FFmpeg      | | - Async Tasks       |
  | - Rate Limiting     | | - Vulkan GPU  | | - Notifications     |
  | - File Upload       | | - ML Models   | | - Exports           |
  | - gRPC Client       | | - Segmentation| | - Downloads         |
  +-----------+---------+ +-------+-------+ +-----------+---------+
              |                     |                     |
              |                     |                     |
              v                     |                     v
  +-----------+---------+          |          +----------+----------+
  |   Redis (Port 6379) |          |          |   Prometheus        |
  |   - Job Queue       |<---------+          |   (Port 9090)       |
  |   - Session Cache   |                     |   - Metrics         |
  |   - Rate Limiting   |                     |   - Alerting        |
  +---------------------+                     +----------+----------+
                                                         |
                                              +----------v----------+
                                              |   Grafana           |
                                              |   (Port 3000)       |
                                              |   - Dashboards      |
                                              |   - Visualization   |
                                              +---------------------+
```

### Service Overview

| Service   | Language | Port   | Protocol | Purpose                      |
|-----------|----------|--------|----------|------------------------------|
| Nginx     | C        | 80/443 | HTTP/WSS | Edge gateway, static files   |
| API       | Go       | 8080   | REST/gRPC| Client-facing API server     |
| Analysis  | Rust     | 50051  | gRPC     | GPU video analysis engine    |
| Worker    | Go       | 8083   | gRPC     | Background job processor     |
| Redis     | C        | 6379   | RESP     | Queue, cache, sessions       |
| Prometheus| Go       | 9090   | HTTP     | Metrics collection           |
| Grafana   | Go/TS    | 3000   | HTTP     | Observability dashboards     |

### Data Flow

1. **Upload**: Client uploads video via `POST /api/v1/upload` -> Nginx -> API
2. **Queue**: API enqueues analysis job via Redis -> Workers pick up job
3. **Analysis**: Worker calls Analysis gRPC service -> FFmpeg decodes -> Vulkan ML inference
4. **Results**: Analysis results stored -> Worker sends notifications -> WebSocket update
5. **Dashboard**: Prometheus scrapes metrics -> Grafana visualizes

---

## Quick Start

### Using Docker (Recommended)

```bash
# Clone the repository
git clone https://github.com/youtube-sentinel/sentinel.git
cd sentinel

# Start all services
docker-compose -f docker/docker-compose.yml up -d

# Verify all services are running
docker-compose -f docker/docker-compose.yml ps

# View logs
docker-compose -f docker/docker-compose.yml logs -f
```

### Access Points

| Service    | URL                     | Default Credentials       |
|------------|-------------------------|---------------------------|
| Web UI     | http://localhost        | N/A                       |
| API        | http://localhost:8080   | N/A                       |
| Grafana    | http://localhost:3000   | admin / admin             |
| Prometheus | http://localhost:9090   | N/A                       |

---

## Prerequisites

### Required

- [Docker](https://docs.docker.com/get-docker/) 24.0+ & Docker Compose 2.20+
- [Git](https://git-scm.com/) 2.40+

### For Local Development

- [Go](https://golang.org/dl/) 1.21+
- [Rust](https://rustup.rs/) 1.75+
- [Zig](https://ziglang.org/download/) 0.11+
- [Node.js](https://nodejs.org/) 18+ (for WASM build tools)
- [Protocol Buffers](https://grpc.io/docs/protoc-installation/) 3.24+
- [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/) 0.12+
- Make 4.3+

### Optional

- [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/install-guide.html) (for GPU support)
- [k6](https://k6.io/docs/get-started/installation/) (for load testing)
- [Trivy](https://aquasecurity.github.io/trivy/) (for security scanning)

---

## Build Instructions

### Full Build (All Languages)

```bash
# Build everything: proto, rust, zig, go, wasm, docker
make all
```

### Language-Specific Builds

#### Rust (Analysis Engine)

```bash
# Build analysis service and shared library
make rust

# Individual components
make rust-analysis     # Analysis service only
make rust-shared       # Shared library only

# Testing and quality
make rust-test         # Run test suite
make rust-clippy       # Run linter
make rust-fmt          # Format code
make rust-check        # Full CI check (fmt + clippy + test)
```

#### Zig (Codec Components)

```bash
# Build all Zig components
make zig

# Individual components
make zig-encoder       # Video encoder
make zig-decoder       # Video decoder

# Testing
make zig-test          # Run Zig tests
```

#### Go (API & Worker)

```bash
# Build both Go services
make go

# Individual services
make go-api            # API server
make go-worker         # Background worker

# Testing and quality
make go-test           # Run tests with race detection
make go-bench          # Run benchmarks
make go-vet            # Static analysis
make go-check          # Full CI check (fmt + vet + test)
```

#### WASM (Browser UI)

```bash
# Build and optimize WASM module
make wasm

# Development build
make wasm-dev

# Testing
make wasm-test         # Run WASM tests
```

#### Protocol Buffers

```bash
# Generate bindings for all languages
make proto

# Individual languages
make proto-go          # Go bindings
make proto-rust        # Rust bindings
make proto-docs        # API documentation
make proto-lint        # Lint definitions
```

### Docker Build

```bash
# Build all Docker images
make docker
make docker-build

# Push to registry
make docker-push

# Pull latest images
make docker-pull
```

---

## API Documentation

### Authentication

Most endpoints require a JWT bearer token:

```bash
curl -X POST http://localhost:8080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username": "user", "password": "pass"}'
```

Include the token in subsequent requests:

```bash
curl -H "Authorization: Bearer <token>" http://localhost:8080/api/v1/videos
```

### Endpoints

#### Videos

```bash
# Upload a video
curl -X POST http://localhost:8080/api/v1/upload \
  -H "Authorization: Bearer <token>" \
  -F "file=@/path/to/video.mp4" \
  -F "title=My Video" \
  -F "description=Video description"

# List all videos
curl -H "Authorization: Bearer <token>" \
  http://localhost:8080/api/v1/videos?page=1&limit=20

# Get video details
curl -H "Authorization: Bearer <token>" \
  http://localhost:8080/api/v1/videos/<video-id>

# Get video analysis results
curl -H "Authorization: Bearer <token>" \
  http://localhost:8080/api/v1/videos/<video-id>/analysis

# Delete a video
curl -X DELETE -H "Authorization: Bearer <token>" \
  http://localhost:8080/api/v1/videos/<video-id>
```

#### Analysis

```bash
# Request analysis for a video
curl -X POST http://localhost:8080/api/v1/analysis \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "video_id": "<video-id>",
    "analysis_types": ["object_detection", "transcription", "classification"]
  }'

# Get analysis status
curl -H "Authorization: Bearer <token>" \
  http://localhost:8080/api/v1/analysis/<analysis-id>/status

# Get analysis results
curl -H "Authorization: Bearer <token>" \
  http://localhost:8080/api/v1/analysis/<analysis-id>/results
```

#### Jobs

```bash
# List job queue status
curl -H "Authorization: Bearer <token>" \
  http://localhost:8080/api/v1/jobs

# Get job details
curl -H "Authorization: Bearer <token>" \
  http://localhost:8080/api/v1/jobs/<job-id>

# Cancel a job
curl -X DELETE -H "Authorization: Bearer <token>" \
  http://localhost:8080/api/v1/jobs/<job-id>
```

#### Health & Metrics

```bash
# API health check
curl http://localhost:8080/api/v1/health

# Prometheus metrics (unauthenticated)
curl http://localhost:8081/metrics

# Analysis health check
curl http://localhost:8082/healthz

# Worker health check
curl http://localhost:8083/healthz
```

### WebSocket (Real-time Updates)

```javascript
const ws = new WebSocket('ws://localhost/ws');

ws.onopen = () => {
  // Subscribe to video analysis events
  ws.send(JSON.stringify({
    action: 'subscribe',
    channel: 'analysis.video-id'
  }));
};

ws.onmessage = (event) => {
  const update = JSON.parse(event.data);
  console.log('Progress:', update.progress, '%');
  console.log('Status:', update.status);
};
```

### Response Format

All API responses follow this structure:

```json
{
  "success": true,
  "data": { ... },
  "meta": {
    "request_id": "uuid",
    "timestamp": "2024-01-01T00:00:00Z",
    "pagination": {
      "page": 1,
      "limit": 20,
      "total": 100
    }
  }
}
```

Error responses:

```json
{
  "success": false,
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Invalid input parameters",
    "details": [ ... ]
  },
  "meta": {
    "request_id": "uuid",
    "timestamp": "2024-01-01T00:00:00Z"
  }
}
```

---

## Docker Deployment

### Development

```bash
# Start development environment
make dev

# Or manually:
docker-compose -f docker/docker-compose.yml \
  -f docker/docker-compose.dev.yml up -d --build
```

### Production

```bash
# 1. Set environment variables
cp .env.example .env
# Edit .env with production values

# 2. Start with production overrides
docker-compose -f docker/docker-compose.yml \
  -f docker/docker-compose.prod.yml up -d

# 3. Verify deployment
docker-compose -f docker/docker-compose.yml ps
docker-compose -f docker/docker-compose.yml logs -f
```

### Environment Variables

Create a `.env` file in the project root:

```env
# Database
DB_DSN=postgres://user:pass@db:5432/sentinel?sslmode=disable

# Security
JWT_SECRET=your-super-secret-jwt-key-min-32-chars
REDIS_PASSWORD=your-redis-password

# Grafana
GRAFANA_ADMIN_USER=admin
GRAFANA_ADMIN_PASSWORD=change-me-strong-password
GRAFANA_ROOT_URL=https://grafana.yourdomain.com

# Sentry (optional)
SENTRY_DSN=https://xxx@yyy.ingest.sentry.io/zzz

# Feature flags
SENTINEL_TRACING_ENABLED=true
SENTINEL_GPU_ENABLED=true
```

### Scaling Workers

```bash
# Scale workers to 5 replicas
docker-compose -f docker/docker-compose.yml up -d --scale worker=5
```

### GPU Support

For NVIDIA GPU acceleration, ensure the NVIDIA Container Toolkit is installed:

```bash
# Verify GPU access
docker run --rm --runtime=nvidia nvidia/cuda:12.0-base nvidia-smi

# The analysis service uses the nvidia runtime by default
docker-compose -f docker/docker-compose.yml up -d analysis
```

### Updating

```bash
# Pull latest images
make docker-pull

# Rebuild and restart
docker-compose -f docker/docker-compose.yml up -d --build
```

---

## Configuration Reference

### API Service (`SENTINEL_*` env vars)

| Variable                  | Default        | Description                        |
|---------------------------|----------------|------------------------------------|
| `SENTINEL_ENV`            | `production`   | Runtime environment                |
| `SENTINEL_LOG_LEVEL`      | `info`         | Log verbosity (debug/info/warn/error) |
| `SENTINEL_HTTP_ADDR`      | `:8080`        | HTTP server bind address           |
| `SENTINEL_GRPC_ADDR`      | `:9090`        | gRPC server bind address           |
| `SENTINEL_REDIS_ADDR`     | `redis:6379`   | Redis connection string            |
| `SENTINEL_REDIS_POOL_SIZE`| `20`           | Redis connection pool size         |
| `SENTINEL_ANALYSIS_GRPC_ADDR` | `analysis:50051` | Analysis gRPC endpoint         |
| `SENTINEL_MAX_UPLOAD_SIZE`| `500MB`        | Maximum upload file size           |
| `SENTINEL_JWT_SECRET`     | `changeme`     | JWT signing secret                 |
| `SENTINEL_RATE_LIMIT_RPS` | `100`          | Rate limit requests per second     |

### Analysis Service

| Variable                      | Default        | Description                    |
|-------------------------------|----------------|--------------------------------|
| `SENTINEL_GRPC_ADDR`          | `:50051`       | gRPC bind address              |
| `SENTINEL_METRICS_ADDR`       | `:8082`        | Metrics HTTP endpoint          |
| `SENTINEL_MODEL_PATH`         | `/models`      | ML model cache directory       |
| `SENTINEL_VULKAN_DEVICE_INDEX`| `0`            | GPU device index               |
| `SENTINEL_MAX_CONCURRENT_JOBS`| `4`            | Max parallel analysis jobs     |
| `SENTINEL_GPU_MEMORY_FRACTION`| `0.8`          | Fraction of GPU memory to use  |

### Worker Service

| Variable                      | Default                        | Description              |
|-------------------------------|--------------------------------|--------------------------|
| `SENTINEL_WORKER_CONCURRENCY` | `5`                            | Concurrent job workers   |
| `SENTINEL_WORKER_QUEUES`      | `analysis,download,notify,export` | Queues to process     |
| `SENTINEL_MAX_RETRIES`        | `3`                            | Max job retry attempts   |
| `SENTINEL_RETRY_BACKOFF`      | `exponential`                  | Retry backoff strategy   |

---

## Development Guide

### Project Structure

```
sentinel/
|-- cmd/                        # Go applications
|   |-- api/                    # API server entrypoint
|   |   |-- main.go
|   |   |-- handlers/
|   |   |-- middleware/
|   |   |-- .air.toml           # Live reload config
|   |-- worker/                 # Background worker entrypoint
|       |-- main.go
|       |-- .air.worker.toml
|-- crates/                     # Rust crates
|   |-- analysis/               # gRPC analysis engine
|   |   |-- Cargo.toml
|   |   |-- src/
|   |   |-- build.rs
|   |-- shared/                 # Shared Rust library
|       |-- Cargo.toml
|       |-- src/
|-- zig/                        # Zig components
|   |-- encoder/
|   |-- decoder/
|-- web/                        # WebAssembly frontend
|   |-- src/
|   |-- static/
|   |-- Cargo.toml
|-- internal/                   # Go internal packages
|   |-- version/
|   |-- config/
|   |-- logger/
|-- pkg/                        # Go public packages
|   |-- proto/
|   |-- queue/
|   |-- storage/
|-- docker/                     # Docker configuration
|   |-- docker-compose.yml
|   |-- docker-compose.prod.yml
|   |-- docker-compose.dev.yml
|   |-- analysis/
|   |   |-- Dockerfile
|   |-- api/
|   |   |-- Dockerfile
|   |-- worker/
|   |   |-- Dockerfile
|   |-- nginx.conf
|   |-- prometheus.yml
|   |-- grafana/
|       |-- dashboards/
|       |   |-- sentinel.json
|       |-- datasources/
|           |-- prometheus.yml
|-- proto/                      # Protocol Buffer definitions
|   |-- sentinel.proto
|-- tests/                      # Integration & E2E tests
|   |-- integration/
|   |-- e2e/
|   |-- load/
|-- migrations/                 # Database migrations
|-- Makefile                    # Build automation
|-- README.md                   # This file
|-- LICENSE                     # Apache 2.0
`-- .gitignore
```

### Running Locally

```bash
# 1. Start infrastructure services
docker-compose -f docker/docker-compose.yml up -d redis

# 2. Run analysis service (Rust)
cd crates/analysis
cargo run --release

# 3. Run API server (Go)
go run ./cmd/api

# 4. Run worker (Go, in another terminal)
go run ./cmd/worker

# 5. Build and serve WASM
make wasm
# Serve web/ directory with any static server
```

### Code Quality

```bash
# Format all code
make fmt

# Run all linters
make lint

# Run all tests
make test

# Full CI check
make check

# Security scan
make trivy
make audit
```

### Adding a New API Endpoint

1. Define the endpoint in `proto/sentinel.proto`
2. Run `make proto-go proto-rust`
3. Implement handler in `cmd/api/handlers/`
4. Add route in `cmd/api/router.go`
5. Add tests in `cmd/api/handlers/*_test.go`

### Adding a New Analysis Type

1. Define analysis parameters in `proto/sentinel.proto`
2. Implement analyzer in `crates/analysis/src/analyzers/`
3. Register in `crates/analysis/src/registry.rs`
4. Add tests in `crates/analysis/src/analyzers/*_test.rs`

---

## Monitoring

### Prometheus Metrics

Each service exposes metrics at `/metrics`:

- `http_requests_total` - HTTP request count by method, path, status
- `http_request_duration_seconds` - Request latency histogram
- `grpc_requests_total` - gRPC request count
- `grpc_request_duration_seconds` - gRPC latency
- `analysis_requests_total` - Analysis jobs by type
- `analysis_duration_seconds` - Analysis duration
- `gpu_memory_used_bytes` - GPU memory usage
- `worker_jobs_processed_total` - Jobs processed by queue
- `redis_commands_total` - Redis command count

### Grafana Dashboards

Access Grafana at http://localhost:3000 with the default credentials.

The pre-configured dashboard includes:

- **Overview**: Service health status, RPS, P95 latency
- **API**: Request rate, latency percentiles, error rate by endpoint
- **Services**: Memory, CPU, goroutine usage for each service
- **Analysis Engine**: Request rate, duration, GPU utilization, job statistics
- **Redis**: Memory usage, command rate, hit rate, connections
- **Workers**: Job processing rate, duration by queue
- **Logs**: Log rate by level for each service

### Alert Rules (Coming Soon)

Planned alerts:

- High error rate (>5% over 5 minutes)
- High P95 latency (>1s over 5 minutes)
- Service downtime (any service down >1 minute)
- High memory usage (>90% for 5 minutes)
- Disk space low (<10% remaining)
- GPU memory exhausted
- Queue depth growing (>1000 jobs)

---

## Troubleshooting

### Common Issues

#### Docker Compose fails to start

```bash
# Check logs
docker-compose -f docker/docker-compose.yml logs

# Ensure ports are not in use
lsof -i :8080 :9090 :3000 :6379 :50051

# Clean restart
docker-compose -f docker/docker-compose.yml down -v
docker-compose -f docker/docker-compose.yml up -d
```

#### Analysis service fails with GPU error

```bash
# Check NVIDIA runtime
docker run --rm --runtime=nvidia nvidia/cuda:12.0-base nvidia-smi

# If GPU unavailable, use CPU fallback
# Set SENTINEL_GPU_ENABLED=false in .env
```

#### Redis connection refused

```bash
# Check Redis is running
docker-compose -f docker/docker-compose.yml ps redis

# Check Redis logs
docker-compose -f docker/docker-compose.yml logs redis

# Test Redis connection
docker-compose -f docker/docker-compose.yml exec redis redis-cli ping
```

#### Build fails with missing dependencies

```bash
# Verify all prerequisites
make env

# Install missing tools and retry
make all
```

### Debug Mode

Set `SENTINEL_LOG_LEVEL=debug` for verbose logging in any service.

---

## Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

### Development Workflow

1. Fork the repository
2. Create a feature branch: `git checkout -b feat/my-feature`
3. Make your changes
4. Run quality checks: `make check`
5. Commit with [Conventional Commits](https://conventionalcommits.org/): `feat: add new feature`
6. Push and create a Pull Request

### Code Style

- **Go**: Follow `gofmt` and standard Go conventions
- **Rust**: Follow `rustfmt` and `clippy` recommendations
- **Zig**: Follow standard Zig conventions
- **Commit messages**: Use conventional commits format

---

## License

YouTube Sentinel is licensed under the [Apache License 2.0](LICENSE).

```
Copyright 2024 YouTube Sentinel Contributors

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

---

## Acknowledgments

- Built with [Go](https://golang.org/), [Rust](https://www.rust-lang.org/),
  [Zig](https://ziglang.org/), and [WebAssembly](https://webassembly.org/)
- Monitoring powered by [Prometheus](https://prometheus.io/) and [Grafana](https://grafana.com/)
- Video processing via [FFmpeg](https://ffmpeg.org/) and [Vulkan](https://www.vulkan.org/)
- UI framework inspired by modern WebAssembly patterns

---

## Support

- Documentation: https://github.com/youtube-sentinel/docs
- Issues: https://github.com/youtube-sentinel/sentinel/issues
- Discussions: https://github.com/youtube-sentinel/sentinel/discussions

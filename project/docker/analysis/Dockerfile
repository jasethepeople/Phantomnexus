# =============================================================================
# YouTube Sentinel - Analysis Service Dockerfile
# =============================================================================
# Multi-stage build for the Rust gRPC analysis engine.
# Stage 1: Build environment with full Rust toolchain and native dependencies
# Stage 2: Production runtime with FFmpeg, Vulkan, and minimal system libraries
#
# Features:
#   - GPU-accelerated ML inference via Vulkan
#   - FFmpeg for video/audio processing
#   - Security-hardened with non-root user
#   - Optimized binary size with strip and LTO
#
# Build:
#   docker build -f docker/analysis/Dockerfile -t sentinel-analysis .
#
# =============================================================================

# ---------------------------------------------------------------------------
# STAGE 1: Builder
# ---------------------------------------------------------------------------
FROM rust:1.75-slim-bookworm AS builder

LABEL maintainer="YouTube Sentinel Team <team@youtube-sentinel.dev>"
LABEL description="Sentinel Analysis Service - Rust gRPC GPU-accelerated video analysis"
LABEL stage="builder"

# Build arguments
ARG RUSTFLAGS="-C target-cpu=sandybridge -C opt-level=3"
ARG CARGO_NET_RETRY=10
ARG DEBIAN_FRONTEND=noninteractive

# Environment
ENV RUSTFLAGS="${RUSTFLAGS}"
ENV CARGO_NET_RETRY=${CARGO_NET_RETRY}
ENV CARGO_HOME=/usr/local/cargo
ENV RUSTUP_HOME=/usr/local/rustup
ENV PATH="${CARGO_HOME}/bin:${PATH}"

# Install build dependencies
# - pkg-config: Required for native library linking
# - libclang-dev: Required by bindgen for FFI
# - protobuf-compiler: For gRPC code generation
# - cmake: Required by several native dependencies
# - nasm: Required by some FFmpeg codecs
# - yasm: Required by some FFmpeg codecs
# - llvm: For LLVM tools
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libclang-dev \
    clang \
    protobuf-compiler \
    libprotobuf-dev \
    cmake \
    nasm \
    yasm \
    llvm \
    libvulkan-dev \
    vulkan-validationlayers-dev \
    spirv-tools \
    libssl-dev \
    libasound2-dev \
    libavutil-dev \
    libavcodec-dev \
    libavformat-dev \
    libswscale-dev \
    libavfilter-dev \
    libavdevice-dev \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Create app directory
WORKDIR /build

# Copy dependency manifests first for layer caching
COPY crates/analysis/Cargo.toml crates/analysis/Cargo.lock ./
COPY crates/shared/Cargo.toml crates/shared/
COPY proto/ proto/

# Create dummy source files to cache dependency compilation
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    mkdir -p crates/shared/src && \
    echo "pub fn dummy() {}" > crates/shared/src/lib.rs

# Build dependencies (cached layer)
RUN cargo fetch && \
    cargo build --release 2>/dev/null || true

# Remove dummy sources
RUN rm -rf src crates/shared/src

# Copy actual source code
COPY crates/analysis/src/ src/
COPY crates/analysis/build.rs ./
COPY crates/shared/src/ crates/shared/src/

# Build release binary with optimizations
# - LTO: Link-time optimization for smaller/faster binaries
# - strip: Remove debug symbols from final binary
RUN cargo build --release --locked && \
    strip --strip-all target/release/sentinel-analysis && \
    cp target/release/sentinel-analysis /tmp/sentinel-analysis && \
    ls -lh /tmp/sentinel-analysis

# Generate gRPC health check binary
RUN cargo build --release --bin sentinel-analysis --features healthcheck 2>/dev/null || \
    echo "Health check built into main binary"

# ---------------------------------------------------------------------------
# STAGE 2: Production Runtime
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS production

LABEL maintainer="YouTube Sentinel Team <team@youtube-sentinel.dev>"
LABEL description="Sentinel Analysis Service - Production runtime"
LABEL stage="production"

ARG DEBIAN_FRONTEND=noninteractive
ARG APP_UID=10001
ARG APP_GID=10001

# Environment
ENV SENTINEL_ENV=production
ENV SENTINEL_GRPC_ADDR=0.0.0.0:50051
ENV SENTINEL_METRICS_ADDR=0.0.0.0:8082
ENV SENTINEL_MODEL_PATH=/models
ENV RUST_BACKTRACE=1
ENV RUST_LOG=info

# Install runtime dependencies
# FFmpeg (full suite for video/audio processing)
# Vulkan (GPU compute for ML inference)
# CA certificates (for TLS)
RUN apt-get update && apt-get install -y --no-install-recommends \
    # FFmpeg and codecs
    ffmpeg \
    libavcodec59 \
    libavformat59 \
    libavutil57 \
    libavfilter8 \
    libswscale6 \
    libavdevice59 \
    libswresample4 \
    # Vulkan runtime
    libvulkan1 \
    vulkan-validationlayers \
    mesa-vulkan-drivers \
    # Additional codecs
    libx264-164 \
    libx265-199 \
    libvpx7 \
    libopus0 \
    libvorbis0a \
    libtheora0 \
    # SSL and networking
    ca-certificates \
    openssl \
    # Utility libraries
    libssl3 \
    libfontconfig1 \
    libfreetype6 \
    # Security updates
    && apt-get upgrade -y \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean \
    && rm -rf /var/cache/apt/archives/*

# Create non-root user
RUN groupadd --gid ${APP_GID} sentinel && \
    useradd --uid ${APP_UID} --gid ${APP_GID} \
    --create-home --home-dir /app \
    --shell /usr/sbin/nologin \
    sentinel

# Create required directories
RUN mkdir -p /models /uploads /tmp/analysis && \
    chown -R sentinel:sentinel /app /models /tmp/analysis

# Copy binary from builder
COPY --from=builder /tmp/sentinel-analysis /app/sentinel-analysis
COPY --from=builder /build/target/release/ /app/bin/ 2>/dev/null || true

# Set permissions
RUN chown -R sentinel:sentinel /app && \
    chmod +x /app/sentinel-analysis

# Switch to non-root user
USER sentinel:sentinel

# Working directory
WORKDIR /app

# Expose gRPC and metrics ports
EXPOSE 50051 8082

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=3 \
    CMD ["/app/sentinel-analysis", "--health-check"]

# Run the analysis service
ENTRYPOINT ["/app/sentinel-analysis"]
CMD ["--grpc-addr=0.0.0.0:50051", "--metrics-addr=0.0.0.0:8082"]

# ---------------------------------------------------------------------------
# STAGE 3: Development (optional)
# ---------------------------------------------------------------------------
FROM builder AS development

ENV SENTINEL_ENV=development
ENV RUST_LOG=debug
ENV RUST_BACKTRACE=full

WORKDIR /build

EXPOSE 50051 8082

CMD ["cargo", "run", "--release"]

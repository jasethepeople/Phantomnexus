# YouTube Sentinel — Pre-Moderation Analysis Engine
## Build Plan (Get Crazy / Get Jiggy Edition)

### Overview
Build a production-grade, distributed YouTube pre-moderation analysis system called "YouTube Sentinel" using Rust (core), Zig (performance-critical path), and Go (API/services). The system analyzes video content before upload to predict and prevent demonetization, community guideline strikes, and copyright claims.

---

### Stage 1 — Architecture & Project Scaffold
**Skill**: vibecoding-general-swarm
**Goal**: Lay out the complete multi-language workspace structure

- Initialize Rust workspace with all crates
- Initialize Go module for API services
- Initialize Zig build for performance components
- Create shared protobuf contracts
- Set up build orchestration (justfile/Makefile)
- Docker compose for local dev

**Agents**:
- `architect_rust` — Rust workspace scaffolding
- `architect_go` — Go module scaffolding
- `architect_zig` — Zig build scaffolding

### Stage 2 — Core Rust Pipeline
**Skill**: vibecoding-general-swarm
**Goal**: Build the heart of the system

- Video ingestion & frame extraction engine
- Multi-modal analysis core (visual + audio + metadata)
- Policy database & matching engine
- Contextual synthesis engine
- gRPC communication layer

**Agents**:
- `rust_frame_engine` — Frame extraction with GPU acceleration
- `rust_analysis_core` — Multi-modal analysis pipeline
- `rust_policy_engine` — Policy matching & scoring
- `rust_synthesis` — Contextual synthesis & final scoring

### Stage 3 — Zig Performance Layer
**Skill**: vibecoding-general-swarm
**Goal**: Raw-performance components

- Frame pixel analysis algorithms
- Audio fingerprinting core
- SIMD-optimized operations

**Agents**:
- `zig_frame_analysis` — Pixel-level frame analysis
- `zig_audio_fingerprint` — Audio fingerprinting

### Stage 4 — Go API & Services
**Skill**: vibecoding-general-swarm
**Goal**: HTTP API, upload handling, job orchestration

- REST API for video submission
- Job queue & worker management
- WebSocket for real-time progress
- YouTube Studio API integration stubs

**Agents**:
- `go_api_service` — HTTP API server
- `go_job_worker` — Background processing workers

### Stage 5 — Auto-Fix & Remediation
**Skill**: vibecoding-general-swarm
**Goal**: Precision content remediation

- Smart blur / object removal
- Audio replacement & crossfading
- Metadata optimization
- Fix validation & re-analysis

**Agents**:
- `rust_autofix` — Remediation pipeline
- `rust_validation` — Fix validation system

### Stage 6 — WASM UI Layer
**Skill**: vibecoding-general-swarm
**Goal**: Browser-based analysis reporting UI

- Three-panel analysis dashboard
- Real-time progress visualization
- Fix customization controls
- Side-by-side comparison view

**Agents**:
- `wasm_ui` — Rust/WASM frontend

### Stage 7 — Integration & Testing
**Skill**: vibecoding-general-swarm
**Goal**: Wire everything together

- Integration tests across all components
- End-to-end pipeline validation
- Performance benchmarks
- Docker deployment config

**Agents**:
- `integration_test` — Full integration suite
- `deployment_config` — Docker + deployment

---

### Output Structure
```
/youtube-sentinel/
  /crates/
    sentinel-core/          — Shared types & contracts
    frame-extractor/        — Video → frames pipeline
    analysis-engine/        — Multi-modal analysis
    policy-engine/          — Policy matching
    synthesis/              — Contextual synthesis
    autofix/                — Remediation system
    grpc-server/            — gRPC service layer
  /zig/
    frame-analysis/         — Pixel analysis engine
    audio-fingerprint/      — Audio fingerprinting
  /go/
    api-service/            — HTTP API
    job-worker/             — Background workers
  /wasm-ui/                 — Browser UI (Rust/WASM)
  /proto/                   — Shared protobuf definitions
  /docker/                  — Container configs
  Makefile / justfile       — Build orchestration
```

### Deliverables
- Full source code for all components
- Working Docker Compose setup
- Integration test suite
- README with architecture docs
- Build scripts for all 3 languages

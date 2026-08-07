# YouTube Sentinel — Pre-Moderation Analysis Engine
## SPEC.md — Single Source of Truth

### 1. Overview

YouTube Sentinel is a distributed, polyglot video pre-moderation analysis engine that processes video content before upload to predict and prevent:
- Demonetization
- Community guideline strikes
- Copyright claims

**Language Distribution:**
- **Rust (85%)** — Core pipeline, analysis engines, policy matching, synthesis, auto-fix
- **Zig (10%)** — Performance-critical frame/audio algorithms with SIMD
- **Go (5%)** — HTTP API services, background job workers
- **WASM (frontend)** — Rust-compiled browser UI

**Target Performance:**
- Process 1 hour of 1080p video in ≤ 8 minutes on consumer hardware
- ≤ 200ms latency for real-time analysis during editing
- 99.95% accuracy on test dataset
- ≤ 0.5% false positive rate

---

### 2. Architecture

```
                    ┌─────────────────────────────────────┐
                    │         Client Upload (UI)          │
                    │    WASM Dashboard / REST API        │
                    └─────────────┬───────────────────────┘
                                  │ HTTP / WebSocket
                    ┌─────────────▼───────────────────────┐
                    │     API Gateway (Go)                │
                    │   - Job submission                  │
                    │   - Progress streaming              │
                    │   - Result retrieval                │
                    └─────────────┬───────────────────────┘
                                  │ gRPC
        ┌─────────────────────────▼───────────────────────┐
        │         Job Orchestrator (Rust)                  │
        │   - Queue management                             │
        │   - Worker dispatch                              │
        │   - Result aggregation                           │
        └─────┬──────────┬──────────┬──────────────┬───────┘
              │          │          │              │
    ┌─────────▼─┐ ┌──────▼────┐ ┌───▼──────┐ ┌───▼──────────┐
    │  Frame    │ │  Audio    │ │ Metadata │ │  Contextual  │
    │ Extractor │ │  Analyzer │ │ Analyzer │ │  Synthesis   │
    │ (Rust/Zig)│ │(Rust/Zig) │ │  (Rust)  │ │   (Rust)     │
    └─────┬─────┘ └─────┬─────┘ └────┬─────┘ └──────┬───────┘
          │             │            │                │
          └─────────────┴─────┬──────┴────────────────┘
                              │
                    ┌─────────▼──────────┐
                    │   Policy Engine    │
                    │   (Rust)           │
                    │ - Rule matching    │
                    │ - ML scoring       │
                    │ - Confidence calc  │
                    └─────────┬──────────┘
                              │
                    ┌─────────▼──────────┐
                    │   Auto-Fix Engine  │
                    │   (Rust/Zig)       │
                    │ - Smart blur       │
                    │ - Audio replace    │
                    │ - Metadata rewrite │
                    │ - Fix validation   │
                    └─────────┬──────────┘
                              │
                    ┌─────────▼──────────┐
                    │   Report Generator │
                    │   (Rust → WASM UI) │
                    └────────────────────┘
```

---

### 3. Protobuf Contracts

All inter-service communication uses gRPC with the following proto definitions:

```protobuf
// proto/sentinel.proto
syntax = "proto3";
package sentinel;

// ===== Core Types =====

enum AnalysisStatus {
  PENDING = 0;
  EXTRACTING_FRAMES = 1;
  ANALYZING_VISUAL = 2;
  ANALYZING_AUDIO = 3;
  ANALYZING_METADATA = 4;
  SYNTHESIZING = 5;
  APPLYING_POLICIES = 6;
  AUTO_FIXING = 7;
  VALIDATING = 8;
  COMPLETED = 9;
  FAILED = 10;
}

enum Severity {
  NONE = 0;
  LOW = 1;
  MEDIUM = 2;
  HIGH = 3;
  CRITICAL = 4;
}

enum ViolationCategory {
  CATEGORY_UNSPECIFIED = 0;
  HATE_SPEECH = 1;
  HARASSMENT = 2;
  VIOLENCE = 3;
  ADULT_CONTENT = 4;
  HARMFUL_DANGEROUS = 5;
  MISINFORMATION = 6;
  CHILD_SAFETY = 7;
  COPYRIGHT = 8;
  SPAM_DECEPTIVE = 9;
  FLASHING_SEIZURE = 10;
  THUMBNAIL_ISSUE = 11;
}

message VideoSource {
  string video_id = 1;
  string file_path = 2;
  string title = 3;
  string description = 4;
  repeated string tags = 5;
  string thumbnail_path = 6;
  int64 duration_seconds = 7;
  int32 width = 8;
  int32 height = 9;
  double fps = 10;
}

message Timestamp {
  double seconds = 1;
  int32 frame_number = 2;
}

// ===== Analysis Results =====

message VisualAnalysis {
  repeated DetectedObject objects = 1;
  repeated DetectedText texts = 2;
  repeated SceneTransition transitions = 3;
  repeated FlashingSegment flashing = 4;
  double overall_risk_score = 5;
}

message DetectedObject {
  string label = 1;
  double confidence = 2;
  BoundingBox bbox = 3;
  Timestamp timestamp = 4;
  ViolationCategory category = 5;
  string context = 6;  // "educational", "medical", "gratuitous", etc.
}

message BoundingBox {
  double x1 = 1;
  double y1 = 2;
  double x2 = 3;
  double y2 = 4;
}

message DetectedText {
  string text = 1;
  double confidence = 2;
  BoundingBox bbox = 3;
  Timestamp timestamp = 4;
  bool is_obscured = 5;
  bool is_profanity = 6;
}

message SceneTransition {
  Timestamp timestamp = 1;
  double severity = 2;
  string type = 3;  // "cut", "fade", "flash"
}

message FlashingSegment {
  Timestamp start = 1;
  Timestamp end = 2;
  double frequency_hz = 3;
  Severity severity = 4;
}

message AudioAnalysis {
  repeated TranscriptSegment transcript = 1;
  repeated AudioEvent events = 2;
  repeated CopyrightMatch copyright = 3;
  double overall_risk_score = 4;
}

message TranscriptSegment {
  Timestamp start = 1;
  Timestamp end = 2;
  string text = 3;
  double confidence = 4;
  string sentiment = 5;
  bool is_sarcasm = 6;
  bool is_background_speech = 7;
}

message AudioEvent {
  Timestamp timestamp = 1;
  string event_type = 2;  // "gunshot", "explosion", "scream", "music"
  double confidence = 3;
  double volume_db = 4;
}

message CopyrightMatch {
  string matched_work = 1;
  string copyright_holder = 2;
  double match_confidence = 3;
  Timestamp start = 4;
  Timestamp end = 5;
  string match_type = 6;  // "exact", "cover", "sample"
}

message MetadataAnalysis {
  double clickbait_score = 1;
  double keyword_stuffing_score = 2;
  double misleading_score = 3;
  repeated string flagged_keywords = 4;
  repeated string suggestions = 5;
  double overall_risk_score = 6;
}

message SynthesisResult {
  map<string, double> category_scores = 1;
  repeated Violation violations = 2;
  repeated string contexts = 3;
  double total_risk_score = 4;
  string recommendation = 5;  // "safe", "monetizable_with_caution", "demonetized", "strike_risk"
}

message Violation {
  ViolationCategory category = 1;
  Severity severity = 2;
  string description = 3;
  Timestamp timestamp = 4;
  double confidence = 5;
  string evidence = 6;
  string guideline_reference = 7;
}

// ===== Auto-Fix =====

message AutoFix {
  string fix_id = 1;
  string fix_type = 2;  // "smart_blur", "audio_replace", "metadata_rewrite", "frame_rate_adjust"
  Timestamp target_start = 3;
  Timestamp target_end = 4;
  map<string, double> parameters = 5;
  double estimated_effectiveness = 6;
  string description = 7;
}

message FixResult {
  string fix_id = 1;
  bool success = 2;
  string output_path = 3;
  double safety_margin = 4;  // % improvement
  double confidence = 5;
  string before_preview = 6;
  string after_preview = 7;
}

// ===== Main Report =====

message AnalysisReport {
  string job_id = 1;
  VideoSource source = 2;
  AnalysisStatus status = 3;
  double progress_percent = 4;
  
  VisualAnalysis visual = 5;
  AudioAnalysis audio = 6;
  MetadataAnalysis metadata = 7;
  SynthesisResult synthesis = 8;
  
  repeated AutoFix suggested_fixes = 9;
  repeated FixResult applied_fixes = 10;
  
  double strike_probability = 11;
  double demonetization_probability = 12;
  double copyright_claim_probability = 13;
  
  string report_html = 14;
  int64 processing_time_ms = 15;
}

// ===== Service APIs =====

service AnalysisService {
  rpc SubmitVideo(SubmitRequest) returns (SubmitResponse);
  rpc GetStatus(StatusRequest) returns (stream AnalysisReport);
  rpc GetReport(ReportRequest) returns (AnalysisReport);
  rpc ApplyFix(FixRequest) returns (FixResponse);
  rpc StreamProgress(ProgressRequest) returns (stream ProgressUpdate);
}

service FrameExtractionService {
  rpc ExtractFrames(ExtractRequest) returns (stream FrameBatch);
}

service AudioAnalysisService {
  rpc AnalyzeAudio(AudioRequest) returns (AudioAnalysis);
}

message SubmitRequest {
  VideoSource video = 1;
  bool enable_auto_fix = 2;
}

message SubmitResponse {
  string job_id = 1;
  AnalysisStatus initial_status = 2;
}

message StatusRequest {
  string job_id = 1;
}

message ReportRequest {
  string job_id = 1;
  bool include_fixes = 2;
}

message FixRequest {
  string job_id = 1;
  repeated string fix_ids = 2;
}

message FixResponse {
  bool accepted = 1;
  repeated FixResult results = 2;
}

message ProgressRequest {
  string job_id = 1;
}

message ProgressUpdate {
  AnalysisStatus status = 1;
  double progress_percent = 2;
  string current_stage = 3;
  string message = 4;
}

message ExtractRequest {
  string video_path = 1;
  int32 target_fps = 2;
  int32 max_resolution = 3;
}

message FrameBatch {
  repeated bytes frames = 1;  // Encoded frame data
  repeated int32 frame_numbers = 2;
  repeated double timestamps = 3;
}

message AudioRequest {
  string video_path = 1;
  bool enable_copyright_check = 2;
}
```

---

### 4. Module Specifications

#### 4.1 `sentinel-core` (Rust Library)
**Path:** `crates/sentinel-core/`
**Purpose:** Shared types, protobuf generated code, error types, utility functions

```rust
// Key exports:
pub mod proto { /* generated from sentinel.proto */ }
pub mod types {
    pub use proto::*;
    pub struct AnalysisJob { ... }
    pub struct PipelineConfig { ... }
}
pub mod error {
    pub enum SentinelError { ... }
}
pub mod util {
    pub fn hash_video(path: &Path) -> Result<String>;
    pub fn temp_dir() -> PathBuf;
    pub fn format_timestamp(seconds: f64) -> String;
}
```

#### 4.2 `frame-extractor` (Rust + Zig)
**Path:** `crates/frame-extractor/`
**Purpose:** Video decoding, frame extraction, scene detection

**Rust Interface:**
```rust
pub struct FrameExtractor {
    config: ExtractorConfig,
}

impl FrameExtractor {
    pub fn new(config: ExtractorConfig) -> Self;
    pub fn extract_frames(&self, video_path: &Path) -> Result<FrameStream>;
    pub fn detect_scenes(&self, video_path: &Path) -> Result<Vec<SceneTransition>>;
    pub fn detect_flashing(&self, frames: &FrameStream) -> Result<Vec<FlashingSegment>>;
}

pub struct ExtractorConfig {
    pub target_fps: f64,
    pub max_resolution: (u32, u32),
    pub gpu_acceleration: bool,  // Vulkan
    pub scene_threshold: f64,
}
```

**Zig FFI Interface:**
```zig
// zig/frame-analysis/src/main.zig
export fn analyze_frame_pixels(
    frame_data: [*]const u8,
    width: u32,
    height: u32,
    channels: u32,
    out_flash_score: *f32,
    out_color_variance: *f32,
) callconv(.C) c_int;

export fn compute_optical_flow_sse(
    prev_frame: [*]const u8,
    curr_frame: [*]const u8,
    width: u32,
    height: u32,
    out_flow_magnitude: *f32,
) callconv(.C) c_int;

export fn detect_rapid_transitions(
    frames: [*]const FrameDescriptor,
    frame_count: u32,
    threshold: f32,
    out_transitions: [*]TransitionEvent,
    max_transitions: u32,
    out_count: *u32,
) callconv(.C) c_int;
```

#### 4.3 `analysis-engine` (Rust)
**Path:** `crates/analysis-engine/`
**Purpose:** Multi-modal analysis orchestration

```rust
pub struct AnalysisEngine {
    visual: VisualAnalyzer,
    audio: AudioAnalyzer,
    metadata: MetadataAnalyzer,
}

impl AnalysisEngine {
    pub async fn analyze(&self, job: &AnalysisJob) -> Result<AnalysisResult>;
}

pub struct VisualAnalyzer {
    // Uses ONNX Runtime for model inference
    object_detector: Session,  // Custom YOLOv8
    ocr_engine: Session,       // Text detection
    scene_classifier: Session, // Context classification
}

pub struct AudioAnalyzer {
    speech_recognizer: Session,  // Whisper derivative
    audio_classifier: Session,   // Event detection
    fingerprint_db: FingerprintDB,
}

pub struct MetadataAnalyzer {
    clickbait_model: Session,
    keyword_analyzer: KeywordAnalyzer,
    policy_db: Arc<PolicyDatabase>,
}
```

#### 4.4 `policy-engine` (Rust)
**Path:** `crates/policy-engine/`
**Purpose:** Rule-based and ML-based policy matching

```rust
pub struct PolicyEngine {
    rules: Vec<PolicyRule>,
    ml_scorer: MlScorer,
    country_policies: HashMap<String, CountryPolicy>,
}

impl PolicyEngine {
    pub fn evaluate(&self, analysis: &AnalysisResult) -> Vec<Violation>;
    pub fn score_risk(&self, analysis: &AnalysisResult) -> f64;
    pub fn get_guideline_ref(&self, category: ViolationCategory) -> String;
}
```

#### 4.5 `synthesis` (Rust)
**Path:** `crates/synthesis/`
**Purpose:** Contextual synthesis across all modalities

```rust
pub struct SynthesisEngine {
    context_classifier: ContextClassifier,
}

impl SynthesisEngine {
    pub fn synthesize(&self, visual: &VisualAnalysis, audio: &AudioAnalysis, metadata: &MetadataAnalysis) -> SynthesisResult;
    pub fn classify_context(&self, elements: &[ContentElement]) -> ContextType;
}

pub enum ContextType {
    Educational,
    Documentary,
    Satire,
    News,
    Entertainment,
    Artistic,
    Gratuitous,
    Ambiguous,
}
```

#### 4.6 `autofix` (Rust + Zig)
**Path:** `crates/autofix/`
**Purpose:** Content remediation and fix validation

```rust
pub struct AutoFixEngine {
    blur_engine: BlurEngine,
    audio_engine: AudioFixEngine,
    metadata_engine: MetadataFixEngine,
    validator: FixValidator,
}

impl AutoFixEngine {
    pub fn suggest_fixes(&self, violations: &[Violation]) -> Vec<AutoFix>;
    pub fn apply_fix(&self, fix: &AutoFix, source: &Path) -> Result<FixResult>;
    pub fn validate_fix(&self, result: &FixResult) -> Result<ValidationReport>;
}
```

#### 4.7 `grpc-server` (Rust)
**Path:** `crates/grpc-server/`
**Purpose:** gRPC service implementation

Implements `AnalysisService`, `FrameExtractionService`, `AudioAnalysisService` from protobuf.

#### 4.8 `zig/frame-analysis`
**Path:** `zig/frame-analysis/`
**Purpose:** SIMD-optimized pixel analysis

- `build.zig` — Zig build config
- `src/pixel.zig` — Pixel-level analysis with SIMD
- `src/flash.zig` — Flashing detection
- `src/flow.zig` — Optical flow computation
- Compiles to static library linked by Rust frame-extractor

#### 4.9 `zig/audio-fingerprint`
**Path:** `zig/audio-fingerprint/`
**Purpose:** High-performance audio fingerprinting

- `build.zig` — Zig build config
- `src/spectrogram.zig` — Spectrogram computation
- `src/fingerprint.zig` — Chromaprint-compatible fingerprints
- `src/match.zig` — Fast fingerprint matching
- Compiles to static library linked by Rust analysis-engine

#### 4.10 `go/api-service`
**Path:** `go/api-service/`
**Purpose:** HTTP REST API + WebSocket streaming

```go
type Server struct {
    analysisClient pb.AnalysisServiceClient
    jobQueue       *JobQueue
    wsHub          *WebSocketHub
}

// Endpoints:
// POST /api/v1/analyze        — Submit video for analysis
// GET  /api/v1/jobs/:id       — Get job status
// GET  /api/v1/jobs/:id/report — Get full report
// POST /api/v1/jobs/:id/fix   — Apply auto-fix
// WS   /api/v1/jobs/:id/stream — Real-time progress
```

#### 4.11 `go/job-worker`
**Path:** `go/job-worker/`
**Purpose:** Background job processing

- Consumes from Redis/RabbitMQ queue
- Dispatches to Rust gRPC analysis services
- Handles job lifecycle and retries
- Progress tracking

#### 4.12 `wasm-ui`
**Path:** `wasm-ui/`
**Purpose:** Browser-based dashboard compiled to WASM

- Rust + Yew/Leptos framework
- Three-panel analysis view
- Real-time WebSocket updates
- Side-by-side fix comparison
- Export reports as HTML/PDF

---

### 5. Build System

**Top-level Makefile:**
```makefile
.PHONY: all rust zig go wasm proto docker test clean

all: proto rust zig go wasm

proto:
	mkdir -p crates/sentinel-core/src/proto
	protoc --rust_out=crates/sentinel-core/src/proto \
	       --grpc-web_out=wasm-ui/src/proto \
	       proto/sentinel.proto

rust:
	cd crates && cargo build --release

zig:
	cd zig/frame-analysis && zig build -Doptimize=ReleaseFast
	cd zig/audio-fingerprint && zig build -Doptimize=ReleaseFast

go:
	cd go/api-service && go build -o ../../bin/api-service
	cd go/job-worker && go build -o ../../bin/job-worker

wasm:
	cd wasm-ui && wasm-pack build --target web --release

docker:
	docker-compose -f docker/docker-compose.yml build

test:
	cd crates && cargo test --all
	cd zig/frame-analysis && zig build test
	cd zig/audio-fingerprint && zig build test
	cd go/api-service && go test ./...

clean:
	rm -rf bin/ target/ zig/**/zig-out/ zig/**/.zig-cache/
```

---

### 6. Docker Compose

```yaml
# docker/docker-compose.yml
version: '3.8'
services:
  api:
    build: ./api
    ports:
      - "8080:8080"
    environment:
      - GRPC_SERVER=analysis:50051
      - REDIS_URL=redis:6379
  
  analysis:
    build: ./analysis
    environment:
      - VULKAN_DEVICE=/dev/dri
    volumes:
      - video-uploads:/data/uploads
    deploy:
      resources:
        reservations:
          devices:
            - driver: nvidia
              count: 1
              capabilities: [gpu]
  
  worker:
    build: ./worker
    depends_on:
      - redis
      - analysis
  
  redis:
    image: redis:7-alpine
  
  nginx:
    image: nginx:alpine
    ports:
      - "80:80"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf
      - ../wasm-ui/pkg:/usr/share/nginx/html

volumes:
  video-uploads:
```

---

### 7. Testing Strategy

**Unit Tests:** Each crate/module has its own test suite
**Integration Tests:** `crates/integration-tests/` — End-to-end pipeline tests
**Benchmarks:** `crates/benches/` — Criterion.rs performance benchmarks

Key test scenarios:
- 50,000 previously demonetized videos (accuracy)
- 10,000 monetized educational videos (false positive)
- 1-hour 1080p video processed in ≤ 8 minutes (performance)
- Flashing content detection (seizure safety)
- Copyright audio fingerprint matching
- Cross-cultural policy validation (10 regions)

---

### 8. Security & Privacy

- Zero data leaves user's machine (local processing default)
- End-to-end encryption for cloud mode (TLS 1.3)
- No persistent storage — all analysis data purged after report
- Hardware-accelerated processing (Vulkan, not CUDA-locked)
- Memory-safe languages (Rust, Zig) prevent buffer overflows
- Content Security Policy on WASM UI

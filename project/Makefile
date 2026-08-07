# =============================================================================
# YouTube Sentinel - Makefile
# =============================================================================
# Build automation for the multi-language Sentinel project.
# Supports: Go, Rust, Zig, WASM, Protocol Buffers, Docker, and testing.
#
# Usage:
#   make all          # Build everything
#   make dev          # Start development environment
#   make test         # Run all test suites
#   make clean        # Remove all build artifacts
#
# Targets organized by category: build, test, docker, proto, utility
# =============================================================================

# ---------------------------------------------------------------------------
# Variables
# ---------------------------------------------------------------------------
VERSION     ?= $(shell git describe --tags --always --dirty 2>/dev/null || echo "dev")
GIT_COMMIT  ?= $(shell git rev-parse --short HEAD 2>/dev/null || echo "unknown")
GIT_BRANCH  ?= $(shell git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
BUILD_TIME  ?= $(shell date -u +"%Y-%m-%dT%H:%M:%SZ")

# Directories
ROOT_DIR    := $(shell pwd)
CRATES_DIR  := $(ROOT_DIR)/crates
CMD_DIR     := $(ROOT_DIR)/cmd
WEB_DIR     := $(ROOT_DIR)/web
PROTO_DIR   := $(ROOT_DIR)/proto
DIST_DIR    := $(ROOT_DIR)/dist
BUILD_DIR   := $(ROOT_DIR)/build

# Language-specific directories
API_DIR     := $(CMD_DIR)/api
WORKER_DIR  := $(CMD_DIR)/worker
ANALYSIS_DIR := $(CRATES_DIR)/analysis
SHARED_DIR  := $(CRATES_DIR)/shared

# Binaries
API_BIN     := $(DIST_DIR)/api-server
WORKER_BIN  := $(DIST_DIR)/worker-server
ANALYSIS_BIN := $(DIST_DIR)/sentinel-analysis

# Docker
COMPOSE     := docker-compose -f $(ROOT_DIR)/docker/docker-compose.yml
COMPOSE_PROD := $(COMPOSE) -f $(ROOT_DIR)/docker/docker-compose.prod.yml

# Colors
BLUE        := \033[34m
GREEN       := \033[32m
YELLOW      := \033[33m
RED         := \033[31m
CYAN        := \033[36m
BOLD        := \033[1m
RESET       := \033[0m

# Go build flags
GO_LDFLAGS  := -s -w \
    -X github.com/youtube-sentinel/internal/version.Version=$(VERSION) \
    -X github.com/youtube-sentinel/internal/version.BuildTime=$(BUILD_TIME) \
    -X github.com/youtube-sentinel/internal/version.GitCommit=$(GIT_COMMIT) \
    -X github.com/youtube-sentinel/internal/version.GitBranch=$(GIT_BRANCH) \
    -extldflags '-static'
GO_BUILD_FLAGS := -trimpath -ldflags "$(GO_LDFLAGS)"

# Rust build flags
RUSTFLAGS   ?= -C target-cpu=sandybridge -C opt-level=3
CARGO_BUILD_FLAGS := --release --locked

# ---------------------------------------------------------------------------
# Default Target
# ---------------------------------------------------------------------------
.PHONY: all
all: proto rust zig go wasm docker
	@echo "$(GREEN)$(BOLD)Build complete!$(RESET) Version: $(VERSION)"

# ---------------------------------------------------------------------------
# Help
# ---------------------------------------------------------------------------
.PHONY: help
help: ## Show this help message
	@echo "$(CYAN)$(BOLD)YouTube Sentinel - Build System$(RESET)"
	@echo "Version: $(VERSION) | Commit: $(GIT_COMMIT) | Branch: $(GIT_BRANCH)"
	@echo ""
	@echo "$(BOLD)Usage: make [target]$(RESET)"
	@echo ""
	@echo "$(BOLD)Build Targets:$(RESET)"
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  $(CYAN)%-20s$(RESET) %s\n", $$1, $$2}' | sort

# ###########################################################################
# PROTOCOL BUFFERS
# ###########################################################################

.PHONY: proto
proto: proto-go proto-rust proto-docs ## Generate protobuf code for all languages

.PHONY: proto-go
proto-go: ## Generate Go protobuf bindings
	@echo "$(BLUE)Generating Go protobuf bindings...$(RESET)"
	@mkdir -p $(PROTO_DIR)/go
	@protoc \
		--go_out=$(PROTO_DIR)/go \
		--go_opt=paths=source_relative \
		--go-grpc_out=$(PROTO_DIR)/go \
		--go-grpc_opt=paths=source_relative \
		--proto_path=$(PROTO_DIR) \
		$(PROTO_DIR)/*.proto
	@echo "$(GREEN)Go protobuf bindings generated$(RESET)"

.PHONY: proto-rust
proto-rust: ## Generate Rust protobuf bindings
	@echo "$(BLUE)Generating Rust protobuf bindings...$(RESET)"
	@cd $(ANALYSIS_DIR) && cargo build --features generate-proto 2>/dev/null || \
		echo "$(YELLOW)Proto generation via cargo build$(RESET)"
	@echo "$(GREEN)Rust protobuf bindings generated$(RESET)"

.PHONY: proto-docs
proto-docs: ## Generate protobuf documentation
	@echo "$(BLUE)Generating protobuf documentation...$(RESET)"
	@mkdir -p $(BUILD_DIR)/docs
	@protoc --doc_out=$(BUILD_DIR)/docs \
		--doc_opt=markdown,proto-api.md \
		--proto_path=$(PROTO_DIR) \
		$(PROTO_DIR)/*.proto 2>/dev/null || \
		echo "$(YELLOW)protoc-gen-doc not installed, skipping$(RESET)"
	@echo "$(GREEN)Proto documentation generated$(RESET)"

.PHONY: proto-lint
proto-lint: ## Lint protobuf definitions
	@echo "$(BLUE)Linting protobuf definitions...$(RESET)"
	@buf lint $(PROTO_DIR) 2>/dev/null || \
		echo "$(YELLOW)buf not installed, skipping lint$(RESET)"
	@echo "$(GREEN)Protobuf linting complete$(RESET)"

.PHONY: proto-breaking
proto-breaking: ## Check for breaking changes in protobuf
	@echo "$(BLUE)Checking for breaking protobuf changes...$(RESET)"
	@buf breaking --against '.git#branch=main' $(PROTO_DIR) 2>/dev/null || \
		echo "$(YELLOW)buf not installed, skipping$(RESET)"

# ###########################################################################
# RUST
# ###########################################################################

.PHONY: rust
rust: rust-analysis rust-shared ## Build all Rust crates

.PHONY: rust-analysis
rust-analysis: ## Build analysis service (Rust)
	@echo "$(BLUE)Building Rust analysis service...$(RESET)"
	@cd $(ANALYSIS_DIR) && RUSTFLAGS="$(RUSTFLAGS)" cargo build $(CARGO_BUILD_FLAGS)
	@mkdir -p $(DIST_DIR)
	@cp $(ANALYSIS_DIR)/target/release/sentinel-analysis $(ANALYSIS_BIN) 2>/dev/null || true
	@echo "$(GREEN)Analysis service built$(RESET)"

.PHONY: rust-shared
rust-shared: ## Build shared Rust library
	@echo "$(BLUE)Building Rust shared library...$(RESET)"
	@cd $(SHARED_DIR) && RUSTFLAGS="$(RUSTFLAGS)" cargo build $(CARGO_BUILD_FLAGS)
	@echo "$(GREEN)Shared library built$(RESET)"

.PHONY: rust-test
rust-test: ## Run Rust test suite
	@echo "$(BLUE)Running Rust tests...$(RESET)"
	@cd $(CRATES_DIR) && RUSTFLAGS="$(RUSTFLAGS)" cargo test --workspace --all-features
	@echo "$(GREEN)Rust tests passed$(RESET)"

.PHONY: rust-clippy
rust-clippy: ## Run Rust linter
	@echo "$(BLUE)Running Rust clippy...$(RESET)"
	@cd $(CRATES_DIR) && cargo clippy --workspace --all-targets --all-features -- -D warnings
	@echo "$(GREEN)Clippy check passed$(RESET)"

.PHONY: rust-fmt
rust-fmt: ## Format Rust code
	@echo "$(BLUE)Formatting Rust code...$(RESET)"
	@cd $(CRATES_DIR) && cargo fmt --all
	@echo "$(GREEN)Rust code formatted$(RESET)"

.PHONY: rust-check
rust-check: rust-fmt rust-clippy rust-test ## Full Rust CI check

.PHONY: rust-clean
rust-clean: ## Clean Rust build artifacts
	@echo "$(BLUE)Cleaning Rust artifacts...$(RESET)"
	@cd $(CRATES_DIR) && cargo clean
	@rm -f $(ANALYSIS_BIN)
	@echo "$(GREEN)Rust artifacts cleaned$(RESET)"

# ###########################################################################
# ZIG
# ###########################################################################

.PHONY: zig
zig: zig-encoder zig-decoder ## Build all Zig components

.PHONY: zig-encoder
zig-encoder: ## Build Zig encoder
	@echo "$(BLUE)Building Zig encoder...$(RESET)"
	@cd $(ROOT_DIR)/zig/encoder && zig build -Doptimize=ReleaseFast
	@mkdir -p $(DIST_DIR)
	@cp $(ROOT_DIR)/zig/encoder/zig-out/bin/encoder $(DIST_DIR)/encoder 2>/dev/null || true
	@echo "$(GREEN)Zig encoder built$(RESET)"

.PHONY: zig-decoder
zig-decoder: ## Build Zig decoder
	@echo "$(BLUE)Building Zig decoder...$(RESET)"
	@cd $(ROOT_DIR)/zig/decoder && zig build -Doptimize=ReleaseFast
	@mkdir -p $(DIST_DIR)
	@cp $(ROOT_DIR)/zig/decoder/zig-out/bin/decoder $(DIST_DIR)/decoder 2>/dev/null || true
	@echo "$(GREEN)Zig decoder built$(RESET)"

.PHONY: zig-test
zig-test: ## Run Zig test suite
	@echo "$(BLUE)Running Zig tests...$(RESET)"
	@cd $(ROOT_DIR)/zig/encoder && zig build test
	@cd $(ROOT_DIR)/zig/decoder && zig build test
	@echo "$(GREEN)Zig tests passed$(RESET)"

.PHONY: zig-clean
zig-clean: ## Clean Zig build artifacts
	@echo "$(BLUE)Cleaning Zig artifacts...$(RESET)"
	@cd $(ROOT_DIR)/zig/encoder && rm -rf zig-out zig-cache
	@cd $(ROOT_DIR)/zig/decoder && rm -rf zig-out zig-cache
	@rm -f $(DIST_DIR)/encoder $(DIST_DIR)/decoder
	@echo "$(GREEN)Zig artifacts cleaned$(RESET)"

# ###########################################################################
# GO
# ###########################################################################

.PHONY: go
## Build all Go services
go: go-api go-worker

.PHONY: go-api
go-api: proto-go ## Build Go API server
	@echo "$(BLUE)Building Go API server...$(RESET)"
	@mkdir -p $(DIST_DIR)
	@cd $(API_DIR) && CGO_ENABLED=0 GOOS=linux GOARCH=amd64 \
		go build $(GO_BUILD_FLAGS) -o $(API_BIN) .
	@echo "$(GREEN)API server built: $(API_BIN)$(RESET)"

.PHONY: go-worker
go-worker: proto-go ## Build Go worker
	@echo "$(BLUE)Building Go worker...$(RESET)"
	@mkdir -p $(DIST_DIR)
	@cd $(WORKER_DIR) && CGO_ENABLED=0 GOOS=linux GOARCH=amd64 \
		go build $(GO_BUILD_FLAGS) -o $(WORKER_BIN) .
	@echo "$(GREEN)Worker built: $(WORKER_BIN)$(RESET)"

.PHONY: go-test
go-test: ## Run Go test suite
	@echo "$(BLUE)Running Go tests...$(RESET)"
	@cd $(ROOT_DIR) && go test -race -coverprofile=coverage.out ./...
	@echo "$(GREEN)Go tests passed$(RESET)"

.PHONY: go-bench
go-bench: ## Run Go benchmarks
	@echo "$(BLUE)Running Go benchmarks...$(RESET)"
	@cd $(ROOT_DIR) && go test -bench=. -benchmem ./...
	@echo "$(GREEN)Benchmarks complete$(RESET)"

.PHONY: go-fmt
go-fmt: ## Format Go code
	@echo "$(BLUE)Formatting Go code...$(RESET)"
	@gofmt -w $(CMD_DIR) $(ROOT_DIR)/internal $(ROOT_DIR)/pkg
	@echo "$(GREEN)Go code formatted$(RESET)"

.PHONY: go-vet
go-vet: ## Run Go vet
	@echo "$(BLUE)Running go vet...$(RESET)"
	@cd $(ROOT_DIR) && go vet ./...
	@echo "$(GREEN)Go vet passed$(RESET)"

.PHONY: go-check
go-check: go-fmt go-vet go-test ## Full Go CI check

.PHONY: go-clean
go-clean: ## Clean Go build artifacts
	@echo "$(BLUE)Cleaning Go artifacts...$(RESET)"
	@rm -f $(API_BIN) $(WORKER_BIN)
	@rm -f $(ROOT_DIR)/coverage.out
	@cd $(ROOT_DIR) && go clean -cache
	@echo "$(GREEN)Go artifacts cleaned$(RESET)"

# ###########################################################################
# WASM
# ###########################################################################

.PHONY: wasm
wasm: wasm-build wasm-opt ## Build and optimize WASM module

.PHONY: wasm-build
wasm-build: ## Build WASM module with wasm-pack
	@echo "$(BLUE)Building WASM module...$(RESET)"
	@cd $(WEB_DIR) && wasm-pack build --target web --out-dir pkg --release
	@echo "$(GREEN)WASM module built$(RESET)"

.PHONY: wasm-dev
wasm-dev: ## Build WASM module in development mode
	@echo "$(BLUE)Building WASM module (dev)...$(RESET)"
	@cd $(WEB_DIR) && wasm-pack build --target web --out-dir pkg --dev
	@echo "$(GREEN)WASM module built (dev)$(RESET)"

.PHONY: wasm-test
wasm-test: ## Run WASM tests with wasm-pack
	@echo "$(BLUE)Running WASM tests...$(RESET)"
	@cd $(WEB_DIR) && wasm-pack test --headless --chrome 2>/dev/null || \
		wasm-pack test --node
	@echo "$(GREEN)WASM tests passed$(RESET)"

.PHONY: wasm-opt
wasm-opt: wasm-build ## Optimize WASM binary with wasm-opt
	@echo "$(BLUE)Optimizing WASM binary...$(RESET)"
	@wasm-opt -O4 -o $(WEB_DIR)/pkg/sentinel_web_bg_optimized.wasm \
		$(WEB_DIR)/pkg/sentinel_web_bg.wasm 2>/dev/null && \
		mv $(WEB_DIR)/pkg/sentinel_web_bg_optimized.wasm $(WEB_DIR)/pkg/sentinel_web_bg.wasm || \
		echo "$(YELLOW)wasm-opt not installed, skipping optimization$(RESET)"
	@echo "$(GREEN)WASM optimization complete$(RESET)"

.PHONY: wasm-clean
wasm-clean: ## Clean WASM build artifacts
	@echo "$(BLUE)Cleaning WASM artifacts...$(RESET)"
	@rm -rf $(WEB_DIR)/pkg
	@rm -rf $(WEB_DIR)/target
	@echo "$(GREEN)WASM artifacts cleaned$(RESET)"

# ###########################################################################
# DOCKER
# ###########################################################################

.PHONY: docker
docker: docker-build ## Build all Docker images

.PHONY: docker-build
docker-build: ## Build all Docker images via compose
	@echo "$(BLUE)Building Docker images...$(RESET)"
	@$(COMPOSE) build --parallel
	@echo "$(GREEN)Docker images built$(RESET)"

.PHONY: docker-push
docker-push: docker-build ## Push Docker images to registry
	@echo "$(BLUE)Pushing Docker images...$(RESET)"
	@$(COMPOSE) push
	@echo "$(GREEN)Docker images pushed$(RESET)"

.PHONY: docker-pull
docker-pull: ## Pull latest Docker images
	@echo "$(BLUE)Pulling Docker images...$(RESET)"
	@$(COMPOSE) pull
	@echo "$(GREEN)Docker images pulled$(RESET)"

.PHONY: docker-up
docker-up: ## Start all services with docker-compose
	@echo "$(BLUE)Starting services...$(RESET)"
	@$(COMPOSE) up -d
	@echo "$(GREEN)Services started$(RESET)"

.PHONY: docker-down
docker-down: ## Stop all services
	@echo "$(BLUE)Stopping services...$(RESET)"
	@$(COMPOSE) down
	@echo "$(GREEN)Services stopped$(RESET)"

.PHONY: docker-down-v
docker-down-v: ## Stop all services and remove volumes
	@echo "$(RED)Stopping services and removing volumes...$(RESET)"
	@$(COMPOSE) down -v
	@echo "$(GREEN)Services and volumes removed$(RESET)"

.PHONY: docker-logs
docker-logs: ## Show docker-compose logs
	@$(COMPOSE) logs -f --tail=100

.PHONY: docker-ps
docker-ps: ## Show running containers
	@$(COMPOSE) ps

.PHONY: docker-restart
docker-restart: ## Restart all services
	@echo "$(BLUE)Restarting services...$(RESET)"
	@$(COMPOSE) restart
	@echo "$(GREEN)Services restarted$(RESET)"

.PHONY: docker-shell-api
docker-shell-api: ## Open shell in API container
	@$(COMPOSE) exec api /bin/sh

.PHONY: docker-shell-analysis
docker-shell-analysis: ## Open shell in analysis container
	@$(COMPOSE) exec analysis /bin/bash

# ###########################################################################
# DEVELOPMENT
# ###########################################################################

.PHONY: dev
dev: ## Start development environment (docker-compose up)
	@echo "$(CYAN)$(BOLD)Starting development environment...$(RESET)"
	@$(COMPOSE) -f $(ROOT_DIR)/docker/docker-compose.yml \
		-f $(ROOT_DIR)/docker/docker-compose.dev.yml \
		up -d --build
	@echo "$(GREEN)Development environment ready!$(RESET)"
	@echo "  API:        http://localhost:8080"
	@echo "  Grafana:    http://localhost:3000"
	@echo "  Prometheus: http://localhost:9090"
	@echo "  Nginx:      http://localhost:80"

.PHONY: dev-down
dev-down: ## Stop development environment
	@$(COMPOSE) -f $(ROOT_DIR)/docker/docker-compose.yml \
		-f $(ROOT_DIR)/docker/docker-compose.dev.yml \
		down

.PHONY: watch
watch: ## Watch for changes and rebuild (requires air)
	@echo "$(CYAN)Watching for changes...$(RESET)"
	@cd $(API_DIR) && air -c $(API_DIR)/.air.toml &
	@cd $(WORKER_DIR) && air -c $(WORKER_DIR)/.air.worker.toml &
	@wait

# ###########################################################################
# TESTING
# ###########################################################################

.PHONY: test
test: proto rust-test zig-test go-test wasm-test ## Run all test suites
	@echo "$(GREEN)$(BOLD)All tests passed!$(RESET)"

.PHONY: test-integration
test-integration: ## Run integration tests
	@echo "$(BLUE)Running integration tests...$(RESET)"
	@cd $(ROOT_DIR)/tests/integration && go test -v ./...
	@echo "$(GREEN)Integration tests passed$(RESET)"

.PHONY: test-e2e
test-e2e: ## Run end-to-end tests
	@echo "$(BLUE)Running E2E tests...$(RESET)"
	@cd $(ROOT_DIR)/tests/e2e && go test -v ./...
	@echo "$(GREEN)E2E tests passed$(RESET)"

.PHONY: test-load
test-load: ## Run load tests with k6
	@echo "$(BLUE)Running load tests...$(RESET)"
	@k6 run $(ROOT_DIR)/tests/load/api-load.js 2>/dev/null || \
		echo "$(YELLOW)k6 not installed, skipping load tests$(RESET)"

.PHONY: coverage
coverage: go-test ## Generate coverage report
	@echo "$(BLUE)Generating coverage report...$(RESET)"
	@cd $(ROOT_DIR) && go tool cover -html=coverage.out -o $(BUILD_DIR)/coverage.html
	@echo "$(GREEN)Coverage report: $(BUILD_DIR)/coverage.html$(RESET)"

# ###########################################################################
# LINTING & FORMATTING
# ###########################################################################

.PHONY: lint
lint: go-vet rust-clippy proto-lint ## Run all linters
	@echo "$(GREEN)$(BOLD)Linting complete$(RESET)"

.PHONY: fmt
fmt: go-fmt rust-fmt ## Format all code
	@echo "$(GREEN)$(BOLD)Formatting complete$(RESET)"

.PHONY: check
check: rust-check go-check proto-lint ## Run full CI check
	@echo "$(GREEN)$(BOLD)All checks passed!$(RESET)"

# ###########################################################################
# SECURITY
# ###########################################################################

.PHONY: security
trivy: ## Run security scan with Trivy
	@echo "$(BLUE)Running security scan...$(RESET)"
	@trivy fs $(ROOT_DIR) 2>/dev/null || \
		echo "$(YELLOW)trivy not installed, skipping$(RESET)"

.PHONY: audit
audit: ## Run dependency audit
	@echo "$(BLUE)Running dependency audit...$(RESET)"
	@cd $(ROOT_DIR) && go list -json -deps ./... | nancy sleuth 2>/dev/null || \
		echo "$(YELLOW)nancy not installed, skipping Go audit$(RESET)"
	@cd $(CRATES_DIR) && cargo audit 2>/dev/null || \
		echo "$(YELLOW)cargo-audit not installed, skipping Rust audit$(RESET)"

# ###########################################################################
# DOCUMENTATION
# ###########################################################################

.PHONY: docs
docs: ## Build documentation
	@echo "$(BLUE)Building documentation...$(RESET)"
	@cd $(ROOT_DIR) && cargo doc --no-deps -p sentinel-analysis 2>/dev/null || true
	@cd $(ROOT_DIR) && godoc -http=:6060 &
	@echo "$(GREEN)Documentation built$(RESET)"

.PHONY: api-docs
api-docs: ## Serve API documentation
	@echo "$(CYAN)API docs available at http://localhost:8080/docs$(RESET)"

# ###########################################################################
# DATABASE
# ###########################################################################

.PHONY: migrate-up
migrate-up: ## Run database migrations up
	@echo "$(BLUE)Running migrations up...$(RESET)"
	@migrate -path $(ROOT_DIR)/migrations -database "$(DB_DSN)" up

.PHONY: migrate-down
migrate-down: ## Rollback database migrations
	@echo "$(BLUE)Running migrations down...$(RESET)"
	@migrate -path $(ROOT_DIR)/migrations -database "$(DB_DSN)" down 1

.PHONY: migrate-create
migrate-create: ## Create a new migration (usage: make migrate-create name=add_users)
	@migrate create -ext sql -dir $(ROOT_DIR)/migrations -seq $(name)

# ###########################################################################
# CLEANING
# ###########################################################################

.PHONY: clean
clean: go-clean rust-clean zig-clean wasm-clean ## Remove all build artifacts
	@echo "$(BLUE)Cleaning all build artifacts...$(RESET)"
	@rm -rf $(DIST_DIR) $(BUILD_DIR)
	@rm -rf $(ROOT_DIR)/coverage.out
	@echo "$(GREEN)All artifacts cleaned$(RESET)"

.PHONY: distclean
distclean: clean docker-down-v ## Deep clean including Docker volumes
	@echo "$(RED)Deep cleaning everything...$(RESET)"
	@docker system prune -f 2>/dev/null || true
	@echo "$(GREEN)Deep clean complete$(RESET)"

# ###########################################################################
# RELEASE
# ###########################################################################

.PHONY: release
release: ## Create a new release (usage: make release version=v1.0.0)
	@echo "$(CYAN)Creating release $(version)...$(RESET)"
	@git tag -a $(version) -m "Release $(version)"
	@git push origin $(version)
	@echo "$(GREEN)Release $(version) created$(RESET)"

.PHONY: changelog
changelog: ## Generate changelog
	@echo "$(BLUE)Generating changelog...$(RESET)"
	@git log --pretty=format:"- %s (%h)" --no-merges $$(git describe --tags --abbrev=0)..HEAD

# ###########################################################################
# INFO
# ###########################################################################

.PHONY: version
version: ## Show build version info
	@echo "$(BOLD)YouTube Sentinel$(RESET)"
	@echo "  Version:    $(VERSION)"
	@echo "  Commit:     $(GIT_COMMIT)"
	@echo "  Branch:     $(GIT_BRANCH)"
	@echo "  Build Time: $(BUILD_TIME)"

.PHONY: env
env: ## Show environment info
	@echo "$(BOLD)Environment$(RESET)"
	@echo "  Go:      $$(go version 2>/dev/null || echo 'not installed')"
	@echo "  Rust:    $$(rustc --version 2>/dev/null || echo 'not installed')"
	@echo "  Zig:     $$(zig version 2>/dev/null || echo 'not installed')"
	@echo "  Node:    $$(node --version 2>/dev/null || echo 'not installed')"
	@echo "  Docker:  $$(docker --version 2>/dev/null || echo 'not installed')"
	@echo "  Compose: $$(docker-compose --version 2>/dev/null || echo 'not installed')"
	@echo "  Protoc:  $$(protoc --version 2>/dev/null || echo 'not installed')"

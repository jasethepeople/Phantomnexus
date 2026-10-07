# Phantomnexus (YouTube Sentinel)

Everything in this repo — `SPEC.md`, `plan.md`, and the project README — names the project **YouTube Sentinel**: a polyglot video pre-moderation analysis engine that scans videos before upload to flag demonetization, guideline-strike, and copyright risk. The repo holds the spec, the build plan, and an in-progress multi-language implementation.

## Features (per the spec and code present)

- **Analysis pipeline** (Rust crates): frame extraction (scene/flash detection), visual/audio/metadata analysis engines, contextual synthesis, and an auto-fix engine (blur, audio, frame-rate, ffmpeg, metadata fixes)
- **Go API service**: HTTP REST API with upload/analyze routes, WebSocket progress streaming, gRPC client, middleware (auth, CORS, logging, rate-limit)
- **Zig components**: audio fingerprinting and frame-analysis (pixel/flow/flash) modules with `build.zig` files
- **wasm-ui**: WebAssembly frontend project (directory present)
- **Marketing/dashboard web app** (`app/`): React + Vite + shadcn site with Home, Dashboard, Pricing, and Analysis pages
- Job orchestration and policy-matching architecture per `SPEC.md`

## Tech stack

- Rust (7 workspace crates), Go, Zig, TypeScript/React (Vite, Tailwind, shadcn) for the web app
- Build: Cargo workspace, `project/Makefile`, Go modules, Zig build files
- Docker configs under `project/docker`

## Getting started

See `project/README.md` for the polyglot build prerequisites (Rust 1.75, Go 1.21, Zig 0.11 per its badges) and `project/Makefile` for build targets; the web app in `app/` follows the standard Vite flow (`npm run dev`, `npm run build`).

## Project structure

```
├── SPEC.md, plan.md        # product spec and staged build plan
├── project/
│   ├── crates/             # 7 Rust crates: sentinel-core, frame-extractor,
│   │                       #   analysis-engine, policy-engine, synthesis,
│   │                       #   autofix, grpc-server
│   ├── go/api-service/     # Go REST + WebSocket API
│   ├── zig/                # audio-fingerprint, frame-analysis
│   ├── wasm-ui/            # WebAssembly frontend
│   ├── docker/, Makefile, README.md
└── app/                    # React marketing/dashboard site
```

## Status

**Real project, in active development.** The multi-language structure from the spec is genuinely present (Rust crates, Go service, Zig modules all contain source). The build plan's performance targets (e.g. 1h of 1080p in ≤8 min, 99.95% accuracy) are spec goals, not measured results. Authorship is not documented in this repo — badges and crate metadata reference `github.com/youtube-sentinel/sentinel` and a "YouTube Sentinel Team" rather than this account; no LICENSE file is present at the repo root.

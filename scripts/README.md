# Scripts

Testing, build, and development scripts for tsz.

## Directory Structure

```
scripts/
  conformance/     # Conformance testing, analysis, and data
  bench/           # Benchmarking and performance
  build/           # Build, WASM, and publishing
  ci/              # CI-specific scripts
  emit/            # Emit test harness (JS + DTS output)
  fourslash/       # Language service fourslash tests
  githooks/        # Git hooks (pre-commit, pre-push, etc.)
  setup/           # Setup, cleanup, and submodule management
  arch/            # Architecture boundary guardrails and tests
```

## Key Scripts

### Conformance Testing
| Script | Purpose |
|--------|---------|
| `scripts/conformance/conformance.sh` | Run type checker conformance tests |
| `scripts/conformance/query-conformance.py` | Offline conformance analysis and querying |
| `scripts/conformance/conformance-snapshot.json` | Snapshot aggregates (offline analysis) |
| `scripts/conformance/conformance-detail.json` | Per-test failure data (offline analysis) |
| `scripts/conformance/tsc-cache-full.json` | TSC expected diagnostics cache |

### Emit & Fourslash
| Script | Purpose |
|--------|---------|
| `scripts/emit/run.sh` | Run emit tests (JS + declaration output) |
| `scripts/fourslash/run-fourslash.sh` | Run language service fourslash tests |

### Benchmarking
| Script | Purpose |
|--------|---------|
| `scripts/bench/bench-vs-tsgo.sh` | Comparative benchmark (tsz vs tsgo) |
| `scripts/bench/perf-hotspots.sh` | Targeted hotspot profiling |
| `scripts/bench/precommit-microbench.sh` | Pre-commit regression gate |
| `scripts/ci/bench-compare.sh` | PR benchmark comparison (CI) |

### Build & Publishing
| Script | Purpose |
|--------|---------|
| `scripts/build/build-wasm.sh` | Build WASM module |
| `scripts/build/build-npm-packages.sh` | Assemble npm packages |
| `scripts/build/publish-crates.sh` | Publish to crates.io |
| `scripts/build/publish-npm.sh` | Publish to npm |

### Architecture & Linting
| Script | Purpose |
|--------|---------|
| `scripts/arch/arch_guard.py` | Architecture boundary violation detection |
| `scripts/arch/check-checker-boundaries.sh` | Checker boundary enforcement |
| `scripts/arch/render_architecture_report.py` | Render architecture guard markdown report |

### Setup & Maintenance
| Script | Purpose |
|--------|---------|
| `scripts/setup/setup.sh` | One-stop setup (submodule, deps, hooks) |
| `scripts/setup/clean.sh` | Build artifact cleanup |
| `scripts/setup/reset-ts-submodule.sh` | Reset TypeScript submodule to pinned SHA |

### Other
| Script | Purpose |
|--------|---------|
| `scripts/gen_diagnostics.mjs` | Generate diagnostic code data |
| `scripts/start-website.sh` | Local website preview |

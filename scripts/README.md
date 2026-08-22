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

### Emit Testing & Analysis
| Script | Purpose |
|--------|---------|
| `scripts/emit/run.sh` | Run emit tests (JS + declaration output) |
| `scripts/emit/run.sh --json-out` | Run emit tests and write `emit-detail.json` |
| `scripts/emit/query-emit.py` | Offline emit results analysis and querying |
| `scripts/emit/query-emit.py --families` | JS/DTS failure-family dashboard |
| `scripts/emit/emit-detail.json` | Per-test emit results (offline analysis) |

### Fourslash Testing & Analysis
| Script | Purpose |
|--------|---------|
| `scripts/fourslash/run-fourslash.sh` | Run language service fourslash tests |
| `scripts/fourslash/run-fourslash.sh ... --json-out` | Run fourslash tests and write `fourslash-snapshot.json` |
| `scripts/fourslash/query-fourslash.py` | Offline fourslash results analysis and querying |
| `scripts/fourslash/fourslash-snapshot.json` | Compact checked-in fourslash snapshot (offline analysis) |

### README Status Contract
| Script | Purpose |
|--------|---------|
| `scripts/refresh-readme.py --check` | Reject retired live-dashboard markers and validate the clean-slate R0 status |
| `scripts/refresh-readme.py --write` | Repair only the managed R0 status block; never imports suite or benchmark artifacts |

### Benchmarking
| Script | Purpose |
|--------|---------|
| `scripts/bench/bench-vs-tsgo.sh` | Comparative benchmark (tsz vs tsgo) |
| `scripts/bench/readme-perf-svg.mjs` | Render the README performance chart SVG or light/dark PNGs from a benchmark artifact |
| `scripts/bench/perf-hotspots.sh` | Targeted hotspot profiling |
| `scripts/bench/precommit-microbench.sh` | Pre-commit regression gate |
| `scripts/ci/bench-compare.sh` | PR benchmark comparison (CI) |

### Build & Publishing
| Script | Purpose |
|--------|---------|
| `scripts/build/build-wasm.sh` | Fail-closed R4 WASM availability gate |
| `scripts/build/build-npm-packages.sh` | Assemble private native R0 packages for inspection |
| `scripts/build/publish-crates.sh` | Fail-closed crates.io publication gate |
| `scripts/build/publish-npm.sh` | Fail-closed npm publication gate |

### Architecture & Linting
| Script | Purpose |
|--------|---------|
| `scripts/arch/arch_guard.py` | Enforce clean-slate workspace, dependency, size, anti-hardcoding, and rewrite-debt ratchets |
| `scripts/arch/rewrite_architecture_metrics.py --check` | Report and verify no growth in mirrored capability, suppression, forcing, recursion, collection, and near-cap-module debt |
| `python3 -m unittest discover -s scripts/arch -p 'test_*.py'` | Exercise the reset architecture guard contract |

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

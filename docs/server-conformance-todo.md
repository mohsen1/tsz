# Server-Mode Conformance: Remaining Work

## Current State (2026-03-24)

Server mode (`--mode server`) passes **44.4%** of conformance tests vs CLI's **89.5%**, at **7.5x** the speed (1m 13s vs 9m 12s).

## Usage

```bash
# Server mode (fast, lower parity)
./scripts/conformance/conformance.sh run --mode server

# CLI mode (default, full parity)
./scripts/conformance/conformance.sh run
```

## Remaining Parity Gaps

### High Impact

**TS2339 extra (~200+)** — "Property does not exist on type"
- Root cause: server's `run_check` lib symbol merging still doesn't expose all global types identically to the CLI pipeline. The two-phase lib loading fix resolved Set/Map/Array.from but some edge cases remain.
- Fix: compare `run_check`'s project environment setup with `driver::compile` to find remaining differences in how lib binders are integrated.

**TS2322/TS2345 extra (~100+)** — Type assignability false positives
- Root cause: likely downstream from missing global types — when a type resolves to `error`/`any` due to missing lib symbols, assignability checks produce different results.
- Fix: resolves automatically once lib loading gap is fully closed.

**TS2503 extra (~50+)** — "Cannot find namespace"
- Root cause: namespace declarations from lib files not properly merged into the unified binder's global scope.
- Fix: investigate `merge_lib_contexts_into_binder` namespace handling.

**TS2307 missing (~80+)** — "Cannot find module"
- Root cause: server mode sends files inline without a filesystem. Module resolution (`import './foo'`) can't resolve relative paths since there's no directory structure.
- Fix options:
  1. For multi-file tests, construct virtual module resolution from the file map keys
  2. Fall back to CLI mode for tests with import statements (conservative)
  3. Add virtual filesystem support to the server's check pipeline

### Medium Impact

**TS7006/TS7010/TS7008 missing (~50+)** — Implicit any / missing return type
- Root cause: these require `noImplicitAny` / `noImplicitReturns` which tsc's test harness enables for specific tests via `@noImplicitAny: true` directives. The directive-to-options conversion handles these, but some tests rely on harness-level defaults that differ from the server.
- Fix: audit tsc test harness default compiler options and replicate them in `options_convert.rs`.

**TS2454 missing (~30+)** — "Variable used before being assigned"
- Root cause: requires `strictNullChecks: true` which we now default, but some tests explicitly set `@strict: false` which disables it.
- Fix: may already be correct — verify these tests actually expect TS2454 with `@strict: false`.

**Fingerprint-only failures (~200+)** — Error codes match but positions/messages differ
- Root cause: the server's legacy protocol returns only error codes, not positions. All fingerprint comparison produces zero matches in server mode.
- Fix: extend the legacy protocol to optionally return diagnostic positions and messages. Add a `"fingerprints": true` option to the check request.

### Low Impact

**TS5107/TS5101** — Config deprecation warnings (already filtered)
- These are tsconfig.json-level warnings the server fundamentally cannot emit. Already handled by filtering in comparison.

**TS2307 for `@filename` multi-file tests** — Module not found
- Tests with `@filename: /other.ts` create virtual multi-file projects. The server receives files inline but doesn't set up module resolution between them.
- Fix: build module resolution maps from the file name keys in the server's `run_check`.

### CheckOptions Gaps

The server's `CheckOptions` struct doesn't support these compiler options, causing tests that use them to fall back to CLI mode:

| Option | Tests Affected | Difficulty |
|--------|---------------|-----------|
| `jsx` / `jsxFactory` / `jsxImportSource` | ~500 | Medium — add fields to CheckOptions, pass to checker |
| `moduleResolution` | ~200 | Medium — affects module resolver behavior |
| `paths` / `baseUrl` | ~100 | Hard — requires virtual filesystem path mapping |
| `types` / `typeRoots` | ~50 | Medium — affects type declaration resolution |

## Architecture Notes

- `ServerPool` (`crates/conformance/src/server_pool.rs`) manages N `tsz-server --protocol legacy` processes
- `options_convert.rs` bridges test directives → server CheckOptions JSON
- `runner.rs` uses server pool when `--mode server` and falls back to CLI for unsupported options
- The server fix in `check.rs` aligned `run_check` with `get_semantic_diagnostics_full`'s two-phase lib loading

## Metrics Tracking

Run this to compare modes:
```bash
echo "=== CLI ===" && ./scripts/conformance/conformance.sh run --mode cli 2>&1 | grep "FINAL"
echo "=== SERVER ===" && ./scripts/conformance/conformance.sh run --mode server 2>&1 | grep "FINAL"
```

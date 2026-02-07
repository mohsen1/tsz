# Incremental LSP Plan

**Date**: 2026-02-07
**Status**: Accepted
**Supersedes**: `salsa-incremental-lsp.md` (kept as research archive)

---

## The Principle: Complexity Is the Enemy

tsz already beats tsgo on all batch benchmarks. Our raw speed is proven.
The LSP strategy is: **leverage that speed, don't add complexity to avoid using it.**

Every feature below is evaluated by: *does the user-visible benefit justify the
maintenance cost and bug surface area?*

---

## Benchmark Data

| File Size | Full Pipeline (Parse+Bind+Check) |
|-----------|----------------------------------|
| 330 lines | 0.7ms |
| 660 lines | 1.3ms |
| 1,322 lines | 2.5ms |
| 2,642 lines | 4.6ms |
| 5,282 lines | 8.6ms |

At 5ms per file, we can re-check 200 files per second. A 50-file project re-checks
entirely in 250ms. A 500-file project in 2.5 seconds. These are the real constraints.

---

## Step 0: Remove Old Salsa Experiment

| File | Action |
|------|--------|
| `crates/tsz-solver/src/salsa_db.rs` | Delete |
| `crates/tsz-solver/src/lib.rs` | Remove `experimental_salsa` module + re-export |
| `crates/tsz-solver/src/tests/db_tests.rs` | Remove 3 salsa test functions |
| `crates/tsz-solver/Cargo.toml` | Remove `salsa` dep + feature |
| `Cargo.toml` (workspace) | Remove `salsa = "0.16"` + `experimental_salsa` feature |

---

## The Plan: Three Steps

### Step 1: Raw Speed (ongoing)

Keep making the checker faster. This is permanent value — every millisecond saved
makes every other optimization less necessary.

- [ ] Fix O(n²) canonicalizer from perf audit
- [ ] Optimize `is_subtype_of` hot path
- [ ] Profile with `samply`, fix what the data shows
- [ ] Benchmark with `benches/phase_timing_bench.rs`
- [ ] Run `bench-vs-tsgo.sh` periodically to maintain the lead

**Complexity cost**: Zero new abstractions. Just making existing code faster.

### Step 2: Global TypeInterner (required for cross-file)

Move `TypeInterner` from per-file to shared on `Project`. This is the minimum
prerequisite for cross-file type checking.

- [ ] Move `TypeInterner` from `ProjectFile` to `Project`
- [ ] Pass shared `&TypeInterner` into each file's checker
- [ ] Share `QueryCache` across file checks (long-lived, keyed by TypeId+flags)
- [ ] Add interner size monitor: if `len() > threshold`, reset interner + clear all caches
      (simple "flush" strategy — happens maybe once every few hours, causes ~200ms blip)

**Complexity cost**: Ownership refactor in `project.rs`. ~100-200 lines changed.
No new crates, no new abstractions. The "flush on high watermark" strategy is ~20 lines
and avoids the entire scoped interner complexity.

**What this enables**: `TypeId(100)` means the same thing in every file. Cross-file
type resolution becomes possible.

### Step 3: Cross-File Type Resolution

Enable the checker to resolve imported types from other files.

- [ ] Implement `ImportResolver` trait (or callback) that CheckerState can call
- [ ] When file A encounters `import { Foo } from './b'`, ask Project for B's export type
- [ ] Project looks up B's cached exports (`HashMap<String, TypeId>`)
- [ ] If B hasn't been checked yet, check B first (lazy pull)
- [ ] Store export types per file after each check
- [ ] Use existing `DependencyGraph` to track who imports whom
- [ ] On file change: re-check the file + all reverse dependencies (brute force)

**Complexity cost**: New trait + wiring in checker. ~300-500 lines.
No smart invalidation. No ExportSignature hashing. Just "if B changed, re-check
everyone who imports B." At <10ms per file, this is fast enough.

**When this breaks down**: Projects with 500+ files where every edit triggers a cascade.
That's the point to add ExportSignature (see "Future" below). Not before.

---

## What We Are NOT Doing (And Why)

| Feature | Why Not | When to Reconsider |
|---------|---------|-------------------|
| **Salsa** | 10x complexity, fights arena architecture, solves a problem we don't have | Never (unless we rewrite the entire compiler) |
| **Scoped TypeInterner** (MSB split) | Complex lifecycle management, "zombie type" bugs, lift operation. Simple "flush on high watermark" is sufficient | If flush happens too frequently (measure first) |
| **ExportSignature** | "Hardest design problem" — SymbolId instability, inferred exports are circular. Brute-force re-checking is fast enough | When projects >500 files show >1s latency |
| **Declaration-level checking** | 5 mandatory file-level setup steps negate the benefit. Full file check is already <10ms | When individual files >10K lines cause latency |
| **Two-tier async response** | Synchronous re-check at <10ms doesn't need async. Adds race conditions | When P95 latency exceeds 100ms |
| **Incremental parsing** | Full reparse is <2ms even for large files | When parsing becomes the measured bottleneck |

---

## LSP Behavior

**On keystroke** (`textDocument/didChange`):
1. Full reparse + rebind + recheck the active file (~5ms)
2. Return diagnostics immediately

**On save / idle** (debounced 300-500ms):
1. Re-check the active file
2. Re-check all files that import the active file (brute force)
3. Push updated diagnostics for affected files

**On file open**:
1. Full check of the opened file
2. If it was marked dirty by a dependency change, user sees fresh diagnostics

**Memory management**:
- Monitor `TypeInterner.len()` 
- If over threshold (e.g., 3M types), flush: reset interner, clear all caches
- Next check rebuilds everything from scratch (~200ms one-time cost)
- Happens rarely in practice (every few hours of active typing)

---

## Success Criteria

- [ ] Single-file diagnostics: <10ms (already achieved)
- [ ] Cross-file type resolution: correct for imports/exports
- [ ] 50-file project edit: <500ms total for active file + dependents
- [ ] 8-hour session: no OOM, no zombie state
- [ ] Zero new crate dependencies
- [ ] Checker and solver code: unchanged

---

## Future (Only If Data Demands It)

**ExportSignature** — position-independent hash of a file's public API to avoid
re-checking importers when only function bodies changed. Add this when:
- Projects >500 files show cross-file re-check latency >1 second
- Design: extract exported names + annotation hashes from binder, ignoring SymbolIds
- See `salsa-incremental-lsp.md` for full design exploration

**Scoped TypeInterner** — MSB-split global/local interner for precise memory management.
Add this when:
- The "flush on high watermark" strategy causes visible latency spikes
- Design: MSB=0 global, MSB=1 local, lift on export. See research archive.

**Parallel checking** — use Rayon to check independent files in parallel. Add this when:
- Cross-file re-checking of 100+ files is the measured bottleneck
- Requires making CheckerState thread-safe or using per-thread instances

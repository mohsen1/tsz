# Conformance Harness Improvements

Investigation date: 2026-02-22
Source: Running tsgo (v7.0.0-dev.20260130) through our conformance harness

## Background

We ran tsgo through our conformance test suite to compare results. tsgo scored
39.6% raw, but after analysis, **~87% of tests actually pass** — the harness
has bugs that inflate failure counts.

### Score Comparison

| Compiler | Raw Score | Path-Adjusted |
|----------|-----------|---------------|
| tsz      | ~71.1%    | ~71.1% (not affected since tsz normalizes paths) |
| tsgo     | 39.6%     | **~87.2%** |
| tsc      | 100%      | 100% (cache baseline) |

## Action Items

### P0: Fix file path normalization in fingerprint comparison

**Impact**: ~5,979 false failures for tsgo (78.8% of all tsgo failures).
Also likely affects tsz to some degree.

**Root cause**: The cache generator (`generate-tsc-cache`) runs tsc with
`.current_dir(work_dir)` so tsc outputs `test.ts` (relative to project root).
The conformance runner does NOT set `.current_dir()` on subprocess invocations,
so the binary runs from the repo root and outputs paths like
`../../../../var/folders/.../test.ts` (relative to repo cwd).

`normalize_diagnostic_path()` in `tsz_wrapper.rs:410` only handles absolute
paths — it strips the project root prefix. But relative paths with `../`
components are not normalized.

**Fix options (pick one)**:
1. **Set `.current_dir(project_dir)` on all subprocess invocations in runner.rs**
   (lines 585, 818, 976). This matches what the cache generator does and is the
   simplest fix. Need to also update batch pool spawning in `batch_pool.rs`.
2. **Resolve relative paths in `normalize_diagnostic_path()`**: Canonicalize the
   diagnostic path against the subprocess cwd, then strip the project root.
3. **Both**: Set cwd AND improve normalization for robustness.

**Files**: `crates/conformance/src/runner.rs`, `crates/conformance/src/tsz_wrapper.rs`

### P1: Remove `--batch` from tsz CLI

**Problem**: The `--batch` flag on the tsz binary exists solely for the
conformance runner to reuse long-running processes. This is a testing concern
leaking into the production CLI.

**Current behavior**: `tsz --batch` reads project directories from stdin, outputs
diagnostics, then emits `---TSZ-BATCH-DONE---` sentinel per compilation.

**Fix options**:
1. **Remove `--batch` from tsz CLI entirely**. Accept per-process overhead in
   conformance runs (~12K spawns). With 16 workers on macOS, the full suite
   takes ~166s in subprocess mode (tsgo measurement), which is acceptable.
2. **Move batch protocol to a separate binary** (e.g., `tsz-batch-server`) that
   the conformance runner owns. This keeps the testing infrastructure out of the
   production binary.
3. **Use stdin/stdout IPC without a CLI flag**: The conformance runner could
   spawn a `tsz` process, pipe project dirs to stdin, and read results from
   stdout — but this requires tsz to support a REPL-like mode, which is worse.

**Recommendation**: Option 1 (just remove it). The perf impact is small
(~2-3min conformance run vs ~1-2min with batch mode) and keeps the CLI clean.

**Files**: `crates/tsz-cli/src/bin/tsz.rs`, `crates/conformance/src/batch_pool.rs`,
`crates/conformance/src/runner.rs`, `crates/conformance/src/cli.rs`

### P1: TS5108 false positives (tsgo-specific, but reveals harness issue)

**Impact**: 423 extra TS5108 errors in tsgo run.

**TS5108**: "Option 'allowImportingTsExtensions' can only be used when either
'noEmit' or 'declaration' is set." tsgo emits this even though `--noEmit` is
passed on the command line — it may not merge CLI flags with tsconfig options
the same way tsc does.

**Harness implication**: Our tsconfig generation may be incomplete. If we're
relying on CLI `--noEmit` but the tsconfig doesn't include it, compilers that
read only from tsconfig (not CLI) will fail.

**Fix**: Add `"noEmit": true` to the generated tsconfig.json in all cases,
not just passing it via CLI. Check `crates/conformance/src/tsz_wrapper.rs`
where tsconfig is generated.

### P1: TS6053 file not found (180 failures)

**Impact**: tsgo can't find `.d.ts` stub files (react.d.ts, react16.d.ts).

**Root cause**: The conformance harness creates symlinks or copies for lib files,
but tsgo may resolve paths differently. The `/.lib/react.d.ts` path suggests
an absolute path that doesn't exist on the filesystem.

**Fix**: Investigate how lib stubs are set up in `tsz_wrapper.rs` and ensure
they're accessible to any compiler, not just tsz.

**Files**: `crates/conformance/src/tsz_wrapper.rs` (search for react, lib stubs)

### P2: TS18003 "No inputs were found" (59 failures)

**Impact**: tsgo reports no inputs in tests that tsc handles fine.

**Root cause**: The generated tsconfig.json include patterns may not match how
tsgo resolves files. Our include patterns are
`["*.ts","*.tsx","*.js","*.jsx","**/*.ts","**/*.tsx","**/*.js","**/*.jsx"]`.
tsgo may handle glob patterns differently in temp directories.

**Fix**: Investigate tsgo's project root resolution with `--project` flag.

### P2: TS5081 "Cannot find tsconfig.json" (52 failures)

**Impact**: tsgo can't find the tsconfig.json that the harness creates.

**Root cause**: Related to the cwd issue. When `--project /tmp/dir` is used,
tsgo may look for tsconfig relative to cwd, not the --project argument.

**Fix**: Same as P0 — setting `.current_dir()` should resolve this.

### P2: TS5102/TS5103 config option differences (141 failures)

**Impact**: tsgo handles deprecated/removed compiler options differently.

**Root cause**: tsgo (v7) may have removed options that tsc (v6 nightly) still
supports, or vice versa. The `ignoreDeprecations` flag handling differs.

**Fix**: Not a harness issue — genuine version/implementation difference.

### P3: Real type-checking differences (~189 estimated)

**Impact**: ~2.5% of failures are genuine diagnostic differences.

These are cases where tsgo produces different error messages, different error
codes at the same location, or extra/missing diagnostics beyond path issues.
Examples include:
- Rest parameter expansion differences (`[f?: any, ...any[]]` vs `[f?: any]`)
- Minor message text variations

**Fix**: Low priority. These represent actual tsgo bugs or version differences.

## Metrics After Fixes

If P0 (path normalization) is fixed:
- tsgo effective score: ~87% (up from 39.6%)
- tsz score: likely improves slightly too (any path normalization bugs fixed)
- Our harness becomes usable for testing any tsc-compatible compiler

## Notes

- tsgo version tested: 7.0.0-dev.20260130.1
- tsc cache version: 6.0.0-dev.20260215
- Version mismatch between tsgo (Jan 30) and tsc cache (Feb 15) may account
  for some real differences
- tsgo CLI is tsc-compatible: accepts `--project`, `--noEmit`, `--pretty false`
  and outputs the same diagnostic format `file(line,col): error TSXXXX: message`

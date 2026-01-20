# Conformance Recovery Plan

## Snapshot (12,053 tests, 2026-01-19)
- Pass rate: **24.7%** (2,983 / 12,053)
- Speed: **106 tests/sec**
- Failures: 8,111
- Crashes: 865 | OOM: 37 | Timeouts: 57
- Top missing: TS2304 (4,764x), TS7053 (2,458x), TS2792 (2,377x), TS2339 (2,147x), TS2583 (1,882x), TS2488 (1,571x)
- Top extra: TS2304 (393,322x), TS2322 (11,939x), TS1005 (3,473x), TS2571 (3,137x), TS2694 (3,105x)

## Findings (root-cause hypotheses)
- **Directive/options regression in workers**: `conformance/src/worker.ts` now applies only `{ strict, target, module, noEmit, skipLibCheck }`; it ignores `@allowJs/@checkJs/@noLib/@lib/@moduleResolution/@module/@jsx/@useDefineForClassFields/@experimentalDecorators` etc. Both the TSC side and WASM side run with the wrong options, producing massive TS2304/TS7053 deltas and JS/JSDoc crashes.
- **Lib loading is minimal**: workers load only a single `lib.d.ts` blob and never honor the `lib` array per target, nor multiple lib files (es2015, es2017, dom, iterable, etc.). Default lib selection is missing, so globals/iterables/symbols are absent; WASM emits TS2304/TS2488/TS2339 while TSC (with proper libs) would not.
- **JS/JSDoc handling**: many crashes come from JS/JSDoc tests (`exportSymbol`/`declarations` undefined). With `allowJs`/`checkJs` ignored, JS files are parsed as TS without the JS binding pathway, leaving source file symbols unset and causing `TypeError: exportSymbol`/`flags`/`declarations` crashes.
- **Module/global merging correctness**: `merge_bind_results` injects every file’s `file_locals` into a global table (even for external modules). This likely diverges from TSC’s external-module isolation and can cascade into “Cannot find name” and duplicate-identifier noise.
- **Crash/OOM amplification**: The 393k extra TS2304 diags inflate memory and respawns (117 worker respawns). Fixing symbol/globals/lib issues should drastically cut crashes and OOMs.

## Plan of Action (priority order)
1) **Restore full directive->CompilerOptions parity in workers**
   - Reuse the richer directive parsing (module/moduleResolution/lib/jsx/noLib/skipLibCheck/allowJs/checkJs/experimentalDecorators/emitDecoratorMetadata/useDefineForClassFields/esModuleInterop/isolatedModules/types/paths/rootDirs/baseUrl/resolveJsonModule/allowSyntheticDefaultImports).
   - Pass options through to both TSC and WASM (ThinParser.setCompilerOptions / WasmProgram-level options).

2) **Load correct lib set per test**
   - Implement a lib loader that resolves the `lib` array (or default lib selection based on target/moduleResolution) from `TypeScript/tests/lib/*.d.ts`.
   - For WASM: extend `WasmProgram` to accept multiple lib files and mark them as lib files; for ThinParser path, add `addLibFile` per lib.
   - For TSC-in-runner: provide the same set via `CompilerHost.getSourceFile`.
   - Honor `// @noLib` and `/// <reference lib="...">` directives.

3) **Enable JS/JSDoc correctness**
   - Respect `allowJs`/`checkJs`; set ScriptKind for `.js`/`.jsx`.
   - Ensure binder sets `source_file_symbol` for JS and supports `export =` in JS mode.
   - Add guards in checker where `exportSymbol`/`declarations` are assumed, and add a targeted crash repro from one of the failing JSDoc tests.

4) **Fix module/global merging**
   - Revisit `merge_bind_results`: top-level symbols from external modules should not be injected into the global symbol table; only scripts/ambient modules should flow to globals.
   - Verify `module_exports`/symbol_arenas are preserved after merge (imports/re-exports) to avoid “Cannot find name” from lost exports.

5) **Reduce diagnostic flood & stability**
   - After fixes above, re-run 100/500/ALL to confirm TS2304 drops from 393k.
   - Add a cap or summarization in the runner output (not in core checker) to avoid worker OOM from pathological files while keeping checker behavior unchanged.

6) **Regression harness**
   - Add targeted mini-repros for: (a) JS/JSDoc crash (exportSymbol undefined), (b) module import/export resolution across files, (c) lib-required global (Symbol/Map/Iterator) to validate lib loading, (d) @allowJs + @checkJs options flow.

## Success Criteria
- TS2304 extra errors drop by an order of magnitude (ideally <10% of current).
- Crashes < 10, OOM = 0, Timeouts < 5 on full suite.
- Pass rate moves toward 50%+ once lib/options/js handling are corrected.

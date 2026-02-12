#!/bin/bash
# Session script: assigns each tsz-N agent to emit test improvement

DIR_NAME=$(basename "$(pwd)")

case "$DIR_NAME" in
  tsz-1) SLICE=1 ;;
  tsz-2) SLICE=2 ;;
  tsz-3) SLICE=3 ;;
  tsz-4) SLICE=4 ;;
  tsz)
    echo "You are the coordinator. The tsz-1 through tsz-4 agents each work on emit test fixes. Help where needed or work on unit tests and general improvements." >&2
    exit 2
    ;;
  *)
    echo "Unknown directory: $DIR_NAME. Expected tsz or tsz-1 through tsz-4." >&2
    exit 2
    ;;
esac

cat >&2 <<EOF
IMPORTANT: Read docs/HOW_TO_CODE.md before writing any code. It covers architecture
rules, coding patterns, recursion safety, testing, and debugging conventions.

Your job is to improve the EMIT test pass rate.
Current: ~62% JS emit pass rate (after module default fix), target: 90%+.

═══════════════════════════════════════════════════════════
EMIT TEST ASSIGNMENTS
═══════════════════════════════════════════════════════════

  Slice 1: Comment preservation (41 line-comment + 11 inline-comment failures)
  Slice 2: Object/expression formatting (25 multiline + 11 indentation failures)
  Slice 3: Destructuring/for-of downlevel (ES5 lowering: destructuring, variable renaming)
  Slice 4: Helper functions + this capture (__values, __read, __spread, _this binding)

You are slice $SLICE.

═══════════════════════════════════════════════════════════
RUNNING EMIT TESTS
═══════════════════════════════════════════════════════════

First, build the binary:
  cargo build --release -p tsz-cli

Quick test run (JS-only, fastest):
  ./scripts/emit/run.sh --max=200 --js-only

Filtered run (focus on your area):
  Slice 1: ./scripts/emit/run.sh --js-only --verbose --filter="APISample"
  Slice 2: ./scripts/emit/run.sh --js-only --verbose --filter="APISample_compile"
  Slice 3: ./scripts/emit/run.sh --js-only --verbose --filter="ES5For-of"
  Slice 4: ./scripts/emit/run.sh --js-only --verbose --filter="ES5For-of33"

Full run (all ~11K tests):
  ./scripts/emit/run.sh --js-only

Emit runner options (./scripts/emit/run.sh --help):
  --max=N           Limit number of tests
  --filter=PATTERN  Filter tests by name
  --verbose / -v    Show full diffs (VERY useful for debugging)
  --js-only         Test JS emit only (skip .d.ts, much faster)
  --dts-only        Test .d.ts emit only
  -jN               Parallel workers (default: CPU count)
  --timeout=MS      Per-test timeout (default: 5000ms)

═══════════════════════════════════════════════════════════
KNOWN FAILURE CATEGORIES (from 500-test sample)
═══════════════════════════════════════════════════════════

1. COMMENT PRESERVATION (52 failures) — Slice 1
   - Line comments (// ...) stripped from output in many contexts
     Example: \`Point.Origin = ""; //expected duplicate identifier error\`
     We emit: \`Point.Origin = "";\` (comment dropped)
   - Inline comments (/* ... */) misplaced
     Example: \`ts.findConfigFile(/*searchPath*/ "./"...)\`
     We emit: \`ts.findConfigFile("./",... /*searchPath*/if...\`
   Emitter code: crates/tsz-emitter/src/emitter/

2. FORMATTING / MULTILINE (36 failures) — Slice 2
   - Object literals: TSC keeps short properties on same line
     Expected: \`{ noEmitOnError: true, noImplicitAny: true, }\`
     We emit:  \`{ noEmitOnError: true,\\n  noImplicitAny: true, }\`
   - Short function bodies should stay on one line
     Expected: \`C.prototype.foo = function () { };\`
     We emit:  \`C.prototype.foo = function () {\\n};\`
   - Indentation issues in nested IIFEs
   Emitter code: crates/tsz-emitter/src/emitter/

3. ES5 LOWERING / DESTRUCTURING (30+ failures) — Slice 3
   - Destructuring not lowered for ES5 target
     Expected: \`var _b = _a[_i], a = _b === void 0 ? 0 : _b;\`
     We emit:  \`var [a = 0, b = 1] = _a[_i];\` (not lowered!)
   - Variable renaming: TSC adds _1, _2 suffixes for shadowed vars
     Expected: \`var v_1 = _c[_b];\`
     We emit:  \`var v = _c[_b];\`
   - Temp variable naming differs (_a, _b, _c vs our choices)
   Lowering code: crates/tsz-emitter/src/lowering_pass.rs
                   crates/tsz-emitter/src/transforms/

4. HELPER FUNCTIONS + THIS CAPTURE (10+ failures) — Slice 4
   - __values, __read, __spread helpers not emitted for ES5
     Expected: emits \`var __values = (this && this.__values) || ...\`
     We emit: nothing (no helper emitted)
   - _this = this capture for arrow functions inside methods
     Expected: \`var _this = this;\` at function start
     We emit: \`this\` directly (not captured)
   - Regex literals: /\\r\\n/g not preserved through emit
   Transforms: crates/tsz-emitter/src/transforms/
   Emit context: crates/tsz-emitter/src/emit_context.rs

═══════════════════════════════════════════════════════════
KEY CODE LOCATIONS
═══════════════════════════════════════════════════════════

  Emitter entry:    crates/tsz-emitter/src/emitter/mod.rs
  Expressions:      crates/tsz-emitter/src/emitter/expressions.rs
  Statements:       crates/tsz-emitter/src/emitter/statements.rs
  Declarations:     crates/tsz-emitter/src/emitter/declarations.rs
  Lowering pass:    crates/tsz-emitter/src/lowering_pass.rs
  Emit context:     crates/tsz-emitter/src/emit_context.rs
  Transforms dir:   crates/tsz-emitter/src/transforms/
  Module CommonJS:  crates/tsz-emitter/src/transforms/module_commonjs.rs
  CLI driver:       crates/tsz-cli/src/driver.rs
  CLI args:         crates/tsz-cli/src/args.rs

═══════════════════════════════════════════════════════════
WORKFLOW
═══════════════════════════════════════════════════════════

1. Run filtered emit tests to see failures in your area
2. Pick a specific failing test, run with --verbose to see the diff
3. Create a minimal .ts reproduction in tmp/
4. Run: .target/release/tsz --noCheck --noLib --target es5 --module none tmp/test.ts
5. Compare output with TypeScript baseline
6. Fix the emitter/transform code
7. Re-run emit tests to verify improvement
8. Run cargo nextest run to ensure no unit test regressions
9. Commit with clear message
10. MANDATORY — sync after EVERY commit:
      git pull --rebase origin main && git push origin main

Focus on fixes that are general (help many tests) rather than narrow one-offs.
Do NOT break existing passing tests. Always verify with cargo nextest run.
EOF

exit 2

# Emit 100% Pass Rate — Design Document

**Date**: 2026-03-03
**Goal**: Achieve 100% pass rate for both JS emit and DTS emit tests.

## Current State (2026-03-03 fresh run)

| Domain | Passing | Total | Rate | Gap |
|--------|---------|-------|------|-----|
| **JS** | 10,290 | 13,427 | **76.6%** | 3,137 |
| **DTS** | 783 | 1,457 | **53.7%** | 674 |

## Strategy: Transform-Heavy → Sweep

Implement missing language transforms first to clear preconditions, then sweep
formatting/comment diffs to land the volume.

---

## Phase 1: Big Transforms (JS — clear preconditions)

### 1.1 TC39 Decorators (`__esDecorate` + `__runInitializers`)
- **Impact**: 217 tests mention these helpers; 6 exclusive
- **What**: Implement TC39 Stage 3 decorator transform for ES2015+ targets
- **Helpers to emit**: `__esDecorate`, `__runInitializers`, `__setFunctionName`
- **Key files**: `transforms/class_es5_ir.rs`, `transforms/class_es5.rs`, `emitter/declarations/class.rs`
- **tsc reference**: `transformers/classFields.ts`, `transformers/esDecorators.ts`
- **Complexity**: Very Hard — full class member decorator IR generation
- **Prerequisite**: None

### 1.2 JSX Transform (`jsx=react` → `createElement`)
- **Impact**: 123 tests; 95 exclusive
- **What**: Transform JSX elements to `React.createElement()` calls
- **Options**: `jsxFactory`, `jsxFragmentFactory`, `jsxImportSource`
- **Key files**: `emitter/jsx.rs` (exists but incomplete)
- **tsc reference**: `transformers/jsx.ts`
- **Complexity**: Medium
- **Prerequisite**: None

### 1.3 Private Fields Transform
- **Impact**: 98 tests; 30 exclusive
- **What**: `#field` → WeakMap/WeakSet pattern for ES2021 and below
- **Helpers**: `__classPrivateFieldGet`, `__classPrivateFieldSet`, `__classPrivateFieldIn`
- **Key files**: New transform needed; `transforms/private_fields_es5.rs` exists but may be incomplete
- **tsc reference**: `transformers/classFields.ts`
- **Complexity**: Hard — class transform integration
- **Prerequisite**: None

### 1.4 `__rest` Helper
- **Impact**: 68 tests; 54 exclusive
- **What**: Emit `__rest` for rest elements in destructuring
- **Pattern**: `const {a, ...rest} = obj` → `var rest = __rest(obj, ["a"])`
- **Key files**: `transforms/destructuring_es5.rs`
- **Complexity**: Medium
- **Prerequisite**: None

### 1.5 Decorator Metadata (`__metadata`)
- **Impact**: 56 tests; 38 exclusive
- **What**: Emit `__metadata("design:type", ...)` when `emitDecoratorMetadata` is true
- **Requires**: Type serialization to emit design-time type references
- **Key files**: `transforms/class_es5_ir.rs`
- **Complexity**: Medium
- **Prerequisite**: Partial — benefits from 1.1

### 1.6 `__awaiter`/`__generator` ES5 Improvements
- **Impact**: 171/96 tests; 28 exclusive
- **What**: Fix parameter default hoisting, `arguments` capture, generator state machine
- **Key files**: `transforms/async_es5*.rs`
- **Complexity**: Hard — state machine transforms
- **Prerequisite**: None

---

## Phase 2: Module Detection (JS — systemic root cause)

### 2.1 Fix Module Kind Detection
- **Impact**: ~726 tests across many categories
- **Root cause**: Files wrongly detected as CJS when they should be ESM
- **Detection needed for**: `.mjs`/`.mts` extensions, `package.json "type": "module"`,
  `import.meta` (partially done), top-level `await`, re-export patterns
- **Cascading fixes**:
  - Extra `__esModule` emission: ~111 tests
  - Extra `"use strict"`: ~109 tests
  - Extra `__importStar`/`__createBinding`: ~48 tests
  - Missing `__esModule`: ~113 tests
- **Key files**: `emitter/module_emission/core.rs`, `cli/driver/emit.rs`
- **Complexity**: Medium-Hard

---

## Phase 3: DTS Emit (parallel workstream)

### 3.1 Fix DTS Command Crashes (44 tests)
- Runner uses wrong binary path for some tests
- `--allowJs --declaration` on `.js` inputs crashes/errors
- Fix: Runner binary path propagation + investigate panics
- **Complexity**: Small

### 3.2 DTS Type Inference (151 tests)
- Types resolve to `any` where tsc produces actual types
- Root cause: Solver/checker not providing rich type information to declaration emitter
- Needs: `TypePrinter` improvements, accessor type inference, anonymous class expansion
- **Key files**: `declaration_emitter/type_emission.rs`, `declaration_emitter/helpers.rs`
- **Complexity**: Hard — solver integration

### 3.3 DTS Import/Export Handling (305 tests)
- Wrong import/export structure in declaration output
- Missing `declare module` wrapping for ambient modules
- Export visibility analysis gaps in `usage_analyzer.rs`
- **Key files**: `declaration_emitter/exports.rs`, `declaration_emitter/usage_analyzer.rs`
- **Complexity**: Hard

### 3.4 DTS Missing Declarations (181 tests)
- Fewer declarations emitted than tsc
- Need: Better symbol visibility analysis, re-export handling, namespace declarations
- **Key files**: `declaration_emitter/core.rs`
- **Complexity**: Hard

---

## Phase 4: Comment & Formatting Sweep (JS + DTS)

### 4.1 Comment Handling (835 exclusive JS tests)
- Systematic fix of comment association (trailing vs leading)
- Comment preservation through transforms (ES5 class, decorators)
- Erased-declaration comment boundary fixes
- `next-sibling position` cap for `emit_trailing_comments`

### 4.2 Export/Import Pattern Fixes (307 JS tests)
- CJS `exports.X` reference rewriting
- Module wrapper patterns (AMD, UMD, System)
- `export {}` sentinel logic refinement
- Anonymous default export naming (`default_1`, `default_2`)

### 4.3 Expression/Statement Formatting (456 JS tests)
- Parenthesization edge cases
- Semicolons (118 tests)
- Spacing/indentation (108 tests)
- Empty body formatting `{ }` vs `{}` (60 tests)
- `var`/`let`/`const` keyword matching (31 tests)

---

## Phase 5: Long Tail Cleanup

### 5.1 Remaining Edge Cases (~200-400 tests)
- Numeric literal normalization (octal, separators)
- Unicode escaping in identifiers
- Multi-file/outFile bundle tests
- `static {}` block transforms for ES2021-
- Optional chain continuation lowering edge cases
- Source map URL handling

---

## Projected Progress Curve

| Phase | JS Gain | DTS Gain | Cumulative JS | Cumulative DTS |
|-------|---------|----------|---------------|----------------|
| Current | — | — | 76.6% | 53.7% |
| Phase 1 (Transforms) | +600-800 | — | ~82-84% | 53.7% |
| Phase 2 (Module Detection) | +200-400 | — | ~85-87% | 53.7% |
| Phase 3 (DTS) | — | +400-500 | ~87% | ~80-88% |
| Phase 4 (Comments/Formatting) | +1,000-1,500 | +100-150 | ~95-98% | ~90-95% |
| Phase 5 (Long Tail) | +200-400 | +50-100 | **100%** | **100%** |

## Measurement

- **JS test runner**: `TSZ_BIN=.target/dist-fast/tsz ./scripts/emit/run.sh --js-only --skip-build`
- **DTS test runner**: `TSZ_BIN=.target/dist-fast/tsz ./scripts/emit/run.sh --dts-only --skip-build`
- **Targeted runs**: `--filter "pattern" --verbose` for investigating specific failures
- **Verbose analysis**: Capture output to file, run Python categorization scripts

## Architecture Notes

- All transform work lives in `crates/tsz-emitter/src/transforms/`
- DTS work lives in `crates/tsz-emitter/src/declaration_emitter/`
- Module detection logic is split between `emitter/module_emission/core.rs` and `cli/driver/emit.rs`
- Comment handling is spread across `emitter/comments/`, `emitter/source_file.rs`, `emitter/statements.rs`
- Follow the existing two-path architecture: Transform path (lowering → directives → emitter) vs Direct path (AST → emitter)

## Key Risks

1. **TC39 decorators are very complex**: The tsc implementation spans thousands of lines. May need to be broken into sub-phases.
2. **DTS type inference depends on solver**: Improving DTS may require solver changes, not just emitter changes.
3. **Comment handling is diffuse**: 835 tests across many different comment patterns. No single fix.
4. **Module detection has regressions risk**: Changing module detection affects many tests simultaneously.

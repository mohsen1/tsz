# Testing Infrastructure Cleanup Plan

## Overview

This document outlines the plan to consolidate and improve the testing infrastructure for Project Zang.

---

## Part 1: Understanding TypeScript Test Directives

### Directive Format

TypeScript tests use `// @directive: value` comments at the top of test files to configure compiler options.

```typescript
// @target: es2017
// @strict: true
// @noEmitHelpers: true
// @filename: file1.ts
```

### Most Common Directives (from 5,200 conformance tests)

| Directive | Count | Purpose |
|-----------|-------|---------|
| `@filename` | 3,414 | Multi-file test: defines file boundaries |
| `@target` | 1,572 | ES target: es5, es6, es2017, esnext |
| `@strict` | 1,437 | Enable all strict checks |
| `@module` | 593 | Module system: commonjs, es2015, esnext |
| `@checkJs` | 501 | Enable JS checking |
| `@declaration` | 490 | Generate .d.ts files |
| `@allowJs` | 483 | Allow JS files |
| `@noEmit` | 447 | Don't emit output |
| `@lib` | 308 | Library files to include |
| `@noTypesAndSymbols` | 285 | Skip types/symbols baseline |
| `@noEmitHelpers` | 256 | Don't emit __awaiter etc |
| `@jsx` | 152 | JSX mode: react, preserve |
| `@skipLibCheck` | 100 | Skip lib.d.ts checking |
| `@noImplicitAny` | 98 | Disallow implicit any |
| `@strictNullChecks` | 85 | Enable null checks |
| `@moduleResolution` | 84 | node, classic, bundler |
| `@experimentalDecorators` | 134 | Enable decorators |
| `@noLib` | 86 | Don't include default lib |

### Multi-File Tests

Use `@filename` (or `@Filename`) to split a test into multiple virtual files:

```typescript
// @filename: declarations.d.ts
declare module "jquery"

// @filename: user.ts
import foo from "jquery";
foo();
```

### Baseline Files

For each test `foo.ts`, TypeScript generates baselines in `tests/baselines/reference/`:

| File | Purpose |
|------|---------|
| `foo.js` | Emitted JavaScript |
| `foo.d.ts` | Emitted declarations |
| `foo.types` | Type annotations |
| `foo.symbols` | Symbol table |
| `foo.errors.txt` | Expected errors (only if test should error) |

**Key insight**: If `foo.errors.txt` doesn't exist, the test should produce **zero errors**.

---

## Part 2: Current State Analysis

### Redundant Test Runners (4,365 lines)

| File | Lines | Status |
|------|-------|--------|
| `conformance/conformance-runner.mjs` | 609 | **DELETE** - superseded |
| `conformance/conformance-child.mjs` | 422 | **DELETE** - unused worker |
| `conformance/conformance-embedded.mjs` | 373 | **DELETE** - alternative |
| `conformance/conformance-worker.mjs` | 388 | **DELETE** - unused |
| `conformance/conformance-simple.mjs` | 114 | **DELETE** - simplified |
| `conformance/parallel-conformance.mjs` | 282 | **DELETE** - superseded |
| `conformance/process-pool-conformance.mjs` | 344 | **DELETE** - superseded |
| `conformance/metrics-tracker.mjs` | 520 | **DELETE** - unused |
| `conformance/test-strict-directive.mjs` | 129 | **DELETE** - one-off test |
| `conformance/test-single-file.mjs` | 53 | **MOVE** to scripts/ |
| **conformance/src/runner.ts** | 744 | **KEEP** - primary |
| **conformance/src/compare.ts** | 310 | **KEEP** - core logic |

### scripts/ Overlap

| File | Lines | Status |
|------|-------|--------|
| `scripts/run-single-test.mjs` | 400 | **KEEP** - useful |
| `scripts/run-batch-tests.mjs` | 350 | **REVIEW** - may overlap |
| `scripts/compare-baselines.mjs` | 530 | **REVIEW** - useful? |
| `scripts/measure-baseline.mjs` | 260 | **DELETE** - one-off |
| `scripts/validate-wasm.mjs` | 90 | **KEEP** - useful |

---

## Part 3: Execution Plan

### Phase 1: Delete Redundant Files

```bash
# Delete redundant conformance runners
rm conformance/conformance-runner.mjs
rm conformance/conformance-child.mjs
rm conformance/conformance-embedded.mjs
rm conformance/conformance-worker.mjs
rm conformance/conformance-simple.mjs
rm conformance/parallel-conformance.mjs
rm conformance/process-pool-conformance.mjs
rm conformance/metrics-tracker.mjs
rm conformance/test-strict-directive.mjs
rm conformance/test-single-file.mjs

# Delete unused scripts
rm scripts/measure-baseline.mjs
```

### Phase 2: Simplify package.json

Update `conformance/package.json` to remove dead scripts:

```json
{
  "scripts": {
    "build": "tsc",
    "test": "npm run build && node dist/runner.js",
    "test:100": "npm run build && node dist/runner.js --max=100",
    "test:500": "npm run build && node dist/runner.js --max=500",
    "test:verbose": "npm run build && node dist/runner.js --verbose"
  }
}
```

### Phase 3: Improve Directive Parsing

Update `conformance/src/runner.ts` to fully support all directives:

**Currently Supported:**
- [x] `@target`
- [x] `@strict`
- [x] `@filename` / `@Filename`
- [x] `@noImplicitAny`
- [x] `@strictNullChecks`
- [x] `@declaration`

**Need to Add:**
- [ ] `@module` (commonjs, es2015, esnext)
- [ ] `@lib` (es5, es2015.promise, dom, etc.)
- [ ] `@jsx` (react, preserve, react-jsx)
- [ ] `@moduleResolution` (node, classic, bundler)
- [ ] `@noLib`
- [ ] `@skipLibCheck`
- [ ] `@checkJs` / `@allowJs`
- [ ] `@experimentalDecorators`
- [ ] `@emitDecoratorMetadata`
- [ ] `@useDefineForClassFields`

### Phase 4: Add Baseline Comparison

The current runner only compares error **codes**. Add comparison against `.errors.txt` baselines:

```typescript
interface BaselineComparison {
  hasBaseline: boolean;
  baselineErrors: ParsedError[];
  actualErrors: ParsedError[];
  exactMatch: boolean;
  missingErrors: ParsedError[];
  extraErrors: ParsedError[];
}

function parseErrorsBaseline(content: string): ParsedError[] {
  // Parse format:
  // foo.ts(1,13): error TS1110: Type expected.
  const regex = /^(.+)\((\d+),(\d+)\): error TS(\d+): (.+)$/gm;
  // ...
}
```

### Phase 5: Add compiler/ Test Category

The `compiler/` directory has 6,500 tests we're not running. Add support:

```typescript
const CATEGORIES = {
  conformance: 'TypeScript/tests/cases/conformance',
  compiler: 'TypeScript/tests/cases/compiler',
};
```

### Phase 6: Directory Structure

Final structure:

```
conformance/
├── src/
│   ├── runner.ts      # Main runner
│   ├── compare.ts     # Diagnostic comparison
│   └── directives.ts  # Directive parsing (new)
├── dist/              # Compiled output
├── package.json
└── tsconfig.json

scripts/
├── run-single-test.mjs
├── validate-wasm.mjs
├── test.sh
└── docker/
    └── Dockerfile
```

---

## Part 4: Implementation Checklist

### Immediate (Phase 1-2)
- [ ] Delete 10 redundant .mjs files
- [ ] Update package.json scripts
- [ ] Verify `npm run test:100` still works

### Short-term (Phase 3-4)
- [ ] Add missing directive support
- [ ] Add baseline file comparison
- [ ] Compare against `.errors.txt` files

### Medium-term (Phase 5)
- [ ] Add compiler/ test support
- [ ] Target: run 10,000+ tests
- [ ] Track pass rate by category

### Long-term
- [ ] Incremental testing (skip unchanged)
- [ ] Parallel execution (worker threads)
- [ ] CI integration with pass rate tracking

---

## Part 5: Success Metrics

| Metric | Current | Target |
|--------|---------|--------|
| Conformance files | 10 | 2 |
| Test coverage | 500 tests | 10,000+ tests |
| Pass rate (100) | 53.5% | 80%+ |
| Pass rate (500) | 40.8% | 70%+ |
| Runner startup | ~2s | <1s |

---

## References

- TypeScript test harness: `TypeScript/src/harness/`
- Baseline files: `TypeScript/tests/baselines/reference/`
- Test cases: `TypeScript/tests/cases/`

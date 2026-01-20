# Testing Infrastructure Cleanup Plan

## Overview

This document outlines the plan to consolidate and improve the testing infrastructure for Project Zang.

---

## ⚠️ IMPORTANT: Run Tests in Docker

**Tests MUST run inside Docker containers.** Running conformance tests directly on the host machine is dangerous and can:

- **Infinite loops**: Some malformed test inputs can cause the parser/checker to loop forever
- **Out of memory (OOM)**: Deep recursion or large type expansions can exhaust system memory
- **System instability**: Runaway processes can freeze or crash the host machine

### Safe Testing Commands

```bash
# ✅ SAFE: Run tests in Docker (recommended)
./scripts/test.sh

# ✅ SAFE: Run conformance tests in Docker
./scripts/test.sh --conformance

# ⚠️ DANGEROUS: Direct execution (only for debugging single files)
cd conformance && npm run test:100  # Can hang or OOM
```


### Docker Configuration

The Docker container enforces resource limits:

```dockerfile
# Memory limit prevents OOM from killing host
--memory=4g

# CPU limit prevents infinite loops from freezing host  
--cpus=2

# Timeout kills runaway processes
timeout 30s node dist/runner.js
```

### When Direct Execution is Acceptable

Only run tests directly on the host when:
1. Debugging a **single specific test file** with `scripts/run-single-test.mjs`
2. You have verified the test doesn't cause infinite loops
3. You're monitoring memory usage

```bash
# Safe for single file debugging:
node scripts/run-single-test.mjs TypeScript/tests/cases/conformance/types/spread/spreadSomething.ts
```

---

## Part 1: Understanding TypeScript Test Directives

### Directive Format

TypeScript tests use `// @directive: value` comments at the top of test files to configure compiler options.

```typescript
// @target: es2017
// @strict: true
// @noEmitHelpers: true
// @Filename: file1.ts
```

**Important**: Directives are case-insensitive. Both `@filename` and `@Filename` are valid.

### Most Common Directives (from 5,200 conformance tests)

| Directive | Count | Purpose |
|-----------|-------|---------|
| `@filename` / `@Filename` | 3,414 | Multi-file test: defines file boundaries |
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
// @Filename: declarations.d.ts
declare module "jquery"

// @Filename: user.ts
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

### .errors.txt Format

The `.errors.txt` baseline has a specific multi-line structure:

```
foo.ts(1,13): error TS1110: Type expected.


==== foo.ts (1 errors) ====
    var v = (a: ) => {
                ~
!!! error TS1110: Type expected.
       
    };
```

**Structure:**
1. **Header lines**: `file(line,col): error TSxxxx: message`
2. **File sections**: `==== filename (N errors) ====`
3. **Source with annotations**: Original code with `~` underlines and `!!! error` markers

---

## Part 2: Current State Analysis

### Redundant Test Runners (4,365 lines)

| File | Lines | Status |
|------|-------|--------|
| `conformance/conformance-runner.mjs` | 609 | **DELETE** |
| `conformance/conformance-child.mjs` | 422 | **DELETE** |
| `conformance/conformance-embedded.mjs` | 373 | **DELETE** |
| `conformance/conformance-worker.mjs` | 388 | **DELETE** |
| `conformance/conformance-simple.mjs` | 114 | **DELETE** |
| `conformance/parallel-conformance.mjs` | 282 | **DELETE** |
| `conformance/process-pool-conformance.mjs` | 344 | **DELETE** |
| `conformance/metrics-tracker.mjs` | 520 | **DELETE** |
| `conformance/test-strict-directive.mjs` | 129 | **DELETE** |
| `conformance/test-single-file.mjs` | 53 | **DELETE** |
| **conformance/src/runner.ts** | 744 | **KEEP** - primary |
| **conformance/src/compare.ts** | 310 | **KEEP** - core logic |

### scripts/ Analysis

| File | Lines | Status |
|------|-------|--------|
| `scripts/run-single-test.mjs` | 400 | **KEEP** - useful for debugging |
| `scripts/run-batch-tests.mjs` | 350 | **DELETE** - overlaps with runner.ts |
| `scripts/compare-baselines.mjs` | 530 | **DELETE** - unused |
| `scripts/measure-baseline.mjs` | 260 | **DELETE** - one-off |
| `scripts/validate-wasm.mjs` | 90 | **KEEP** - useful |

---

## Part 3: Execution Plan

### Phase 1: Dependency Scan & Delete Redundant Files

**Step 1: Scan for references before deleting**

```bash
# Check what references these files
rg "conformance-runner|conformance-child|conformance-embedded" .
rg "parallel-conformance|process-pool|metrics-tracker" .
rg "run-batch-tests|compare-baselines|measure-baseline" .
```

**Step 2: Update any references found (test.sh, docs, CI)**

**Step 3: Delete redundant files**

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
rm scripts/run-batch-tests.mjs
rm scripts/compare-baselines.mjs
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

### Phase 3: Fix Directive Parsing (Case-Insensitive)

**Bug**: Current parser only matches lowercase `@filename`, missing `@Filename` (1,095 occurrences).

**Fix in `conformance/src/runner.ts`:**

```typescript
// BEFORE (broken):
const filenameMatch = trimmed.match(/^\/\/\s*@filename:\s*(.+)$/);

// AFTER (case-insensitive):
const filenameMatch = trimmed.match(/^\/\/\s*@filename:\s*(.+)$/i);

// Also normalize all directive keys to lowercase:
const optionMatch = trimmed.match(/^\/\/\s*@(\w+):\s*(.+)$/i);
if (optionMatch) {
  const key = optionMatch[1].toLowerCase(); // normalize
  // ...
}
```

**Currently Supported:**
- [x] `@target`
- [x] `@strict`
- [x] `@filename` / `@Filename` ✅ (case-insensitive, fixed)
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

The current runner only compares error **codes**. Add full comparison against `.errors.txt` baselines.

**Step 1: Parse header errors**

```typescript
interface ParsedError {
  file: string;
  line: number;
  column: number;
  code: number;
  message: string;
}

function parseErrorsBaseline(content: string): ParsedError[] {
  const errors: ParsedError[] = [];
  
  // Parse header lines: "foo.ts(1,13): error TS1110: Type expected."
  const headerRegex = /^(.+)\((\d+),(\d+)\): error TS(\d+): (.+)$/gm;
  let match;
  while ((match = headerRegex.exec(content)) !== null) {
    errors.push({
      file: match[1],
      line: parseInt(match[2]),
      column: parseInt(match[3]),
      code: parseInt(match[4]),
      message: match[5],
    });
  }
  
  return errors;
}
```

**Step 2: Load baseline if exists**

```typescript
function loadBaseline(testPath: string): ParsedError[] | null {
  const baseName = path.basename(testPath, '.ts');
  const baselinePath = path.join(
    'TypeScript/tests/baselines/reference',
    `${baseName}.errors.txt`
  );
  
  if (!fs.existsSync(baselinePath)) {
    return []; // No errors expected
  }
  
  const content = fs.readFileSync(baselinePath, 'utf8');
  return parseErrorsBaseline(content);
}
```

**Step 3: Compare with actual errors**

```typescript
interface BaselineComparison {
  hasBaseline: boolean;
  expectedErrors: ParsedError[];
  actualErrors: ParsedError[];
  exactMatch: boolean;
  missingErrors: ParsedError[];  // in baseline but not actual
  extraErrors: ParsedError[];    // in actual but not baseline
}
```

### Phase 5: Add compiler/ Test Category

The `compiler/` directory has 6,500 tests we're not running. Add support:

```typescript
const CATEGORIES = {
  conformance: 'TypeScript/tests/cases/conformance',
  compiler: 'TypeScript/tests/cases/compiler',
};

// Update CLI:
// node dist/runner.js --category=compiler --max=1000
```

### Phase 6: Final Directory Structure

```
conformance/
├── src/
│   ├── runner.ts      # Main runner
│   ├── compare.ts     # Diagnostic comparison
│   ├── directives.ts  # Directive parsing (extracted)
│   └── baseline.ts    # Baseline loading/parsing (new)
├── dist/              # Compiled output
├── package.json
└── tsconfig.json

scripts/
├── run-single-test.mjs  # Debug single test
├── validate-wasm.mjs    # Validate WASM build
├── test.sh              # Main test script
├── build-wasm.sh
└── docker/
    └── Dockerfile
```

---

## Part 4: Implementation Checklist

### Immediate (Phase 1-2) ✅ COMPLETED
- [x] Run dependency scan for references to deleted files
- [x] Update run-conformance.sh to use TypeScript runner
- [x] Delete 10 redundant .mjs files in conformance/
- [x] Delete 3 unused scripts in scripts/
- [x] Update conformance/package.json
- [x] Verify conformance tests still work

### Short-term (Phase 3-4) ✅ COMPLETED
- [x] Fix case-insensitive directive matching (`@Filename`)
- [x] Add missing directive support (`@module`, `@lib`, `@jsx`, `@moduleResolution`)
- [x] Implement baseline file loading (`conformance/src/baseline.ts`)
- [x] Implement `.errors.txt` parsing (header format)
- [x] Add baseline comparison to test output

### Medium-term (Phase 5) ✅ COMPLETED
- [x] Add compiler/ test support
- [x] Distribute tests evenly across categories
- [x] Track pass rate by category

### Long-term
- [ ] Incremental testing (skip unchanged)
- [ ] Parallel execution (worker threads)
- [ ] CI integration with pass rate tracking

---

## Part 5: Success Metrics

| Metric | Before | After | Target |
|--------|--------|-------|--------|
| Runner files in conformance/ | 10 .mjs + 2 .ts | 3 .ts only ✅ | 3 .ts |
| Test coverage | conformance only | conformance + compiler ✅ | 10,000+ tests |
| Baseline comparison | None | Implemented ✅ | Full comparison |
| Directive support | ~6 directives | ~15 directives ✅ | All directives |
| Pass rate (conformance) | ~35% | ~35% | 80%+ |
| Pass rate (compiler) | N/A | ~50% | 80%+ |

---

## References

- TypeScript test harness: `TypeScript/src/harness/`
- Baseline files: `TypeScript/tests/baselines/reference/`
- Test cases: `TypeScript/tests/cases/`

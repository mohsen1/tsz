# Test Infrastructure Specification

## Executive Summary

This document specifies the testing infrastructure required to achieve TypeScript Compiler (TSC) conformance without polluting source code with test-aware logic. The key architectural principle is: **test infrastructure reads configuration from test files and passes it to the compiler, not the other way around**.

## Problem Statement

Current architectural debt:
- Source code contains ~40+ `file_name.contains("conformance")` checks
- Checker suppresses errors based on file path patterns
- Test-specific logic leaks into production code paths
- Violates separation of concerns (tests shouldn't affect source behavior)

**The Rule:** Source code must not know about tests. If a test fails, fix the underlying logic, not by adding special cases for test file names.

---

## 1. Overview: How TypeScript's Test Suite Works

TypeScript's conformance test suite uses a directive-based approach:

### Test File Structure
```typescript
// @strict: true
// @noImplicitAny: true
// @target: ES2015

// Test code here
function add(a, b) {  // Should error with noImplicitAny
    return a + b;
}
```

### Multi-File Tests
```typescript
// @filename: a.ts
export const x = 1;

// @filename: b.ts
import { x } from './a';
const y: string = x;  // Should error: Type 'number' is not assignable to type 'string'
```

### How It Works
1. **Test Runner** reads the test file
2. **Directive Parser** extracts `@` directives from comments
3. **Compiler** is invoked with the extracted options
4. **Output** is compared to baseline files

**Key Insight:** The compiler never sees the directives. The test infrastructure handles them.

---

## 2. Current State: What's Missing

Our test infrastructure has partial directive parsing but lacks:

1. **Complete Directive Support**
   - Currently parses some directives (`@strict`, `@target`, etc.)
   - Missing many compiler options (`@noImplicitReturns`, `@strictPropertyInitialization`, etc.)
   - No support for lib file directives (`@lib: es2015,dom`)

2. **CompilerOptions Flow**
   - Directives are parsed but not fully mapped to checker configuration
   - `CompilerOptions` struct exists but isn't used consistently
   - No validation of option values

3. **Lib File Handling**
   - Lib files detected by filename pattern in `WasmProgram.add_file()` (lines 1562-1566 in lib.rs)
   - **WRONG:** This is test infrastructure logic in source code
   - Should be handled by test runner, not compiler

4. **Baseline Comparison**
   - Test runner compares diagnostic codes
   - Needs more sophisticated comparison (messages, positions, categories)

---

## 3. Required Features

### 3.1 Directive Parsing

The test infrastructure must parse all TypeScript test directives.

#### Supported Directives

**Compiler Behavior:**
- `@strict: boolean` - Enable all strict checks
- `@noImplicitAny: boolean` - Raise error on implicit any types
- `@noImplicitReturns: boolean` - Error on missing return statements
- `@noImplicitThis: boolean` - Error on implicit this types
- `@strictNullChecks: boolean` - Enable strict null checking
- `@strictFunctionTypes: boolean` - Enable strict function type checking
- `@strictPropertyInitialization: boolean` - Ensure properties are initialized
- `@useUnknownInCatchVariables: boolean` - Catch variables are unknown, not any

**Output Options:**
- `@target: string` - ECMAScript target version (ES3, ES5, ES2015, ES2020, ESNext)
- `@module: string` - Module system (CommonJS, ES2015, ESNext, Node16, NodeNext)
- `@lib: string` - Comma-separated list of lib files (es5, es2015, dom, webworker)

**JavaScript Support:**
- `@allowJs: boolean` - Allow JavaScript files
- `@checkJs: boolean` - Type check JavaScript files

**Emit Options:**
- `@noEmit: boolean` - Don't emit output
- `@declaration: boolean` - Generate .d.ts files
- `@outdir: string` - Output directory

**Multi-File:**
- `@filename: string` - Starts a new file section in multi-file tests

#### Example Implementation (JavaScript)

```javascript
/**
 * Parse test directives from source code.
 * Returns: { options: CompilerOptions, isMultiFile: boolean, files: File[] }
 */
function parseTestDirectives(code) {
  const lines = code.split('\n');
  const options = {};
  let isMultiFile = false;
  const files = [];
  let currentFile = null;

  for (const line of lines) {
    const trimmed = line.trim();

    // Multi-file directive
    const filenameMatch = trimmed.match(/^\/\/\s*@filename:\s*(.+)$/);
    if (filenameMatch) {
      isMultiFile = true;
      if (currentFile) files.push(currentFile);
      currentFile = { name: filenameMatch[1], lines: [] };
      continue;
    }

    // Compiler option directive
    const optionMatch = trimmed.match(/^\/\/\s*@(\w+):\s*(.+)$/);
    if (optionMatch) {
      const [, key, value] = optionMatch;
      options[camelCase(key)] = parseValue(value);
      continue;
    }

    // Regular code line
    if (currentFile) {
      currentFile.lines.push(line);
    }
  }

  if (currentFile) files.push(currentFile);

  return { options, isMultiFile, files };
}
```

### 3.2 CompilerOptions Configuration

Map parsed directives to `CompilerOptions` struct:

```rust
// In src/lib.rs (already exists, lines 160-200)
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompilerOptions {
    pub strict: bool,
    pub no_implicit_any: bool,
    pub strict_null_checks: bool,
    pub strict_function_types: bool,
    pub target: String,
    pub module: String,
    // Add more options as needed
}

impl CompilerOptions {
    fn to_checker_options(&self) -> crate::cli::config::CheckerOptions {
        // Convert to internal checker options
    }
}
```

**Flow:**
```
Test File → parseTestDirectives() → CompilerOptions JSON →
ThinParser.setCompilerOptions() → CheckerOptions → ThinCheckerState
```

### 3.3 Baseline Comparison

Compare actual output to expected baselines:

```javascript
function compareToBaseline(testFile, actualDiagnostics) {
  const baselinePath = getBaselinePath(testFile);
  const expectedDiagnostics = readBaseline(baselinePath);

  return {
    exactMatch: deepEqual(actualDiagnostics, expectedDiagnostics),
    missingErrors: expectedDiagnostics.filter(e => !actualDiagnostics.includes(e)),
    extraErrors: actualDiagnostics.filter(e => !expectedDiagnostics.includes(e)),
  };
}
```

---

## 4. The ts-tests/ Directory Structure

```
ts-tests/
├── cases/
│   ├── conformance/           # TypeScript conformance test suite
│   │   ├── types/             # Type system tests
│   │   ├── expressions/       # Expression tests
│   │   ├── statements/        # Statement tests
│   │   ├── jsdoc/            # JSDoc tests
│   │   └── ...
│   └── compiler/             # Compiler behavior tests
├── baselines/                # Expected output files
│   └── reference/            # Reference baselines
│       ├── types/
│       └── ...
└── lib/                      # Library definition files
    ├── lib.d.ts              # Core TypeScript lib
    ├── lib.es5.d.ts          # ES5 types
    ├── lib.es2015.d.ts       # ES2015+ types
    └── lib.dom.d.ts          # DOM types
```

### Purpose of Each Directory

- **cases/conformance/**: Test files with `@` directives
- **baselines/reference/**: Expected diagnostic output for each test
- **lib/**: Type definitions that provide global types (Array, Promise, console, etc.)

---

## 5. Data Flow: Test Directives → Checker Configuration

```
┌─────────────────────────────────────────────────────────────────┐
│                     TEST INFRASTRUCTURE                          │
│                     (conformance-runner.mjs)                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. Read test file from disk                                    │
│     ↓                                                            │
│  2. parseTestDirectives(code)                                   │
│     ├─> Extract @strict, @noImplicitAny, etc.                   │
│     ├─> Extract @filename for multi-file tests                  │
│     └─> Build CompilerOptions object                            │
│     ↓                                                            │
│  3. Create compiler instance                                    │
│     const parser = new ThinParser(fileName, code)               │
│     ↓                                                            │
│  4. Configure compiler with directives                          │
│     parser.setCompilerOptions(JSON.stringify(options))          │
│     ↓                                                            │
│  5. Add lib files (if needed)                                   │
│     parser.addLibFile("lib.d.ts", libSource)                    │
│     ↓                                                            │
│  6. Run compilation                                             │
│     parser.parseSourceFile()                                    │
│     parser.checkSourceFile()                                    │
│     ↓                                                            │
│  7. Extract diagnostics                                         │
│     const result = parser.checkSourceFile()                     │
│     ↓                                                            │
│  8. Compare to baseline                                         │
│     compareToBaseline(testFile, result.diagnostics)             │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│                        WASM BOUNDARY                             │
│                   (ThinParser public API)                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  setCompilerOptions(json: String)                               │
│    ↓                                                             │
│  Deserialize CompilerOptions                                    │
│    ↓                                                             │
│  Store in self.compiler_options                                 │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│                      SOURCE CODE                                 │
│                   (src/thin_checker.rs)                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ThinCheckerState::new(                                         │
│    arena,                                                        │
│    binder,                                                       │
│    type_interner,                                               │
│    file_name,                                                    │
│    checker_options  ← Derived from CompilerOptions              │
│  )                                                               │
│    ↓                                                             │
│  Check types using configuration                                │
│    if options.no_implicit_any && type_is_any(...) {             │
│      emit_diagnostic(TS7006);                                   │
│    }                                                             │
│    ↓                                                             │
│  Return diagnostics                                             │
│                                                                  │
│  NO FILE NAME CHECKS!                                           │
│  NO if (file_name.contains("test")) { ... }                     │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### ASCII Flow Diagram

```
Test File            Test Infrastructure           WASM API              Source Code
─────────            ───────────────────           ────────              ───────────

foo.test.ts
  |
  | // @strict: true
  | // @noImplicitAny: true
  |
  v
[Read & Parse]
  |
  v
{
  strict: true,   ──────────────────────────────>  ThinParser
  noImplicitAny: true                               .setCompilerOptions(json)
}                                                    |
                                                     v
                                                   Store options
                                                     |
[Invoke checker] ─────────────────────────────────> |
                                                     v
                                                   ThinParser.checkSourceFile()
                                                     |
                                                     v
                                                   ThinCheckerState::new(
                                                     ...,
                                                     checker_options
                                                   )
                                                     |
                                                     v
                                                   if no_implicit_any {
                                                     check_for_implicit_any()
                                                   }
                                                     |
                                                     v
[Collect results] <─────────────────────────────── diagnostics
  |
  v
[Compare to baseline]
  |
  v
[Report]
```

---

## 6. Examples of @ Directives and Their Meanings

### Example 1: Strict Mode Test

```typescript
// @strict: true

// Should error: Parameter 'x' implicitly has an 'any' type (TS7006)
function add(x, y) {
    return x + y;
}
```

**How it works:**
1. Test runner parses `@strict: true`
2. Creates `CompilerOptions { strict: true, ... }`
3. Passes to `ThinParser.setCompilerOptions(...)`
4. Checker enables all strict checks
5. Emits TS7006 for implicit any

### Example 2: Target Version

```typescript
// @target: ES5

// Should be transformed to ES5 (var instead of let)
let x = 10;

// Should error if using ES6+ features without polyfill
const p = Promise.resolve(42);
```

**How it works:**
1. Test runner parses `@target: ES5`
2. Checker uses ES5 type definitions
3. Emitter transforms modern syntax to ES5

### Example 3: Multi-File Test

```typescript
// @filename: lib.ts
export interface User {
    name: string;
    age: number;
}

// @filename: main.ts
import { User } from './lib';

// Should error: Property 'email' does not exist on type 'User' (TS2339)
const user: User = { name: "Alice", age: 30 };
console.log(user.email);
```

**How it works:**
1. Test runner detects `@filename` directives
2. Creates two virtual files: `lib.ts` and `main.ts`
3. Uses `WasmProgram` API for multi-file checking
4. Checker resolves import from `lib.ts`
5. Emits TS2339 for missing property

### Example 4: Lib Files

```typescript
// @lib: es2015,dom
// @target: ES2015

// Should work: Promise is in es2015 lib
const p = Promise.resolve(42);

// Should work: document is in dom lib
document.getElementById('app');

// Should error if @lib didn't include dom:
// Cannot find name 'document' (TS2304)
```

**How it works:**
1. Test runner parses `@lib: es2015,dom`
2. Loads `lib.es2015.d.ts` and `lib.dom.d.ts`
3. Calls `ThinParser.addLibFile(...)` for each
4. Checker has access to `Promise` and `document` types

---

## 7. Implementation Requirements

### 7.1 Where Directive Parsing Should Happen

**✓ CORRECT: Test Infrastructure**

```javascript
// In differential-test/conformance-runner.mjs

function parseTestDirectives(code) {
  // Extract @strict, @noImplicitAny, @target, etc.
  // Build CompilerOptions object
  // Return { options, files, isMultiFile }
}

async function runTest(testFile) {
  const code = readFileSync(testFile);
  const { options, files } = parseTestDirectives(code);

  const parser = new ThinParser(testFile, code);
  parser.setCompilerOptions(JSON.stringify(options));

  // Run checker with configured options
  const result = parser.checkSourceFile();

  return result;
}
```

**✗ WRONG: Source Code**

```rust
// BAD - DO NOT DO THIS in src/thin_checker.rs

fn check_source_file(&mut self, root: NodeIndex) {
    // WRONG: Checking file name to change behavior
    if self.ctx.file_name.contains("strict") {
        self.ctx.options.strict = true;  // BAD!
    }

    // WRONG: Suppressing errors for test files
    if self.ctx.file_name.contains("conformance") {
        return;  // BAD!
    }
}
```

### 7.2 How to Pass Configuration to the Checker

**Use the existing `CompilerOptions` API:**

```rust
// Public API in src/lib.rs (already implemented, lines 330-338)

impl ThinParser {
    #[wasm_bindgen(js_name = setCompilerOptions)]
    pub fn set_compiler_options(&mut self, json: String) -> Result<(), JsValue> {
        let options: CompilerOptions = serde_json::from_str(&json)?;
        self.compiler_options = Some(options);
        Ok(())
    }
}

// Internal usage in check_source_file (lines 439-442)
let compiler_options = self.compiler_options
    .as_ref()
    .map(|opts| opts.to_checker_options())
    .unwrap_or_default();

let checker = ThinCheckerState::new(
    arena,
    binder,
    type_interner,
    file_name,
    compiler_options  // ← Configuration flows here
);
```

**Test infrastructure calls:**

```javascript
const options = parseTestDirectives(code);
parser.setCompilerOptions(JSON.stringify(options));
```

### 7.3 How This Prevents Test-Aware Code in Source

**Before (BAD):**
```rust
// In src/thin_checker.rs
let is_test = file_name.contains("conformance")
    || file_name.contains("test");

if is_test && file_name.contains("strict") {
    // Suppress errors for strict tests
    return;
}
```

**After (GOOD):**
```rust
// In src/thin_checker.rs
// NO file name checks!
// Behavior controlled by self.options.strict

if self.options.no_implicit_any && is_implicit_any(type_id) {
    self.emit_diagnostic(DiagnosticCode::TS7006, ...);
}
```

**Why this works:**
1. Test infrastructure reads `@strict: true` directive
2. Passes `{ strict: true }` to compiler via `setCompilerOptions`
3. Checker uses `self.options.strict` to control behavior
4. **No file name inspection needed**

---

## 8. Integration with differential-test/run-conformance.sh

The test runner script coordinates the entire testing process.

### Current Implementation (differential-test/conformance-runner.mjs)

Already implements most required features:

```javascript
// Line 68-125: parseTestDirectives() function
function parseTestDirectives(code) {
  // ✓ Parses @filename for multi-file tests
  // ✓ Parses compiler options (@strict, @target, etc.)
  // ✓ Returns { options, isMultiFile, cleanCode, files }
}

// Line 130-196: runTsc() - reference implementation
async function runTsc(code, fileName, testOptions) {
  // ✓ Builds CompilerOptions from test directives
  // ✓ Creates TypeScript program with options
  // ✓ Returns diagnostics
}

// Line 284-332: runWasm() - our implementation
async function runWasm(code, fileName, testOptions) {
  const parser = new ThinParser(fileName, code);

  // ✓ Adds lib files
  if (!testOptions.nolib) {
    parser.addLibFile(DEFAULT_LIB_NAME, DEFAULT_LIB_SOURCE);
  }

  // MISSING: Need to pass testOptions to parser
  // TODO: parser.setCompilerOptions(JSON.stringify(testOptions));

  parser.parseSourceFile();
  const checkResult = parser.checkSourceFile();

  return { diagnostics: checkResult.diagnostics };
}
```

### Required Changes

**1. Pass options to WASM compiler:**

```javascript
async function runWasm(code, fileName, testOptions) {
  const parser = new ThinParser(fileName, code);

  // ADD THIS: Configure compiler with test directives
  parser.setCompilerOptions(JSON.stringify(testOptions));

  if (!testOptions.nolib) {
    parser.addLibFile(DEFAULT_LIB_NAME, DEFAULT_LIB_SOURCE);
  }

  parser.parseSourceFile();
  const checkResult = parser.checkSourceFile();

  return { diagnostics: checkResult.diagnostics };
}
```

**2. Support more directives:**

```javascript
function parseTestDirectives(code) {
  // ... existing code ...

  // Add support for more options:
  if (testOptions.noimplicitreturns !== undefined) {
    compilerOptions.noImplicitReturns = testOptions.noimplicitreturns;
  }
  if (testOptions.strictpropertyinitialization !== undefined) {
    compilerOptions.strictPropertyInitialization = testOptions.strictpropertyinitialization;
  }
  if (testOptions.lib) {
    compilerOptions.lib = testOptions.lib.split(',').map(s => s.trim());
  }

  // ... rest of code ...
}
```

**3. Remove lib file detection from WasmProgram:**

Currently in src/lib.rs (lines 1562-1566):

```rust
// REMOVE THIS from WasmProgram.add_file():
let is_lib_file = file_name.contains("lib.d.ts")
    || file_name.contains("lib.es")
    || file_name.contains("lib.dom")
    || file_name.contains("lib.webworker")
    || file_name.contains("lib.scripthost");
```

Instead, test infrastructure should explicitly specify lib files:

```javascript
// In test runner
if (testOptions.lib) {
  for (const libName of testOptions.lib) {
    const libPath = getLibPath(libName);
    const libSource = readFileSync(libPath);
    parser.addLibFile(libName, libSource);
  }
}
```

Or use the new `markAsLibFile()` API (lines 342-345):

```rust
#[wasm_bindgen(js_name = markAsLibFile)]
pub fn mark_as_lib_file(&mut self, file_id: u32) {
    self.lib_file_ids.insert(file_id);
}
```

---

## 9. Architecture Overview

### Component Responsibilities

```
┌───────────────────────────────────────────────────────────────┐
│                   Test Infrastructure Layer                    │
│                  (differential-test/*.mjs)                     │
├───────────────────────────────────────────────────────────────┤
│                                                                │
│  Responsibilities:                                             │
│  • Read test files from disk                                  │
│  • Parse @ directives from comments                           │
│  • Load lib files based on @lib directive                     │
│  • Create CompilerOptions from directives                     │
│  • Invoke compiler with correct configuration                 │
│  • Compare output to baselines                                │
│  • Report results                                             │
│                                                                │
│  KNOWS ABOUT: Test files, directives, baselines               │
│  DOES NOT: Implement type checking logic                      │
│                                                                │
└───────────────────────────────────────────────────────────────┘
                              ▲
                              │ CompilerOptions JSON
                              │ addLibFile(name, source)
                              │ parseSourceFile()
                              │ checkSourceFile()
                              ▼
┌───────────────────────────────────────────────────────────────┐
│                        WASM Public API                         │
│                      (ThinParser struct)                       │
├───────────────────────────────────────────────────────────────┤
│                                                                │
│  Responsibilities:                                             │
│  • Accept compiler configuration via setCompilerOptions        │
│  • Accept lib files via addLibFile                            │
│  • Parse TypeScript code                                      │
│  • Bind symbols                                               │
│  • Type check with provided configuration                     │
│  • Return diagnostics                                         │
│                                                                │
│  KNOWS ABOUT: CompilerOptions, lib files, TypeScript syntax   │
│  DOES NOT: Know about test files, directives, or baselines    │
│                                                                │
└───────────────────────────────────────────────────────────────┘
                              ▲
                              │ CheckerOptions
                              │
                              ▼
┌───────────────────────────────────────────────────────────────┐
│                      Type Checker Core                         │
│                  (ThinCheckerState, solver)                    │
├───────────────────────────────────────────────────────────────┤
│                                                                │
│  Responsibilities:                                             │
│  • Type check TypeScript code                                 │
│  • Resolve types using solver                                 │
│  • Emit diagnostics based on CheckerOptions                   │
│  • Implement TypeScript type system rules                     │
│                                                                │
│  KNOWS ABOUT: TypeScript semantics, type system rules         │
│  DOES NOT: Know about file paths, test infrastructure, or     │
│            special cases for test files                       │
│                                                                │
│  NO file_name.contains() CHECKS ALLOWED!                      │
│                                                                │
└───────────────────────────────────────────────────────────────┘
```

### Separation of Concerns

| Layer | Can Know About | Cannot Know About |
|-------|---------------|-------------------|
| **Test Infrastructure** | Test files, @ directives, baselines, conformance suite structure | Type checking implementation details |
| **WASM Public API** | CompilerOptions, parsing, binding, checking | Test files, baseline locations, directive syntax |
| **Type Checker Core** | TypeScript semantics, compiler options | Test file paths, conformance patterns, baselines |

### Why This Architecture Prevents file_name.contains() Checks

1. **Configuration, Not Detection**
   - Test behavior is controlled by `CompilerOptions`, not file path inspection
   - The checker doesn't need to detect "this is a test" - it just uses the options

2. **Inversion of Control**
   - Test infrastructure tells the compiler what to do
   - Compiler doesn't try to figure out what's expected

3. **Single Responsibility**
   - Checker implements TypeScript rules
   - Test infrastructure handles test execution
   - Clear boundary at WASM API

4. **Testability**
   - Can test checker with any options without special file names
   - Production code uses same paths as test code
   - No hidden behavior based on file names

---

## 10. Implementation Checklist

### Phase 1: Test Infrastructure Enhancements

- [ ] Expand `parseTestDirectives()` to support all compiler options
- [ ] Add support for `@lib` directive
- [ ] Implement lib file loading based on directives
- [ ] Pass `CompilerOptions` to WASM via `setCompilerOptions()`
- [ ] Update `runWasm()` and `runWasmMultiFile()` to use options

### Phase 2: Remove Test-Aware Code from Source

- [ ] Audit all `file_name.contains()` calls in src/
- [ ] Replace with proper option-based logic
- [ ] Remove lib file detection from `WasmProgram.add_file()`
- [ ] Use `markAsLibFile()` API or explicit lib file handling

### Phase 3: Baseline Comparison

- [ ] Implement baseline file reading
- [ ] Add diagnostic message comparison (not just codes)
- [ ] Add position comparison (start, length)
- [ ] Generate baseline update mechanism

### Phase 4: Documentation and Testing

- [ ] Document supported @ directives
- [ ] Add tests for directive parsing
- [ ] Add tests for CompilerOptions flow
- [ ] Verify no file_name checks remain

---

## 11. Success Criteria

1. **No Test-Aware Code**
   - Zero `file_name.contains()` checks in src/
   - Checker behavior determined solely by `CheckerOptions`

2. **Complete Directive Support**
   - All TypeScript test directives parsed and applied
   - Multi-file tests work correctly

3. **Baseline Parity**
   - 95%+ exact match on conformance tests
   - Missing/extra errors clearly reported

4. **Architectural Integrity**
   - Clear separation: test infrastructure vs. compiler
   - No leakage of test logic into production code
   - Maintainable and extensible

---

## Appendix A: Complete Directive Reference

### Type Checking Options
- `@strict: boolean`
- `@noImplicitAny: boolean`
- `@noImplicitReturns: boolean`
- `@noImplicitThis: boolean`
- `@strictNullChecks: boolean`
- `@strictFunctionTypes: boolean`
- `@strictPropertyInitialization: boolean`
- `@strictBindCallApply: boolean`
- `@useUnknownInCatchVariables: boolean`
- `@alwaysStrict: boolean`

### Module Options
- `@module: string` (CommonJS, AMD, UMD, System, ES2015, ES2020, ES2022, ESNext, Node16, NodeNext)
- `@moduleResolution: string` (classic, node, node16, nodenext, bundler)
- `@allowSyntheticDefaultImports: boolean`
- `@esModuleInterop: boolean`

### Target and Lib
- `@target: string` (ES3, ES5, ES2015-ES2022, ESNext)
- `@lib: string` (comma-separated: es5, es2015, dom, webworker, scripthost, etc.)

### JavaScript Support
- `@allowJs: boolean`
- `@checkJs: boolean`
- `@maxNodeModuleJsDepth: number`

### Emit Options
- `@noEmit: boolean`
- `@declaration: boolean`
- `@declarationMap: boolean`
- `@sourceMap: boolean`
- `@outDir: string`
- `@outFile: string`
- `@removeComments: boolean`

### Multi-File
- `@filename: string` - Starts a new file section

---

## Appendix B: Migration Path for Existing Code

### Step 1: Identify Test-Aware Checks

```bash
# Find all file name checks
grep -r "file_name.contains" src/

# Find test-specific conditions
grep -r "is_test" src/
grep -r "conformance" src/
```

### Step 2: For Each Check, Determine the Intent

Example:
```rust
// Current code
if file_name.contains("strict") {
    self.options.strict = true;  // Force strict mode
}
```

Ask: "Why does this test need different behavior?"
Answer: "It should have strict mode enabled"

Solution: Test infrastructure should pass `@strict: true` directive

### Step 3: Replace with Option-Based Logic

```rust
// Before
if file_name.contains("strict") {
    // special logic
}

// After
if self.options.strict {
    // same logic, but controlled by options
}
```

### Step 4: Update Test Infrastructure

```javascript
// Test file has:
// @strict: true

// Test runner now:
const options = parseTestDirectives(code);
parser.setCompilerOptions(JSON.stringify(options));
```

---

## Conclusion

This architecture achieves two critical goals:

1. **Clean Source Code**: The compiler implements TypeScript semantics without test-aware hacks
2. **Comprehensive Testing**: Tests can configure the compiler to match any scenario

By following the principle "test infrastructure reads configuration from test files and passes it to the compiler", we maintain a clear separation of concerns and ensure the compiler remains production-ready.

**Remember:** If a test fails, fix the underlying logic, not by adding special cases for test file names.

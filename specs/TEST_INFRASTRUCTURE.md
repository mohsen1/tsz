# Test Infrastructure Specification

## Executive Summary

This document specifies the testing infrastructure required to achieve TypeScript Compiler (TSC) conformance without polluting source code with test-aware logic. The key architectural principle is: **test infrastructure reads configuration from test files and passes it to the compiler, not the other way around**.

---

## 1. Overview

The test infrastructure processes TypeScript test files that may contain special comment directives. These directives configure how the compiler parses, type-checks, and emits code. The infrastructure supports both single-file and multi-file tests.

### Key Principles

1. **Explicit Configuration**: Compiler options should be passed explicitly, not inferred from file paths
2. **Directive-Based**: Test files use comment directives (e.g., `// @strict: true`) to specify compiler options
3. **Multi-File Support**: Tests can specify multiple files using `// @filename:` directives
4. **Independence**: Each test should be self-contained with explicit option configuration

### Problem Statement

Current architectural debt:
- Source code contains ~40+ `file_name.contains("conformance")` checks
- Checker suppresses errors based on file path patterns
- Test-specific logic leaks into production code paths
- Violates separation of concerns (tests shouldn't affect source behavior)

**The Rule:** Source code must not know about tests. If a test fails, fix the underlying logic, not by adding special cases for test file names.

---

## 2. How TypeScript's Test Suite Works

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

## 3. Test Directive Parsing

Test directives are special comments at the beginning of test files that configure compiler behavior.

### Directive Format

Directives follow this pattern:
```typescript
// @directiveName: value
```

### Common Directives

| Directive | Description | Example |
|-----------|-------------|---------|
| `@strict` | Enable strict mode checking | `// @strict: true` |
| `@target` | Set ECMAScript target version | `// @target: es5` |
| `@module` | Set module system | `// @module: commonjs` |
| `@lib` | Specify lib files to include | `// @lib: esnext` |
| `@noImplicitAny` | Control implicit any checking | `// @noImplicitAny: true` |
| `@strictNullChecks` | Enable strict null checking | `// @strictNullChecks: true` |
| `@declaration` | Generate .d.ts files | `// @declaration: true` |
| `@jsx` | Configure JSX handling | `// @jsx: preserve` |
| `@filename` | Start a new file in multi-file tests | `// @filename: lib.ts` |

### How Directives are Parsed

The test runner (`scripts/run-batch-tests.mjs`) parses these directives:

1. **File Reading**: Test files are read with proper encoding handling (UTF-8, UTF-16 BE/LE)
2. **Directive Extraction**: Comments starting with `// @` are identified as directives
3. **Multi-File Detection**: Files with `@filename:` directives are treated as multi-file tests
4. **Header Parsing**: Lines before the first `@filename:` are treated as global options

```javascript
// Example from run-batch-tests.mjs
function parseMultiFileTest(source) {
    const files = [];
    const lines = source.split('\n');

    let currentFile = null;
    let currentContent = [];
    let headerLines = [];  // Lines before first @filename

    for (const line of lines) {
        // Match @filename: directive (case insensitive)
        const filenameMatch = line.match(/^\/\/\s*@filename:\s*(.+)$/i);

        if (filenameMatch) {
            // Save previous file and start new one
            if (currentFile !== null) {
                files.push({
                    filename: currentFile,
                    content: currentContent.join('\n')
                });
            }
            currentFile = filenameMatch[1].trim();
            currentContent = [];
        } else if (currentFile !== null) {
            currentContent.push(line);
        } else {
            // Header line (compiler options, etc.)
            headerLines.push(line);
        }
    }

    return { files, headerLines };
}
```

### Example Implementation (JavaScript)

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

---

## 4. CompilerOptions Flow

The compiler options flow through several layers from test directives to actual type checking.

### Architecture Overview

```
Test File Directives
    ↓
Test Runner Parser (run-batch-tests.mjs)
    ↓
ResolvedCompilerOptions (src/cli/config.rs)
    ↓
CheckerContext (src/checker/context.rs)
    ↓
CheckerState (src/checker/state.rs)
```

### Configuration Structures

#### 1. CompilerOptions (Raw Configuration)

Located in `src/cli/config.rs`:

```rust
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CompilerOptions {
    pub target: Option<String>,
    pub module: Option<String>,
    pub module_resolution: Option<String>,
    pub jsx: Option<String>,
    pub lib: Option<Vec<String>>,
    pub strict: Option<bool>,
    pub declaration: Option<bool>,
    pub source_map: Option<bool>,
    // ... other options
}
```

#### 2. ResolvedCompilerOptions (Processed Configuration)

Also in `src/cli/config.rs`:

```rust
pub struct ResolvedCompilerOptions {
    pub printer: PrinterOptions,
    pub checker: CheckerOptions,
    pub jsx: Option<JsxEmit>,
    pub lib_files: Vec<PathBuf>,
    pub module_resolution: Option<ModuleResolutionKind>,
    pub emit_declarations: bool,
    pub source_map: bool,
    pub no_emit: bool,
    // ... other resolved options
}

pub struct CheckerOptions {
    pub strict: bool,
}
```

#### 3. CheckerContext (Runtime State)

Located in `src/checker/context.rs`:

```rust
pub struct CheckerContext<'a> {
    pub arena: &'a NodeArena,
    pub binder: &'a BinderState,
    pub types: &'a TypeInterner,
    pub file_name: String,

    // Strict mode flags (all controlled by strict option)
    pub no_implicit_any: bool,
    pub no_implicit_returns: bool,
    pub use_unknown_in_catch_variables: bool,
    pub strict_function_types: bool,
    pub strict_property_initialization: bool,
    pub strict_null_checks: bool,
    pub no_implicit_this: bool,

    // ... other state
}
```

### Option Resolution Process

The `resolve_compiler_options` function in `src/cli/config.rs` converts raw options to resolved options:

```rust
pub fn resolve_compiler_options(
    options: Option<&CompilerOptions>,
) -> Result<ResolvedCompilerOptions> {
    let mut resolved = ResolvedCompilerOptions::default();
    let Some(options) = options else {
        return Ok(resolved);
    };

    // Parse target
    if let Some(target) = options.target.as_deref() {
        resolved.printer.target = parse_script_target(target)?;
    }

    // Parse module kind
    if let Some(module) = options.module.as_deref() {
        resolved.printer.module = parse_module_kind(module)?;
    }

    // Parse strict mode
    if let Some(strict) = options.strict {
        resolved.checker.strict = strict;
    }

    // ... other options

    Ok(resolved)
}
```

### Data Flow Diagram

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
│     const parser = new Parser(fileName, code)               │
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
│                     (Parser public API)                          │
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
│                 (src/checker/state.rs)                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  CheckerState::new(                                              │
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

---

## 5. Strict Mode Configuration

Strict mode is a meta-option that enables multiple strict checking flags.

### What Strict Mode Enables

When `strict: true` is set, the following flags are enabled:

```rust
// From src/checker/context.rs
impl<'a> CheckerContext<'a> {
    pub fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a TypeInterner,
        file_name: String,
        strict: bool,  // Single strict flag controls all below
    ) -> Self {
        CheckerContext {
            // Strict mode enables all these checks
            no_implicit_any: strict,
            use_unknown_in_catch_variables: strict,
            strict_function_types: strict,
            strict_property_initialization: strict,
            strict_null_checks: strict,
            no_implicit_this: strict,
            // ... rest of initialization
        }
    }
}
```

### Individual Strict Flags

Each flag can be controlled independently in non-strict mode:

| Flag | Description | Error Codes |
|------|-------------|-------------|
| `noImplicitAny` | Error on expressions with implicit 'any' type | TS7006, TS7031 |
| `strictNullChecks` | null and undefined are not assignable to other types | TS2322, TS2345 |
| `strictFunctionTypes` | Check function parameters contravariantly | TS2322 |
| `strictPropertyInitialization` | Check class properties are initialized | TS2564 |
| `noImplicitThis` | Error on 'this' expressions with type 'any' | TS2683 |
| `useUnknownInCatchVariables` | Catch clause variables default to 'unknown' | TS18046 |

### Setting Strict Mode in Tests

```typescript
// Option 1: Enable all strict checks
// @strict: true

function foo(x) {  // TS7006: Parameter 'x' implicitly has an 'any' type
    return x;
}

// Option 2: Enable individual checks
// @noImplicitAny: true
// @strictNullChecks: true

function bar(x: string | null) {
    console.log(x.length);  // TS2531: Object is possibly 'null'
}
```

### Programmatic Configuration

When creating a checker in Rust tests:

```rust
use crate::checker::CheckerState;
use crate::binder::BinderState;
use crate::parser::ParserState;
use crate::checker::context::CheckerOptions;
use crate::solver::TypeInterner;

#[test]
fn test_strict_mode_checking() {
    let source = r#"
function foo(x) {
    return x + 1;
}
"#;

    let mut parser = ParserState::new(
        "test.ts".to_string(),
        source.to_string()
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();

    // Create checker with strict mode ENABLED
    let checker_options = CheckerOptions { strict: true, ..Default::default() }
        .apply_strict_defaults();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        checker_options,
    );

    checker.check_source_file(root);

    // Should have TS7006: implicit any error
    assert!(!checker.ctx.diagnostics.is_empty());
}
```

---

## 6. The TypeScript/tests/ Directory Structure

```
TypeScript/tests/
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

## 7. Examples of @ Directives and Their Meanings

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
3. Passes to `Parser.setCompilerOptions(...)`
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
// @strict: true
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
3. Calls `Parser.addLibFile(...)` for each
4. Checker has access to `Promise` and `document` types

---

## 8. Configuring Tests Properly

### Best Practices

1. **Always Use Explicit Options**: Don't rely on file path inspection to determine compiler options
2. **Use Directives**: Specify all required compiler options via test directives
3. **Be Self-Contained**: Tests should work independently of their file location
4. **Document Intent**: Use directives to make test requirements clear

### Incorrect Approach (Anti-Pattern)

```rust
// DON'T: Infer options from file path
fn should_use_strict_mode(file_path: &str) -> bool {
    file_path.contains("strict") || file_path.contains("Strict")
}
```

This approach is problematic because:
- It couples test behavior to file system structure
- It makes tests fragile when files are moved
- It's not obvious from the test file what options are used
- It violates the principle of explicit configuration

### Correct Approach

```typescript
// test-strict-mode.ts
// @strict: true

// Test code here
let x: number = 42;
```

In the test infrastructure:

```rust
// Create checker with explicit strict flag
let mut checker = CheckerState::new(
    arena,
    binder,
    types,
    file_name,
    strict  // Passed explicitly, not inferred
);
```

---

## 9. Implementation Requirements

### 9.1 Where Directive Parsing Should Happen

**CORRECT: Test Infrastructure**

```javascript
// In conformance/conformance-runner.mjs

function parseTestDirectives(code) {
  // Extract @strict, @noImplicitAny, @target, etc.
  // Build CompilerOptions object
  // Return { options, files, isMultiFile }
}

async function runTest(testFile) {
  const code = readFileSync(testFile);
  const { options, files } = parseTestDirectives(code);

  const parser = new Parser(testFile, code);
  parser.setCompilerOptions(JSON.stringify(options));

  // Run checker with configured options
  const result = parser.checkSourceFile();

  return result;
}
```

**WRONG: Source Code**

```rust
// BAD - DO NOT DO THIS in src/checker/state.rs

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

### 9.2 How to Pass Configuration to the Checker

**Use the existing `CompilerOptions` API:**

```rust
// Public API in src/lib.rs (already implemented, lines 330-338)

impl Parser {
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

let checker = CheckerState::new(
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

### 9.3 How This Prevents Test-Aware Code in Source

**Before (BAD):**
```rust
// In src/checker/state.rs
let is_test = file_name.contains("conformance")
    || file_name.contains("test");

if is_test && file_name.contains("strict") {
    // Suppress errors for strict tests
    return;
}
```

**After (GOOD):**
```rust
// In src/checker/state.rs
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

## 10. Example Workflows

### Single-File Test

```typescript
// strictModeExample.ts
// @strict: true
// @target: es2015

function processUser(user) {  // Will error: implicit any
    return user.name.toUpperCase();
}

const result = processUser({ name: "Alice" });
```

**Flow:**

1. Test file contains directives at the top
2. Test runner extracts `strict: true` and `target: es2015`
3. Creates `CompilerOptions { strict: Some(true), target: Some("es2015"), ... }`
4. Resolves to `ResolvedCompilerOptions` with `checker.strict = true`
5. Creates `CheckerContext` with all strict flags enabled
6. `CheckerState` performs type checking with strict mode
7. Produces TS7006 diagnostic for implicit 'any' parameter

### Multi-File Test

```typescript
// multiFileTest.ts
// @strict: true
// @filename: lib.ts
export interface User {
    name: string;
    age: number;
}

// @filename: main.ts
import { User } from "./lib";

function greet(user: User) {
    console.log(`Hello, ${user.name}!`);
}

greet({ name: "Bob" });  // TS2345: Missing 'age' property
```

**Flow:**

1. Test runner detects `@filename:` directives
2. Splits into two files: `lib.ts` and `main.ts`
3. Header directives (`@strict: true`) apply to all files
4. Each file is parsed, bound, and checked independently
5. Import resolution connects the files
6. Type checking detects missing property in strict mode

---

## 11. Architecture Overview

### Component Responsibilities

```
┌───────────────────────────────────────────────────────────────┐
│                   Test Infrastructure Layer                    │
│                  (conformance/*.mjs)                     │
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
│                      (Parser struct)                       │
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
│                  (CheckerState, solver)                    │
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

## 12. Common Patterns

### Pattern 1: Testing Strict vs Non-Strict Behavior

```typescript
// @strict: false
// Test that shows different behavior in strict/non-strict mode

let implicitAny = function(x) { return x; };  // OK in non-strict
```

```typescript
// @strict: true
// Same code should error in strict mode

let implicitAny = function(x) { return x; };  // TS7006
```

### Pattern 2: Module Resolution Tests

```typescript
// @module: commonjs
// @filename: a.ts
export const value = 42;

// @filename: b.ts
import { value } from "./a";
console.log(value);
```

### Pattern 3: Target-Specific Tests

```typescript
// @target: es5
// Test ES5 output (no arrow functions, etc.)

const add = (a: number, b: number) => a + b;
// Should transpile to function expression
```

### Pattern 4: Declaration Emit Tests

```typescript
// @declaration: true
// @emitDeclarationOnly: true

export class MyClass {
    private x: number;
    constructor(x: number) {
        this.x = x;
    }
}
// Should generate .d.ts with public API only
```

### Pattern 5: JSX Tests

```typescript
// @jsx: preserve
// @filename: Component.tsx

const element = <div>Hello</div>;
// JSX should be preserved in output
```

---

## 13. Implementation Checklist

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

## 14. Success Criteria

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

## Summary

The test infrastructure follows these principles:

1. **Explicit Configuration**: Always pass compiler options explicitly through directives or function parameters
2. **No Path Inference**: Never infer compiler behavior from file paths
3. **Self-Contained Tests**: Each test file specifies its own requirements
4. **Clear Separation**: Distinct layers for parsing directives, resolving options, and type checking
5. **Strict Mode Cascade**: The `strict` flag is a meta-option that enables multiple strict checks

When writing tests:
- Use `// @directive: value` comments to configure behavior
- Create checkers with explicit `strict` parameter (true/false)
- Don't rely on file naming conventions for configuration
- Test both strict and non-strict modes explicitly when behavior differs

This architecture ensures tests are portable, maintainable, and clearly express their requirements.

---

**Document Version**: 1.1
**Last Updated**: 2025-01-17
**Status**: Active

---

**Related Documents**:
- `PROJECT_DIRECTION.md` - Overall project priorities and rules
- `specs/ARCHITECTURE_CLEANUP.md` - Architecture cleanup documentation
- `specs/DIAGNOSTICS.md` - Diagnostic implementation guidelines

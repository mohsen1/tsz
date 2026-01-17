# Test Infrastructure Documentation

This document describes how the test infrastructure works in the TypeScript compiler, including how test directives are parsed, how compiler options are configured, and the proper way to set up tests.

## Table of Contents

1. [Overview](#overview)
2. [Test Directive Parsing](#test-directive-parsing)
3. [CompilerOptions Flow](#compileroptions-flow)
4. [Configuring Tests Properly](#configuring-tests-properly)
5. [Strict Mode Configuration](#strict-mode-configuration)
6. [Example Workflow](#example-workflow)
7. [Common Patterns](#common-patterns)

## Overview

The test infrastructure processes TypeScript test files that may contain special comment directives. These directives configure how the compiler parses, type-checks, and emits code. The infrastructure supports both single-file and multi-file tests.

### Key Principles

1. **Explicit Configuration**: Compiler options should be passed explicitly, not inferred from file paths
2. **Directive-Based**: Test files use comment directives (e.g., `// @strict: true`) to specify compiler options
3. **Multi-File Support**: Tests can specify multiple files using `// @filename:` directives
4. **Independence**: Each test should be self-contained with explicit option configuration

## Test Directive Parsing

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

## CompilerOptions Flow

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
ThinCheckerState (src/thin_checker.rs)
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
    pub arena: &'a ThinNodeArena,
    pub binder: &'a ThinBinderState,
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

## Configuring Tests Properly

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
let mut checker = ThinCheckerState::new(
    arena,
    binder,
    types,
    file_name,
    strict  // Passed explicitly, not inferred
);
```

## Strict Mode Configuration

Strict mode is a meta-option that enables multiple strict checking flags.

### What Strict Mode Enables

When `strict: true` is set, the following flags are enabled:

```rust
// From src/checker/context.rs
impl<'a> CheckerContext<'a> {
    pub fn new(
        arena: &'a ThinNodeArena,
        binder: &'a ThinBinderState,
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
use crate::thin_checker::ThinCheckerState;
use crate::thin_binder::ThinBinderState;
use crate::thin_parser::ThinParserState;
use crate::solver::TypeInterner;

#[test]
fn test_strict_mode_checking() {
    let source = r#"
function foo(x) {
    return x + 1;
}
"#;

    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        source.to_string()
    );
    let root = parser.parse_source_file();

    let mut binder = ThinBinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();

    // Create checker with strict mode ENABLED
    let mut checker = ThinCheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        true  // strict = true enables all strict checks
    );

    checker.check_source_file(root);

    // Should have TS7006: implicit any error
    assert!(!checker.ctx.diagnostics.is_empty());
}
```

## Example Workflow

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
6. `ThinCheckerState` performs type checking with strict mode
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

### Rust Unit Test Pattern

```rust
#[test]
fn test_strict_null_checks() {
    let source = r#"
let x: string = "hello";
let y: string = null;  // Should error in strict mode
"#;

    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        source.to_string()
    );
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = ThinBinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();

    // Test with strict mode OFF
    let mut checker_non_strict = ThinCheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        false  // strict = false
    );
    checker_non_strict.check_source_file(root);
    assert!(checker_non_strict.ctx.diagnostics.is_empty());

    // Test with strict mode ON
    let mut checker_strict = ThinCheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        true  // strict = true
    );
    checker_strict.check_source_file(root);

    // Should have error: Type 'null' not assignable to 'string'
    assert!(!checker_strict.ctx.diagnostics.is_empty());
    let codes: Vec<u32> = checker_strict.ctx.diagnostics
        .iter()
        .map(|d| d.code)
        .collect();
    assert!(codes.contains(&2322));  // TS2322
}
```

## Common Patterns

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

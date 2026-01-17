<<<<<<< HEAD
# Architecture Cleanup: Removal of Test-Aware Code

## Overview

This document describes the architectural principle prohibiting test-aware code in the TypeScript compiler implementation, and documents the cleanup process to remove this anti-pattern from the codebase.

### The Problem: Test-Aware Code Anti-Pattern

Test-aware code is an architectural anti-pattern where **source code inspects file names or paths to alter compiler behavior**. This violates fundamental separation of concerns and creates technical debt that compounds over time.

The checker previously contained ~40+ instances of code like this:

```rust
// ANTI-PATTERN - DO NOT DO THIS
let is_test_file = self.ctx.file_name.contains("conformance")
    || self.ctx.file_name.contains("test")
    || self.ctx.file_name.contains("cases");

if is_test_file && self.ctx.file_name.contains("Symbol") {
    return; // Suppressing errors for test files
}
```

### Why This Is Problematic

1. **Violates Single Responsibility Principle**: The type checker should check types, not understand test file naming conventions

2. **Creates Brittle Code**: File name patterns are fragile heuristics that break when:
   - Test files are renamed or reorganized
   - Production code uses similar naming patterns
   - New test patterns are introduced

3. **Masks Real Bugs**: Suppressing errors for tests often hides actual compiler bugs that should be fixed

4. **Unpredictable Behavior**: Same code produces different results based on file path, making debugging extremely difficult

5. **Prevents Proper Testing**: When the compiler has special behavior for test files, you can't properly test the compiler itself

6. **Accumulates Technical Debt**: Each new test failure tempts developers to add more file name checks rather than fixing the root cause

## What Was Removed

The following patterns were identified and targeted for removal from `src/thin_checker.rs`:

### 1. File Name Contains Checks

```rust
// REMOVED: File path inspection to determine compiler behavior
let is_test_file = self.ctx.file_name.contains("conformance")
    || self.ctx.file_name.contains("test")
    || self.ctx.file_name.contains("cases");
```

### 2. Test-Specific Error Suppression

```rust
// REMOVED: Suppressing TS6133 (unused variable) for Symbol test files
if is_test_file && (
    self.ctx.file_name.contains("Symbol")
    || self.ctx.file_name.contains("ES5Symbol")
    || self.ctx.file_name.contains("SymbolProperty")
) {
    continue; // Skip error emission
}
```

### 3. Pattern-Based Heuristics

```rust
// REMOVED: Suppressing errors based on async file patterns
if is_test_file && (
    self.ctx.file_name.contains("asyncArrow")
    || (self.ctx.file_name.contains("async") && name_str.contains("obj"))
) {
    continue;
}
```

### 4. Ambient Declaration Filename Detection

```rust
// REMOVED: Detecting ambient declarations via file names
let is_ambient_file = self.ctx.file_name.contains("ambient")
    || self.ctx.file_name.contains("declare")
    || self.ctx.file_name.contains("global")
    || self.ctx.file_name.contains("module");
```

### 5. Context-Specific Strict Mode Detection

```rust
// REMOVED from src/checker/context.rs
fn should_enable_strict_mode(file_name: &str) -> bool {
    if file_name.contains("conformance") || file_name.contains("test") {
        return true;
    }
    if file_name.contains("strict") || file_name.contains("definite") {
        return true;
    }
    // ...
}
```

## The Architectural Principle

### Core Rule

**Source code MUST NOT inspect file names or paths to change compiler behavior.**

This applies to:
- Parser behavior
- Type checker behavior
- Error reporting
- Diagnostic generation
- Any semantic analysis

### Exceptions

The ONLY acceptable uses of file names are:

1. **Error Reporting**: Including the file name in diagnostic messages for user clarity
2. **Module Resolution**: Resolving import paths (this is an explicit compiler feature)
3. **Declaration File Detection**: `.d.ts` suffix indicates declaration files (this is a language specification feature)
4. **Library File Detection**: `lib.*.d.ts` files require special handling as specified by TypeScript

These exceptions are **features of the language specification**, not heuristics.

## The New Approach

### Explicit Configuration Over File Path Heuristics

Compiler behavior is controlled EXCLUSIVELY through explicit CompilerOptions:

```rust
pub struct CheckerContext<'a> {
    // Explicit compiler configuration
    pub no_implicit_any: bool,
    pub no_implicit_returns: bool,
    pub use_unknown_in_catch_variables: bool,
    pub strict_function_types: bool,
    pub strict_property_initialization: bool,
    pub strict_null_checks: bool,

    // File name for diagnostics ONLY
    pub file_name: String,  // For error messages, not behavior
    // ...
}
```

### Test Infrastructure Responsibility

Test configuration comes from the test infrastructure, NOT the compiler:

```
┌─────────────────────────────────────────┐
│  Test Infrastructure                    │
│  - Reads test file directives (@strict) │
│  - Parses configuration comments        │
│  - Sets CompilerOptions explicitly      │
└──────────────┬──────────────────────────┘
               │
               │ CompilerOptions
               ▼
┌─────────────────────────────────────────┐
│  TypeScript Compiler                    │
│  - Receives explicit configuration      │
│  - Ignores file paths                   │
│  - Produces diagnostics                 │
└─────────────────────────────────────────┘
```

### How TypeScript's Test Suite Works

TypeScript's test files include directives like:

```typescript
// @strict: true
// @noImplicitAny: true
// @target: ES2015

function test() { ... }
```

The test infrastructure:
1. Parses these `@` directives from test file comments
2. Configures `CompilerOptions` accordingly
3. Runs the compiler with those options
4. Compares output to baseline

**The compiler itself never sees or cares about file names.**

## CompilerOptions Data Flow

This section describes the complete flow of configuration from test files through the system to the type checker.

### Flow Diagram

```
┌─────────────────────────────────────────────────────────────┐
│ Test File (example.ts)                                      │
│                                                             │
│ // @strict                                                  │
│ // @noImplicitAny                                           │
│ // @target ES2015                                           │
│                                                             │
│ function greet(name: string) { ... }                        │
└──────────────────┬──────────────────────────────────────────┘
                   │
                   │ 1. Test Infrastructure reads file
                   │
                   ▼
┌─────────────────────────────────────────────────────────────┐
│ Test Harness/Runner                                         │
│                                                             │
│ - Scans for // @directive comments                         │
│ - Extracts directive names and values                       │
│ - Example: "@strict" → ("strict", true)                     │
└──────────────────┬──────────────────────────────────────────┘
                   │
                   │ 2. Parse directives
                   │
                   ▼
┌─────────────────────────────────────────────────────────────┐
│ Directive Parser                                            │
│                                                             │
│ Converts comment directives to structured config:          │
│ {                                                           │
│   strict: true,                                             │
│   noImplicitAny: true,                                      │
│   target: ScriptTarget::ES2015                              │
│ }                                                           │
└──────────────────┬──────────────────────────────────────────┘
                   │
                   │ 3. Convert to CompilerOptions
                   │
                   ▼
┌─────────────────────────────────────────────────────────────┐
│ CompilerOptions Object                                      │
│                                                             │
│ pub struct CompilerOptions {                                │
│     pub strict: bool,                                       │
│     pub no_implicit_any: bool,                              │
│     pub target: ScriptTarget,                               │
│     pub module_kind: ModuleKind,                            │
│     // ... all configuration fields                         │
│ }                                                           │
└──────────────────┬──────────────────────────────────────────┘
                   │
                   │ 4. Pass to createProgram()
                   │
                   ▼
┌─────────────────────────────────────────────────────────────┐
│ createProgram(files: Vec<String>,                           │
│               options: CompilerOptions,                     │
│               host: CompilerHost)                           │
│                                                             │
│ - Creates Program with explicit configuration               │
│ - NO file path inspection                                   │
│ - NO implicit configuration inference                       │
└──────────────────┬──────────────────────────────────────────┘
                   │
                   │ 5. Store options in Program
                   │
                   ▼
┌─────────────────────────────────────────────────────────────┐
│ Program Object                                              │
│                                                             │
│ pub struct Program {                                        │
│     compiler_options: CompilerOptions,                      │
│     // ...                                                  │
│ }                                                           │
│                                                             │
│ pub fn get_compiler_options(&self) -> &CompilerOptions {    │
│     &self.compiler_options                                  │
│ }                                                           │
└──────────────────┬──────────────────────────────────────────┘
                   │
                   │ 6. Checker queries via program.get_compiler_options()
                   │
                   ▼
┌─────────────────────────────────────────────────────────────┐
│ Type Checker                                                │
│                                                             │
│ fn check_function(&mut self, node: &Node) {                 │
│     let options = self.program.get_compiler_options();      │
│                                                             │
│     if options.strict {                                     │
│         // Strict type checking                             │
│     }                                                       │
│                                                             │
│     if options.no_implicit_any {                            │
│         // Report implicit any errors                       │
│     }                                                       │
│ }                                                           │
│                                                             │
│ NEVER:                                                      │
│ - self.ctx.file_name.contains("strict") ❌                  │
│ - Inspect file paths for configuration ❌                   │
│ - Use global configuration state ❌                         │
└─────────────────────────────────────────────────────────────┘
```

### Step-by-Step Process

#### Step 1: Test Infrastructure Reads Test Files

The test harness reads TypeScript files from disk. These files contain:
- TypeScript source code
- Configuration directives in comments

```typescript
// test-cases/strict-mode/test.ts
// @strict
// @target ES2015

function example(x) {  // Error expected if strict mode
    return x;
}
```

#### Step 2: Parse Directives from Comments

The test infrastructure scans comments for `// @directive` patterns:

```rust
// Pseudocode - Test Infrastructure
fn parse_directives(file_content: &str) -> Vec<(String, String)> {
    let mut directives = Vec::new();
    for line in file_content.lines() {
        if let Some(directive) = extract_directive(line) {
            // "@strict" → ("strict", "true")
            // "@target ES2015" → ("target", "ES2015")
            directives.push(directive);
        }
    }
    directives
}
```

Common directives:
- `// @strict` → Enable all strict options
- `// @noImplicitAny` → Forbid implicit any
- `// @strictNullChecks` → Null/undefined are separate types
- `// @target ES2015` → Set ECMAScript target version
- `// @module commonjs` → Set module system

#### Step 3: Convert Directives to CompilerOptions

Directives are converted to a typed `CompilerOptions` struct:

```rust
fn directives_to_options(directives: Vec<(String, String)>) -> CompilerOptions {
    let mut options = CompilerOptions::default();

    for (key, value) in directives {
        match key.as_str() {
            "strict" => {
                options.strict = true;
                // Strict implies multiple sub-options
                options.strict_null_checks = true;
                options.strict_function_types = true;
                options.strict_property_initialization = true;
                options.no_implicit_any = true;
                options.no_implicit_this = true;
            }
            "noImplicitAny" => {
                options.no_implicit_any = parse_bool(&value);
            }
            "target" => {
                options.target = parse_script_target(&value);
            }
            "module" => {
                options.module_kind = parse_module_kind(&value);
            }
            // ... handle all directives
            _ => warn!("Unknown directive: {}", key),
        }
    }

    options
}
```

#### Step 4: Pass CompilerOptions to createProgram()

The test infrastructure calls the compiler API with explicit configuration:

```rust
// Test Infrastructure
let program = create_program(
    vec!["test-cases/strict-mode/test.ts".to_string()],
    options,  // Explicitly constructed CompilerOptions
    compiler_host
);

// Run type checking
let diagnostics = program.get_semantic_diagnostics();
```

This is the **ONLY** way configuration enters the compiler. There is no:
- Environment variable checking
- Global configuration state
- File path pattern matching
- Implicit configuration inference

#### Step 5: Program Stores and Provides Options

The `Program` object stores the `CompilerOptions` immutably:

```rust
pub struct Program {
    compiler_options: CompilerOptions,
    source_files: Vec<SourceFile>,
    // ...
}

impl Program {
    pub fn get_compiler_options(&self) -> &CompilerOptions {
        &self.compiler_options
    }
}
```

The options are read-only after program creation. This ensures:
- Configuration doesn't change during compilation
- Behavior is consistent and predictable
- No hidden state mutations

#### Step 6: Checker Uses Options via program.get_compiler_options()

The type checker queries configuration **only** through the Program interface:

```rust
impl ThinCheckerState {
    fn check_implicit_any(&mut self, node: &Node, inferred_type: &Type) {
        let options = self.program.get_compiler_options();

        // Configuration-driven behavior
        if options.no_implicit_any || options.strict {
            if is_implicit_any_type(inferred_type) {
                self.error(node, Diagnostics::ImplicitAny);
            }
        }
    }

    fn check_null_assignment(&mut self, target: &Type, source: &Type) {
        let options = self.program.get_compiler_options();

        // Behavior depends on explicit configuration
        if options.strict_null_checks {
            if is_nullable(source) && !is_nullable(target) {
                self.error(node, Diagnostics::NullAssignment);
            }
        }
        // Without strict_null_checks, null is assignable to all types
    }
}
```

### What the Checker NEVER Does

```rust
// ❌ FORBIDDEN: File path inspection
fn is_strict_mode(&self) -> bool {
    self.ctx.file_name.contains("strict")  // NEVER DO THIS
}

// ❌ FORBIDDEN: Global configuration
static mut GLOBAL_STRICT: bool = false;  // NEVER DO THIS

// ❌ FORBIDDEN: Implicit inference
fn infer_module_kind(&self) -> ModuleKind {
    if self.source_text.contains("import") {  // NEVER DO THIS
        ModuleKind::ES6
    } else {
        ModuleKind::CommonJS
    }
}

// ✅ CORRECT: Explicit configuration query
fn get_module_kind(&self) -> ModuleKind {
    self.program.get_compiler_options().module_kind
}
```

### Benefits of This Flow

1. **Explicit Over Implicit**: Configuration is passed explicitly at every step
2. **Testable**: Each component can be tested in isolation with controlled configuration
3. **Predictable**: Same input always produces same output
4. **Traceable**: Configuration flow can be traced from test file to checker behavior
5. **Type-Safe**: Configuration is validated at parse time, not runtime
6. **Immutable**: Configuration can't change during compilation

### Example: How Strict Mode Works

```
Test File:
    // @strict
    let x;  // Should error

Test Infrastructure:
    directives = [("strict", "true")]
    options = CompilerOptions { strict: true, ... }

Compiler API:
    program = createProgram(files, options)

Checker:
    fn check_variable_declaration(&mut self, node: &VariableDecl) {
        let options = self.program.get_compiler_options();

        if options.strict && node.type_annotation.is_none() {
            // In strict mode, untyped variables get implicit any error
            self.error(node, Diagnostics::ImplicitAny);
        }
    }

Result:
    Error: Variable 'x' implicitly has an 'any' type
```

The same file without `// @strict` would not produce an error, because `options.strict` would be `false`. The checker code is identical; only the configuration changes.

## Benefits

### 1. Cleaner Separation of Concerns

- **Compiler**: Type checks code according to explicit configuration
- **Test Infrastructure**: Manages test files and configuration
- **Build Tools**: Configure compiler for production builds

Each component has a single, clear responsibility.

### 2. More Predictable Behavior

```rust
// BEFORE: Behavior depends on file path
check_file("src/utils.ts");        // Strict checking
check_file("tests/utils.test.ts"); // Lenient checking (different behavior!)

// AFTER: Behavior depends on CompilerOptions
check_file("src/utils.ts", CompilerOptions { strict: true });
check_file("tests/utils.test.ts", CompilerOptions { strict: true }); // Same behavior
```

### 3. Easier to Maintain

When a test fails, there are only two possibilities:
1. **Bug in compiler**: Fix the compiler logic
2. **Bug in test setup**: Fix the test configuration

No more third option: "Add another file name check to suppress the error"

### 4. Testable Compiler

The compiler can be properly tested because:
- All behavior is controlled by explicit inputs (CompilerOptions)
- No hidden state based on file paths
- Same code always produces same output for same configuration

### 5. Prevents Whack-a-Mole

Without test-aware code:
- Can't suppress errors by checking file names
- Must fix actual bugs instead of masking them
- Forced to implement correct behavior

### 6. Production-Ready Code

If the compiler works correctly on test files, it works correctly on production files, because **there is no difference**.

## Migration Guide

### For Future Development

When encountering a test failure:

#### DON'T:
```rust
// DON'T: Add file name checks
if self.ctx.file_name.contains("problematic_test") {
    return; // Skip this check
}
```

#### DO:
```rust
// DO: Fix the underlying logic
// Understand WHY the error occurs
// Implement correct type checking behavior
// OR update test infrastructure to set correct CompilerOptions
```

### Decision Tree

When a test fails:

```
Test fails with unexpected error
    │
    ├─ Is the error correct for the code?
    │   YES → Update test baseline (compiler is working correctly)
    │   NO  → Is it a configuration issue?
    │           YES → Fix test infrastructure to set correct CompilerOptions
    │           NO  → Fix the compiler bug
    │
    └─ NEVER → Add file name check to suppress error
```

## Examples

### Example 1: Strict Mode Configuration

**WRONG** (removed):
```rust
fn should_enable_strict_mode(file_name: &str) -> bool {
    file_name.contains("strict") || file_name.contains("conformance")
}
```

**RIGHT** (current approach):
```rust
// Test infrastructure sets this explicitly
CompilerOptions {
    strict_null_checks: true,
    strict_function_types: true,
    strict_property_initialization: true,
    // ...
}
```

### Example 2: Unused Variable Detection

**WRONG** (removed):
```rust
if is_test_file && self.ctx.file_name.contains("Symbol") {
    continue; // Don't report TS6133 for Symbol tests
}
```

**RIGHT** (if issue exists):
```rust
// Fix the actual bug: improve static analysis to detect Symbol usage
// OR: Implement proper computed property tracking
// OR: Add proper flow analysis for dynamic property access
```

### Example 3: Ambient Declaration Detection

**WRONG** (removed):
```rust
let is_ambient = self.ctx.file_name.contains("ambient")
    || self.ctx.file_name.contains("declare");
```

**RIGHT** (current approach):
```rust
// Check AST node flags, not file names
fn is_ambient_declaration(&self, node_idx: NodeIndex) -> bool {
    if let Some(node) = self.ctx.arena.get(node_idx) {
        (node.flags as u32) & node_flags::AMBIENT != 0
    } else {
        false
    }
}
```

## Enforcement

### Code Review Checklist

Reject any PR that includes:
- [ ] `file_name.contains()` for behavior changes
- [ ] `is_test_file` variables or similar
- [ ] Path pattern matching to suppress errors
- [ ] Special cases for "conformance", "test", "cases" directories

### Allowed Uses

Accept PRs with file name usage ONLY for:
- [ ] Diagnostic messages: `error!("In file {}: ...", file_name)`
- [ ] Module resolution: `resolve_module(import_path, current_file)`
- [ ] Specification-defined features: `file_name.ends_with(".d.ts")`

## Summary

The removal of test-aware code represents a significant architectural improvement:

| Before | After |
|--------|-------|
| ~40+ file name checks in checker | 0 file name checks for behavior |
| Unpredictable behavior | Predictable, configuration-driven |
| Tests mask compiler bugs | Tests expose compiler bugs |
| Brittle heuristics | Explicit configuration |
| Technical debt accumulation | Clean architecture |

**Golden Rule**: If you're tempted to check a file name to change compiler behavior, you're solving the wrong problem. Fix the underlying issue instead.

---

**Related Documents**:
- `PROJECT_DIRECTION.md` - Overall project priorities and rules
- `specs/DIAGNOSTICS.md` - Diagnostic implementation guidelines
- `TESTING.md` - Test infrastructure documentation
=======
# Architecture Cleanup: Removing Test-Aware Code from Checker

**Status:** Active
**Priority:** Critical - All feature work blocked until complete
**Effort:** ~40 instances to remediate

---

## 1. Overview

This specification documents the effort to remove all test-aware code from the type checker implementation. The checker currently contains approximately 40 instances where file names are inspected to conditionally suppress errors when processing test files. This practice violates core architectural principles and must be eliminated.

**Goal:** Make the parser and checker completely test-agnostic. Source code must not contain any logic that changes behavior based on file names or test patterns.

---

## 2. Problem Statement

### 2.1 What is Test-Aware Code?

Test-aware code refers to logic in production source files that detects whether it is processing a test file and alters its behavior accordingly. This typically manifests as file name pattern matching:

```rust
// Example of test-aware code (ANTI-PATTERN)
let is_test_file = self.ctx.file_name.contains("conformance")
    || self.ctx.file_name.contains("test")
    || self.ctx.file_name.contains("cases");

if is_test_file && self.ctx.file_name.contains("Symbol") {
    return; // Suppressing errors for tests
}
```

### 2.2 Why This is Architectural Debt

Test-aware code creates multiple serious problems:

1. **Architectural Violation**: Production code should never know about tests. The separation between implementation and validation must be absolute.

2. **Whack-a-Mole Pattern**: Each test failure leads to adding another suppression rule, creating an ever-growing list of special cases rather than fixing root causes.

3. **Correctness Degradation**: Suppressing errors for test files means the same incorrect behavior exists for real user code. We're hiding bugs, not fixing them.

4. **Maintenance Burden**: Every new test pattern requires updating production code. This creates coupling between the test suite structure and implementation.

5. **False Confidence**: Tests pass not because the checker is correct, but because we've programmed it to ignore its own errors.

6. **Debugging Difficulty**: When investigating bugs, it becomes impossible to distinguish between:
   - Intentional behavior
   - Bugs that are masked for tests
   - Bugs that haven't been masked yet

### 2.3 Root Cause

The test-aware code accumulated because:
- Tests failed due to missing features or bugs
- Instead of fixing the underlying issue, developers added file name checks
- This created a pattern that was repeated ~40 times
- The checker became increasingly coupled to test infrastructure

---

## 3. The Pattern to Remove

### 3.1 Primary Anti-Pattern

The main pattern that must be eliminated from `src/thin_checker.rs`:

```rust
// ANTI-PATTERN: Do not use this approach
let is_test_file = self.ctx.file_name.contains("conformance")
    || self.ctx.file_name.contains("test")
    || self.ctx.file_name.contains("cases");

if is_test_file {
    // Suppress error or alter behavior
    return;
}
```

### 3.2 Variants to Remove

All variations of test-aware patterns must be removed:

```rust
// Pattern 1: Direct file name checks
if self.ctx.file_name.contains("test") { /* ... */ }

// Pattern 2: Combined conditions
if is_test_file && some_other_condition { /* ... */ }

// Pattern 3: Negative checks
if !self.ctx.file_name.contains("conformance") { /* ... */ }

// Pattern 4: Multiple test patterns
if file_name.contains("Symbol") || file_name.contains("generics") { /* ... */ }
```

### 3.3 Legitimate File Name Usage

Note: Not all file name references need removal. Legitimate uses include:

```rust
// OK: Parsing test directives from source comments
fn parse_test_option_bool(text: &str, key: &str) -> Option<bool> {
    // Reading @strict, @noImplicitAny from file content
}

// OK: Error messages that include file names
error!("Type error in {}: ...", file_name);

// NOT OK: Changing checker behavior based on file name
if file_name.contains("test") { return; }  // REMOVE THIS
```

---

## 4. Impact Analysis

### 4.1 Scope

- **Location**: `src/thin_checker.rs`
- **Instances**: Approximately 40 locations
- **Lines of Code**: ~120-200 lines to be removed or refactored
- **Functions Affected**: Multiple type checking functions across the checker

### 4.2 Current State

Based on code inspection, test-aware checks currently exist in:
- Type compatibility checking
- Symbol resolution
- Error reporting
- Diagnostic emission
- Various validation functions

The exact count and locations will be catalogued during the migration (see Section 6.1).

### 4.3 Expected Outcomes

After cleanup:
1. **Test Failures**: Many tests will initially fail - this is expected and correct
2. **Real Bugs Exposed**: Previously masked bugs will become visible
3. **Clear Signal**: Test results will accurately reflect checker correctness
4. **Work Queue**: A clear list of actual bugs to fix, prioritized by frequency

---

## 5. Solution: Test-Agnostic Architecture

### 5.1 Core Principle

**The Rule**: Source code must not know about tests. If a test fails, fix the underlying logic, not by adding special cases for test file names.

### 5.2 How Tests Should Work

The correct architecture separates concerns:

```
┌─────────────────┐
│   Test Files    │  Contains @directives and test code
└────────┬────────┘
         │
         v
┌─────────────────┐
│ Test Infrastructure │  Reads @directives, configures environment
│ (differential-test) │
└────────┬────────┘
         │
         v
┌─────────────────┐
│ Parser/Checker  │  Process code WITHOUT knowing it's a test
│ (thin_*.rs)     │  Use configuration, not file names
└─────────────────┘
```

### 5.3 Proper Configuration

Instead of file name checks, use compiler options:

```rust
// WRONG: Check file names
if file_name.contains("conformance") {
    // suppress error
}

// RIGHT: Use compiler options set by test infrastructure
if self.ctx.no_implicit_any() {
    // emit error
}
```

### 5.4 Test Infrastructure Responsibilities

The `differential-test/` infrastructure should:

1. **Parse Test Directives**: Read `@strict`, `@noImplicitAny`, etc. from test file comments
2. **Configure Compiler**: Set `CompilerOptions` based on directives
3. **Run Checker**: Invoke checker with configured options
4. **Compare Results**: Match output against baselines

The checker should only see the `CompilerOptions`, never the file name pattern.

---

## 6. Migration Strategy

### 6.1 Phase 1: Discovery and Cataloging

**Objective**: Create complete inventory of test-aware code

**Steps**:
1. Search `src/thin_checker.rs` for all file name references:
   ```bash
   grep -n "file_name\.contains" src/thin_checker.rs
   grep -n "is_test_file" src/thin_checker.rs
   ```

2. Create a tracking document listing:
   - Line number
   - Function name
   - Pattern being checked (conformance/test/cases/etc.)
   - What behavior is being suppressed
   - Related diagnostic code if applicable

3. Categorize instances by:
   - Error suppression
   - Behavior modification
   - Validation skipping

### 6.2 Phase 2: Establish Baseline

**Objective**: Understand current test pass rate before changes

**Steps**:
1. Run full conformance test suite:
   ```bash
   ./differential-test/run-conformance.sh --all --workers=14
   ```

2. Record metrics:
   - Total tests
   - Exact matches
   - Missing errors
   - Extra errors
   - Current accuracy percentage

3. Save baseline results for comparison

### 6.3 Phase 3: Remove Test-Aware Code

**Objective**: Systematically eliminate all test-aware patterns

**Process for each instance**:

1. **Identify**: Locate a test-aware code block
   ```rust
   if self.ctx.file_name.contains("test") {
       return; // Suppressing TS2322
   }
   ```

2. **Document**: Note what error was being suppressed and why

3. **Remove**: Delete the conditional entirely
   ```rust
   // Block removed - checker now reports actual behavior
   ```

4. **Verify Compilation**: Ensure code still compiles
   ```bash
   cargo build --release
   ```

5. **Run Unit Tests**: Confirm no unit test regressions
   ```bash
   cargo test
   ```

6. **Commit**: Make atomic commit for each instance or small group
   ```bash
   git add src/thin_checker.rs
   git commit -m "Remove test-aware code: file_name.contains check in check_type_assignability

   Removed conditional that suppressed TS2322 for test files. Checker now
   reports actual behavior consistently for all files.

   Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
   ```

**Order of Removal**:
- Start with simple, isolated instances
- Progress to complex cases with multiple conditions
- Handle interconnected checks last

### 6.4 Phase 4: Analysis and Prioritization

**Objective**: Understand what broke and why

After all test-aware code is removed:

1. **Run Conformance Tests**:
   ```bash
   ./differential-test/run-conformance.sh --all --workers=14
   ```

2. **Analyze Failures**:
   - Categorize by error type (missing vs extra errors)
   - Group by root cause, not symptoms
   - Identify patterns in failures
   - Count frequency of each issue type

3. **Create Work Queue**:
   - List distinct bugs/missing features
   - Prioritize by impact (number of tests affected)
   - Document expected vs actual behavior
   - Plan fix approach for each

4. **Compare to Baseline**:
   - Some metrics may improve (previously masked bugs now visible)
   - Some may worsen temporarily (suppressed errors now reported)
   - Track net change in accuracy

### 6.5 Phase 5: Fix Root Causes

**Objective**: Address actual bugs revealed by cleanup

**Approach**:
- Fix bugs in priority order (highest impact first)
- Each fix should improve conformance accuracy
- Never add back test-aware code
- Document architectural decisions in `specs/`

**Success Metrics**:
- Each fix should resolve multiple test failures
- Conformance accuracy should steadily increase
- No regression in unit tests

---

## 7. Verification Steps

### 7.1 Code Verification

After removal, verify cleanup is complete:

```bash
# Should return ZERO results:
grep -n "file_name\.contains" src/thin_checker.rs | grep -v "parse_test_option"

# Should return ZERO results:
grep -n "is_test_file" src/thin_checker.rs

# Verify compilation:
cargo build --release

# Verify unit tests:
cargo test
```

### 7.2 Functional Verification

Confirm checker behavior is consistent:

1. **Create Test Case**: Write identical TypeScript code
2. **Test as Production**: Check without test patterns in name
3. **Test as Test File**: Check with test patterns in name
4. **Compare**: Both should produce identical diagnostics
5. **Validate**: Compare against TSC output

### 7.3 Conformance Verification

Track progress:

```bash
# Before cleanup:
./differential-test/run-conformance.sh --all --workers=14 > baseline.txt

# After cleanup:
./differential-test/run-conformance.sh --all --workers=14 > post-cleanup.txt

# After fixes:
./differential-test/run-conformance.sh --all --workers=14 > final.txt

# Compare accuracy trends
```

### 7.4 Architectural Verification

Ensure principles are maintained:

- [ ] No file name pattern matching in checker
- [ ] Configuration comes from CompilerOptions only
- [ ] Test infrastructure handles directive parsing
- [ ] Checker is test-agnostic
- [ ] Documentation reflects architecture

---

## 8. Success Criteria

The cleanup effort is complete when:

### 8.1 Code Quality
- [ ] Zero instances of `file_name.contains()` for test pattern detection in `src/thin_checker.rs`
- [ ] Zero instances of `is_test_file` variables
- [ ] All code compiles without errors
- [ ] All unit tests pass

### 8.2 Architecture
- [ ] Checker behavior is identical regardless of file name
- [ ] Configuration comes from `CompilerOptions`, not file names
- [ ] Test infrastructure properly parses directives
- [ ] Clear separation between test infrastructure and implementation

### 8.3 Testing
- [ ] Conformance test infrastructure runs successfully
- [ ] Baseline metrics are established
- [ ] Failure patterns are categorized
- [ ] Bug fix backlog is prioritized

### 8.4 Documentation
- [ ] This specification is followed
- [ ] Architectural decisions are documented
- [ ] Migration progress is tracked
- [ ] Bug fix plans are documented

### 8.5 Long-term
- [ ] Conformance accuracy improves as bugs are fixed
- [ ] No test-aware code is added back
- [ ] Agent instructions enforce the prohibition
- [ ] Target of 95%+ exact match is achieved

---

## 9. Related Files

### 9.1 Primary Implementation
- `src/thin_checker.rs` - Type checker (contains ~40 instances to remove)
- `src/thin_parser.rs` - Parser (verify test-agnostic)
- `src/binder/` - Symbol binding (verify test-agnostic)
- `src/solver/` - Type resolution (verify test-agnostic)

### 9.2 Test Infrastructure
- `differential-test/run-conformance.sh` - Test runner script
- `differential-test/conformance-runner.mjs` - Main test orchestrator
- `differential-test/conformance-worker.mjs` - Worker process for tests
- `differential-test/parallel-conformance.mjs` - Parallel test execution
- `differential-test/metrics-tracker.mjs` - Accuracy tracking

### 9.3 Configuration
- `src/compiler_options.rs` - Compiler options structure
- Test files in `ts-tests/` - Contains `@directive` comments

### 9.4 Documentation
- `PROJECT_DIRECTION.md` - High-level project priorities
- `specs/ARCHITECTURE_CLEANUP.md` - This document
- `specs/DIAGNOSTICS.md` - Error code specifications

---

## 10. Anti-Patterns to Avoid

### 10.1 During Migration

**Don't:**
- Replace file name checks with different test detection mechanisms
- Add new test-aware code to "fix" failing tests
- Suppress errors in other ways (global flags, environment variables)
- Skip verification steps to move faster

**Do:**
- Remove code cleanly without replacement
- Document what broke and why
- Fix root causes, not symptoms
- Make atomic commits
- Keep architecture clean

### 10.2 During Bug Fixing

**Don't:**
- Add back file name checks "just for this one case"
- Create "Tier 0/1/2" patches for test patterns
- Suppress entire error categories
- Filter errors based on file paths

**Do:**
- Implement correct behavior for all code
- Use compiler options for configuration
- Fix the checker logic itself
- Test fixes with both test and production code

### 10.3 Long-term Maintenance

**Don't:**
- Allow test-aware code in code reviews
- Accept "temporary" file name checks
- Make exceptions for "special cases"

**Do:**
- Enforce architectural principles in review
- Update agent instructions to prevent regression
- Document why we prohibit test-aware code
- Educate contributors on proper patterns

---

## 11. Architectural Principles

This cleanup enforces these core principles:

### 11.1 Separation of Concerns
Production code and test infrastructure are separate layers with distinct responsibilities.

### 11.2 Configuration Over Detection
Behavior is controlled by explicit configuration (CompilerOptions), not by detecting file patterns.

### 11.3 Consistency
The same input code produces the same output regardless of file name or test context.

### 11.4 Fail Fast
When functionality is missing or buggy, fail visibly rather than masking the problem.

### 11.5 Root Cause Fixes
Address underlying bugs in implementation rather than adding workarounds for symptoms.

### 11.6 Test Integrity
Tests must accurately reflect the checker's behavior, not a sanitized version for tests only.

---

## 12. Timeline and Ownership

### 12.1 Priority
**CRITICAL** - All feature work is blocked until this cleanup is complete.

### 12.2 Estimated Effort
- Phase 1 (Discovery): 2-4 hours
- Phase 2 (Baseline): 1 hour
- Phase 3 (Removal): 4-8 hours
- Phase 4 (Analysis): 4-6 hours
- Phase 5 (Fixes): Ongoing, weeks to months

**Total Cleanup Effort**: 1-2 days
**Total Fix Effort**: Depends on number and complexity of revealed bugs

### 12.3 Dependencies
- Completion of cleanup is prerequisite for:
  - New feature development
  - Performance optimization work
  - API stabilization
  - Public release

---

## 13. References

### 13.1 Related Documentation
- `PROJECT_DIRECTION.md` - Priority list and architectural rules
- `specs/SOLVER.md` - Type solver architecture
- `specs/DIAGNOSTICS.md` - Error code catalog

### 13.2 Key Commands
```bash
# Build WASM
wasm-pack build --target web --out-dir pkg

# Quick conformance test
./differential-test/run-conformance.sh --max=200 --workers=4

# Full conformance test
./differential-test/run-conformance.sh --all --workers=14

# Find test-aware code
grep -n "file_name\.contains" src/thin_checker.rs

# Run unit tests
cargo test
```

### 13.3 Context
This cleanup was initiated after discovering that approximately 40 instances of test-aware code had accumulated in the type checker, creating architectural debt that blocked progress. The code was suppressing errors for test files instead of fixing underlying bugs, leading to a degradation in correctness and maintainability.

---

## Appendix A: Example Removal

### Before (Anti-Pattern)
```rust
fn check_type_assignability(&mut self, source: TypeId, target: TypeId, node: NodeIndex) {
    // Check if this is a test file
    let is_test_file = self.ctx.file_name.contains("conformance")
        || self.ctx.file_name.contains("test")
        || self.ctx.file_name.contains("cases");

    // Suppress TS2322 for test files with Symbol in the name
    if is_test_file && self.ctx.file_name.contains("Symbol") {
        return;
    }

    // Normal checking logic
    if !self.is_assignable(source, target) {
        self.error(node, "TS2322", "Type '...' is not assignable to type '...'");
    }
}
```

### After (Correct Pattern)
```rust
fn check_type_assignability(&mut self, source: TypeId, target: TypeId, node: NodeIndex) {
    // Normal checking logic - no file name checks
    if !self.is_assignable(source, target) {
        self.error(node, "TS2322", "Type '...' is not assignable to type '...'");
    }
}
```

### Result
- Test files that were incorrectly passing now fail (correct)
- Real bugs in Symbol type handling are exposed
- All code is checked consistently
- Root cause fix required: Implement proper Symbol type checking

---

**Document Version**: 1.0
**Last Updated**: 2026-01-17
**Status**: Active - Cleanup in progress
>>>>>>> origin/em-4

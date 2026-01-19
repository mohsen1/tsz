# Architecture Rules for Agents

This document defines critical architecture rules and anti-patterns for Project Zang. All contributors (AI agents and humans) must follow these guidelines to maintain code quality and architectural integrity.

---

## Section 1: Critical Anti-Patterns to Avoid

### 1.1 Test-Aware Code

**DO NOT** add logic to source code that checks file names or paths to alter behavior for tests.

#### The Anti-Pattern

```rust
// BAD - This is architectural debt and must NEVER be added:
let is_test_file = self.ctx.file_name.contains("conformance")
    || self.ctx.file_name.contains("test")
    || self.ctx.file_name.contains("cases");

if is_test_file && self.ctx.file_name.contains("Symbol") {
    return; // Suppressing errors for tests
}
```

This pattern has appeared in the checker code and represents a fundamental architectural violation. The checker had approximately 40 instances of this pattern that needed removal.

#### Why This Is Wrong

1. **Breaks separation of concerns**: Production code should not know about test infrastructure
2. **Hides bugs**: Suppressing errors for tests means the underlying logic is incorrect
3. **Creates technical debt**: Each test-aware check is a bug that needs fixing later
4. **Violates test-agnostic principle**: Code should behave identically regardless of file name
5. **Makes code unpredictable**: Same TypeScript input produces different results based on file path

#### The Rule

**Source code must not know about tests.** If a test fails, fix the underlying logic, not by adding special cases for test file names.

### 1.2 Whack-a-Mole Error Suppression

**DO NOT** suppress specific errors without understanding and fixing the root cause.

```rust
// BAD - Suppressing symptoms instead of fixing the cause:
if error_code == 2304 && some_specific_condition {
    return; // Skip this error for now
}
```

Instead, investigate why the error occurs and implement the correct type checking logic.

### 1.3 Tier-Based Patches

**DO NOT** create "tiers" of fixes or patches that apply different logic to different groups of tests.

```rust
// BAD - Tier-based approach:
if tier_0_test {
    apply_strict_logic();
} else if tier_1_test {
    apply_relaxed_logic();
} else {
    apply_legacy_logic();
}
```

This approach leads to unmaintainable code. Fix the root cause once, correctly, for all cases.

### 1.4 File Name-Based Heuristics

**DO NOT** use file name patterns to determine compiler behavior.

```rust
// BAD - Only acceptable for lib file detection during migration:
let is_lib_file = file_name.contains("lib.d.ts")
    || file_name.contains("lib.es")
    || file_name.contains("lib.dom");

// ACCEPTABLE ONLY TEMPORARILY - Should use CompilerOptions instead
```

Note: The lib file detection pattern is acceptable only as a temporary measure during migration. The correct approach is to use explicit configuration via `CompilerOptions.markAsLibFile()`.

---

## Section 2: Correct Approaches

### 2.1 Test-Agnostic Architecture

**Principle**: All core logic (parser, checker, binder, solver) must be completely unaware of tests.

#### Implementation

1. **No file path inspection**: Never check file names or paths in core logic
2. **Configuration-driven**: Use `CompilerOptions` to control behavior
3. **Same code path**: Production and test code must execute identical logic
4. **Universal correctness**: If code is correct, it works for all inputs

### 2.2 Explicit CompilerOptions

**Use `CompilerOptions` to configure compiler behavior, not file name heuristics.**

#### Correct Pattern

```rust
// GOOD - Configuration-driven approach:
pub struct CompilerOptions {
    pub strict: bool,
    pub no_implicit_any: bool,
    pub strict_null_checks: bool,
    pub strict_function_types: bool,
    pub target: String,
    pub module: String,
}

// Usage in ThinParser:
parser.set_compiler_options(r#"{
    "strict": true,
    "noImplicitAny": true
}"#);

// Mark library files explicitly:
parser.mark_as_lib_file(true);
```

#### API Methods

```rust
// Public API for configuring parser:
#[wasm_bindgen(js_name = setCompilerOptions)]
pub fn set_compiler_options(&mut self, json: &str) -> Result<(), JsValue>

#[wasm_bindgen(js_name = markAsLibFile)]
pub fn mark_as_lib_file(&mut self, is_lib: bool)
```

### 2.3 Root Cause Fixes

When a test fails, follow this process:

1. **Understand the failure**: What error is expected vs. what we produce?
2. **Find the root cause**: What type system rule or logic is incorrect?
3. **Fix the core logic**: Update parser/checker/solver to implement correct behavior
4. **Verify broadly**: Ensure the fix doesn't break other tests
5. **No suppression**: Never suppress the error as a "fix"

#### Example Process

```
Test fails: Missing error TS2304 "Cannot find name 'Foo'"
↓
Root cause analysis: Symbol lookup doesn't check parent scopes
↓
Fix: Implement proper scope chain traversal in binder
↓
Test passes: Now correctly reports TS2304
↓
Verify: Check that other symbol lookup tests still pass
```

### 2.4 Directive-Based Test Configuration

Test infrastructure should read directives from test files and configure the compiler accordingly.

```typescript
// Test file with directives:
// @strict
// @noImplicitAny
// @target: ES2015

// Test code here...
```

The test runner should:
1. Parse `@` directives from the test file
2. Build a `CompilerOptions` object
3. Pass it to the compiler via `setCompilerOptions()`
4. Run the test with that configuration

---

## Section 3: Code Review Checklist

Use this checklist when reviewing code changes (both AI-generated and human-written).

### 3.1 Test-Aware Code Detection

- [ ] Search for `file_name.contains()` in all modified files
- [ ] Search for `path.contains()` or similar path inspection
- [ ] Search for `"test"`, `"conformance"`, `"cases"` in conditional logic
- [ ] Verify no file path-based branching in core logic
- [ ] Check that all behavior changes are driven by `CompilerOptions`

### 3.2 Error Suppression Detection

- [ ] Look for early returns that skip error reporting
- [ ] Check for comments like "TODO: fix this properly" near error handling
- [ ] Verify error codes are emitted correctly, not filtered
- [ ] Ensure diagnostic logic is based on type rules, not test names

### 3.3 Architecture Compliance

- [ ] Parser changes are test-agnostic
- [ ] Checker changes are test-agnostic
- [ ] Binder changes are test-agnostic
- [ ] No "tier" or "phase" based logic
- [ ] Configuration uses `CompilerOptions`, not heuristics

### 3.4 Questions to Ask Before Merging

1. **Does this change check file names or paths?**
   - If yes: Reject and request refactor to use `CompilerOptions`

2. **Does this suppress an error for specific tests?**
   - If yes: Reject and request root cause fix

3. **Is this a workaround or the correct solution?**
   - If workaround: Reject and request proper implementation

4. **Will this work for all TypeScript code, not just tests?**
   - If no: Reject and request universal solution

5. **Does this add technical debt?**
   - If yes: Reject unless there's a documented plan to remove it

---

## Section 4: Examples

### 4.1 Bad Example: File Name-Based Behavior

```rust
// BAD - Don't do this:
fn check_variable_declaration(&mut self, node: ThinNodeId) {
    let is_test_file = self.ctx.file_name.contains("conformance")
        || self.ctx.file_name.contains("test");

    if is_test_file {
        // Skip strict checks for test files
        return;
    }

    // Normal checking logic...
}
```

**Problems:**
- Source code knows about test infrastructure
- Different behavior for same TypeScript code
- Hides bugs in the checker logic
- Creates maintenance burden

### 4.2 Good Example: Configuration-Driven Behavior

```rust
// GOOD - Use explicit configuration:
fn check_variable_declaration(&mut self, node: ThinNodeId) {
    // Behavior controlled by CompilerOptions, not file name
    if self.options.strict || self.options.no_implicit_any {
        self.check_explicit_type(node);
    }

    // Check for uninitialized variables
    if self.options.strict_null_checks {
        self.check_initialization(node);
    }

    // Universal logic that works for all code
    self.check_variable_scope(node);
}
```

**Benefits:**
- Test-agnostic: same logic for all files
- Configuration-driven: behavior is explicit
- Predictable: same input produces same output
- Maintainable: no special cases

### 4.3 Bad Example: Error Suppression

```rust
// BAD - Suppressing symptoms:
fn report_diagnostic(&mut self, code: u32, message: &str) {
    // Don't report certain errors in test files
    if self.ctx.file_name.contains("test") && code == 2304 {
        return;
    }

    self.diagnostics.push(Diagnostic { code, message });
}
```

### 4.4 Good Example: Correct Implementation

```rust
// GOOD - Fix the root cause:
fn resolve_symbol(&mut self, name: &str) -> Option<SymbolId> {
    // Properly implemented scope chain traversal
    let mut current_scope = self.current_scope;

    loop {
        if let Some(symbol) = self.scopes[current_scope].lookup(name) {
            return Some(symbol);
        }

        // Walk up the scope chain
        match self.scopes[current_scope].parent {
            Some(parent) => current_scope = parent,
            None => return None,
        }
    }
}
```

### 4.5 Bad Example: Lib File Detection

```rust
// BAD - File name heuristics (legacy pattern):
let is_lib_file = file_name.contains("lib.d.ts")
    || file_name.contains("lib.es")
    || file_name.contains("lib.dom")
    || file_name.contains("lib.webworker")
    || file_name.contains("lib.scripthost");
```

### 4.6 Good Example: Explicit Lib File Marking

```rust
// GOOD - Explicit configuration:
let mut parser = ThinParser::new();

// Configure as library file explicitly
parser.mark_as_lib_file(true);

// Or from test infrastructure:
if is_lib_declaration_file(&file_path) {
    parser.mark_as_lib_file(true);
}
```

---

## Section 5: References

### 5.1 Core Documentation

- **PROJECT_DIRECTION.md**: Overall project priorities and rules
  - Section: "Priority List (Current Focus)" - Test-aware code removal
  - Section: "Anti-Patterns to Avoid" - Comprehensive list
  - Section: "Rules" - Architecture principles

### 5.2 Specifications

- **specs/ARCHITECTURE_CLEANUP.md**: Architecture cleanup plan (create if needed)
- **specs/TEST_INFRASTRUCTURE.md**: Test infrastructure design (create if needed)
- **specs/WASM_ARCHITECTURE.md**: WASM boundary and public API design

### 5.3 Key Implementation Files

- **src/lib.rs**: Public API including `CompilerOptions` struct
  - Lines 164-183: `CompilerOptions` definition
  - Lines 332-336: `setCompilerOptions()` method
  - Lines 337-339: `markAsLibFile()` method
- **src/thin_parser.rs**: Parser implementation
- **src/thin_checker.rs**: Type checker (previous location of anti-patterns)
- **src/binder/**: Symbol binding and scope management

### 5.4 Related Commands

```bash
# Search for test-aware code anti-patterns:
rg "file_name\.contains" src/
rg "path\.contains.*test" src/
rg "conformance|cases" src/ --type rust

# Build and test:
wasm-pack build --target web --out-dir pkg
./conformance/run-conformance.sh --max=200 --workers=4
```

---

## Section 6: Enforcement

### 6.1 For AI Agents

AI agents must:
1. Read this document before making changes to core logic
2. Never introduce test-aware code patterns
3. Always use `CompilerOptions` for configuration
4. Fix root causes, not symptoms
5. Ask for clarification if uncertain about architecture

### 6.2 For Human Reviewers

Human reviewers must:
1. Check for all anti-patterns listed in Section 3
2. Reject PRs that violate these rules
3. Require root cause analysis for error fixes
4. Verify test-agnostic architecture
5. Ensure no regression of architectural quality

### 6.3 For Code Authors

When writing code:
1. Think: "Does this work for all TypeScript code?"
2. Ask: "Am I fixing the cause or the symptom?"
3. Verify: "Is this configuration-driven or path-driven?"
4. Consider: "Will this create technical debt?"
5. Document: "Why is this the correct approach?"

---

## Section 7: Test Infrastructure

### 7.1 Test Organization

Tests are organized in three tiers:

1. **Unit Tests** (`src/*_tests.rs`)
   - Fast, focused tests for individual components
   - Located alongside the module being tested
   - Run with `cargo test --lib`

2. **Integration Tests** (`tests/`)
   - End-to-end tests for the compiler pipeline
   - Test interaction between components
   - Run with `cargo test --test integration_tests`

3. **Conformance Tests** (`conformance/`)
   - Compare output against TypeScript compiler
   - Large-scale validation against official tests
   - Run with `./conformance/run-conformance.sh`

### 7.2 Test Harness

The `src/test_harness.rs` module provides utilities for:

```rust
// Run a test with timeout to prevent hanging
use crate::test_harness::{run_with_timeout, DEFAULT_TEST_TIMEOUT};

let result = run_with_timeout(DEFAULT_TEST_TIMEOUT, || {
    // Test code here
});
assert!(result.is_passed());

// Use test fixtures for common setups
use crate::test_harness::ParserTestFixture;

let fixture = ParserTestFixture::new("const x = 1;");
let parser = fixture.parse();
```

### 7.3 Timeout Constants

| Context | Timeout | Constant |
|---------|---------|----------|
| Unit tests | 10 seconds | `DEFAULT_TEST_TIMEOUT` |
| Parser tests | 30 seconds | `PARSER_TEST_TIMEOUT` |
| Checker tests | 60 seconds | `CHECKER_TEST_TIMEOUT` |
| Conformance tests | 30 seconds | Per-test in runner |

### 7.4 Writing Good Tests

**DO:**
- Use descriptive test names that explain the behavior being tested
- Include edge cases and boundary conditions
- Use the test harness timeout wrappers for potentially slow tests
- Test both positive and negative cases

**DON'T:**
- Write tests that depend on file system layout
- Create tests that only pass with specific test file names
- Skip proper cleanup of test resources
- Ignore test failures by commenting them out

### 7.5 Conformance Test Runner

The Rust-side conformance runner (`tests/conformance_runner.rs`) provides:

```rust
// Parse test directives from conformance files
let directives = parse_directives(source);

// Run a single conformance test
let result = run_conformance_test(source, "test.ts");

// Check results
assert!(result.parse_errors.is_empty());
assert!(!result.timed_out);
```

### 7.6 Adding New Tests

When adding new functionality:

1. **First**: Add unit tests in the module's `*_tests.rs` file
2. **Then**: Add integration tests if the feature spans multiple components
3. **Finally**: Verify against TypeScript conformance tests if applicable

### 7.7 Test Commands

```bash
# Run all unit tests
cargo test --lib

# Run specific test module
cargo test --lib thin_parser_tests

# Run integration tests
cargo test --test integration_tests

# Run conformance tests (JavaScript runner)
./conformance/run-conformance.sh --max=1000

# Run with verbose output
cargo test --lib -- --nocapture
```

---

## Summary

**Core Principle**: Write test-agnostic, configuration-driven code that correctly implements TypeScript semantics for all inputs.

**Golden Rule**: If you're checking file names in core logic, you're doing it wrong.

**When in doubt**: Use `CompilerOptions`, fix root causes, and maintain architectural integrity.

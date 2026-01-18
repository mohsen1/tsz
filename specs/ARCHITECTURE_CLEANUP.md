# Architecture Cleanup: Removal of Test-Aware Code

## Overview

This document describes the architectural principle prohibiting test-aware code in the TypeScript compiler implementation, and documents the cleanup process to remove this anti-pattern from the codebase.

**Status:** Active
**Priority:** Critical - All feature work blocked until complete
**Effort:** ~40 instances to remediate

---

## 1. The Problem: Test-Aware Code Anti-Pattern

Test-aware code is an architectural anti-pattern where **source code inspects file names or paths to alter compiler behavior**. This violates fundamental separation of concerns and creates technical debt that compounds over time.

**Goal:** Make the parser and checker completely test-agnostic. Source code must not contain any logic that changes behavior based on file names or test patterns.

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

### Root Cause

The test-aware code accumulated because:
- Tests failed due to missing features or bugs
- Instead of fixing the underlying issue, developers added file name checks
- This created a pattern that was repeated ~40 times
- The checker became increasingly coupled to test infrastructure

---

## 2. What Was Removed

The following patterns were identified and targeted for removal from `src/thin_checker.rs`:

### 2.1 File Name Contains Checks

```rust
// REMOVED: File path inspection to determine compiler behavior
let is_test_file = self.ctx.file_name.contains("conformance")
    || self.ctx.file_name.contains("test")
    || self.ctx.file_name.contains("cases");
```

### 2.2 Test-Specific Error Suppression

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

### 2.3 Pattern-Based Heuristics

```rust
// REMOVED: Suppressing errors based on async file patterns
if is_test_file && (
    self.ctx.file_name.contains("asyncArrow")
    || (self.ctx.file_name.contains("async") && name_str.contains("obj"))
) {
    continue;
}
```

### 2.4 Ambient Declaration Filename Detection

```rust
// REMOVED: Detecting ambient declarations via file names
let is_ambient_file = self.ctx.file_name.contains("ambient")
    || self.ctx.file_name.contains("declare")
    || self.ctx.file_name.contains("global")
    || self.ctx.file_name.contains("module");
```

### 2.5 Context-Specific Strict Mode Detection

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

---

## 3. The Architectural Principle

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

---

## 4. The New Approach

### 4.1 Explicit Configuration Over File Path Heuristics

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

### 4.2 Test Infrastructure Responsibility

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

### 4.3 How TypeScript's Test Suite Works

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

---

## 5. CompilerOptions Data Flow

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
   ./conformance/run-conformance.sh --all --workers=14
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
   ./conformance/run-conformance.sh --all --workers=14
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
./conformance/run-conformance.sh --all --workers=14 > baseline.txt

# After cleanup:
./conformance/run-conformance.sh --all --workers=14 > post-cleanup.txt

# After fixes:
./conformance/run-conformance.sh --all --workers=14 > final.txt

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

## 8. Benefits

### 8.1 Cleaner Separation of Concerns

- **Compiler**: Type checks code according to explicit configuration
- **Test Infrastructure**: Manages test files and configuration
- **Build Tools**: Configure compiler for production builds

Each component has a single, clear responsibility.

### 8.2 More Predictable Behavior

```rust
// BEFORE: Behavior depends on file path
check_file("src/utils.ts");        // Strict checking
check_file("tests/utils.test.ts"); // Lenient checking (different behavior!)

// AFTER: Behavior depends on CompilerOptions
check_file("src/utils.ts", CompilerOptions { strict: true });
check_file("tests/utils.test.ts", CompilerOptions { strict: true }); // Same behavior
```

### 8.3 Easier to Maintain

When a test fails, there are only two possibilities:
1. **Bug in compiler**: Fix the compiler logic
2. **Bug in test setup**: Fix the test configuration

No more third option: "Add another file name check to suppress the error"

### 8.4 Testable Compiler

The compiler can be properly tested because:
- All behavior is controlled by explicit inputs (CompilerOptions)
- No hidden state based on file paths
- Same code always produces same output for same configuration

### 8.5 Prevents Whack-a-Mole

Without test-aware code:
- Can't suppress errors by checking file names
- Must fix actual bugs instead of masking them
- Forced to implement correct behavior

### 8.6 Production-Ready Code

If the compiler works correctly on test files, it works correctly on production files, because **there is no difference**.

---

## 9. Success Criteria

The cleanup effort is complete when:

### 9.1 Code Quality
- [ ] Zero instances of `file_name.contains()` for test pattern detection in `src/thin_checker.rs`
- [ ] Zero instances of `is_test_file` variables
- [ ] All code compiles without errors
- [ ] All unit tests pass

### 9.2 Architecture
- [ ] Checker behavior is identical regardless of file name
- [ ] Configuration comes from `CompilerOptions`, not file names
- [ ] Test infrastructure properly parses directives
- [ ] Clear separation between test infrastructure and implementation

### 9.3 Testing
- [ ] Conformance test infrastructure runs successfully
- [ ] Baseline metrics are established
- [ ] Failure patterns are categorized
- [ ] Bug fix backlog is prioritized

### 9.4 Documentation
- [ ] This specification is followed
- [ ] Architectural decisions are documented
- [ ] Migration progress is tracked
- [ ] Bug fix plans are documented

### 9.5 Long-term
- [ ] Conformance accuracy improves as bugs are fixed
- [ ] No test-aware code is added back
- [ ] Agent instructions enforce the prohibition
- [ ] Target of 95%+ exact match is achieved

---

## 10. Migration Guide

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

---

## 11. Anti-Patterns to Avoid

### 11.1 During Migration

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

### 11.2 During Bug Fixing

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

### 11.3 Long-term Maintenance

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

## 12. Examples

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

---

## 13. Related Files

### 13.1 Primary Implementation
- `src/thin_checker.rs` - Type checker (contains ~40 instances to remove)
- `src/thin_parser.rs` - Parser (verify test-agnostic)
- `src/binder/` - Symbol binding (verify test-agnostic)
- `src/solver/` - Type resolution (verify test-agnostic)

### 13.2 Test Infrastructure
- `conformance/run-conformance.sh` - Test runner script
- `conformance/conformance-runner.mjs` - Main test orchestrator
- `conformance/conformance-worker.mjs` - Worker process for tests
- `conformance/parallel-conformance.mjs` - Parallel test execution
- `conformance/metrics-tracker.mjs` - Accuracy tracking

### 13.3 Configuration
- `src/compiler_options.rs` - Compiler options structure
- Test files in `TypeScript/tests/` - Contains `@directive` comments

### 13.4 Documentation
- `PROJECT_DIRECTION.md` - High-level project priorities
- `specs/ARCHITECTURE_CLEANUP.md` - This document
- `specs/DIAGNOSTICS.md` - Error code specifications
- `specs/TEST_INFRASTRUCTURE.md` - Test infrastructure documentation

---

## 14. Enforcement

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

---

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

**Document Version**: 1.1
**Last Updated**: 2025-01-17
**Status**: Active - Cleanup in progress

---

**Related Documents**:
- `PROJECT_DIRECTION.md` - Overall project priorities and rules
- `specs/DIAGNOSTICS.md` - Diagnostic implementation guidelines
- `specs/TEST_INFRASTRUCTURE.md` - Test infrastructure documentation

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

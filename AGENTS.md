# Architecture Rules for Agents

This document defines critical architecture rules for Project Zang. All contributors must follow these guidelines.

## Primary Goal: TypeScript Compiler Compatibility

**Match tsc behavior exactly.** Every error, every type inference, every edge case must behave identically to TypeScript's compiler. If tsc reports an error, we must report it. If tsc allows code, we must allow it.

## Core Principles

1. **No shortcuts** - Implement correct logic, not quick fixes
2. **Test-agnostic code** - Source code must never check file names or paths
3. **Configuration-driven** - Use `CompilerOptions` for all behavior changes
4. **Fix root causes** - Never suppress errors or add special cases

## Critical Anti-Patterns

### 1. Test-Aware Code (NEVER DO THIS)

```rust
// BAD - NEVER check file names in core logic:
let is_test_file = self.ctx.file_name.contains("conformance")
    || self.ctx.file_name.contains("test");

if is_test_file {
    return; // Suppressing errors for tests
}
```

**Why this is wrong:** Production code should not know about tests. If a test fails, fix the underlying logic.

### 2. Error Suppression (NEVER DO THIS)

```rust
// BAD - NEVER suppress specific errors:
if error_code == 2304 && some_condition {
    return; // Skip this error
}
```

**Why this is wrong:** This hides bugs. Fix the root cause instead.

### 3. Tier-Based Patches (NEVER DO THIS)

```rust
// BAD - NEVER apply different logic to different tests:
if tier_0_test {
    apply_strict_logic();
} else {
    apply_relaxed_logic();
}
```

**Why this is wrong:** Fix the logic correctly once, for all cases.

## Correct Approaches

### Use CompilerOptions

```rust
// GOOD - Configuration-driven:
fn check_variable(&mut self, node: ThinNodeId) {
    if self.options.strict {
        self.check_explicit_type(node);
    }
    // Universal logic for all code
}
```

### Fix Root Causes

When tests fail:
1. Understand what tsc produces vs. what we produce
2. Find the incorrect type system logic
3. Fix the core implementation (parser/checker/solver)
4. Verify against broader test suite

### Test Infrastructure Configuration

Test runners should configure the compiler via directives:

```typescript
// @strict
// @noImplicitAny
// @target: ES2015

// Test runner parses these and calls:
parser.set_compiler_options(JSON.stringify({
    strict: true,
    noImplicitAny: true,
    target: "ES2015"
}));
```

## Code Review Checklist

Before merging changes:
- [ ] No `file_name.contains()`, `path.contains()` in core logic
- [ ] No error suppression based on test names
- [ ] No "tier" or "phase" based logic
- [ ] All behavior driven by `CompilerOptions`
- [ ] Fix addresses root cause, not symptom
- [ ] Works for all TypeScript code, not just tests

## Key Questions

1. Does this check file names or paths? → Reject
2. Does this suppress errors for specific tests? → Reject
3. Is this a workaround or correct solution? → Reject workarounds
4. Will this match tsc behavior for all TypeScript code? → Must be yes

## References

- **src/lib.rs**: `CompilerOptions` struct, `setCompilerOptions()`, `markAsLibFile()`
- **PROJECT_DIRECTION.md**: Project priorities and architecture rules

## When work is done?

All unit tests should pass. There should be zero clippy warnings. It's okay if conformance goes down after some work but a huge drop in conformance is not acceptables

## Run commands with a reasonable timeout

ALWAYS run commands with a reasonable timeout to avoid commands that will hang
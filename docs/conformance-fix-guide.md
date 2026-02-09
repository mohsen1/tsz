# Conformance Test Fix Implementation Guide

This guide provides step-by-step instructions for fixing conformance test failures based on the analysis in `conformance-analysis-slice3.md`.

## General Workflow

1. **Pick a test**
   ```bash
   # Find close-to-passing tests
   ./scripts/conformance.sh analyze --offset 6318 --max 3159 | grep "CLOSE TO PASSING" -A 50
   ```

2. **Understand what's expected**
   ```bash
   # Run the specific test
   ./.target/dist-fast/tsz --noEmit <test-file-path>

   # Compare with expected (from analyze output)
   ```

3. **Write a failing unit test**
   - Add test in appropriate crate (tsz-checker, tsz-solver, tsz-parser)
   - Verify it fails: `cargo nextest run <test-name>`

4. **Implement the fix**
   - Follow architecture rules in `docs/HOW_TO_CODE.md`
   - Use tracing, not eprintln!

5. **Verify**
   ```bash
   # Unit tests
   cargo nextest run -E 'not test(test_run_with_timeout_fails)'

   # Conformance slice
   ./scripts/conformance.sh run --offset 6318 --max 3159
   ```

6. **Commit and sync**
   ```bash
   git add <files>
   git commit -m "<description>

   https://claude.ai/code/session_01BUuJsGfUqEKJ9ecFqev7hV"
   git pull --rebase origin main
   git push -u origin claude/improve-conformance-tests-nGsTY
   ```

## Specific Fix Patterns

### Pattern 1: Extra Error Being Emitted (Easiest)

**Symptom**: We emit error X, but TSC doesn't.

**Examples**:
- `classWithPredefinedTypesAsNames2.ts`: We emit extra TS1068
- `derivedClassTransitivity3.ts`: We emit extra TS2345
- `privateNamesConstructorChain-1.ts`: We emit extra TS2416

**Approach**:
1. Find where the error is emitted
   ```bash
   grep -rn "TS2345\|2345" crates/tsz-checker/src/
   ```

2. Add a condition to NOT emit it in the specific case
3. Write unit test showing the case should not error

**Example Investigation**:
```bash
# For derivedClassTransitivity3.ts
./.target/dist-fast/tsz --noEmit TypeScript/tests/cases/.../derivedClassTransitivity3.ts

# Output shows:
# Line 19: error TS2345: Argument of type 'string' is not assignable to parameter of type 'number'

# The call is: c.foo('', '')
# Where c: C<string> and C.foo(x: T, y: T)
# Both args should be string, not number

# Issue: After failed assignment c = e, we're using e's signature
# Fix: Don't use assigned type after assignment check fails
```

### Pattern 2: Missing Error Code (Medium)

**Symptom**: TSC emits error X, we don't.

**Examples**:
- `varianceAnnotationValidation.ts`: Missing TS2636
- `classAbstractFactoryFunction.ts`: Missing TS2345
- `privateNameCircularReference.ts`: Missing TS7022

**Approach**:
1. Understand when TSC emits the error
2. Find similar error emissions in our code
3. Add the missing check
4. Write unit test

**Example**:
```rust
// For TS2636 (Variance annotation validation)
// Find: grep -rn "2636" crates/
// If not found, find similar validation
// Add validation at appropriate point
```

### Pattern 3: Wrong Error Code (Hard)

**Symptom**: We emit error X, TSC emits error Y.

**Examples**:
- Private name shadowing: We emit TS2339, should emit TS18014
- Symbol issues: We emit generic errors, should emit specific ones

**Approach**:
1. Understand the context that determines which error
2. Add context detection
3. Emit correct error based on context

**Example for Private Names**:
```rust
// Current: Always emit TS2339 for property not found
// Fix: Detect if property is private name (#name)
//      Detect if shadowing occurs (current class has same private name)
//      Emit TS18014 if shadowed, TS18016 if not declared, etc.

// Pseudocode:
if is_private_name(property_name) {
    if current_class_has_private_name(property_name) {
        // Shadowing
        emit(TS18014, "shadowed by another private identifier");
    } else if !target_class_has_private_name(property_name) {
        // Not declared
        emit(TS18016, "private identifier not declared");
    }
    // ... other cases
} else {
    emit(TS2339, "property does not exist");
}
```

## Debugging Techniques

### Use Tracing

```bash
# Trace checker behavior
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- file.ts 2>&1 | head -200

# Filter to specific module
TSZ_LOG="tsz_checker::state_type_analysis=trace" cargo run -- file.ts 2>&1 | less
```

### Compare with TSC Cache

```bash
# Find expected errors for a test
cat tsc-cache-full.json | jq '.[] | select(.file and (.file | contains("testname")))'
```

### Minimal Reproduction

Create minimal test case:
```typescript
// test.ts
class C { #x; }
class D extends C {
    #x;
    test(c: C) {
        c.#x;  // Should emit TS18014, not TS2339
    }
}
```

Test directly:
```bash
./.target/dist-fast/tsz --noEmit test.ts
```

## Common Pitfalls

1. **Don't break existing tests**: Always run full unit tests
2. **Don't add performance regressions**: Profile if touching hot paths
3. **Follow architecture**: Checker calls Solver, doesn't inspect types
4. **Commit frequently**: After each logical fix
5. **Sync with main**: Pull and push after EVERY commit

## Error Code Reference

Key error codes to understand:

| Code | Message | When to Emit |
|------|---------|--------------|
| TS1005 | '{' expected | Parser syntax errors |
| TS1068 | Unexpected token | Parser recovery (often cascading) |
| TS2322 | Type X not assignable to Y | Type compatibility |
| TS2339 | Property does not exist | Generic property access failure |
| TS2345 | Argument not assignable | Function call argument mismatch |
| TS2454 | Used before being assigned | Definite assignment analysis |
| TS18013 | Cannot access private identifier | Private name access from outside |
| TS18014 | Private identifier shadowed | Same private name in derived class |
| TS18016 | Private identifier not declared | Private name doesn't exist |
| TS18047 | Possibly null | Strict null checks with identifier |
| TS18048 | Possibly undefined | Strict null checks with identifier |
| TS2532 | Object possibly undefined | Strict null checks (generic) |

## Quick Wins to Try

1. **Extra TS1068 errors**: Parser recovery emitting cascading errors
2. **Extra TS2345 errors**: Function calls using wrong type after failed check
3. **Extra TS2416 errors**: Override compatibility being too strict
4. **Missing simple validation errors**: Find where TSC checks, add our check

## Resources

- `docs/architecture/NORTH_STAR.md` - Target architecture
- `docs/HOW_TO_CODE.md` - Coding patterns and rules
- `crates/tsz-checker/src/types/diagnostics.rs` - Error code definitions
- TypeScript source: `TypeScript/src/compiler/diagnosticMessages.json` - All TSC error messages

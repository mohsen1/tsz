# TSZ Implementation Roadmap

**Current Status**: 61.2% of conformance tests passing (7,700/12,583)
**Target**: Match TypeScript compiler behavior exactly

---

## Current State Overview

### What's Working Well ✓

1. **Core Compilation Pipeline**
   - Scanner (tokenization) - Complete
   - Parser (AST generation) - Complete
   - Binder (symbols & scopes) - Complete
   - Solver (type computation) - Comprehensive
   - Basic type checking - Mostly complete
   - Emitter (code generation) - Comprehensive

2. **Type System Features**
   - Structural types ✓
   - Generics with constraints ✓
   - Conditional types ✓
   - Mapped types ✓
   - Template literal types ✓
   - Union/intersection types ✓
   - Control flow narrowing ✓ (mostly)
   - TypeScript compatibility quirks ✓

3. **Error Reporting**
   - 2,129 diagnostic codes defined
   - ~1,301 codes always correct
   - Good error locations and messages

### What Needs Work ✗

1. **False Positives (1,280 tests)** - We're too strict
   - TS2339: Property access over-checking
   - TS2345: Function arguments over-strict
   - TS2322: Assignment over-strict
   - **Fix**: Refine existing checks, not new features

2. **Missing Error Detection (1,441 tests)** - We're too lenient
   - TS2322: Assignment edge cases
   - TS2339: Property lookups incomplete
   - TS2300: Duplicate detection missing
   - TS2411: Type extends validation missing
   - **Fix**: Complete partial implementations

3. **Unimplemented Codes (626 codes, 1,927 tests)**
   - TS7026: Unused variables
   - TS2551: Private member access
   - TS2585: Object patterns
   - TS2528: Readonly violations
   - TS1100: Super validation
   - Many others (see MISSING_FEATURES.md)

---

## Implementation Strategy

### Phase 1: Quick Wins (2-3 weeks)
**Goal: 70% → 75%+ pass rate**

#### Task 1.1: Fix TS2339 False Positives (284 tests)
- **Root cause**: Property access checks too strict
- **Expected improvement**: +284 tests (false positive fixes) + some assignment fixes
- **Files to change**:
  - `crates/tsz-checker/src/expr.rs` - property access checking
  - `crates/tsz-checker/src/symbol_resolver.rs` - property lookup
  - `crates/tsz-checker/src/assignability_checker.rs` - any propagation
- **Effort**: 4-6 hours
- **Testing**: Run `./scripts/conformance.sh analyze --category false-positive | grep TS2339`

#### Task 1.2: Fix TS2345 False Positives (262 tests)
- **Root cause**: Function call argument checking too strict
- **Expected improvement**: +262 tests (false positives) + some quick wins
- **Files to change**:
  - `crates/tsz-checker/src/call_checker.rs` - function call validation
  - `crates/tsz-checker/src/expr.rs` - call expressions
  - Improve overload resolution accuracy
- **Effort**: 4-6 hours
- **Testing**: `./scripts/conformance.sh analyze --category false-positive | grep TS2345`

#### Task 1.3: Fix TS2322 False Positives (241 tests)
- **Root cause**: Assignment compatibility over-strict
- **Expected improvement**: +241 tests (false positives) + 85 quick wins
- **Files to change**:
  - `crates/tsz-checker/src/assignability_checker.rs` - main logic
  - `crates/tsz-solver/src/judge.rs` & `lawyer.rs` - type compatibility
  - Add special cases for object literals, readonly handling
- **Effort**: 6-8 hours
- **Testing**: Run multiple test patterns to find all edge cases

**Phase 1 Total**: 14-20 hours → **~75%+ pass rate**

### Phase 2: Core Features (2-3 weeks)
**Goal: 75%+ → 80%+ pass rate**

#### Task 2.1: Implement TS2300 - Duplicate Identifier (39 all-missing + 19 quick wins)
- **What**: Detect when same name declared twice (except valid merging)
- **Files to change**:
  - `crates/tsz-binder/src/state_binding.rs` - Check before binding
  - Already has symbol tracking, just need validation
- **Effort**: 2-3 hours
- **Expected gain**: +58 tests

#### Task 2.2: Implement TS2411 - Type Extends (21 all-missing + 18 quick wins)
- **What**: Validate `extends` clauses in class/interface
- **Files to change**:
  - `crates/tsz-checker/src/declarations.rs`
  - `crates/tsz-checker/src/interface_type.rs`
  - Solver already has constraint support
- **Effort**: 3-4 hours
- **Expected gain**: +39 tests

#### Task 2.3: Implement TS2540 - Cannot Assign to Const (30+ tests)
- **What**: Flag reassignment to const variables
- **Files to change**:
  - `crates/tsz-checker/src/assignability_checker.rs`
  - `crates/tsz-checker/src/expr.rs` - assignment expressions
  - Parser already flags const, need to check
- **Effort**: 2-3 hours
- **Expected gain**: +30 tests

#### Task 2.4: Implement TS2528 - Readonly Property Assignment (17 tests)
- **What**: Flag assignment to readonly properties
- **Files to change**:
  - `crates/tsz-checker/src/assignability_checker.rs`
  - Check property flags during assignment
  - Solver has readonly support
- **Effort**: 3-4 hours
- **Expected gain**: +17 tests

#### Task 2.5: Implement TS1100 - Invalid Super (17 tests)
- **What**: Validate `super` usage (only in constructors/methods)
- **Files to change**:
  - `crates/tsz-checker/src/expr.rs` - super expressions
  - `crates/tsz-checker/src/statements.rs` - super() in constructors
  - Add context tracking to CheckerContext
- **Effort**: 2-3 hours
- **Expected gain**: +17 tests

**Phase 2 Total**: 12-17 hours → **~80%+ pass rate**

### Phase 3: Advanced Features (3-4 weeks)
**Goal: 80%+ → 85%+ pass rate**

#### Task 3.1: Fix Module Resolution (TS2307, TS2305)
- Implement proper file path resolution
- Validate module existence
- Check package.json exports
- **Effort**: 8-10 hours
- **Expected gain**: +150-200 tests

#### Task 3.2: Implement Module Augmentation (TS2708)
- Support `declare module` statements
- Merge augmentations with original module
- Validate augmentation rules
- **Effort**: 10-12 hours
- **Expected gain**: +50-80 tests

#### Task 3.3: Improve Private Member Access (TS2551, TS2371)
- Connect access checking with privacy flags
- Validate member accessibility
- Cross-file privacy checking
- **Effort**: 6-8 hours
- **Expected gain**: +50-70 tests

#### Task 3.4: Improve Overload Resolution (TS2769)
- Better call signature matching
- Improved error messages
- Handle constructor overloads
- **Effort**: 6-8 hours
- **Expected gain**: +60-100 tests

**Phase 3 Total**: 30-38 hours → **~85%+ pass rate**

### Phase 4: Long Tail (Ongoing)
**Goal: 85%+ → 95%+ pass rate**

- Implement remaining unimplemented error codes (626 total)
- Focus on highest-impact codes first
- Refinements for edge cases
- Performance optimization

---

## Implementation Checklist

### Before Starting Any Task

- [ ] Read the relevant implementation file
- [ ] Understand current code structure
- [ ] Write a failing unit test for the feature
- [ ] Check if Solver already has needed infrastructure
- [ ] Design the fix (don't just patch)

### While Implementing

- [ ] Use tracing (NOT eprintln!) for debugging
- [ ] Run conformance tests frequently to measure progress
- [ ] Keep modules under 2000 lines
- [ ] Follow coding conventions in CLAUDE.md
- [ ] Add unit tests for new logic

### After Completing Task

- [ ] Run full conformance suite: `./scripts/conformance.sh analyze`
- [ ] Measure exact pass rate improvement
- [ ] Update IMPLEMENTATION_ROADMAP.md with completion
- [ ] Commit with clear message including test count improvement
- [ ] Document any blockers or surprises found

---

## Monitoring Progress

### Commands to Track Progress

```bash
# See current pass rate
./scripts/conformance.sh analyze | grep "Total failing\|FINAL RESULTS"

# Test specific error code
./scripts/conformance.sh run --error-code 2339

# Test specific category
./scripts/conformance.sh analyze --category false-positive

# See which tests are closest to passing
./scripts/conformance.sh analyze --category close
```

### Expected Progress Timeline

| Phase | Weeks | Pass Rate | Tests Added |
|-------|-------|-----------|------------|
| Start | - | 61.2% | 7,700 |
| Phase 1 | 2-3 | 75%+ | +1,400 |
| Phase 2 | 2-3 | 80%+ | +600 |
| Phase 3 | 3-4 | 85%+ | +600 |
| Phase 4+ | Ongoing | 90%+ → 99%+ | Remaining tests |

---

## Architecture Guidelines for Implementation

### Key Principle: Solver-First

**Before implementing a feature:**

1. Check if the **Solver** (type computation) already supports it
2. If yes → Just add **Checker** logic (orchestration)
3. If no → Implement in **Solver** first, then add **Checker** logic

**Example**: Readonly property checking
- Solver: Already represents readonly types ✓
- Checker: Needs to check readonly flag during assignment ✗
- Solution: Just add check in `assignability_checker.rs`

### Where to Implement by Feature Type

| Feature Type | Primary File | Secondary Files |
|--------------|--------------|-----------------|
| Assignment compatibility | `assignability_checker.rs` | `judge.rs`, `lawyer.rs` (solver) |
| Property access | `expr.rs` | `symbol_resolver.rs` |
| Function calls | `call_checker.rs` | `expr.rs` |
| Declarations | `declarations.rs` | `interface_type.rs`, `class_checker.rs` |
| Type narrowing | `flow_analyzer.rs` | `narrowing.rs` (solver) |
| Name resolution | `symbol_resolver.rs` | `scope_finder.rs` |
| Error reporting | `error_reporter.rs` | All modules emit diagnostics |

### Error Reporting Pattern

```rust
// Pattern 1: Simple validation
if !is_valid {
    self.diagnostics.push(Diagnostic::new(
        node.span,
        DiagnosticCode::TS2300,  // Duplicate identifier
        format!("Duplicate declaration '{}'", name),
    ));
}

// Pattern 2: With suggestion
if !is_valid {
    self.diagnostics.push(Diagnostic::new(
        node.span,
        DiagnosticCode::TS2322,  // Type not assignable
        format!("Type '{}' is not assignable to type '{}'",
                self.type_to_string(actual),
                self.type_to_string(expected)),
    ).with_suggestion(...));
}
```

---

## Dependencies & Blockers

### No Blockers for Phase 1
- All required infrastructure exists
- Just need to refine existing checks

### Potential Blockers for Phase 2
- **None identified** - all features have necessary Solver support

### Potential Blockers for Phase 3
- **Module resolution** - May need file system abstraction improvements
- **Declaration merging** - May need symbol table restructuring

### Risk Mitigation

1. **Incremental testing** - Run conformance tests after each sub-task
2. **Feature isolation** - Each feature in separate commit
3. **Rollback plan** - Easy to revert if something breaks

---

## Testing Strategy

### Unit Test Pattern

```rust
#[test]
fn test_duplicate_identifier_error() {
    let (source, diags) = check_source(r#"
        const x = 5;
        const x = 10;  // Error: duplicate
    "#);

    assert_has_diagnostic(&diags, DiagnosticCode::TS2300, "Duplicate");
}

#[test]
fn test_interface_extends_validation() {
    let (source, diags) = check_source(r#"
        interface Foo extends number {}  // Error: must be object type
    "#);

    assert_has_diagnostic(&diags, DiagnosticCode::TS2411, "constraint");
}
```

### Conformance Test Validation

After implementing a feature:

```bash
# See all tests with that error code
./scripts/conformance.sh run --error-code 2300 --verbose

# Test on smaller subset first
./scripts/conformance.sh run --max 100 --error-code 2300

# Full validation before submitting
./scripts/conformance.sh analyze
```

---

## Documentation Updates

As features are implemented, update:

1. **MISSING_FEATURES.md**
   - Mark feature as implemented
   - Note any quirks discovered

2. **IMPLEMENTATION_ROADMAP.md** (this file)
   - Check off completed tasks
   - Update expected timelines
   - Document blockers if any

3. **Code comments**
   - Document non-obvious design choices
   - Link to TypeScript spec sections
   - Note edge cases handled

---

## Success Criteria

### Phase 1 Complete
- [ ] TS2339 false positives fixed (verify with `--error-code 2339`)
- [ ] TS2345 false positives fixed (verify with `--error-code 2345`)
- [ ] TS2322 false positives + quick wins fixed
- [ ] Pass rate >= 75%
- [ ] No regressions in other tests

### Phase 2 Complete
- [ ] TS2300 implemented (duplicate detection)
- [ ] TS2411 implemented (extends validation)
- [ ] TS2540 implemented (const reassignment)
- [ ] TS2528 implemented (readonly assignment)
- [ ] TS1100 implemented (super validation)
- [ ] Pass rate >= 80%
- [ ] All Phase 1 fixes still working

### Phase 3 Complete
- [ ] Module resolution improved
- [ ] Module augmentation basic support
- [ ] Private member access checking
- [ ] Overload resolution improved
- [ ] Pass rate >= 85%
- [ ] Previous phases still passing

### Phase 4 Complete
- [ ] Pass rate >= 95%
- [ ] All critical error codes implemented
- [ ] Edge cases handled
- [ ] Performance meets targets

---

## Resources & References

### Key Documentation
- `/home/user/tsz/CLAUDE.md` - Project rules
- `/home/user/tsz/docs/HOW_TO_CODE.md` - Coding conventions
- `/home/user/tsz/docs/architecture/NORTH_STAR.md` - Architecture
- `/home/user/tsz/docs/CONFORMANCE_REPORT.md` - Test analysis
- `/home/user/tsz/docs/MISSING_FEATURES.md` - What's not implemented

### TypeScript References
- TypeScript source: `/home/user/tsz/TypeScript/`
- TypeScript handbook for spec details
- Error code definitions in `crates/tsz-common/src/diagnostics.rs`

### Tools
- Conformance runner: `./scripts/conformance.sh`
- Tracing: Use `tsz-tracing` skill (NOT `eprintln!`)
- Profiling: `cargo build --release` + profiler

### Debugging Workflow
```bash
# 1. Find failing test
./scripts/conformance.sh run --max 50 | grep FAIL

# 2. Extract test file path
# Example: ./TypeScript/tests/cases/compiler/acceptSymbolAsWeakType.ts

# 3. Run with tracing
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- \
    ./TypeScript/tests/cases/compiler/acceptSymbolAsWeakType.ts

# 4. Compare with tsc
npx tsc --noEmit ./TypeScript/tests/cases/compiler/acceptSymbolAsWeakType.ts

# 5. Find difference and fix
```

---

## Next Steps

1. **Verify this roadmap** - Share with team, adjust based on feedback
2. **Start Phase 1, Task 1.1** - Fix TS2339 false positives
3. **Measure baseline** - Run conformance before changes
4. **Implement incrementally** - Small commits, frequent testing
5. **Track progress** - Update this document weekly

---

## Notes

- This roadmap is based on static analysis of 4,883 failing tests
- Actual implementation may reveal surprises (blocked dependencies, etc.)
- Be prepared to adjust timelines and priorities
- The false positive fixes are highest priority (easiest, biggest impact)

**Current Date**: 2026-02-12
**Last Updated**: 2026-02-12
**Next Review**: After Phase 1 completion

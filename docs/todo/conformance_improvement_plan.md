# Conformance Improvement Plan (Jan 2026)

## Executive Summary

**Current State:** 31.1% pass rate (3,746/12,054 tests)

This document outlines the critical issues causing conformance failures, prioritized by impact. The analysis was conducted using Gemini AI with full codebase context across solver, checker, parser, and module resolution components.

### Top Issues by Impact

| Issue | Extra Errors | Missing Errors | Root Cause |
|-------|-------------|----------------|------------|
| Type Assignability | 12,108 (TS2322) | 755 | ERROR type propagation + Readonly bugs |
| Readonly Properties | 10,488 (TS2540) | 0 | Arrays incorrectly marked readonly |
| Global Types | 0 | 7,560 (TS2318) | Embedded libs never loaded |
| Module Resolution | 3,950 (TS2307) | 948 (TS2792) | Ambient modules not checked |
| Parser Errors | 3,635 (TS1005) | 0 | Strict mode keywords as identifiers |
| Name Resolution | 3,402 (TS2304) | 1,684 | Global suppression + namespace issues |
| Circular Constraints | 2,123 (TS2313) | 0 | Premature cycle detection |
| Target Library | 0 | 1,748 (TS2583) | lib_loader not wired to checker |
| Value/Type | 1,739 (TS2749) | 0 | Namespace discrimination |
| Arguments | 1,686 (TS2345) | 0 | Cascading from TS2322/TS2540 |
| Iterators | 0 | 1,558 (TS2488) | Iterable checker incomplete |

---

## Phase 1: Critical Fixes (Highest Impact)

### 1.1 Fix ERROR Type Propagation [CRITICAL]

**Impact:** Fixes ~12,108 extra TS2322 and many cascading TS2345 errors

**Problem:** The solver treats `TypeId::ERROR` as incompatible with everything, causing cascading errors. When a type cannot be resolved (TS2304), every subsequent usage triggers TS2322.

**Location:** `src/solver/subtype.rs`, `src/solver/compat.rs`

**Current (broken):**
```rust
// src/solver/subtype.rs
if source == TypeId::ERROR || target == TypeId::ERROR {
    return SubtypeResult::False;  // Causes cascading errors!
}
```

**Fix:**
```rust
// src/solver/subtype.rs - check_subtype
if source == TypeId::ERROR || target == TypeId::ERROR {
    return SubtypeResult::True;  // Allow ERROR types to suppress cascading
}

// src/solver/compat.rs - check_assignable_fast_path
if source == TypeId::ERROR || target == TypeId::ERROR {
    return Some(true);  // ERROR is assignable to/from anything
}
```

**Tasks:**
- [ ] Update `src/solver/subtype.rs` to return `True` for ERROR types
- [ ] Update `src/solver/compat.rs` fast path
- [ ] Add tests for ERROR type suppression

---

### 1.2 Fix Array Readonly Logic [CRITICAL]

**Impact:** Fixes ~10,488 extra TS2540 errors

**Problem:** `is_readonly()` in index_signatures.rs unconditionally marks ALL `Array` and `Tuple` types as readonly, causing every array element assignment to fail.

**Location:** `src/solver/index_signatures.rs`

**Current (broken):**
```rust
pub fn is_readonly(&self, obj: TypeId, kind: IndexKind) -> bool {
    match self.db.lookup(obj) {
        Some(TypeKey::Array(_) | TypeKey::Tuple(_)) => {
            match kind {
                IndexKind::Number => true,  // BUG: Should be false!
                IndexKind::String => true,  // BUG: Should be false!
            }
        }
        // ...
    }
}
```

**Fix:**
```rust
pub fn is_readonly(&self, obj: TypeId, kind: IndexKind) -> bool {
    match self.db.lookup(obj) {
        // Regular arrays are mutable
        Some(TypeKey::Array(_) | TypeKey::Tuple(_)) => false,
        
        // Only ReadonlyType wrapper makes them readonly
        Some(TypeKey::ReadonlyType(_)) => true,
        
        // ... existing ObjectWithIndex logic ...
    }
}
```

**Tasks:**
- [ ] Fix `is_readonly` in `src/solver/index_signatures.rs`
- [ ] Add tests for mutable array assignment
- [ ] Add tests for `readonly T[]` vs `T[]` distinction

---

### 1.3 Wire Embedded Libraries [CRITICAL]

**Impact:** Fixes ~7,560 missing TS2318 and ~1,748 missing TS2583 errors

**Problem:** `driver.rs` only loads lib.d.ts files from disk. The `embedded_libs.rs` module (containing all standard libraries) is **never used**. In test environments without disk libs, all global types are missing.

**Location:** `src/cli/driver.rs`, `src/embedded_libs.rs`, `src/lib_loader.rs`

**Current (broken):**
```rust
// driver.rs - load_lib_files_for_contexts
if lib_path.exists() {
    // Load from disk
} 
// MISSING: else { use embedded_libs }
```

**Fix:**
```rust
// driver.rs - load_lib_files_for_contexts
if lib_path.exists() {
    // Load from disk
} else if let Some(content) = embedded_libs::get_lib_by_file_name(&lib_name) {
    // Use embedded lib
    let lib_ctx = parse_lib_file(lib_name, content);
    lib_contexts.push(lib_ctx);
}
```

**Tasks:**
- [ ] Update `load_lib_files_for_contexts` in `src/cli/driver.rs` to use embedded libs as fallback
- [ ] Wire `lib_loader::emit_error_lib_target_mismatch` (TS2583) to checker
- [ ] Wire `lib_loader::emit_error_global_type_missing` (TS2318) to checker
- [ ] Add intrinsic type checks for Array, Boolean, String when encountering related syntax
- [ ] Ensure `is_es2015_plus_type` check upgrades TS2304 to TS2583 for known ES2015+ types

---

### 1.4 Check Ambient Modules Before TS2307 [HIGH]

**Impact:** Fixes ~3,950 extra TS2307 errors

**Problem:** `ModuleResolver` only checks the file system. It doesn't check if `declare module "foo"` exists in loaded .d.ts files before reporting TS2307.

**Location:** `src/checker/import_checker.rs`, `src/module_resolver.rs`

**Current (broken):**
```rust
// Resolver returns NotFound immediately
// Checker emits TS2307 without checking ambient modules
```

**Fix:**
```rust
// In import_checker.rs or wherever resolver.resolve is called
if let Err(ResolutionFailure::NotFound { specifier }) = resolver.resolve(specifier) {
    // Check ambient modules BEFORE emitting error
    if self.ctx.binder.declared_modules.contains(&specifier) {
        return Ok(AmbientModuleResolution { ... });
    }
    if self.is_ambient_module_match(&specifier) {
        return Ok(AmbientModuleResolution { ... });
    }
    // Only now emit TS2307
    self.emit_error(TS2307, ...);
}
```

**Tasks:**
- [ ] Add ambient module check in `src/checker/import_checker.rs` before emitting TS2307
- [ ] Add ambient module check in `src/checker/module_checker.rs` for dynamic imports
- [ ] Implement TS2792 hint ("set moduleResolution to nodenext") for Classic mode failures

---

## Phase 2: High Impact Fixes

### 2.1 Fix Circular Constraint Detection [HIGH]

**Impact:** Fixes ~2,123 extra TS2313 errors + timeout issues

**Problem:** `should_resolve_recursive_type_alias` in checker state returns `true` for classes, causing `get_type_of_symbol` to detect a false cycle when resolving constraints like `class C<T extends C<T>>`.

**Location:** `src/checker/state.rs`

**Current (broken):**
```rust
fn should_resolve_recursive_type_alias(&self, symbol: &Symbol) -> bool {
    // Returns true for classes, triggering cycle detection
    !symbol.is_type_alias()  // BUG: Classes shouldn't trigger early resolution
}
```

**Fix:**
The cycle detection for constraints needs special handling:
1. Type parameters should be resolved BEFORE adding the class to `symbol_resolution_set`
2. OR use a separate tracking set for constraint resolution
3. OR allow incomplete type references in constraints (deferred resolution)

**Tasks:**
- [ ] Analyze exact cycle detection flow for type parameter constraints
- [ ] Fix `should_resolve_recursive_type_alias` or constraint resolution order
- [ ] Add tests for `class C<T extends C<T>>` pattern
- [ ] Fix timeout issues on `classExtendsItself*.ts` tests

---

### 2.2 Fix Parser Keyword Handling [HIGH]

**Impact:** Fixes ~3,635 extra TS1005 errors

**Problem:** The scanner classifies strict-mode reserved words (`package`, `implements`, `interface`, `public`, `private`, etc.) as Keywords instead of Identifiers. `parse_identifier()` then emits TS1005.

**Location:** `src/parser/state.rs`, `src/scanner_impl.rs`

**Current (broken):**
```rust
fn parse_identifier(&mut self) -> NodeIndex {
    self.parse_expected(SyntaxKind::Identifier); // Fails for reserved words
}
```

**Fix:**
```rust
fn parse_identifier(&mut self) -> NodeIndex {
    // Allow contextual keywords as identifiers in non-strict mode
    if self.token_is_identifier_or_keyword() {
        return self.parse_identifier_name();
    }
    self.parse_expected(SyntaxKind::Identifier);
}

fn token_is_identifier_or_keyword(&self) -> bool {
    self.current_token == SyntaxKind::Identifier
        || self.is_contextual_keyword()
        || (!self.is_strict_mode() && self.is_future_reserved_word())
}
```

**Tasks:**
- [ ] Add `token_is_identifier_or_keyword()` helper to parser
- [ ] Update `parse_identifier()` to accept contextual keywords
- [ ] Track strict mode properly for reserved word handling
- [ ] Add tests for `type package = number` and similar patterns

---

### 2.3 Fix Value/Type Namespace Discrimination [MEDIUM]

**Impact:** Fixes ~1,739 extra TS2749 errors

**Problem:** We're incorrectly reporting "refers to a value, but is being used as a type" when the name resolution should find the type binding.

**Location:** `src/checker/symbol_resolver.rs`, `src/checker/type_checking.rs`

**Root Cause:** Symbol resolution may be returning value symbols when type symbols are needed, or vice versa.

**Tasks:**
- [ ] Audit `resolve_identifier_symbol` for namespace handling
- [ ] Ensure type position lookups check type namespace first
- [ ] Ensure value position lookups check value namespace first
- [ ] Add tests for dual-namespace names (classes, enums, namespaces)

---

## Phase 3: Medium Impact Fixes

### 3.1 Implement Iterator Checking [MEDIUM]

**Impact:** Fixes ~1,558 missing TS2488 errors, improves for-of (11.4% → higher)

**Problem:** `src/checker/iterable_checker.rs` is likely a stub or overly permissive. It doesn't emit TS2488 when iterating non-iterable types.

**Location:** `src/checker/iterable_checker.rs`, `src/solver/type_queries.rs`

**Tasks:**
- [ ] Implement `check_for_of_iterability` to use `is_iterable_type_kind` from solver
- [ ] Emit TS2488 when type doesn't have `[Symbol.iterator]()` method
- [ ] Handle `any`/`unknown` correctly (may need different errors)
- [ ] Add tests for iterating non-iterable types

---

### 3.2 Implement Generator/Yield Checking [MEDIUM]

**Impact:** Fixes generators (0% → higher), yield expressions (0% → higher)

**Problem:** The checker side (`generators.rs`, `expr.rs`) is not utilizing solver capabilities (`contextual.rs` has `get_generator_yield_type` etc.) to validate yield expressions.

**Location:** `src/checker/generators.rs`, `src/checker/expr.rs`

**Tasks:**
- [ ] Implement `check_yield_expression` using solver's generator type utilities
- [ ] Validate yield type against function's `Generator<Y, R, N>` return type
- [ ] Handle `yield*` delegation
- [ ] Add tests for generator function bodies

---

### 3.3 Implement `using` Declarations [MEDIUM]

**Impact:** Fixes usingDeclarations (0% → higher)

**Problem:** The `using` keyword is parsed but type checking doesn't validate `Disposable`/`AsyncDisposable` protocol.

**Location:** `src/checker/type_checking.rs`

**Tasks:**
- [ ] Detect `using` / `await using` in `check_variable_declaration`
- [ ] Validate type has `[Symbol.dispose]()` method for `using`
- [ ] Validate type has `[Symbol.asyncDispose]()` method for `await using`
- [ ] Ensure `Disposable`/`AsyncDisposable` global types are available
- [ ] Add TS2850/TS2851 errors for invalid disposal types

---

### 3.4 Fix Property Access Errors [MEDIUM]

**Impact:** Fixes ~621 missing TS2339 and ~679 missing TS18050 errors

**Problem:** 
- TS2339 ("Property does not exist"): May be suppressed when object is ANY/ERROR/UNKNOWN
- TS18050 ("Value cannot be used here"): Control flow not inferring `never` correctly for unreachable code

**Location:** `src/checker/type_checking.rs`, `src/checker/flow_analysis.rs`

**Tasks:**
- [ ] Audit `get_type_of_property_access_inner` for over-suppression
- [ ] Ensure control flow correctly narrows to `never` in unreachable branches
- [ ] Distinguish `null` keyword access (TS18050) from possibly-null variable (TS2531)

---

## Phase 4: Lower Impact Fixes

### 4.1 Dynamic Import Type Integration [LOW]

**Problem:** Dynamic imports work (Phase 1.3 of module_resolution_plan.md) but conformance is 0% due to missing `Promise` global type.

**Tasks:**
- [ ] Ensure Phase 1.3 fixes (embedded libs) enable Promise<T> return types
- [ ] Verify `get_dynamic_import_type` works with lib files loaded

---

### 4.2 Node.js Module Resolution [LOW]

**Problem:** Node.js resolution (0%) depends on correct lib loading and module resolver integration.

**Tasks:**
- [ ] Ensure Phase 1.3/1.4 fixes enable Node.js tests
- [ ] Verify `resolved_modules` map is populated by driver
- [ ] Check `index.js` resolution in Node16/NodeNext modes

---

### 4.3 Async/Await Validation [LOW]

**Problem:** Async function checks are disabled due to `is_promise_global_available` not detecting Promise correctly.

**Location:** `src/checker/function_type.rs`, `src/checker/promise_checker.rs`

**Tasks:**
- [ ] Fix `is_promise_global_available` after Phase 1.3 lib loading
- [ ] Re-enable TS2697 ("Async function must return Promise")
- [ ] Add tests for async function return type validation

---

## Testing Strategy

### Priority Test Categories

After fixes, monitor these categories:

| Category | Current | Target | Depends On |
|----------|---------|--------|------------|
| compiler | 33.8% | 50%+ | Phases 1-2 |
| modules | 2.1% | 30%+ | Phase 1.3, 1.4 |
| for-ofStatements | 11.4% | 50%+ | Phase 3.1 |
| generators | 0% | 30%+ | Phase 3.2 |
| usingDeclarations | 0% | 30%+ | Phase 3.3 |
| dynamicImport | 0% | 30%+ | Phase 1.3, 4.1 |
| node | 0% | 30%+ | Phase 1.4, 4.2 |

### Regression Tests

Add specific tests for:
- ERROR type propagation (TS2322 cascading)
- Mutable vs readonly array assignment
- Ambient module pattern matching
- Circular constraint detection
- Contextual keyword parsing
- Iterator protocol checking

---

## Implementation Order

### Week 1: Critical Type System Fixes
1. **1.1 ERROR Type Propagation** - Biggest impact on TS2322
2. **1.2 Array Readonly Logic** - Biggest impact on TS2540

### Week 2: Library Loading
3. **1.3 Wire Embedded Libraries** - Enables global types
4. **1.4 Ambient Module Checking** - Reduces TS2307

### Week 3: Parser/Checker Fixes  
5. **2.1 Circular Constraint Detection** - Fixes TS2313 + timeouts
6. **2.2 Parser Keyword Handling** - Fixes TS1005

### Week 4: Feature Implementation
7. **2.3 Value/Type Namespaces** - Fixes TS2749
8. **3.1 Iterator Checking** - Enables for-of tests
9. **3.2 Generator Checking** - Enables generator tests

### Week 5+: Polish
10. **3.3 using Declarations**
11. **3.4 Property Access Errors**
12. **Phase 4 items**

---

## Success Metrics

| Metric | Current | Phase 1 Target | Final Target |
|--------|---------|----------------|--------------|
| Pass Rate | 31.1% | 45% | 60%+ |
| TS2322 extra | 12,108 | <2,000 | <500 |
| TS2540 extra | 10,488 | <500 | <100 |
| TS2318 missing | 7,560 | <1,000 | <200 |
| TS2307 extra | 3,950 | <1,000 | <300 |
| TS1005 extra | 3,635 | <1,000 | <300 |
| OOM tests | 5 | 0 | 0 |
| Timeout tests | 94 | <20 | <5 |

---

## Appendix: Error Code Reference

| Code | Message | Type |
|------|---------|------|
| TS1005 | '{0}' expected | Parser |
| TS2304 | Cannot find name '{0}' | Name Resolution |
| TS2307 | Cannot find module '{0}' | Module Resolution |
| TS2313 | Type parameter '{0}' has circular constraint | Type System |
| TS2318 | Cannot find global type '{0}' | Library Loading |
| TS2322 | Type '{0}' is not assignable to type '{1}' | Type Compatibility |
| TS2339 | Property '{0}' does not exist on type '{1}' | Property Access |
| TS2345 | Argument of type '{0}' is not assignable | Argument Checking |
| TS2488 | Type '{0}' must have '[Symbol.iterator]()' | Iterator Protocol |
| TS2540 | Cannot assign to '{0}' because it is readonly | Readonly Checking |
| TS2583 | Cannot find name - change target library | Library Target |
| TS2749 | '{0}' refers to a value, but used as a type | Namespace |
| TS2792 | Cannot find module - set moduleResolution | Resolution Hint |
| TS18050 | The value '{0}' cannot be used here | Never/Null Access |

---

## Key Files Reference

| Area | Files |
|------|-------|
| Type Compatibility | `src/solver/subtype.rs`, `src/solver/compat.rs` |
| Readonly Checking | `src/solver/index_signatures.rs` |
| Library Loading | `src/cli/driver.rs`, `src/embedded_libs.rs`, `src/lib_loader.rs` |
| Module Resolution | `src/module_resolver.rs`, `src/checker/import_checker.rs` |
| Circular Detection | `src/checker/state.rs` |
| Parser | `src/parser/state.rs`, `src/scanner_impl.rs` |
| Name Resolution | `src/checker/symbol_resolver.rs` |
| Iterators | `src/checker/iterable_checker.rs`, `src/solver/type_queries.rs` |
| Generators | `src/checker/generators.rs`, `src/solver/contextual.rs` |

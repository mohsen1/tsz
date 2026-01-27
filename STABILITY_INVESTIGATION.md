# Stability Issues Investigation - Team 10

## Problem Statement
113 worker crashes, 11 test crashes, 10 OOM kills, 52 timeouts affecting test stability.

## Analyzed Test Cases

### OOM Issues (Likely Infinite Recursion)
1. **staticPropSuper.ts** - Constructor with static properties and super calls
2. **superCallWithCommentEmit01.ts** - Super call with comments in emit phase
3. **checkSuperCallBeforeThisAccessing5.ts** - Property access before super() call

### Timeout Issues (Likely Infinite Loops)
4. **typeofOperatorWithEnumType.ts** - typeof operator on enum types
5. **typeofOperatorWithNumberType.ts** - typeof operator on number types

### Crash Issues
6. **requireOfJsonFileWithoutExtensionResolvesToTs.ts** - Module resolution edge case
7. **sourceMapWithNonCaseSensitiveFileNames.ts** - Source map with case-insensitive paths
8. **awaitClassExpression_es5.ts** - Async/await with class expression ES5 lowering
9. **templateLiteralTypes6.ts** - Template literal with indexed access types

## Findings

### Existing Protections (GOOD)
The codebase already has comprehensive recursion limits:

1. **Solver/Type System:**
   - `MAX_SUBTYPE_DEPTH: 100` (src/solver/subtype.rs:26)
   - `MAX_TOTAL_SUBTYPE_CHECKS: 100,000` (src/solver/subtype.rs:190)
   - `MAX_INSTANTIATION_DEPTH: 50` (src/solver/instantiate.rs:21)
   - `MAX_EVALUATE_DEPTH: 50` (src/solver/evaluate.rs:39)
   - `MAX_CONSTRAINT_RECURSION_DEPTH: 100` (src/solver/operations.rs:46)
   - `TEMPLATE_LITERAL_EXPANSION_LIMIT: 100,000` (src/solver/intern.rs:41)

2. **Checker:**
   - `MAX_CALL_DEPTH: 20` (src/checker/state.rs:128)
   - `MAX_INSTANTIATION_DEPTH: 50` (src/checker/state.rs:125)
   - `MAX_EXPR_CHECK_DEPTH: 500` (src/checker/expr.rs:13)
   - `MAX_OPTIONAL_CHAIN_DEPTH: 1000` (src/checker/optional_chain.rs:38)
   - `MAX_FLOW_ANALYSIS_ITERATIONS: 100,000` (src/checker/flow_analyzer.rs:22)

3. **Emitter:**
   - `MAX_EMIT_RECURSION_DEPTH: 1000` (src/emitter/mod.rs:183)

4. **Parser:**
   - `MAX_RECURSION_DEPTH: 1000` (src/parser/state.rs:135)

### Identified Gaps (NEEDS FIX)

#### 1. TypeQuery (typeof) Resolution Cycle Detection
**Location:** `src/checker/type_checking.rs:10247` - `resolve_type_query_type()`

**Issue:** The function calls `get_type_of_symbol()` without cycle detection. If typeof resolution creates a cycle (e.g., enum member typeof chains), it could loop infinitely.

```rust
pub(crate) fn resolve_type_query_type(&mut self, type_id: TypeId) -> TypeId {
    match key {
        TypeKey::TypeQuery(SymbolRef(sym_id)) => self.get_type_of_symbol(SymbolId(sym_id)),
        // ^ No cycle detection for recursive typeof chains
    }
}
```

**Affected Tests:**
- typeofOperatorWithEnumType.ts (timeout)
- typeofOperatorWithNumberType.ts (timeout)

**Fix Required:** Add a HashSet to track visited symbols during typeof resolution.

#### 2. Super Call Flow Analysis Missing Iteration Limit
**Location:** `src/checker/flow_analysis.rs:159` - `find_super_statement_start()`

**Issue:** Simple linear search, but if called repeatedly in a complex control flow graph, could contribute to timeouts. The flow analysis has MAX_FLOW_ANALYSIS_ITERATIONS but individual helper functions don't.

**Affected Tests:**
- staticPropSuper.ts (OOM)
- superCallWithCommentEmit01.ts (OOM)
- checkSuperCallBeforeThisAccessing5.ts (OOM)

**Fix Required:** Add depth/iteration guards to flow analysis helper functions.

#### 3. Template Literal Nested Type Resolution
**Location:** `src/solver/evaluate_rules/template_literal.rs:42`

**Issue:** While TEMPLATE_LITERAL_EXPANSION_LIMIT exists, the `count_literal_members()` function (line 124) recursively processes unions without depth tracking:

```rust
pub fn count_literal_members(&self, type_id: TypeId) -> usize {
    if let Some(TypeKey::Union(members)) = self.interner().lookup(type_id) {
        for &member in members.iter() {
            let member_count = self.count_literal_members(member); // <- recursive
        }
    }
}
```

**Affected Tests:**
- templateLiteralTypes6.ts (crash)

**Fix Required:** Add recursion depth parameter to `count_literal_members()`.

#### 4. Source Map Generation with Recursive Structures
**Location:** `src/emitter/mod.rs:1074`

**Issue:** While emit has `MAX_EMIT_RECURSION_DEPTH: 1000`, source map generation might trigger additional recursion without being counted properly if case-sensitive path handling creates cycles.

**Affected Tests:**
- sourceMapWithNonCaseSensitiveFileNames.ts (crash)

**Fix Required:** Audit source map path resolution for cycles.

#### 5. Module Resolution Infinite Cycles
**Location:** Module resolution code (need to identify exact location)

**Issue:** JSON module resolution with extension fallback might create resolution cycles.

**Affected Tests:**
- requireOfJsonFileWithoutExtensionResolvesToTs.ts (crash)

**Fix Required:** Add visited set to module resolution.

#### 6. Async/Await ES5 Transform Emit Depth
**Location:** `src/transforms/async_es5_*.rs` and `src/emitter/es5_helpers.rs`

**Issue:** Async/await lowering combined with class expressions might exceed emit depth limits due to nested transformation.

**Affected Tests:**
- awaitClassExpression_es5.ts (crash)

**Fix Required:** Review transform depth tracking.

## Recommended Fixes (Priority Order)

### HIGH PRIORITY (Causes timeouts/OOM)

#### Fix 1: Add typeof resolution cycle detection
```rust
// src/checker/type_checking.rs

// Add field to CheckerState context:
// typeof_resolution_stack: RefCell<FxHashSet<u32>>,

pub(crate) fn resolve_type_query_type(&mut self, type_id: TypeId) -> TypeId {
    use crate::solver::{SymbolRef, TypeKey};

    let Some(key) = self.ctx.types.lookup(type_id) else {
        return type_id;
    };

    match key {
        TypeKey::TypeQuery(SymbolRef(sym_id)) => {
            // Check for cycle
            if self.ctx.typeof_resolution_stack.borrow().contains(&sym_id) {
                eprintln!("Warning: typeof resolution cycle detected for symbol {}", sym_id);
                return TypeId::ERROR;
            }

            // Mark as visiting
            self.ctx.typeof_resolution_stack.borrow_mut().insert(sym_id);

            // Resolve
            let result = self.get_type_of_symbol(SymbolId(sym_id));

            // Unmark
            self.ctx.typeof_resolution_stack.borrow_mut().remove(&sym_id);

            result
        }
        // ... rest of the function
    }
}
```

#### Fix 2: Add depth limit to count_literal_members
```rust
// src/solver/evaluate_rules/template_literal.rs

const MAX_LITERAL_COUNT_DEPTH: u32 = 50;

pub fn count_literal_members_impl(&self, type_id: TypeId, depth: u32) -> usize {
    if depth > MAX_LITERAL_COUNT_DEPTH {
        return 0; // Abort - too deep
    }

    if let Some(TypeKey::Union(members)) = self.interner().lookup(type_id) {
        let members = self.interner().type_list(members);
        let mut count = 0;
        for &member in members.iter() {
            let member_count = self.count_literal_members_impl(member, depth + 1);
            if member_count == 0 {
                return 0;
            }
            count += member_count;
        }
        count
    } else if let Some(TypeKey::Literal(_)) = self.interner().lookup(type_id) {
        1
    } else if type_id == TypeId::STRING || ... {
        0
    } else {
        0
    }
}

pub fn count_literal_members(&self, type_id: TypeId) -> usize {
    self.count_literal_members_impl(type_id, 0)
}
```

### MEDIUM PRIORITY (Causes crashes)

#### Fix 3: Add module resolution cycle detection
Need to audit module_resolver.rs and add visited set to prevent re-resolving the same module+extension combination.

#### Fix 4: Review source map path canonicalization
Ensure case-insensitive path resolution doesn't create infinite cycles in source map generation.

### LOW PRIORITY (Rare cases)

#### Fix 5: Audit async/await transform depth
Review transform application order and ensure depth counters propagate correctly.

## Testing Strategy

1. Create minimal reproduction tests for each failure mode
2. Add unit tests for cycle detection logic
3. Add integration tests with intentionally pathological inputs
4. Run full conformance suite to verify no regressions

## Success Metrics

- Worker crashes: 113 → <5
- Test crashes: 11 → 0
- OOM kills: 10 → 0
- Timeouts: 52 → <10

## Timeline Estimate

- High priority fixes: 2-3 hours
- Medium priority fixes: 1-2 hours
- Testing and validation: 2-3 hours
- Total: ~6-8 hours

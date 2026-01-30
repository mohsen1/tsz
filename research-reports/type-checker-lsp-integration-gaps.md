# Type Checker Gaps Affecting LSP Integration
## Research Team 5 - Detailed Investigation Report

**Date**: 2026-01-30
**Team**: Research Team 5 - Type Checker/LSP Integration
**Focus**: How type checker gaps impact LSP features and implementation priorities

---

## Executive Summary

This report analyzes type checker implementation gaps and their direct impact on Language Server Protocol (LSP) features in TSZ. Our investigation reveals that while significant progress has been made in type system completeness, several critical gaps remain that severely degrade the editor experience.

### Key Findings

1. **Control Flow Narrowing** is the most critical gap affecting LSP - without it, hover shows wrong types and completions suggest invalid properties
2. **Definite Assignment Analysis** is currently stubbed, causing false positives in diagnostics
3. **TDZ Checking** is incomplete, leading to runtime errors that the LSP cannot prevent
4. **Intersection Type Reduction** is actually well-implemented, contrary to what the gaps summary suggests
5. **Module Resolution** gaps impact cross-file completions and navigation

---

## 1. Type Checker Gaps by LSP Feature

### 1.1 Hover Feature Impact

**Current Implementation**: `/Users/mohsenazimi/code/tsz/src/lsp/hover.rs` (Line 164)
```rust
let type_id = checker.get_type_of_symbol(symbol_id);
let type_string = checker.format_type(type_id);
```

**Critical Gap**: Type Narrowing Not Applied to Hover

The hover provider retrieves the **declared type** of a symbol, not the **narrowed type** at the cursor position. This means:

#### Example Impact
```typescript
function process(value: string | null) {
    if (value !== null) {
        // User hovers over 'value' here
        // Expected: "string"
        // Actual:   "string | null"
        console.log(value.length);
    }
}
```

**Why This Happens**:
1. `get_type_of_symbol()` returns the symbol's declared type
2. No API exists to query the narrowed type at a specific location
3. Flow graph analysis exists in the binder but isn't exposed to the checker

**Solution Required**:
```rust
// Proposed API in CheckerState
pub fn get_type_at_location(&self, node_idx: NodeIndex) -> Option<TypeId> {
    // 1. Find the FlowNode for this AST node
    // 2. Traverse flow graph backwards to find narrowing
    // 3. Apply type guards to compute narrowed type
    // 4. Return narrowed type or fall back to declared type
}
```

#### Affected Code Paths

- **File**: `src/lsp/hover.rs`
- **Method**: `get_hover_internal()` (Line 103)
- **Impact**: Lines 164-165 - type resolution without flow narrowing

**Estimated Fix Effort**: 3-5 days
- Design API for location-based type queries: 1 day
- Implement flow graph traversal: 2 days
- Integrate with hover provider: 0.5 days
- Testing and edge cases: 1.5 days

---

### 1.2 Completions Feature Impact

**Current Implementation**: `/Users/mohsenazimi/code/tsz/src/lsp/completions.rs` (Line 478)
```rust
let type_id = checker.get_type_of_node(expr_idx);
```

#### Gap #1: Un-narrowed Types in Member Completions

When typing `obj.`, completions show properties for all union members instead of just the narrowed type:

```typescript
function foo(obj: { a: string } | { b: number }) {
    if ('a' in obj) {
        obj.|  // Should only show 'a', but shows both 'a' and 'b'
    }
}
```

**Root Cause**: Same as hover - `get_type_of_node()` returns declared type, not narrowed type.

#### Gap #2: TDZ Violations in Completions

**Location**: `src/checker/flow_analysis.rs` (Lines 1691-1719)

The binder creates TDZ flow nodes, but the checker doesn't validate them:

```rust
// TODO: Implement TDZ checking for static blocks
pub(crate) fn is_in_tdz_static_block(&self, _sym_id: SymbolId, _usage_idx: NodeIndex) -> bool {
    false  // Always returns false - TDZ not enforced
}
```

**Impact on Completions**:
```typescript
function test() {
    x.|  // Suggests 'x' even though it's in TDZ
    let x = 42;
}
```

The ScopeWalker (`src/lsp/resolver.rs`) walks up the scope chain but doesn't filter based on TDZ status.

#### Gap #3: Intersection Type Property Collection

**Location**: `src/lsp/completions.rs` (Lines 546-549)

```rust
TypeKey::Union(members) | TypeKey::Intersection(members) => {
    let members = interner.type_list(members);
    for &member in members.iter() {
        self.collect_properties_for_type(member, interner, checker, visited, props);
    }
}
```

**Good News**: Intersection types are handled correctly! The LSP layer recursively collects properties from all intersection members.

**Caveat**: If the checker fails to detect an impossible intersection (e.g., `string & number`), completions might show properties for `never` types. However, our investigation found that intersection reduction is **well-implemented** in the solver:

```rust
// src/solver/intern.rs:1267
fn property_types_disjoint(&self, left: TypeId, right: TypeId) -> bool {
    // Detects {a: string} & {a: number} -> never
    // Correctly reduces to never for disjoint property types
}
```

**Estimated Fix Effort**:
- Narrowed types in completions: 3 days (leverages hover fix)
- TDZ-aware completion filtering: 2-3 days
- Intersection improvements: Already complete ✅

---

### 1.3 Signature Help Impact

**Current Implementation**: `/Users/mohsenazimi/code/tsz/src/lsp/signature_help.rs` (Line 236)

**Good News**: Signature help is **largely unaffected** by type checker gaps because:

1. Function signatures are resolved at declaration time
2. Overload resolution is based on call signatures, not flow analysis
3. The `Callable` shape correctly stores multiple call/construct signatures

**Minor Issue**: Generic function signatures may show un-resolved type parameters:

```typescript
function identity<T>(x: T): T {
    return x;
}

identity(|  // Should show <T>(x: T): T, might show <unknown>(x: unknown): unknown
```

This is a type inference issue, not a flow analysis gap.

**Estimated Impact**: Low - Signature help works well for most cases

---

## 2. Type Checker Gap Analysis

### 2.1 Definite Assignment Analysis (CRITICAL)

**Location**: `src/checker/flow_analysis.rs` (Line 1670)

```rust
pub(crate) fn is_definitely_assigned_at(&self, _idx: NodeIndex) -> bool {
    // TODO: Implement proper flow-sensitive definite assignment analysis
    // For now, return true to avoid excessive TS2454 errors in conformance tests.
    true  // ALWAYS RETURNS TRUE - NOT IMPLEMENTED
}
```

**Conformance Impact**:
- **Error Code**: TS2454 (Variable used before assignment)
- **False Negative**: Misses real errors where variables are used before assignment
- **Current Strategy**: Return `true` to avoid false positives in tests

**Why This Matters for LSP**:
1. Code actions can't suggest "Initialize variable" fixes
2. Diagnostics don't catch common runtime errors
3. Refactoring operations may introduce bugs

**Implementation Requirements**:
```rust
pub(crate) fn is_definitely_assigned_at(&self, idx: NodeIndex) -> bool {
    // 1. Get the symbol for this node
    // 2. Find all assignment paths in the flow graph
    // 3. Check if all paths to this node assign the symbol
    // 4. Return true only if definitely assigned on all paths
}
```

**Data Flow Analysis Needed**:
1. **Forward Analysis**: Track assignments on all control flow paths
2. **Merge Points**: At join points, intersect "assigned" sets from predecessors
3. **Loop Handling**: Handle loop back-edges with fixpoint iteration
4. **Conditional Checks**: Account for `typeof` and discriminant checks

**Estimated Fix Effort**: 5-7 days
- Design flow-sensitive assignment tracking: 1 day
- Implement forward flow analysis: 2-3 days
- Handle control flow merges: 1 day
- Loop analysis and fixpoint iteration: 1-2 days
- Testing and edge cases: 1 day

---

### 2.2 TDZ (Temporal Dead Zone) Checking (HIGH)

**Location**: `src/checker/flow_analysis.rs` (Lines 1691-1719)

**Three Missing Implementations**:

1. **Static Block TDZ** (Line 1691)
   ```rust
   // TODO: Implement TDZ checking for static blocks
   pub(crate) fn is_in_tdz_static_block(&self, ...) -> bool { false }
   ```

2. **Computed Property TDZ** (Line 1704)
   ```rust
   // TODO: Implement TDZ checking for computed properties
   pub(crate) fn is_in_tdz_computed_property(&self, ...) -> bool { false }
   ```

3. **Heritage Clause TDZ** (Line 1717)
   ```rust
   // TODO: Implement TDZ checking for heritage clauses
   pub(crate) fn is_in_tdz_heritage_clause(&self, ...) -> bool { false }
   ```

**Why TDZ Matters for LSP**:

```typescript
class C {
    static {
        console.log(x);  // Should error: TDZ violation
        let x = 42;
    }
}
```

Without TDZ checking:
- Completions suggest `x` before it's valid
- Hover shows type as if it's accessible
- No squiggly underline for the error

**Implementation Approach**:
```rust
pub(crate) fn is_in_tdz_static_block(&self, sym_id: SymbolId, usage_idx: NodeIndex) -> bool {
    // 1. Find the declaration node of sym_id
    // 2. Find the static block containing usage_idx
    // 3. Check if usage_idx comes before the declaration in source order
    // 4. Return true if in TDZ (before declaration)
}
```

**Estimated Fix Effort**: 2-3 days per implementation (6-9 days total)
- Static block TDZ: 2-3 days
- Computed property TDZ: 2-3 days
- Heritage clause TDZ: 2-3 days

---

### 2.3 Control Flow Type Narrowing (CRITICAL)

**Current State**: Partially Implemented

**What Works**:
- Flow graph construction in binder (✅ Complete)
- typeof narrowing (✅ Implemented)
- Discriminant property narrowing (✅ Implemented)
- Nullish narrowing (✅ Implemented)

**What's Missing**:
- **Location-based type queries** for LSP features
- **Assignment narrowing** (e.g., `x = 5` narrows `x: number | string` to `number`)
- **Closure invalidation** (Gap #42 - CFA invalidation in closures)

**Evidence of Partial Implementation**:

From `src/checker/flow_narrowing.rs`:
```rust
pub fn has_discriminant_properties(&self, type_id: TypeId) -> bool {
    // ✅ Checks for discriminant properties in union types
    // ✅ Used by narrowing logic
}

pub fn is_nullish_type(&self, type_id: TypeId) -> bool {
    // ✅ Checks for null/undefined in unions
}

pub fn non_null_type(&self, type_id: TypeId) -> TypeId {
    // ✅ Removes null from type
}
```

**The Missing Piece**: Connecting narrowing to LSP queries

The narrowing infrastructure exists, but there's no API to query "what is the type of this symbol at this specific location?"

**Estimated Fix Effort**: 3-5 days (see Hover section above)

---

### 2.4 Module Resolution Gaps (MODERATE)

**Location**: `docs/walkthrough/07-gaps-summary.md` (Line 97)

**Impact**: Cross-file completions and navigation fail

**Conformance Errors**:
- **TS2694** (3,104x): Namespace no exported member
- **TS2307** (2,139x): Cannot find module
- **TS2318** (3,386x): Cannot find global type

**Why This Affects LSP**:
```typescript
// file1.ts
export function foo(): string { ... }

// file2.ts
import { foo } from './file1';
foo.|  // Completions fail if module resolution fails
```

**Root Cause**: Import resolution depends on external module resolver pre-populating `module_exports`.

**Estimated Fix Effort**: 4-6 days
- Implement file system-based module resolution: 3-4 days
- Handle re-exports and namespace merging: 1-2 days
- Cache invalidation for incremental builds: 1 day

---

### 2.5 Solver Gaps Affecting LSP

**Intersection Type Reduction** (Rule #21) - ✅ **ALREADY COMPLETE**

Despite being listed as a gap in the summary, our investigation found this is **well-implemented**:

```rust
// src/solver/intern.rs:1267
fn property_types_disjoint(&self, left: TypeId, right: TypeId) -> bool {
    // Detects incompatible property types
    // {a: string} & {a: number} -> never ✅
}
```

Test coverage confirms this works:
- `test_intersection_reduction_disjoint_primitives` ✅
- `test_intersection_reduction_disjoint_object_literals` ✅
- `test_intersection_reduction_disjoint_discriminant` ✅

**Recommendation**: Remove from gaps list - this is complete.

**Rest Parameter Bivariance** (Rule #16) - 🟡 **PARTIAL**

**Impact**: Signature help may show incorrect parameter types for functions with rest parameters.

**Example**:
```typescript
function foo(...args: string[]) { }
function bar(...args: number[]) { }

function baz(...args: (string | number)[]) {
    // Should rest parameters be bivariant?
}
```

**Estimated Fix Effort**: 2-3 days

---

## 3. Priority Ranking for LSP Impact

### Tier 1: Critical for Editor Experience (Fix First)

| Gap | LSP Impact | User-Facing Effect | Fix Effort |
|-----|-----------|-------------------|------------|
| **Control Flow Narrowing API** | 🔴 CRITICAL | Hover shows wrong types; completions suggest invalid members | 3-5 days |
| **Definite Assignment Analysis** | 🔴 CRITICAL | Runtime errors not caught; code actions missing | 5-7 days |

**Rationale**:
- Narrowing is fundamental to TypeScript's value proposition
- Without it, users can't trust the editor
- Affects the most common features (hover, completions)

---

### Tier 2: High Impact (Fix Second)

| Gap | LSP Impact | User-Facing Effect | Fix Effort |
|-----|-----------|-------------------|------------|
| **TDZ Checking** | 🟠 HIGH | Runtime errors; completions suggest invalid identifiers | 6-9 days |
| **Module Resolution** | 🟠 HIGH | Cross-file completions fail; go-to-definition breaks | 4-6 days |

**Rationale**:
- TDZ violations cause runtime ReferenceErrors
- Module resolution is essential for real-world projects
- Both affect feature correctness

---

### Tier 3: Moderate Impact (Fix Third)

| Gap | LSP Impact | User-Facing Effect | Fix Effort |
|-----|-----------|-------------------|------------|
| **Rest Parameter Bivariance** | 🟡 MODERATE | Signature help may show incorrect types | 2-3 days |
| **Base Constraint Checking** | 🟡 MODERATE | Generic constraints not validated | 2-3 days |

**Rationale**:
- Edge cases that don't affect most code
- Users can work around the limitations
- Nice to have for completeness

---

### Tier 4: Low Impact (Fix Last)

| Gap | LSP Impact | User-Facing Effect | Fix Effort |
|-----|-----------|-------------------|------------|
| **CFA Invalidation in Closures** | 🟢 LOW | Stale narrowing in async callbacks | 3-5 days |
| **Template Literal Optimization** | 🟢 LOW | Performance issue with large unions | 2-3 days |

**Rationale**:
- Performance/correctness edge cases
- Don't affect typical usage patterns
- Can be deferred without impacting UX

---

## 4. Implementation Dependencies

### Dependency Graph

```
┌─────────────────────────────────────────┐
│   Control Flow Narrowing API            │
│   (get_type_at_location)                │
└────────────┬────────────────────────────┘
             │
             ├──► Enables: Narrowed Hover
             ├──► Enables: Narrowed Completions
             └──► Enables: Accurate Signature Help
                    (for generic inference)
             │
┌────────────┴────────────────────────────┐
│   Definite Assignment Analysis          │
└────────────┬────────────────────────────┘
             │
             ├──► Enables: TS2454 Diagnostics
             ├──► Enables: "Initialize Variable" Code Actions
             └──► Enables: Safe Refactoring
             │
┌────────────┴────────────────────────────┐
│   TDZ Checking                           │
└────────────┬────────────────────────────┘
             │
             ├──► Enables: TDZ Diagnostics
             └──► Enables: TDZ-Aware Completions
                    (filter out variables in TDZ)
             │
┌────────────┴────────────────────────────┐
│   Module Resolution                      │
└──────────────────────────────────────────┘
             │
             ├──► Enables: Cross-File Completions
             ├──► Enables: Go-to-Definition (cross-file)
             └──► Enables: Import Auto-Fix
```

### Implementation Phases

**Phase 1: Flow-Based Type Queries** (3-5 days)
```
1. Add CheckerState::get_type_at_location()
   ├─ Design API (1 day)
   ├─ Implement flow graph traversal (2 days)
   └─ Integration testing (1 day)

2. Update Hover Provider (0.5 day)
3. Update Completions Provider (0.5 day)
4. Add LSP integration tests (1 day)
```

**Phase 2: Definite Assignment** (5-7 days)
```
1. Implement forward flow analysis (2-3 days)
2. Add control flow merge logic (1 day)
3. Handle loops with fixpoint iteration (1-2 days)
4. Integrate with diagnostics (0.5 day)
5. Add code action providers (1 day)
```

**Phase 3: TDZ Checking** (6-9 days)
```
1. Static block TDZ (2-3 days)
2. Computed property TDZ (2-3 days)
3. Heritage clause TDZ (2-3 days)
4. LSP integration (1 day)
```

**Phase 4: Module Resolution** (4-6 days)
```
1. Implement file system resolver (2-3 days)
2. Handle re-exports and namespaces (1-2 days)
3. Cache invalidation (1 day)
4. LSP navigation integration (0.5 day)
```

**Total Effort**: 18-27 days (3.5-5.5 weeks)

---

## 5. Code Change Estimates

### File-by-File Impact

#### Core Checker Files

**`src/checker/flow_analysis.rs`** (+400 lines estimated)
- `is_definitely_assigned_at()`: 80 lines
- `is_in_tdz_static_block()`: 60 lines
- `is_in_tdz_computed_property()`: 80 lines
- `is_in_tdz_heritage_clause()`: 80 lines
- Flow traversal utilities: 100 lines

**`src/checker/state.rs`** (+150 lines estimated)
- `get_type_at_location()`: 80 lines
- `get_narrowed_type_at_node()`: 40 lines
- Helper methods: 30 lines

**`src/binder/state.rs`** (+80 lines estimated)
- Enhance flow node tracking for LSP queries
- Add position-to-flow-node mapping

#### LSP Files

**`src/lsp/hover.rs`** (+20 lines)
- Use `get_type_at_location()` instead of `get_type_of_symbol()`

**`src/lsp/completions.rs`** (+40 lines)
- Use narrowed types for member completions
- Filter out TDZ variables from suggestions

**`src/lsp/code_actions.rs`** (+150 lines)
- Add "Initialize variable" quick fix
- Add "Add missing property" quick fix

**`src/lsp/diagnostics.rs`** (+30 lines)
- Publish definite assignment errors
- Publish TDZ violation errors

#### Module Resolution

**`src/binder/state.rs`** (+200 lines)
- Implement file system module resolution
- Handle re-export chains

**`src/module_resolver.rs`** (+300 lines) - New file
- File system path resolution
- Node.js module resolution algorithms
- TypeScript path mapping support

### Total Lines of Code

| Component | Lines Added | Lines Modified | Total |
|-----------|-------------|----------------|-------|
| Flow Analysis | 400 | 50 | 450 |
| Checker APIs | 150 | 20 | 170 |
| LSP Integration | 240 | 40 | 280 |
| Module Resolution | 500 | 30 | 530 |
| **TOTAL** | **1,290** | **140** | **1,430** |

### Test Coverage Needed

| Area | Test Files | Estimated Test Lines |
|------|-----------|---------------------|
| Narrowing API | 3 files | 300 lines |
| Definite Assignment | 5 files | 500 lines |
| TDZ Checking | 4 files | 400 lines |
| LSP Integration | 4 files | 400 lines |
| Module Resolution | 3 files | 300 lines |
| **TOTAL** | **19 files** | **1,900 lines** |

---

## 6. Recommendations

### Immediate Actions (Week 1-2)

1. **Implement `get_type_at_location()` API**
   - Start with the highest-impact, lowest-effort item
   - Enables both hover and completion improvements
   - Leverages existing narrowing infrastructure

2. **Update Conformance Baseline**
   - Run `./conformance/run.sh --server` to get current pass rate
   - Document specific test failures related to narrowing
   - Track improvement as fixes are implemented

### Short-Term Goals (Week 3-4)

3. **Implement Definite Assignment Analysis**
   - Critical for catching runtime errors
   - Enables code action quick fixes
   - Improves user trust in the LSP

4. **Add LSP Integration Tests**
   - Test hover with narrowed types
   - Test completions in narrowed contexts
   - Test TDZ filtering in completions

### Medium-Term Goals (Week 5-8)

5. **Complete TDZ Checking**
   - All three implementations
   - LSP integration
   - Conformance improvements

6. **Module Resolution**
   - File system-based resolver
   - Cross-file navigation
   - Import/export completion

### Long-Term Considerations

7. **Performance Monitoring**
   - Benchmark LSP response times with flow analysis
   - Add caching for expensive flow traversals
   - Consider incremental flow analysis

8. **Documentation**
   - Update `docs/walkthrough/07-gaps-summary.md`
   - Remove completed items (intersection reduction)
   - Add LSP-specific gap documentation

---

## 7. Risk Assessment

### Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|-----------|
| Flow analysis performance degradation | Medium | High | Add caching, limit traversal depth |
| Circular type dependencies | Low | Medium | Existing cycle detection should handle |
| Breaking conformance tests | Medium | Medium | Update baselines, track improvements |
| Module resolution edge cases | High | Low | Follow Node.js resolution spec closely |

### Resource Risks

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|-----------|
| Implementation effort underestimated | Medium | Medium | Start with highest-value items |
| Test coverage insufficient | Medium | High | Prioritize test writing alongside features |
| LSP integration complexity | Low | Medium | LSP layer is well-architected, should be straightforward |

---

## 8. Success Metrics

### Quantitative Metrics

- **Conformance Pass Rate**: Target +5% improvement from baseline
- **LSP Response Time**: Keep hover/completions under 100ms
- **Test Coverage**: >90% for new flow analysis code
- **False Positive Rate**: <5% for definite assignment diagnostics

### Qualitative Metrics

- **User Trust**: Hover types match user expectations in narrowed contexts
- **Editor Experience**: Completions are contextually appropriate
- **Error Prevention**: TDZ and definite assignment errors caught before runtime

---

## 9. Appendix

### A. Related Files

**Gap Documentation**:
- `/Users/mohsenazimi/code/tsz/docs/walkthrough/07-gaps-summary.md`

**LSP Implementation**:
- `/Users/mohsenazimi/code/tsz/src/lsp/hover.rs`
- `/Users/mohsenazimi/code/tsz/src/lsp/completions.rs`
- `/Users/mohsenazimi/code/tsz/src/lsp/signature_help.rs`
- `/Users/mohsenazimi/code/tsz/src/lsp/resolver.rs`

**Checker Implementation**:
- `/Users/mohsenazimi/code/tsz/src/checker/flow_analysis.rs`
- `/Users/mohsenazimi/code/tsz/src/checker/flow_narrowing.rs`
- `/Users/mohsenazimi/code/tsz/src/checker/state.rs`

**Solver Implementation**:
- `/Users/mohsenazimi/code/tsz/src/solver/intern.rs`
- `/Users/mohsenazimi/code/tsz/src/solver/subtype.rs`

### B. Test Commands

```bash
# Run conformance tests
cd /Users/mohsenazimi/code/tsz
./conformance/run.sh --server

# Run LSP tests
cargo test --package tsz --lib lsp::tests

# Run flow analysis tests
cargo test --package tsz --lib checker::flow_analysis
```

### C. Gemini Analysis

This report incorporated analysis from Gemini AI (via `ask-gemini.mjs`) which identified:
- Intersection type handling in completions
- Control flow narrowing gaps in hover
- Signature help robustness
- Priority ranking from LSP perspective

---

## Conclusion

The type checker gaps affecting LSP features are significant but addressable. The highest-impact items (flow narrowing and definite assignment) can be implemented in 2-3 weeks with focused effort. The existing infrastructure (flow graph construction, narrowing utilities) provides a solid foundation to build upon.

**Key Takeaway**: The type system completeness is high, but the **LSP integration layer** needs enhanced APIs to expose type information at specific locations in the code. Once `get_type_at_location()` is implemented, both hover and completions will see dramatic improvements.

---

**Report Prepared By**: Research Team 5
**Report Date**: 2026-01-30
**Next Review**: After Phase 1 completion (expected 2026-02-13)

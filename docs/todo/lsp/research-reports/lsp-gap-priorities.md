# LSP Gap Fix Priorities - Quick Reference

## Tier 1: CRITICAL (Fix First - Weeks 1-2)

### 1. Control Flow Narrowing API (3-5 days)
**Files**: `src/checker/state.rs`, `src/lsp/hover.rs`, `src/lsp/completions.rs`

**Problem**: Hover shows declared type, not narrowed type
```typescript
function foo(x: string | null) {
    if (x !== null) {
        // Hover shows: string | null
        // Should show: string
    }
}
```

**Solution**: Add `CheckerState::get_type_at_location(node_idx) -> Option<TypeId>`
- Traverse flow graph to find narrowed type at cursor position
- Apply type guards (typeof, discriminant, nullish checks)
- Return narrowed type or fall back to declared type

**Impact**:
- ✅ Accurate hover types in narrowed contexts
- ✅ Contextually appropriate completions
- ✅ Foundation for other LSP features

**Effort**: 3-5 days
- Design API: 1 day
- Implement flow traversal: 2 days
- LSP integration: 1 day
- Testing: 1 day

---

### 2. Definite Assignment Analysis (5-7 days)
**Files**: `src/checker/flow_analysis.rs`, `src/lsp/code_actions.rs`

**Problem**: `is_definitely_assigned_at()` always returns `true`
```typescript
let x: string;
console.log(x);  // Should error: TS2454, but doesn't
```

**Solution**: Implement flow-sensitive definite assignment
- Track assignments on all control flow paths
- At merge points, intersect "assigned" sets
- Use fixpoint iteration for loops

**Impact**:
- ✅ Catch "use before assignment" errors
- ✅ Enable "Initialize variable" code actions
- ✅ Prevent runtime errors

**Effort**: 5-7 days
- Design tracking: 1 day
- Forward flow analysis: 2-3 days
- Merge logic: 1 day
- Loop handling: 1-2 days

---

## Tier 2: HIGH (Fix Second - Weeks 3-4)

### 3. TDZ Checking (6-9 days)
**Files**: `src/checker/flow_analysis.rs`, `src/lsp/completions.rs`

**Problem**: Three TDZ implementations return `false`
```typescript
// Static block TDZ
class C {
    static {
        console.log(x);  // Should error: TDZ violation
        let x = 42;
    }
}
```

**Solution**: Implement TDZ detection
- `is_in_tdz_static_block()`: Check if usage precedes declaration in static block
- `is_in_tdz_computed_property()`: Check TDZ in computed property keys
- `is_in_tdz_heritage_clause()`: Check TDZ in class extends clauses

**Impact**:
- ✅ Prevent ReferenceError from TDZ violations
- ✅ Filter TDZ variables from completions
- ✅ Improve diagnostic accuracy

**Effort**: 2-3 days per implementation (6-9 days total)

---

### 4. Module Resolution (4-6 days)
**Files**: `src/binder/state.rs`, `src/module_resolver.rs` (new)

**Problem**: External module resolution not implemented
```typescript
// file1.ts
export function foo() { }

// file2.ts
import { foo } from './file1';  // Resolution fails
foo.|  // Completions don't work
```

**Solution**: Implement file system-based module resolution
- Follow Node.js resolution algorithm
- Handle re-export chains
- Support TypeScript path mapping

**Impact**:
- ✅ Cross-file completions
- ✅ Go-to-definition across files
- ✅ Auto-import suggestions

**Effort**: 4-6 days
- File system resolver: 2-3 days
- Re-export handling: 1-2 days
- Cache invalidation: 1 day

---

## Tier 3: MODERATE (Fix Third - Week 5)

### 5. Rest Parameter Bivariance (2-3 days)
**Files**: `src/solver/subtype_rules/functions.rs`

**Problem**: Rest parameter bivariance incomplete
**Impact**: Signature help may show incorrect rest parameter types

### 6. Base Constraint Assignability (2-3 days)
**Files**: `src/solver/subtype_rules/generics.rs`

**Problem**: Generic constraint checking partial
**Impact**: Some generic constraint violations missed

---

## Implementation Order

### Phase 1: Flow-Based Type Queries (Days 1-5)
```
Day 1:   Design get_type_at_location() API
Day 2-3: Implement flow graph traversal
Day 4:   Update hover provider
Day 5:   Update completions provider
```

### Phase 2: Definite Assignment (Days 6-12)
```
Day 6-7:   Design flow-sensitive tracking
Day 8-10:  Implement forward analysis
Day 11:    Handle control flow merges
Day 12:    Loop analysis and testing
```

### Phase 3: TDZ Checking (Days 13-21)
```
Day 13-15: Static block TDZ
Day 16-18: Computed property TDZ
Day 19-21: Heritage clause TDZ
```

### Phase 4: Module Resolution (Days 22-27)
```
Day 22-24: File system resolver
Day 25-26: Re-export handling
Day 27:    Cache invalidation
```

**Total Estimated Effort**: 27 days (5.5 weeks)

---

## Quick Wins

### Already Complete ✅
- **Intersection Type Reduction**: Well-implemented in solver
- **Type Narrowing Infrastructure**: Flow graph, discriminant checks exist
- **Signature Help**: Robust for most cases

### Leverage Existing Work
- `src/checker/flow_narrowing.rs` has utilities for discriminant/narrowish checks
- Binder already creates flow nodes for control flow
- ScopeWalker exists for scope-based queries

---

## Success Metrics

### Before Implementation
```bash
./conformance/run.sh --server
# Baseline: XX% pass rate
```

### After Phase 1 (Week 2)
- Hover types match narrowed context ✅
- Completions contextually appropriate ✅
- Conformance: +2-3% improvement

### After Phase 2 (Week 3)
- TS2454 errors detected ✅
- "Initialize variable" code actions work ✅
- Conformance: +1-2% improvement

### After Phase 3 (Week 5)
- TDZ violations caught ✅
- Completions filter TDZ variables ✅
- Conformance: +1% improvement

### After Phase 4 (Week 6)
- Cross-file completions work ✅
- Go-to-definition works across files ✅
- Conformance: +2-3% improvement

**Total Target**: +6-9% conformance improvement

---

## Code Changes Summary

| File | Lines Added | Purpose |
|------|-------------|---------|
| `src/checker/state.rs` | +150 | Add `get_type_at_location()` API |
| `src/checker/flow_analysis.rs` | +400 | Definite assignment + TDZ |
| `src/lsp/hover.rs` | +20 | Use narrowed types |
| `src/lsp/completions.rs` | +40 | Use narrowed types, filter TDZ |
| `src/lsp/code_actions.rs` | +150 | Add quick fixes |
| `src/module_resolver.rs` | +300 | New file for resolution |
| `src/binder/state.rs` | +280 | Module resolution integration |

**Total**: ~1,340 lines of new code

---

## Test Coverage Required

- **Narrowing API**: 300 lines (3 files)
- **Definite Assignment**: 500 lines (5 files)
- **TDZ Checking**: 400 lines (4 files)
- **LSP Integration**: 400 lines (4 files)
- **Module Resolution**: 300 lines (3 files)

**Total**: ~1,900 lines of tests

---

## Risk Mitigation

### Performance Concerns
- Add caching for flow graph traversals
- Limit traversal depth to prevent timeouts
- Benchmark LSP response times

### Correctness Concerns
- Comprehensive test coverage before merging
- Conformance testing for each phase
- Manual testing with real codebases

### Integration Concerns
- Start with isolated, low-risk changes
- Incremental rollout with feature flags
- Rollback plan for each phase

---

## Next Steps

1. **Today**: Review and approve this plan
2. **Day 1**: Create branch `lsp/narrowing-api`
3. **Day 1-5**: Implement Phase 1 (Flow-Based Type Queries)
4. **Day 6**: Review, test, merge Phase 1
5. **Day 7**: Begin Phase 2 (Definite Assignment)

---

**Questions? Refer to full report**: `type-checker-lsp-integration-gaps.md`

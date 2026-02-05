# Session tsz-2: Clippy Warning Cleanup (Phase 4.3 Migration)

**Started**: 2026-02-05
**Status**: Active
**Focus**: Complete Ref -> Lazy/DefId migration by fixing deprecation warnings

**Previous Session**: Coinductive Subtyping (COMPLETE)
**Next Session**: Conditional Types Refinement

## Problem Statement

The codebase has **~70 clippy warnings** that need to be cleaned up. The most critical are **46 deprecation warnings** related to the incomplete Phase 4.3 migration from `Ref`/`SymbolRef` to `Lazy`/`DefId`.

### Warning Categories

**1. Deprecated Functions (46 warnings) - CRITICAL**
- `get_ref_symbol()` → Should use `get_lazy_def_id()`
- `get_symbol_ref()` → Should use `get_lazy_def_id()`
- `get_ref_if_symbol()` → Should use `get_lazy_if_def()`
- Deprecated enum variants: `Ref`, `SymbolRef` → Should use `Lazy`

**2. Unused Code (~20 warnings)**
- Unused imports: `NodeAccess` (2 locations)
- Unused variables: `has_primitive`
- Unused methods: ~10 methods across checker/binder
- Unused fields: `loop_mutation_cache`, `symbol_aliases`, etc.

**3. Other Warnings (~4 warnings)**
- Irrefutable if let patterns
- Visibility issues (private types in public signatures)

## Implementation Strategy

### Phase A: Pre-Implementation Validation (MANDATORY)

**Question 1** (Before ANY code changes):
```bash
./scripts/ask-gemini.mjs --include=src/solver --include=src/checker "
I am cleaning up 46 deprecation warnings related to the Ref -> Lazy/DefId migration.
My plan is to replace get_ref_symbol and get_symbol_ref with get_lazy_def_id across the checker,
and update the corresponding enum variants in type_queries_extended.rs.

1) Is there any case where a DefId cannot be used where a SymbolRef was previously expected?
2) Are there specific files in src/checker/ where this replacement is particularly risky?
3) What is the correct way to handle the transition for NamespaceMemberKind::SymbolRef?

Please provide specific file paths, functions, and edge cases to watch out for."
```

**Expected Output**: Gemini should tell us:
- Which files are safe to update
- Which files need special handling
- Any edge cases where DefId behaves differently than SymbolRef

### Phase B: Deprecated Function Migration

**Step 1: Find all call sites**
```bash
grep -rn "get_ref_symbol\|get_symbol_ref\|get_ref_if_symbol" src/solver/ src/checker/ src/binder/
```

**Step 2: Update call sites systematically**
- Replace `get_ref_symbol()` with `get_lazy_def_id()`
- Replace `get_symbol_ref()` with `get_lazy_def_id()`
- Replace `get_ref_if_symbol()` with `get_lazy_if_def()`
- Update return type handling (SymbolRef → DefId)

**Files to modify** (preliminary list from clippy output):
- `src/solver/type_queries.rs` - Remove deprecated functions
- `src/solver/type_queries_extended.rs` - Update enum variants
- `src/solver/subtype.rs` - Update `resolve_ref()` calls
- `src/checker/*.rs` - Update type query calls
- `src/binder/*.rs` - Update symbol resolution calls

**Step 3: Remove deprecated code**
- Remove `get_ref_symbol()` function
- Remove `get_symbol_ref()` function
- Remove `get_ref_if_symbol()` function
- Remove `RefTypeKind` type alias
- Remove deprecated enum variants

### Phase C: Unused Code Cleanup

**Step 1: Fix unused imports**
- Remove `NodeAccess` imports (2 locations)
- Run `cargo fix --allow-dirty` to auto-fix simple unused imports

**Step 2: Fix unused variables**
- Remove or prefix with underscore: `has_primitive` → `_has_primitive`

**Step 3: Fix unused methods/fields**
- Evaluate if they're genuinely dead code
- If yes, remove them
- If no, annotate with `#[allow(dead_code)]` with justification

### Phase D: Post-Implementation Review (MANDATORY)

**Question 2** (After all changes):
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver --include=src/checker "
I completed the Ref -> Lazy/DefId migration to fix 46 clippy warnings.

Changes made:
[PASTE DIFF OR DESCRIBE CHANGES]

Please review:
1) Is this migration complete and correct?
2) Did I miss any edge cases where DefId behaves differently than SymbolRef?
3) Are there any remaining architectural issues with the type resolution?

Be specific if anything is wrong - tell me exactly what to fix."
```

**Expected Output**: Gemini Pro should validate:
- All deprecated functions are removed
- New DefId-based calls are correct
- No edge cases were missed
- Type resolution still works correctly

## Success Criteria

- [ ] All 46 deprecation warnings resolved
- [ ] `get_ref_symbol`, `get_symbol_ref`, `get_ref_if_symbol` removed
- [ ] All deprecated enum variants removed
- [ ] Unused imports removed
- [ ] Unused variables fixed
- [ ] `cargo clippy` shows 0 warnings
- [ ] All tests still pass
- [ ] Gemini Pro review validates the migration

## Estimated Complexity

**Overall**: MEDIUM (6-8 hours)
- Phase A (Validation): 30 minutes
- Phase B (Migration): 4-5 hours
- Phase C (Cleanup): 1-2 hours
- Phase D (Review): 30 minutes

## Session History

*Created 2026-02-05 after completing Coinductive Subtyping.*
*This session completes the Phase 4.3 Ref -> Lazy migration that was started earlier.*
*After this session, the next focus will be Conditional Types Refinement (per Gemini Flash recommendation).*

## Notes

**Why This Priority:**
1. **Technical Debt**: 46 deprecation warnings block clean builds
2. **Architecture**: Completes Phase 4.3 migration (Ref → Lazy)
3. **Safety**: Must follow Two-Question Rule per AGENTS.md
4. **Validation**: Requires Gemini Pro review for solver/checker changes

**Key Risk Areas:**
- Symbol resolution in binder (may have assumptions about SymbolRef)
- Type identity checks (DefId vs SymbolRef semantics)
- Nominal type comparisons

**Alternative Tasks** (if this takes too long):
- Conditional Types Refinement
- Property Access Visitor (tsz-6-3)
- Literal Type Narrowing (CFA)

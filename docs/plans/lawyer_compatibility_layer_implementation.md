# Lawyer Compatibility Layer Implementation Plan

**Status:** Planning - ✅ **Gemini Approved** (with architectural corrections)
**Created:** 2025-02-03
**Author:** Claude Sonnet 4.5 with extensive Gemini guidance
**Priority:** High - Addresses TS2322 extra=594 errors and Hook feedback requirements

---

## Gemini Review Status

**✅ APPROVED** (with critical architectural corrections)

### Key Changes from Initial Plan (per Gemini Review):

1. **CRITICAL:** Do NOT change `is_subtype_of` semantics
   - Add `is_assignable_to` as a NEW separate method
   - `is_subtype_of` = Strict structural (Judge) - for internal solver
   - `is_assignable_to` = Loose with TS rules (Lawyer) - for Checker

2. **CRITICAL:** Add separate caches
   - `subtype_cache` - for strict `is_subtype_of` results
   - `assignability_cache` - for loose `is_assignable_to` results
   - Prevents cache poisoning (e.g., `any` assignability contaminating strict checks)

3. **Adjusted Time Estimates:**
   - Phase 2 (conditional.rs): 3-4h → 4-6h (any distribution is complex)
   - Phase 3 (QueryDatabase): 1-2h → 2-3h (separate method + cache logic)
   - Total: ~1.5 days → ~2 days

4. **Additional Warnings:**
   - Circular dependency risk: Ensure `SubtypeChecker` calls `is_subtype_of`, NOT `is_assignable_to`
   - Inference warning: `infer.rs` needs strict bounds, don't use `CompatChecker` loosely

5. **Testing Addition:**
   - Added cache poisoning prevention test
   - Verifies separate caches don't interfere

---

## Executive Summary

This plan implements the **"Lawyer" compatibility layer** - a middleware that applies TypeScript's intentional unsoundness rules before delegating to the strict "Judge" (SubtypeChecker). The goal is to fix the **594 extra TS2322 errors** we're emitting by ensuring we use `CompatChecker` instead of raw `SubtypeChecker` throughout the codebase.

**Key Insight:** The architecture already exists (`src/solver/compat.rs`), but several code paths bypass it and create `SubtypeChecker` directly with strict defaults, causing false positives.

---

## Current State Analysis

### ✅ What's Already Working

From Gemini's analysis of the codebase:

1. **`CompatChecker` exists and is well-structured** (`src/solver/compat.rs`)
   - Wraps `SubtypeChecker` (The Judge)
   - Implements key unsoundness rules:
     - Rule #1: Any Type propagation
     - Rule #9: Legacy Null/Undefined
     - Rule #13: Weak Types
     - Rule #20: Object Trifecta (empty objects)

2. **Most assignability logic is in the solver**
   - `src/checker/assignability_checker.rs` does **not exist** (already migrated)
   - `src/checker/state.rs` delegates to `CompatChecker`

3. **Override bridge pattern works**
   - `AssignabilityOverrideProvider` trait for symbol-dependent rules
   - `CheckerState` implements enum/constructor overrides

### ❌ The Problem: QueryDatabase Bypass

**Critical Issue Found by Gemini:**

```rust
// src/solver/db.rs - CURRENT (BROKEN)
fn is_subtype_of(&self, source: TypeId, target: TypeId) -> bool {
    // Creates SubtypeChecker with DEFAULT strict settings!
    // Ignores CompilerOptions completely.
    // Bypasses CompatChecker (The Lawyer).
    crate::solver::subtype::is_subtype_of(self.as_type_database(), source, target)
}
```

**Impact:** When code goes through `QueryDatabase::is_subtype_of`, all TypeScript compatibility rules are **bypassed**. The solver acts as a strict "Judge" and rejects valid TypeScript code.

### Other Direct SubtypeChecker Usages

Gemini identified these locations creating `SubtypeChecker` directly:

1. **`src/solver/object_literal.rs`** (line 170-173)
   - Contextual typing for object literals
   - Needs `CompatChecker` for **freshness** checks

2. **`src/solver/evaluate_rules/conditional.rs`**
   - `T extends U` conditional type evaluation
   - Needs `SubtypeChecker` with `Any` distribution logic

3. **`src/solver/infer.rs`** (detect_conflicts)
   - Calls `crate::solver::subtype::is_subtype_of` directly

---

## Implementation Strategy

### Recommended Approach: **Bottom-Up Fix (Option B)**

From Gemini's recommendation:

> "Fix the direct `SubtypeChecker` usages in specific modules first, and then expose/enable `CompatChecker` via `QueryDatabase`."

**Why this order:**
1. **Isolation:** Changing `QueryDatabase` globally carries high risk
2. **Correct Semantics:** Different contexts need different checkers:
   - Object literals → **Lawyer** (assignability + freshness)
   - Conditionals → **Judge** (structural + any distribution)
3. **Incremental:** Can test each change independently

---

## Implementation Plan

### Phase 1: Fix `object_literal.rs` (2-3 hours)

**Goal:** Ensure object literal contextual typing respects TypeScript's assignment rules.

**File:** `src/solver/object_literal.rs`

**Current Code (lines 170-173):**
```rust
// BROKEN: Uses SubtypeChecker directly
let mut checker = SubtypeChecker::new(self.db);
if checker.is_subtype_of(value_type, ctx_type) {
    // ...
}
```

**Fixed Code:**
```rust
// CORRECT: Use CompatChecker for assignability
let mut checker = CompatChecker::new(self.db);
if checker.is_assignable(value_type, ctx_type) {
    // ...
}
```

**Why This Fixes Bugs:**
- `SubtypeChecker` (Judge) doesn't handle freshness/excess properties
- Causes "Zombie Freshness" bug where excess properties are ignored
- `CompatChecker` (Lawyer) correctly applies freshness rules

**Tests to Run:**
```bash
./scripts/conformance.sh run --filter "objectLiteral"
./scripts/conformance.sh run --filter "freshness"
```

---

### Phase 2: Fix `evaluate_rules/conditional.rs` (4-6 hours)

**⚠️ Time Estimate Adjustment (from Gemini review):**
> Allow 4-6 hours instead of 3-4. Implementing `any` distribution correctly involves:
> 1. Checking if the type is `any`
> 2. Evaluating both "true" and "false" branches
> 3. Constructing a Union of the results
> 4. Handling recursive result cases

**Goal:** Ensure conditional types use strict structural checks but handle `any` correctly.

**File:** `src/solver/evaluate_rules/conditional.rs`

**Current Code:**
```rust
// Creates SubtypeChecker with resolver
let mut checker = SubtypeChecker::with_resolver(self.db, resolver);
if checker.is_subtype_of(source, target) {
    // ...
}
```

**Fixed Code:**
```rust
// CRITICAL: Do NOT use CompatChecker here!
// T extends U is a structural check, not assignment.
// But we MUST handle any distribution first.

// Handle any distribution BEFORE calling subtype checker
if let Some(TypeKey::Intrinsic(IntrinsicKind::Any)) = self.db.lookup(source) {
    // any distributes over conditional types
    // Return union of true and false branches
    return self.evaluate_conditional_distribution(...);
}

// Now use strict structural checking
let mut checker = SubtypeChecker::with_resolver(self.db, resolver);
if checker.is_subtype_of(source, target) {
    // ...
}
```

**Why Keep SubtypeChecker Here:**
- `T extends U` is structural, not assignment
- Example: `void extends undefined` should be `false` with `strictNullChecks`
- Assignment might allow it in legacy modes, but extends shouldn't

**Tests to Run:**
```bash
./scripts/conformance.sh run --filter "conditional"
./scripts/conformance.sh run --filter "any"
```

---

### Phase 3: Update `QueryDatabase` Interface (2-3 hours)

**⚠️ CRITICAL ARCHITECTURAL DECISION (from Gemini review):**

> **Do NOT change `is_subtype_of` semantics.** Add `is_assignable_to` as a separate method.
>
> **Reason:** Solver internals need strict structural subtyping. The Checker needs assignability.
> Conflating them breaks solver invariants and causes cache poisoning.

**File:** `src/solver/db.rs`

**Step 3.1: Add Config to QueryCache**

```rust
// src/solver/db.rs

use crate::solver::judge::JudgeConfig;
use crate::solver::compat::CompatChecker;

pub struct QueryCache<'a> {
    interner: &'a TypeInterner,
    config: JudgeConfig,  // ADD THIS
    eval_cache: RwLock<FxHashMap<TypeId, TypeId>>,
    subtype_cache: RwLock<FxHashMap<(TypeId, TypeId), bool>>,
    // CRITICAL: Separate cache for assignability to prevent cache poisoning
    assignability_cache: RwLock<FxHashMap<(TypeId, TypeId), bool>>,
}

impl<'a> QueryCache<'a> {
    pub fn new(interner: &'a TypeInterner) -> Self {
        QueryCache {
            interner,
            config: JudgeConfig::default(),
            eval_cache: RwLock::new(FxHashMap::default()),
            subtype_cache: RwLock::new(FxHashMap::default()),
            assignability_cache: RwLock::new(FxHashMap::default()),
        }
    }

    pub fn with_config(mut self, config: JudgeConfig) -> Self {
        self.config = config;
        self
    }
}
```

**Step 3.2: Add `is_assignable_to` method to trait**

```rust
// src/solver/db.rs

pub trait QueryDatabase {
    // ... existing methods ...

    /// Strict structural subtyping (The Judge) - DO NOT CHANGE
    fn is_subtype_of(&self, source: TypeId, target: TypeId) -> bool;

    /// TypeScript assignability with unsoundness rules (The Lawyer) - NEW METHOD
    fn is_assignable_to(&self, source: TypeId, target: TypeId) -> bool;
}
```

**Step 3.3: Implement both methods in QueryCache**

```rust
// src/solver/db.rs

impl QueryDatabase for QueryCache<'_> {
    fn is_subtype_of(&self, source: TypeId, target: TypeId) -> bool {
        // STRICT: Use SubtypeChecker (The Judge)
        // This is for internal solver use - structural checks only
        let key = (source, target);

        if let Some(result) = self.check_cache(&self.subtype_cache, key) {
            return result;
        }

        // Use strict SubtypeChecker - no compatibility rules
        let result = crate::solver::subtype::is_subtype_of(
            self.as_type_database(),
            source,
            target,
        );

        self.insert_cache(&self.subtype_cache, key, result);
        result
    }

    fn is_assignable_to(&self, source: TypeId, target: TypeId) -> bool {
        // LOOSE: Use CompatChecker (The Lawyer)
        // This is for Checker diagnostics - full TypeScript compatibility rules
        let key = (source, target);

        if let Some(result) = self.check_cache(&self.assignability_cache, key) {
            return result;
        }

        // Use CompatChecker with all compatibility rules
        let mut checker = CompatChecker::new(self.as_type_database());
        checker.apply_config(&self.config);

        let result = checker.is_assignable(source, target);

        self.insert_cache(&self.assignability_cache, key, result);
        result
    }
}
```

**Step 3.4: Add helper in compat.rs**

```rust
// src/solver/compat.rs

use crate::solver::judge::JudgeConfig;

impl<'a, R: TypeResolver> CompatChecker<'a, R> {
    /// Apply configuration from JudgeConfig
    pub fn apply_config(&mut self, config: &JudgeConfig) {
        self.strict_function_types = config.strict_function_types;
        self.strict_null_checks = config.strict_null_checks;
        self.exact_optional_property_types = config.exact_optional_property_types;
        self.no_unchecked_indexed_access = config.no_unchecked_indexed_access;
        // Clear cache as configuration changed
        self.cache.clear();
    }
}
```

**⚠️ Important: Prevent Circular Dependencies**

```rust
// In SubtypeChecker, ensure it calls is_subtype_of (NOT is_assignable_to)
// This prevents: SubtypeChecker → QueryDatabase → CompatChecker → SubtypeChecker

// src/solver/subtype.rs

// When SubtypeChecker needs recursive checks, use:
self.db.is_subtype_of(inner_source, inner_target)  // ✅ Correct

// NEVER use:
self.db.is_assignable_to(inner_source, inner_target)  // ❌ Would cause loop
```

---

### Phase 4: Wire CompilerOptions (1 hour)

**Goal:** Pass `CompilerOptions` through to `QueryDatabase`.

**File:** `src/checker/context.rs` or initialization code

```rust
// When creating QueryCache, convert CheckerOptions to JudgeConfig
use crate::solver::judge::JudgeConfig;

let judge_config = JudgeConfig {
    strict_function_types: options.strict_function_types,
    strict_null_checks: options.strict_null_checks,
    exact_optional_property_types: options.exact_optional_property_types,
    no_unchecked_indexed_access: options.no_unchecked_indexed_access,
};

let query_db = QueryCache::new(interner).with_config(judge_config);
```

---

### Phase 5: Fix `infer.rs` (1-2 hours)

**⚠️ Inference Warning (from Gemini review):**
> Inference often needs **strict bounds**. Be careful using `CompatChecker` here.
> Only use it if you're sure inference should allow unsound assignments.
> Most inference should use `is_subtype_of`, not `is_assignable_to`.

**File:** `src/solver/infer.rs`

**Current:**
```rust
// In detect_conflicts
crate::solver::subtype::is_subtype_of(self.interner, a, b)
```

**Fixed:**
```rust
// Use QueryDatabase for strict subtype checks
// Do NOT use is_assignable_to here - inference needs strictness
if let Some(query_db) = &self.query_db {
    query_db.is_subtype_of(a, b)  // Strict (Judge)
} else {
    crate::solver::subtype::is_subtype_of(self.interner, a, b)
}
```

**Note:** For `any` type inference specifically, there may be special handling needed. Consult the type inference rules in the TypeScript spec.

---

## Testing Strategy

### 1. Verification Tests

**Behavioral Tests (ensure nothing breaks):**

```rust
// src/solver/tests/compat_tests.rs

#[test]
fn test_lawyer_strict_cases_still_work() {
    // Verify structural checking still works
    assert!(number_is_subtype_of_number);  // Passes in both
    assert!(!string_is_subtype_of_number); // Fails in both
}

#[test]
fn test_lawyer_any_propagation() {
    // Rule #1: any is assignable to everything
    assert!(compat.is_assignable(ANY, NUMBER));
    assert!(compat.is_assignable(NUMBER, ANY));
}

#[test]
fn test_lawyer_void_exception() {
    // Rule #6: () => void accepts () => string
    assert!(compat.is_assignable(fn_returning_string, fn_returning_void));
}

#[test]
fn test_lawyer_weak_types() {
    // Rule #13: Detect weak type violations
    assert!(!compat.is_assignable(empty_object, interface_with_optional_props));
}

#[test]
fn test_lawyer_freshness() {
    // Verify excess property checking in object literals
    assert!(!compat.is_assignable(
        fresh_literal_with_excess_props,
        target_type
    ));
}

#[test]
fn test_cache_poisoning_prevention() {
    // ⚠️ CRITICAL: Ensure separate caches don't interfere
    // From Gemini review - prevents poisoning strict checks with loose results

    let db = QueryCache::new(&interner);

    // 1. Check assignability (loose) - any is assignable to number
    assert!(db.is_assignable_to(ANY, NUMBER));  // Should be true

    // 2. Check subtype (strict) - any is NOT subtype of number
    assert!(!db.is_subtype_of(ANY, NUMBER));  // Should be false

    // 3. Verify caches are separate
    // If caches were shared, step 1 would poison step 2
}
```

### 2. Conformance Test Suites

```bash
# Baseline - Run BEFORE changes
./scripts/conformance.sh run --summary > before_baseline.txt

# After Phase 1 (object_literal)
./scripts/conformance.sh run --filter "objectLiteral"
./scripts/conformance.sh run --filter "freshness"

# After Phase 2 (conditional)
./scripts/conformance.sh run --filter "conditional"
./scripts/conformance.sh run --filter "any"

# After Phase 3 (QueryDatabase)
./scripts/conformance.sh run --pattern "typeRelationships/assignmentCompatibility"
./scripts/conformance.sh run --pattern "functions/strictFunctionTypes"

# Full conformance - Run AFTER all changes
./scripts/conformance.sh run --summary > after_baseline.txt

# Compare TS2322 metrics
echo "=== TS2322 Comparison ==="
grep "TS2322" before_baseline.txt
grep "TS2322" after_baseline.txt
```

### 3. Measuring Progress

**Target:** Reduce `TS2322 extra` from 594 to closer to 0

```bash
# Track TS2322 specifically
./scripts/conformance.sh run --summary | grep "TS2322"
# Expected: extra=594 → should decrease significantly
# Expected: missing=293 → should not increase (no regressions)
```

**Success Criteria:**
- ✅ `TS2322 extra` decreases by 200+ (first iteration)
- ✅ `TS2322 missing` does not increase (no regressions)
- ✅ All existing unit tests still pass

---

## Rollback Plan

**Strategy:** Use a configuration flag instead of git revert for immediate rollback.

**File:** `src/checker/context.rs` or `src/solver/judge.rs`

```rust
// src/solver/judge.rs

#[derive(Clone, Debug)]
pub struct JudgeConfig {
    pub strict_function_types: bool,
    pub strict_null_checks: bool,
    pub exact_optional_property_types: bool,
    pub no_unchecked_indexed_access: bool,

    // TEMPORARY: Rollback flag for CompatChecker migration
    #[deprecated(note = "Remove after migration is stable")]
    pub use_legacy_subtype_check: bool,  // Defaults to false
}

impl Default for JudgeConfig {
    fn default() -> Self {
        Self {
            strict_function_types: true,
            strict_null_checks: true,
            exact_optional_property_types: false,
            no_unchecked_indexed_access: false,
            use_legacy_subtype_check: false,  // Use new behavior
        }
    }
}
```

**Usage in db.rs:**

```rust
fn is_subtype_of(&self, source: TypeId, target: TypeId) -> bool {
    // Rollback mechanism
    if self.config.use_legacy_subtype_check {
        // Old strict behavior (immediate rollback)
        return crate::solver::subtype::is_subtype_of(
            self.as_type_database(),
            source,
            target
        );
    }

    // New CompatChecker behavior
    let mut compat = CompatChecker::new(self.as_type_database());
    compat.apply_config(&self.config);
    compat.is_assignable(source, target)
}
```

**To Rollback:**
1. Set `use_legacy_subtype_check: true` in config
2. Rebuild and deploy
3. Debug issue offline
4. Fix and flip flag back to `false`

---

## Potential Pitfalls (from Gemini)

### 1. The "Any" Trap

**Pitfall:** `any` behaves differently in `extends` vs assignment

**Examples:**
- In `T extends U`: If `T` is `any`, it distributes (returns union of both branches)
- In assignment: `any` is just valid without distribution

**Fix:** Ensure `conditional.rs` handles `any` distribution **explicitly before** calling subtype checker.

### 2. Freshness (Excess Properties)

**Pitfall:** Using `SubtypeChecker` for object literals will miss excess properties.

**Example:**
```typescript
const x: { a: number } = { a: 1, b: 2 };  // Should error!
```

**Fix:** `object_literal.rs` **MUST** use `CompatChecker` which checks `ObjectFlags::FRESH_LITERAL`.

### 3. Recursion Limits

**Pitfall:** Creating fresh checkers in loops can cause stack overflow or OOM.

**Fix:** Ensure checkers share state:
- Reuse `recursion_depth` counter
- Share `in_progress` cache for cycle detection
- Don't create new checker for every property in deeply nested objects

---

## Timeline

| Phase | Task | Estimate | Complexity |
|-------|------|----------|------------|
| 1 | Fix `object_literal.rs` | 2-3 hours | Medium |
| 2 | Fix `conditional.rs` | 4-6 hours | High |
| 3 | Update `QueryDatabase` | 2-3 hours | Medium (separate cache) |
| 4 | Wire CompilerOptions | 1 hour | Low |
| 5 | Fix `infer.rs` | 1-2 hours | Low (see warning) |
| 6 | Add unit tests | 2-3 hours | Medium |
| 7 | Run conformance | 1 hour | Low |
| **Total** | | **~2 Days** | |

**Recommended Start:** Phase 1 (object_literal.rs)
- Fixes immediate correctness bug
- Lowest risk
- Isolated change

**Recommended Start:** Phase 1 (object_literal.rs)
- Fixes immediate correctness bug
- Lowest risk
- Isolated change

---

## Success Metrics

### Quantitative

- **TS2322 extra:** 594 → <400 (33% reduction in first iteration)
- **TS2322 missing:** 293 → ≤293 (no regressions)
- **Conformance pass rate:** 41.6% → 43%+ (improvement)

### Qualitative

- ✅ All `CompatChecker` code paths use configuration from `CompilerOptions`
- ✅ No direct `SubtypeChecker` instantiation in non-solver code
- ✅ Object literal freshness checks work correctly
- ✅ Conditional types handle `any` distribution correctly

---

## Related Work

- **`docs/investigations/structural_erasure_bug.md`** - Earlier investigation
- **`src/solver/unsoundness_audit.rs`** - Implementation status of all 44 rules
- **`docs/specs/TS_UNSOUNDNESS_CATALOG.md`** - Complete rule catalog
- **Commit 6381ff3dd** - Recent lazy type resolution fix

---

## Open Questions

1. ~~**Should we add a new `QueryDatabase` method?**~~
   - ~~Option A: Keep `is_subtype_of` (semantics change to use Lawyer)~~
   - ~~Option B: Add `is_assignable_to` (keep both Judge and Lawyer)~~
   - **RESOLVED (per Gemini review):** Option B - Add `is_assignable_to` as separate method
     - `is_subtype_of` = Strict (Judge) - for internal solver use
     - `is_assignable_to` = Loose (Lawyer) - for Checker diagnostics
     - Prevents cache poisoning and maintains solver invariants

2. **What about other direct usages?**
   - Gemini found 3 locations, but there may be more
   - **Action:** Run `grep -r "SubtypeChecker::new" src/` to find all
   - **Command:** `grep -rn "SubtypeChecker::new\|SubtypeChecker::with" src/solver/`

3. **Performance impact?**
   - `CompatChecker` adds overhead (rule checks before delegation)
   - **Mitigation:** Caching already exists in `QueryCache`
   - **Mitigation:** Separate caches prevent redundant checks
   - **Monitor:** Benchmark before/after if conformance is slow

4. **Should we add additional Audit rules?**
   - Current audit in `unsoundness_audit.rs` shows all 44 rules as "FullyImplemented"
   - **Question:** Is this accurate, or are the rules implemented but not wired up?
   - **Action:** Verify each rule's actual integration after Phase 3

---

## Next Steps

1. **Review this plan with team** - Get feedback on approach
2. **Create feature branch** - `feature/lawyer-compat-layer`
3. **Start with Phase 1** - Fix `object_literal.rs`
4. **Test after each phase** - Don't batch all changes
5. **Monitor metrics** - Track TS2322 progress
6. **Iterate** - Adjust plan based on findings

---

**Ready to proceed with Gemini review.**

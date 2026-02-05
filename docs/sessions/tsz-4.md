# Session TSZ-4: Strict Null Checks & Lawyer Layer Hardening

**Status**: In Progress
**Focus**: Fix known strict-null bugs and audit Lawyer layer for missing compatibility rules
**Blocker**: `TypeScript/tests` submodule missing. Using manual unit tests in `src/checker/tests/`.

## Goals

1. [ ] **Infrastructure**: Create `src/checker/tests/strict_null_manual.rs` for regression testing
2. [ ] **Bugfix**: Fix TS18050/TS2531 error code selection for property access on `null`/`undefined`
3. [ ] **Feature**: Implement "Weak Type" detection (TS2559) in Lawyer layer
4. [ ] **Audit**: Verify Object Literal Freshness (Excess Property Checking) logic

## Current Context (2026-02-05)

### Completed Previously ✅
- **TS18050 strictNullChecks gating** (Partial with known issues)
- **Lawyer Layer verification**: Confirmed `any` propagation, method bivariance, void return working
- **Wiring verification**: Confirmed Checker uses `is_assignable_to` correctly
- **Testing**: Created test scenarios matching `tsc` behavior

### Known Issues ⚠️
- **Error code selection**: `null.toString()` without strictNullChecks emits TS2531 (tsc emits TS18050)
- **Type inference**: `const x = null` without strictNullChecks needs work

### Previous Commits
- `ec8035b41` → `94650bcdb` - TS18050 gating implementation
- `bd67716ef` → `7b25d5bbd` - Session restoration and push

## Priority 1: Fix TS18050 & Null Property Access Error Codes

**Problem:** Accessing properties on `null`/`undefined` triggers wrong error codes.

**Task:**
1. Create `src/checker/tests/strict_null_manual.rs` with test cases
2. Distinguish literal null vs variable vs union types
3. Fix error code selection to match `tsc`

**Test Cases:**
```typescript
const x: null = null; x.prop;           // Expect TS2531
const x: undefined = undefined; x.prop;   // Expect TS2532
const x: string | null = null; x.prop;    // Expect TS2531
const x: any = null; x.prop;               // Expect NO error
```

**Validation Steps:**
```bash
# 1. Create test file
# 2. Ask Gemini about implementation in src/checker/expr.rs
./scripts/ask-gemini.mjs --include=src/checker/expr.rs \
  "I need to fix error code selection for property access on literal null"

# 3. Run test
cargo nextest run strict_null_manual
```

## Priority 2: Implement "Weak Type" Detection (Lawyer Layer)

**Goal:** Implement TS2559 (Type has no properties in common with weak type)

**Context:** "Weak types" are object types where all properties are optional. Assigning object literals requires at least one matching property.

**Task:**
1. Check `src/solver/lawyer.rs` for weak type logic
2. If missing, implement `is_weak_type` query
3. Add check in `check_assignment` logic
4. Test: `interface Weak { a?: number }` assigned `{ b: 1 }`

**Validation:**
```bash
./scripts/ask-gemini.mjs --include=src/solver/lawyer.rs \
  "Does current Lawyer implementation handle TypeScript's 'Weak Type' check (TS2559)?
   If not, how should I add is_weak_type detection?"
```

## Priority 3: Verify Object Literal Freshness

**Goal:** Ensure object literals undergo excess property checking ONLY when fresh

**Task:**
1. Verify where "freshness" is stored (flag on TypeId or context)
2. Ensure `src/solver/lawyer.rs` enforces strictness for fresh literals
3. Test: `{ extra: 1 }` vs `{ a: number }` assignment

**Focus Areas**
- `src/checker/expr.rs` - Property access logic and error reporting
- `src/solver/lawyer.rs` - Compatibility rules (Weak types, Any propagation)
- `src/solver/compat.rs` - `is_assignable_to` logic
- `src/checker/tests/` - Manual verification tests

## Next Actions (Priority Order)

1. **Immediate**: Create manual test infrastructure (`src/checker/tests/strict_null_manual.rs`)
2. **Then**: Ask Gemini (Pro) for TS18050/TS2531 fix approach
3. **Then**: Implement fix and verify with `cargo nextest`
4. **Later**: Weak type detection and freshness audit

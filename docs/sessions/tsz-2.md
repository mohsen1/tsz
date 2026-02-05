# Session: tsz-2 - Phase 5: Enforce Solver-First Architecture

**Started**: 2026-02-05
**Status**: Active - Anti-Pattern 8.1 Refactoring
**Focus**: Remove direct TypeKey inspection from Checker, enforce Solver-First boundary

**Previous Session**: Coinductive Subtyping (COMPLETE)
**Next Session**: TBD

## Progress Summary

**✅ Phase 4.3 Migration (COMPLETE):**
- All 46 deprecation warnings eliminated (70 → 0)
- Replaced get_ref_symbol/get_symbol_ref with resolve_type_to_symbol_id
- Removed deprecated functions from type_queries.rs
- Clean separation between type-space (Lazy/DefId) and value-space (TypeQuery)

**🎯 Current Focus: Phase 5 - Anti-Pattern 8.1 Removal**
- Violation: Checker directly inspects TypeKey internals
- Goal: Checker must use Solver API or Visitor pattern
- Principle: "Solver-First Architecture" (NORTH_STAR.md Section 3.1)

## Anti-Pattern 8.1 Context

Per `docs/architecture/NORTH_STAR.md` (Section 8.1), the Checker must **never** inspect `TypeKey` internals directly.

**Current Violation Example:**
```rust
// src/checker/some_file.rs
if let TypeKey::Intrinsic(IntrinsicKind::String) = types.get(id) { ... }
```

**Target Architecture:**
```rust
// src/checker/some_file.rs
if solver.is_string(id) { ... }
```

## Specific Tasks

### Step 1: Audit & Categorize (IN PROGRESS)
Search `src/checker/` for TypeKey usages and categorize:

```bash
# Find TypeKey pattern matches in Checker
grep -rn "TypeKey::" src/checker/*.rs | grep -v "use crate::solver::TypeKey"
```

**Categories:**
- **Identity Checks**: is_string, is_any, is_never
- **Structure Extraction**: Getting properties from Object, members from Union
- **Relation Checks**: Assignability, subtyping checks
- **Type Traversal**: Recursively visiting nested types

**Audit Results:**
- 109 TypeKey usages across 18 checker files identified
- Top files: assignability_checker.rs (15), generators.rs (10), iterators.rs (9)

### Step 2: Refactor Primitives (Low Risk)
Target: Simple type identity checks

**Process:**
1. Ask Gemini (Question 1): "I found TypeKey matching for primitives. Plan: expose is_string(TypeId) in Solver. Is this correct?"
2. Implement helper methods in `src/solver/type_queries.rs`
3. Replace Checker matches with Solver calls
4. Ask Gemini (Question 2): Review the changes

### Step 3: Refactor Complex Types (High Risk)
Target: Union/Object iteration, complex type traversal

**Process:**
1. Identify where Checker manually iterates Unions/Objects
2. Ask Gemini (Question 1): "Checker manually iterating Union to find X. Should I add specific query to Solver or use TypeVisitor trait?"
3. Implement solution (likely `src/solver/operations.rs` or Visitor pattern)
4. Ask Gemini (Question 2): Review implementation

## Success Criteria

- [ ] `src/checker/` no longer imports `TypeKey`
- [ ] `src/checker/` no longer imports `IntrinsicKind`, `LiteralValue`, etc.
- [ ] All type logic resides in `src/solver/`
- [ ] `cargo nextest run` passes (no regressions)

## Two-Question Rule (MANDATORY)

For ANY changes to `src/solver/` or `src/checker/`:

**Question 1 (Before Implementation):**
```bash
./scripts/ask-gemini.mjs --include=src/solver "I found X in checker/file.rs.
Plan: [YOUR PLAN]

Is this correct? What edge cases might I miss?"
```

**Question 2 (After Implementation):**
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver "I implemented X.
Code: [PASTE CODE]

Does this handle all edge cases correctly?"
```

## Session History

- 2026-02-05: Completed Phase 4.3 Migration (Ref → Lazy/DefId)
- 2026-02-05: Redefined session to Phase 5 - Anti-Pattern 8.1 Removal
- 2026-02-05: Gemini consultation complete - clear path forward

## Notes

**Why This Priority:**
1. **Architecture**: Enforces Solver-First separation (NORTH_STAR.md)
2. **Maintainability**: Solver can evolve internal representation without breaking Checker
3. **Correctness**: Centralized type logic in Solver reduces bugs

**Key Risk:**
- Direct TypeKey matching misses edge cases (Unions, Intersections, Lazy)
- Manual traversal in Checker duplicates Solver logic
- Tight coupling prevents architectural improvements

**Credits:**
- Session redefined by Gemini Pro consultation
- Two-Question Rule ensures correctness
- All changes require pre/post validation

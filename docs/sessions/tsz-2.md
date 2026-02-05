# Session tsz-2: Enforce Solver-First Architecture (Anti-Pattern 8.1)

**Started**: 2026-02-05
**Status**: Active - Anti-Pattern 8.1 Refactoring
**Focus**: Remove direct TypeKey inspection from Checker, enforce Solver-First boundary

**Previous Session**: Coinductive Subtyping (COMPLETE)
**Next Session**: TBD

## Progress Summary

**Completed (Phase 4.3 Migration):**
- ✅ All 46 deprecation warnings eliminated (70 → 0)
- ✅ Ref → Lazy/DefId migration complete
- ✅ Changes pushed to main

**Current Focus: Anti-Pattern 8.1 - Remove Direct TypeKey Matching**
- Checker is violating NORTH_STAR.md Rule 3: "Checker NEVER inspects type internals"
- This blocks Solver from evolving its internal representation
- Must decouple Checker from TypeKey to achieve Solver-First Architecture

## Current Plan

### Task 1: Audit & Categorize (IN PROGRESS)
Search `src/checker/` for TypeKey usages and categorize by purpose:

```bash
# Find all TypeKey pattern matches in Checker
grep -rn "TypeKey::" src/checker/*.rs | grep -v "use crate::solver::TypeKey"
```

**Categorization:**
- Type checking (is this a string/object/function?)
- Property access (does this object have property 'x'?)
- Type traversal (ensure refs resolved, collect dependencies)
- Structural inspection (get members, get params, etc.)

### Task 2: Extend Solver API
For each identified need, create semantic queries in `src/solver/`:

**Examples:**
- Instead of: `matches!(key, TypeKey::Intrinsic(String))`
- Use: `solver.is_string_type(type_id)`
- Instead of: `TypeKey::Object(shape_id) => { check props }`
- Use: `solver.get_property_type(type_id, property_name)`

**Constraint:** New API methods MUST handle:
- Lazy types (resolve via DefId)
- Union types (check all members)
- Intersection types (check all members)
- Readonly wrappers (unwrap first)

### Task 3: Refactor Checker
Replace pattern matching with Solver API calls:

**Goal:** Checker should NOT import `TypeKey` at all
- Remove all `match type_key { TypeKey::... }` patterns
- Replace with `solver.is_...()` or `solver.get_...()` calls
- Checker reads like high-level logic, not structural manipulation

## Success Criteria

- [ ] Zero direct matches on `TypeKey` variants in `src/checker/`
- [ ] Checker does not import `TypeKey` (except possibly in type definitions)
- [ ] All tests pass (conformance and unit tests)
- [ ] Solver API extended with necessary semantic queries

## Two-Question Rule (MANDATORY)

For ANY changes to `src/solver/` or `src/checker/`:

**Question 1 (Before):**
```bash
./scripts/ask-gemini.mjs --include=src/solver "I found TypeKey pattern matching in checker/file.rs.
Pattern: [PASTE CODE]
Purpose: [DESCRIBE WHY]
Plan: Replace with solver.is_xyz().

Is this correct? What edge cases might I miss?"
```

**Question 2 (After):**
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver "I implemented solver.is_xyz().
Code: [PASTE CODE]

Does this handle Unions/Intersections/Lazy correctly? Any bugs?"
```

## Notes

**Why This Priority:**
1. **Architecture**: Anti-Pattern 8.1 is biggest blocker to Solver-First Architecture
2. **Evolution**: Solver cannot optimize internal representation if Checker depends on TypeKey
3. **Correctness**: Pattern matching often misses edge cases (Unions, Intersections, Lazy)

**Key Risk:**
- Simple pattern `matches!(key, TypeKey::String)` misses:
  - `TypeKey::Union(String, Number)` - should match String
  - `TypeKey::Lazy(def_id)` - might resolve to String
  - `TypeKey::Intersection(String, ...)` - should match String

**Files of Interest:**
- src/checker/assignability_checker.rs (15+ TypeKey matches)
- src/checker/call_checker.rs (TypeKey::Lazy match)
- src/solver/visitor.rs (existing visitor pattern)
- src/solver/type_queries.rs (add new semantic queries here)

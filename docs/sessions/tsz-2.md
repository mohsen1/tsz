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

**✅ Completed (2026-02-05):**
- Implemented `for_each_child` traversal helper in `src/solver/visitor.rs`
- Handles ALL 24+ TypeKey variants with complete field coverage:
  - Object properties: type_id + write_type
  - Index signatures: key_type + value_type
  - Functions/Callables: return_type, this_type, type_predicate.type_id, params, type_params constraints/defaults
  - Mapped types: type_param constraint + default, constraint, template, name_type
- Refactored `assignability_checker.rs` ensure_refs_resolved_inner from ~170 to ~70 lines
- Refactored `state_type_analysis.rs` removed 3 TypeKey inspections:
  - is_same_type_parameter: Uses get_type_parameter_info()
  - contextual_type_allows_literal_inner: Uses get_lazy_def_id(), is_keyof_type(), is_index_access_type()
- Started refactoring `state_type_environment.rs`:
  - get_enum_identity: Uses enum_components() instead of TypeKey::Enum match
- Enhanced Solver's enum compatibility logic in `compat.rs`:
  - Enhanced enum_assignability_override for union->enum handling
  - Added are_types_identical_for_redeclaration method with get_enum_def_id helper
  - get_enum_def_id handles both direct Enum members AND Union-based Enums
- All implementations reviewed by Gemini Pro (identified and fixed critical bugs)
- All changes committed and pushed to origin

**🎯 Current Focus: Investigating enum_assignability_override removal (Priority 1)**

**✅ Completed (2026-02-05):**
- Added `is_enum_type(TypeId, &dyn TypeDatabase) -> bool` to TypeResolver trait (src/solver/subtype.rs)
- Implemented `is_enum_type` in CheckerContext (src/checker/context.rs):
  - Distinguishes enum TYPE (E) from enum MEMBER (E.A) using symbol flags
  - Handles Union-based enum types by comparing parent SymbolIds (not member DefIds)
  - Bug fix: Was comparing member DefIds which are unique per member, causing false for enums with >1 member
- Implemented `is_numeric_enum` in CheckerContext:
  - Bug fix: Now handles enum members by looking up their parent enum symbol
  - Traverses AST to check for string literal initializers
- Enhanced CompatChecker.enum_assignability_override (src/solver/compat.rs):
  - Added early check before match: number -> Union enum type (e.g., number -> E where E = E.A | E.B)
  - Uses is_enum_type to distinguish enum types from members for Rule #7
  - All 11 enum_nominality_tests pass

**⚠️ Blocking Issue:**
Attempted to remove Checker's `enum_assignability_override` (return None from CheckerOverrideProvider), but this caused 4 test failures:
- test_enum_member_to_whole_enum
- test_number_literal_to_numeric_enum_type
- test_number_to_numeric_enum_type
- test_string_enum_not_to_string

**Root Cause:** Checker's enum_assignability_override has additional logic not yet migrated to Solver. Need to identify and migrate missing logic before removal.

**Next Steps:**
1. Test removal of Checker's enum_assignability_override (now that Solver has all logic)
2. If tests pass, remove Checker's enum logic from state_type_environment.rs
3. Move to other TypeKey removals (generators.rs, iterators.rs, etc.)

**✅ Latest Progress (2026-02-05 evening):**
- Added `get_enum_parent_def_id(DefId) -> Option<DefId>` to TypeResolver trait
- Implemented in CheckerContext using symbol.parent and symbol_to_def_id
- Fixed Case 1 nominality bug (E.A -> E.B should reject even with same value)
- Fixed Gap B (String Enum Opacity with Union sources)
- Added early check for string enum TYPE -> string rejection
- All changes reviewed by Gemini Pro (Question 2)
- Committed and pushed to origin

### Step 2: Refactor Primitives (Low Risk)
Target: Simple type identity checks

**✅ Completed (2026-02-05):**
- Implemented `for_each_child` helper in visitor.rs
- Refactored assignability_checker.rs to use the helper

**Next Steps:**
1. Use `for_each_child` pattern in other Checker files with complex TypeKey traversal:
   - generators.rs (10 usages)
   - iterators.rs (9 usages)
   - state_type_environment.rs (19 usages)
   - state_type_analysis.rs (18 usages)
2. For simple type identity checks, add Solver API methods like:
   - `is_string(type_id)`, `is_any(type_id)`, `is_never(type_id)`
   - `get_object_properties(type_id) -> Option<&[Property]>`
   - `get_union_members(type_id) -> Option<&[TypeId]>`

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
- 2026-02-05: Implemented `for_each_child` traversal helper with Gemini review
- 2026-02-05: Refactored assignability_checker.rs using new helper (60% code reduction)

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

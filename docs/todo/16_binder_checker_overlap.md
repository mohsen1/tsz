# Remove Binder/Checker Overlap

**Reference**: Architectural Review Summary - Issue #10  
**Severity**: 🟠 High  
**Status**: TODO  
**Priority**: High - Architecture clarity

---

## Problem

Checker contains complex scope-walking logic (`resolve_identifier_symbol`, `find_enclosing_scope`). Binder should have already resolved all identifiers to `SymbolId`s. Checker walking scopes implies Binder pass was incomplete or Checker doesn't trust Binder.

**Impact**: Redundant work, potential divergence, unclear responsibility boundaries.

**Location**: `src/checker/symbol_resolver.rs`

---

## Goal

Eliminate scope-walking logic in the Checker (`resolve_identifier_symbol`, `find_enclosing_scope`) and rely on the Binder's symbol resolution.

---

## Phase 1: Audit & Verification

1. **Audit `resolve_identifier_symbol` Usage**
   - Identify all call sites of `resolve_identifier_symbol` in `src/checker/`.
   - Determine if any call sites rely on dynamic/contextual information that the Binder wouldn't have (e.g., control-flow dependent narrowing, though symbol resolution should be static).
   - *Target Files*: `src/checker/state.rs`, `src/checker/symbol_resolver.rs`, `src/checker/state_type_analysis.rs`.

2. **Verify Binder Resolution Completeness**
   - Create a temporary "verification mode" in `resolve_identifier_symbol`:
     ```rust
     // Temporary logic
     let binder_sym = self.ctx.binder.get_node_symbol(idx);
     let checker_sym = self.resolve_identifier_symbol_inner(idx);
     if binder_sym != checker_sym {
         // Log mismatch for debugging
     }
     ```
   - Run conformance tests to identify where the Binder fails to resolve identifiers that the Checker currently resolves.
   - *Key Areas to Check*:
     - Global variables (lib.d.ts).
     - Cross-file imports/exports.
     - Shadowed variables.
     - Type parameters in nested generic functions.

---

## Phase 2: Binder Enhancement (If Gaps Found)

*If Phase 1 reveals that the Binder is not resolving certain identifiers (e.g., usages vs declarations), move the logic from Checker to Binder.*

3. **Move Scope Walking to Binder**
   - If the Binder currently only binds *declarations*, implement the `resolve_identifier` logic (walking up `scopes`) inside the Binder's pass.
   - Ensure `node_symbols` is populated for *identifier references* (usages), not just declarations.
   - *File*: `src/binder/state.rs` (or `src/binder/mod.rs`).

4. **Handle Cross-File/Lib Resolution in Binder**
   - The Checker currently iterates `lib_contexts` and `all_binders`.
   - Ensure the Binder has access to necessary global scopes or that `get_node_symbol` logic in the Binder (or the lookup wrapper) can access these without the Checker's manual iteration.

---

## Phase 3: Checker Refactoring

5. **Deprecate `find_enclosing_scope` in Checker**
   - The Binder already calculates node scopes (`node_scope_ids`).
   - Replace `find_enclosing_scope(node)` with `ctx.binder.get_scope_for_node(node)`.
   - *File*: `src/checker/scope_finder.rs` (or `symbol_resolver.rs`).

6. **Refactor `resolve_identifier_symbol`**
   - Rewrite `resolve_identifier_symbol` to wrap `ctx.binder.get_node_symbol(idx)`.
   - Remove the manual `while !scope_id.is_none()` loop.
   - Remove `resolve_identifier_symbol_inner`.
   - *File*: `src/checker/symbol_resolver.rs`.

   ```rust
   // New implementation sketch
   pub(crate) fn resolve_identifier_symbol(&self, idx: NodeIndex) -> Option<SymbolId> {
       // 1. Trust the binder
       if let Some(sym) = self.ctx.binder.get_node_symbol(idx) {
           self.ctx.referenced_symbols.borrow_mut().insert(sym);
           return Some(sym);
       }

       // 2. Fallback for Lib/Global symbols if Binder doesn't link them yet
       // (Only keep this if Phase 2 doesn't fully solve global linking)
       self.resolve_global_fallback(idx)
   }
   ```

7. **Update `resolve_qualified_symbol`**
   - This function also does manual resolution. Update it to rely on the Binder resolving the left-hand side of qualified names.

---

## Phase 4: Cleanup & Testing

8. **Remove Redundant Code**
   - Delete the manual scope walking logic from `src/checker/symbol_resolver.rs`.
   - Remove unused helper methods related to scope traversal if they are no longer needed.

9. **Testing Strategy**
   - **Regression Testing**: Run the full conformance test suite (`./scripts/conformance/run.sh`).
   - **Unit Testing**: Create a specific test case in `src/checker/tests/symbol_resolution_tests.rs` that asserts:
     - Local variable shadowing works.
     - Global variables (e.g., `console`) are resolved.
     - Type parameters in nested scopes are resolved correctly.

---

## Success Criteria

- [ ] `resolve_identifier_symbol` no longer iterates through scopes
- [ ] Conformance test pass rate does not decrease
- [ ] Code size in `checker/symbol_resolver.rs` is significantly reduced
- [ ] Binder resolves all identifiers that Checker needs
- [ ] No functionality lost
- [ ] Clear separation of responsibilities: Binder resolves, Checker uses

---

## Acceptance Criteria

- [ ] All scope walking removed from Checker
- [ ] Checker trusts Binder's symbol resolution
- [ ] Binder resolves all necessary identifiers
- [ ] `symbol_resolver.rs` significantly simplified
- [ ] Conformance tests pass with no regressions
- [ ] Clear documentation of Binder/Checker boundaries

//! Subtype checking and redeclaration compatibility.
//!
//! Extracted from `assignability_checker.rs` to keep modules focused.
//! This module owns:
//! - `is_subtype_of`
//! - `are_var_decl_types_compatible` (TS2403)

use crate::query_boundaries::assignability::{
    is_redeclaration_identical_with_resolver, is_relation_cacheable, is_subtype_with_resolver,
};
use crate::state::CheckerState;
use tsz_solver::RelationCacheKey;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Subtype Checking
    // =========================================================================

    /// Check if `source` type is a subtype of `target` type.
    ///
    /// This is the main entry point for subtype checking, used for type compatibility
    /// throughout the type system. Subtyping is stricter than assignability.
    pub fn is_subtype_of(&mut self, source: TypeId, target: TypeId) -> bool {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_binder::symbol_flags;

        // Fast path: identity check
        if source == target {
            return true;
        }

        // Keep subtype preconditions aligned with assignability to avoid
        // caching relation answers before lazy/application refs are prepared.
        self.ensure_relation_input_ready(source);
        self.ensure_relation_input_ready(target);

        // Check relation cache for non-inference types
        // Construct RelationCacheKey with Lawyer-layer flags to prevent cache poisoning
        let is_cacheable = is_relation_cacheable(self.ctx.types, source, target);
        let flags = self.ctx.pack_relation_flags();

        if is_cacheable {
            // Note: For subtype checks in the checker, we use AnyPropagationMode::All (0)
            // since the checker doesn't track depth like SubtypeChecker does
            let cache_key = RelationCacheKey::subtype(source, target, flags, 0);

            if let Some(cached) = self.ctx.types.lookup_subtype_cache(cache_key) {
                return cached;
            }
        }

        let binder = self.ctx.binder;

        // Helper to check if a symbol is a class (for nominal subtyping)
        let is_class_fn = |sym_ref: tsz_solver::SymbolRef| -> bool {
            let sym_id = tsz_binder::SymbolId(sym_ref.0);
            if let Some(sym) = binder.get_symbol(sym_id) {
                (sym.flags & symbol_flags::CLASS) != 0
            } else {
                false
            }
        };
        let relation_result = {
            let env = self.ctx.type_env.borrow();
            is_subtype_with_resolver(
                self.ctx.types,
                &*env,
                source,
                target,
                flags,
                &self.ctx.inheritance_graph,
                Some(&is_class_fn),
            )
        };

        if relation_result.depth_exceeded {
            self.error_at_current_node(
                diagnostic_messages::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                diagnostic_codes::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
            );
        }

        let result = relation_result.is_related();

        // Cache the result for non-inference types
        if is_cacheable {
            let cache_key = RelationCacheKey::subtype(source, target, flags, 0);

            self.ctx.types.insert_subtype_cache(cache_key, result);
        }

        result
    }

    /// Resolve a `TypeQuery` chain iteratively until a non-TypeQuery type is reached.
    ///
    /// Used for TS2403 checking where `typeof x` type annotations may chain
    /// through multiple symbols (e.g., `typeof e` → `typeof d` → actual type).
    /// Returns the fully resolved type, or `any` if a cycle is detected.
    ///
    /// Unlike `resolve_type_query_type` which only resolves one level,
    /// this maintains a visited set across iterations to detect cycles like
    /// `typeof d` → `typeof e` → `typeof d` → ...
    fn resolve_type_query_chain(&mut self, mut type_id: TypeId) -> TypeId {
        use crate::query_boundaries::type_checking_utilities::{
            TypeQueryKind, classify_type_query,
        };

        // Track visited symbols to detect cycles across iterations.
        // resolve_type_query_type pushes/pops its typeof_resolution_stack within
        // each call, so it can't detect cross-iteration cycles on its own.
        let mut visited = Vec::<u32>::new();

        for _ in 0..8 {
            let sym_id = match classify_type_query(self.ctx.types, type_id) {
                TypeQueryKind::TypeQuery(sym_ref) => sym_ref.0,
                TypeQueryKind::ApplicationWithTypeQuery { base_sym_ref, .. } => base_sym_ref.0,
                _ => return type_id,
            };

            // If we've already tried to resolve this symbol, we have a cycle.
            if visited.contains(&sym_id) {
                return TypeId::ANY;
            }
            visited.push(sym_id);

            let resolved = self.resolve_type_query_type(type_id);
            if resolved == TypeId::ERROR {
                // Cycle detected — circular typeof resolves to `any` in tsc
                return TypeId::ANY;
            }
            if resolved == type_id {
                // No progress — the TypeQuery resolved back to itself.
                // This indicates a circular typeof chain (e.g., e's cached type
                // is typeof e, pointing to itself). In tsc, circular typeof
                // resolves to `any`.
                return TypeId::ANY;
            }
            type_id = resolved;
        }
        // Exceeded iteration limit — treat as `any` to avoid false positives
        TypeId::ANY
    }

    /// Check if variable declaration types are compatible (used for multiple declarations).
    ///
    /// Delegates to the Solver's `CompatChecker` to determine if two types are
    /// compatible for redeclaration (TS2403). This moves enum comparison logic
    /// from Checker to Solver per Phase 5 Anti-Pattern 8.1 removal.
    pub(crate) fn are_var_decl_types_compatible(
        &mut self,
        prev_type: TypeId,
        current_type: TypeId,
    ) -> bool {
        // Resolve TypeQuery (typeof) types before compatibility checking.
        // Type annotations like `typeof x` may produce unresolved TypeQuery types
        // that need to be resolved to the actual symbol type for proper comparison.
        // Resolve iteratively since one resolution may produce another TypeQuery
        // (e.g., typeof e → typeof d → actual_type).
        let prev_type = self.resolve_type_query_chain(prev_type);
        let current_type = self.resolve_type_query_chain(current_type);

        // Ensure Ref/Lazy types are resolved before checking compatibility
        self.ensure_relation_input_ready(prev_type);
        self.ensure_relation_input_ready(current_type);

        let flags = self.ctx.pack_relation_flags();
        // Delegate to the Solver's Lawyer layer for redeclaration identity checking
        {
            let env = self.ctx.type_env.borrow();
            if is_redeclaration_identical_with_resolver(
                self.ctx.types,
                &*env,
                prev_type,
                current_type,
                flags,
                &self.ctx.inheritance_graph,
                self.ctx.sound_mode(),
            ) {
                return true;
            }
        }

        // TS2403 enum-object fallback: When one type is an enum type and the other is
        // a structural object type, TypeScript considers them compatible if the object
        // type matches the enum's "typeof" shape (its property object form).
        if let Some(result) = self.try_enum_object_redeclaration_check(prev_type, current_type) {
            return result;
        }

        // Bidirectional subtype fallback for structural equivalences.
        // The solver's identity check cannot normalize all cases (intersection
        // distribution, typeof evaluation, generic application results), so we
        // fall back to bidirectional subtype checking which handles these correctly.
        //
        // Excluded cases:
        // - `any` is never identical to non-`any` for redeclaration (tsc behavior)
        // - Union vs non-union mismatch (e.g., `C` vs `C | D` where D extends C)
        //   must NOT use this fallback — tsc's isTypeIdenticalTo rejects these
        if prev_type == TypeId::ANY || current_type == TypeId::ANY {
            return false;
        }
        // Guard: when exactly one side is a union and the other is not, tsc's
        // isTypeIdenticalTo rejects them even if they're bidirectionally subtypes.
        // e.g., `C` vs `C | D` (where D extends C) fails identity in tsc.
        {
            use tsz_solver::type_queries;
            let prev_is_union = type_queries::is_union_type(self.ctx.types, prev_type);
            let curr_is_union = type_queries::is_union_type(self.ctx.types, current_type);
            if prev_is_union != curr_is_union {
                return false;
            }
        }
        self.is_subtype_of(prev_type, current_type) && self.is_subtype_of(current_type, prev_type)
    }

    /// Try checking redeclaration compatibility using enum object shape substitution.
    ///
    /// When one type is a nominal enum type (`TypeData::Enum`) and the other is a
    /// structural non-enum type, attempts to replace the enum type with its
    /// "typeof enum" object shape and retries the compatibility check.
    ///
    /// This handles: `var e = E1; var e: { readonly A: E1.A; ... }`
    /// where TSC considers both to be `typeof E1`.
    ///
    /// Returns Some(bool) if enum substitution was applicable, None otherwise.
    fn try_enum_object_redeclaration_check(
        &mut self,
        prev_type: TypeId,
        current_type: TypeId,
    ) -> Option<bool> {
        use tsz_binder::symbol_flags;
        use tsz_solver::visitor::enum_components;

        // Extract the SymbolId for a type if it's an enum TYPE (not a member).
        // Separated from the closure to allow reborrowing.
        fn get_enum_type_sym(
            type_id: TypeId,
            types: &dyn tsz_solver::TypeDatabase,
            ctx: &crate::context::CheckerContext<'_>,
        ) -> Option<tsz_binder::SymbolId> {
            let (def_id, _) = enum_components(types, type_id)?;
            let sym_id = ctx.def_to_symbol_id(def_id)?;
            let symbol = ctx.binder.get_symbol(sym_id)?;
            // Must be an enum but NOT an enum member (we want the enum type itself)
            if (symbol.flags & symbol_flags::ENUM) != 0
                && (symbol.flags & symbol_flags::ENUM_MEMBER) == 0
            {
                Some(sym_id)
            } else {
                None
            }
        }

        let prev_enum_sym = get_enum_type_sym(prev_type, self.ctx.types, &self.ctx);
        let current_enum_sym = get_enum_type_sym(current_type, self.ctx.types, &self.ctx);

        // Only proceed if exactly one side is an enum type and the other is NOT.
        let (enum_sym, non_enum_type) = match (prev_enum_sym, current_enum_sym) {
            (Some(sym), None) => (sym, current_type),
            (None, Some(sym)) => (sym, prev_type),
            _ => return None,
        };

        // Build the "typeof Enum" object shape using checker's enum_object_type helper.
        let enum_obj_type = self.enum_object_type(enum_sym)?;

        // Retry the check with the enum's object shape substituted in.
        let flags = self.ctx.pack_relation_flags();
        let env = self.ctx.type_env.borrow();
        let compatible = is_redeclaration_identical_with_resolver(
            self.ctx.types,
            &*env,
            enum_obj_type,
            non_enum_type,
            flags,
            &self.ctx.inheritance_graph,
            self.ctx.sound_mode(),
        );
        Some(compatible)
    }
}

//! Subtype checking, type identity, and redeclaration compatibility.
//!
//! Extracted from `assignability_checker.rs` to keep modules focused.
//! This module owns:
//! - `is_subtype_of` / `is_subtype_of_with_env`
//! - `are_types_identical`
//! - `are_var_decl_types_compatible` (TS2403)
//! - `is_assignable_to_union`

use crate::query_boundaries::assignability::{
    is_assignable_with_resolver, is_redeclaration_identical_with_resolver, is_relation_cacheable,
    is_subtype_with_resolver,
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

    /// Check if source type is a subtype of target type with explicit environment.
    pub fn is_subtype_of_with_env(
        &mut self,
        source: TypeId,
        target: TypeId,
        env: &tsz_solver::TypeEnvironment,
    ) -> bool {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_binder::symbol_flags;

        // CRITICAL: Before checking subtypes, ensure all Ref types are resolved
        self.ensure_relation_input_ready(source);
        self.ensure_relation_input_ready(target);

        // Helper to check if a symbol is a class (for nominal subtyping)
        let is_class_fn = |sym_ref: tsz_solver::SymbolRef| -> bool {
            let sym_id = tsz_binder::SymbolId(sym_ref.0);
            if let Some(sym) = self.ctx.binder.get_symbol(sym_id) {
                (sym.flags & symbol_flags::CLASS) != 0
            } else {
                false
            }
        };

        let result = is_subtype_with_resolver(
            self.ctx.types,
            env,
            source,
            target,
            self.ctx.pack_relation_flags(),
            &self.ctx.inheritance_graph,
            Some(&is_class_fn),
        );

        if result.depth_exceeded {
            self.error_at_current_node(
                diagnostic_messages::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                diagnostic_codes::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
            );
        }

        result.is_related()
    }

    // =========================================================================
    // Type Identity and Compatibility
    // =========================================================================

    /// Check if two types are identical (same `TypeId`).
    pub fn are_types_identical(&self, type1: TypeId, type2: TypeId) -> bool {
        type1 == type2
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

        false
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

    /// Check if source type is assignable to ANY member of a target union.
    pub fn is_assignable_to_union(&self, source: TypeId, targets: &[TypeId]) -> bool {
        let flags = self.ctx.pack_relation_flags();
        let env = self.ctx.type_env.borrow();

        for &target in targets {
            if is_assignable_with_resolver(
                self.ctx.types,
                &*env,
                source,
                target,
                flags,
                &self.ctx.inheritance_graph,
                self.ctx.sound_mode(),
            ) {
                return true;
            }
        }
        false
    }
}

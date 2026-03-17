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

    /// Resolve a namespace `Lazy(DefId)` type to its structural Object form.
    ///
    /// Namespace symbols are cached as `Lazy(DefId)` which self-references through
    /// the `symbol_types` cache. The solver's evaluator cannot expand these because
    /// `resolve_lazy` returns the same `Lazy(DefId)`. For TS2403 redeclaration
    /// checking, we need the structural form so that bidirectional subtype checks
    /// can compare namespace types against structurally equivalent object literals.
    ///
    /// Returns the original type unchanged if it is not a namespace Lazy type.
    fn resolve_namespace_lazy_for_redeclaration(&mut self, type_id: TypeId) -> TypeId {
        use tsz_binder::symbol_flags;
        use tsz_solver::type_queries;

        // Check if this is a Lazy(DefId) type
        let Some(def_id) = type_queries::get_lazy_def_id(self.ctx.types, type_id) else {
            return type_id;
        };

        // Map DefId -> SymbolId
        let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) else {
            return type_id;
        };

        // Check if this is a pure namespace symbol (not function, variable, or enum)
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return type_id;
        };
        let flags = symbol.flags;
        let is_namespace =
            flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0;
        let is_also_function_or_var_or_enum =
            flags & (symbol_flags::FUNCTION | symbol_flags::VARIABLE | symbol_flags::ENUM) != 0;
        if !is_namespace || is_also_function_or_var_or_enum {
            return type_id;
        }

        // Build a structural object type from the namespace's exports.
        // This mirrors the pattern in `merge_namespace_exports_into_object`.
        let Some(exports) = symbol.exports.as_ref().cloned() else {
            return type_id;
        };

        let mut properties = Vec::new();
        for (name, member_id) in exports.iter() {
            // Skip circular references
            if self.ctx.symbol_resolution_set.contains(member_id) {
                continue;
            }

            let Some(member_symbol) = self.ctx.binder.get_symbol(*member_id) else {
                continue;
            };
            // Skip type-only exports (interfaces, type aliases without value)
            if member_symbol.flags & symbol_flags::VALUE == 0 {
                continue;
            }

            let member_type = self.get_type_of_symbol(*member_id);
            let name_atom = self.ctx.types.intern_string(name);
            properties.push(tsz_solver::PropertyInfo {
                name: name_atom,
                type_id: member_type,
                write_type: member_type,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: tsz_solver::Visibility::Public,
                parent_id: None,
                declaration_order: 0,
            });
        }

        if properties.is_empty() {
            return type_id;
        }

        self.ctx.types.factory().object(properties)
    }

    /// Build a structural object type from a namespace symbol's exports.
    ///
    /// Used when `resolve_type_query_type` returns UNKNOWN for a namespace symbol
    /// (e.g., merged namespace+interface). Directly builds the namespace's value-side
    /// structural type from its exports, bypassing `get_type_of_symbol` which may
    /// return the interface side for merged symbols.
    fn resolve_namespace_lazy_for_symbol(&mut self, sym_id: tsz_binder::SymbolId) -> TypeId {
        use tsz_binder::symbol_flags;

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return TypeId::UNKNOWN;
        };
        let flags = symbol.flags;
        let is_namespace =
            flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0;
        if !is_namespace {
            return TypeId::UNKNOWN;
        }

        let Some(exports) = symbol.exports.as_ref().cloned() else {
            return TypeId::UNKNOWN;
        };

        let mut properties = Vec::new();
        for (name, member_id) in exports.iter() {
            if self.ctx.symbol_resolution_set.contains(member_id) {
                continue;
            }

            let Some(member_symbol) = self.ctx.binder.get_symbol(*member_id) else {
                continue;
            };
            // Skip type-only exports (interfaces, type aliases without value)
            if member_symbol.flags & symbol_flags::VALUE == 0 {
                continue;
            }

            let member_type = self.get_type_of_symbol(*member_id);
            let name_atom = self.ctx.types.intern_string(name);
            properties.push(tsz_solver::PropertyInfo {
                name: name_atom,
                type_id: member_type,
                write_type: member_type,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: tsz_solver::Visibility::Public,
                parent_id: None,
                declaration_order: 0,
            });
        }

        if properties.is_empty() {
            return TypeId::UNKNOWN;
        }

        self.ctx.types.factory().object(properties)
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
                // No progress — return as-is
                return type_id;
            }
            // When a TypeQuery for a namespace symbol resolves to UNKNOWN
            // (because type_of_value_declaration_for_symbol returns UNKNOWN for
            // MODULE_DECLARATION nodes), build the structural namespace object
            // directly from the symbol's exports. This prevents false TS2403
            // errors when comparing `typeof NS.Point` against a structurally
            // equivalent object literal like `{ Origin(): { x: number; y: number } }`.
            if resolved == TypeId::UNKNOWN {
                let binder_sym = tsz_binder::SymbolId(sym_id);
                let ns_type = self.resolve_namespace_lazy_for_symbol(binder_sym);
                if ns_type != TypeId::UNKNOWN && ns_type != TypeId::ERROR {
                    return ns_type;
                }
                // If we couldn't build a namespace type, treat UNKNOWN as any
                // to avoid false TS2403 errors from unresolved typeof queries.
                return TypeId::ANY;
            }
            // When a TypeQuery for an enum symbol resolves to the nominal
            // Enum(DefId, _) type, convert it to the structural enum constructor
            // object. This ensures `typeof M.Color` (TypeQuery -> Enum) produces
            // the same structural type as `m.Color` (property access -> Object).
            if tsz_solver::is_enum_type(self.ctx.types, resolved) {
                let binder_sym = tsz_binder::SymbolId(sym_id);
                if let Some(symbol) = self.ctx.binder.get_symbol(binder_sym)
                    && (symbol.flags & tsz_binder::symbol_flags::ENUM) != 0
                    && (symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) == 0
                    && let Some(enum_obj) = self.enum_object_type(binder_sym)
                {
                    return enum_obj;
                }
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

        // Nominal identity check: when both types come from different named type
        // references (Application with different bases, or Lazy with different DefIds),
        // tsc's isTypeIdenticalTo rejects them even if structurally equivalent.
        // e.g., `IPromise<string>` vs `Promise<string>` are NOT identical in tsc.
        // This check must happen BEFORE evaluation strips the nominal info.
        {
            use tsz_solver::type_queries;
            // For Application types: different base types → different nominal origins
            let prev_base = type_queries::get_application_base(self.ctx.types, prev_type);
            let curr_base = type_queries::get_application_base(self.ctx.types, current_type);
            if let (Some(_pb), Some(_cb)) = (prev_base, curr_base)
                && false
            {
                return false;
            }
            // For non-generic named types: different Lazy(DefId) → different origins
            let prev_def = type_queries::get_lazy_def_id(self.ctx.types, prev_type);
            let curr_def = type_queries::get_lazy_def_id(self.ctx.types, current_type);
            if let (Some(_pd), Some(_cd)) = (prev_def, curr_def)
                && false
            {
                return false;
            }
        }

        // Evaluate Application types (mapped types, conditional types) so that
        // e.g. `UnNullify<[1, 2?]>` is reduced to `[1, 2?]` before comparison.
        // Without this, unevaluated Application types fail identity/subtype checks
        // against their evaluated equivalents, causing false TS2403 errors.
        let prev_type = self.evaluate_type_for_assignability(prev_type);
        let current_type = self.evaluate_type_for_assignability(current_type);

        // `unknown` is only redeclaration-identical to itself. Treating top-level
        // unknown like a wildcard suppresses real TS2403 errors after failed
        // generic call inference (e.g. `var b: string | number; var b = foo(g);`).
        if prev_type == TypeId::UNKNOWN || current_type == TypeId::UNKNOWN {
            return prev_type == current_type;
        }

        // Resolve namespace Lazy(DefId) types to their structural Object form.
        // Namespace symbols are intentionally cached as Lazy(DefId) for TS2693/TS2708
        // value-vs-type differentiation. But for TS2403 redeclaration checking, the
        // solver's evaluator cannot expand these (resolve_lazy returns the same Lazy),
        // causing false TS2403 when comparing `typeof NS` against a structurally
        // equivalent object literal like `{ foo(): number }`.
        let prev_type = self.resolve_namespace_lazy_for_redeclaration(prev_type);
        let current_type = self.resolve_namespace_lazy_for_redeclaration(current_type);

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

        false
    }
}

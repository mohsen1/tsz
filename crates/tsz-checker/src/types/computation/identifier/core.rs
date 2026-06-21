//! Core identifier type computation — `get_type_of_identifier` and its
//! direct helpers (TDZ, definite assignment, flow narrowing).

use crate::context::TypingRequest;
use crate::query_boundaries::common as common_query;
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
// (AliasCycleTracker is also used by the declaration-boundary projection below.)
use tracing::trace;
use tsz_binder::symbol_flags;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn identifier_is_property_access_receiver(&self, idx: NodeIndex) -> bool {
        let Some(ext) = self.ctx.arena.get_extended(idx) else {
            return false;
        };
        let Some(parent_node) = self.ctx.arena.get(ext.parent) else {
            return false;
        };
        matches!(
            parent_node.kind,
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        ) && self
            .ctx
            .arena
            .get_access_expr(parent_node)
            .is_some_and(|access| access.expression == idx)
    }

    pub(super) fn should_preserve_declared_generic_index_access_for_fresh_flow(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        declared_type: TypeId,
        flow_type: TypeId,
    ) -> bool {
        if !self.symbol_value_decl_has_explicit_type_annotation(sym_id) {
            return false;
        }

        let Some((_, index_type)) = common_query::index_access_parts(self.ctx.types, declared_type)
        else {
            return false;
        };

        let has_type_parameter_key =
            common_query::is_type_parameter_like(self.ctx.types, index_type)
                || common_query::is_type_parameter_like(
                    self.ctx.types,
                    self.resolve_lazy_type(index_type),
                );
        has_type_parameter_key
            && (flow_type == TypeId::ANY
                || common_query::is_fresh_object_type(self.ctx.types, flow_type))
    }

    pub(super) fn import_equals_alias_value_type(
        &mut self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(symbol_flags::ALIAS) {
            return None;
        }

        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .iter()
                .copied()
                .find(|idx| idx.is_some())
                .unwrap_or(NodeIndex::NONE)
        };
        if !decl_idx.is_some() {
            return None;
        }
        let decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return None;
        }
        let import = self.ctx.arena.get_import_decl(decl_node)?;
        if let Some(module_specifier) = self.get_require_module_specifier(import.module_specifier) {
            if let Some(exports) = self
                .resolve_effective_module_exports_from_file(
                    &module_specifier,
                    Some(self.ctx.current_file_idx),
                )
                .or_else(|| self.resolve_effective_module_exports(&module_specifier))
                && let Some(export_equals_sym) = exports.get("export=")
            {
                let mut candidates = Vec::new();

                if let Some(export_equals_symbol) = self.get_cross_file_symbol(export_equals_sym)
                    && export_equals_symbol.has_any_flags(symbol_flags::ALIAS)
                {
                    let mut visited = AliasCycleTracker::new();
                    if let Some(resolved) =
                        self.resolve_alias_symbol(export_equals_sym, &mut visited)
                    {
                        candidates.push(resolved);
                    }
                }
                candidates.push(export_equals_sym);

                for candidate in candidates {
                    let ty = self.get_type_of_symbol(candidate);
                    if ty != TypeId::UNKNOWN && ty != TypeId::ERROR {
                        return Some(ty);
                    }
                }
            }

            return self
                .commonjs_module_value_type(&module_specifier, Some(self.ctx.current_file_idx));
        }

        let target_sym = self.resolve_qualified_symbol(import.module_specifier)?;
        let mut candidates = Vec::new();
        if let Some(partner) = self.ctx.alias_partner_for(self.ctx.binder, target_sym) {
            candidates.push(partner);
        }

        let mut visited = AliasCycleTracker::new();
        if let Some(resolved) = self.resolve_alias_symbol(target_sym, &mut visited) {
            if let Some(partner) = self.ctx.alias_partner_for(self.ctx.binder, resolved) {
                candidates.push(partner);
            }
            candidates.push(resolved);
        }
        candidates.push(target_sym);

        let lib_binders = self.get_lib_binders();
        let mut seen = rustc_hash::FxHashSet::default();
        for candidate in candidates {
            if !seen.insert(candidate) {
                continue;
            }
            let Some(candidate_symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(candidate, &lib_binders)
            else {
                continue;
            };
            if !candidate_symbol.has_any_flags(symbol_flags::VALUE | symbol_flags::ALIAS)
                && candidate_symbol.value_declaration.is_none()
            {
                continue;
            }

            let mut ty = candidate_symbol
                .value_declaration
                .is_some()
                .then(|| {
                    self.ctx
                        .arena
                        .get(candidate_symbol.value_declaration)
                        .and_then(|node| self.ctx.arena.get_variable_declaration(node))
                        .map(|decl| decl.initializer)
                        .filter(|initializer| initializer.is_some())
                })
                .flatten()
                .map(|initializer| self.get_type_of_node(initializer))
                .unwrap_or(TypeId::UNKNOWN);
            if (ty == TypeId::UNKNOWN || ty == TypeId::ERROR)
                && candidate_symbol.value_declaration.is_some()
                && candidate_symbol.has_any_flags(symbol_flags::VALUE)
            {
                ty = if self
                    .ctx
                    .arena
                    .get(candidate_symbol.value_declaration)
                    .is_some()
                {
                    self.type_of_value_declaration_for_symbol(
                        candidate,
                        candidate_symbol.value_declaration,
                    )
                } else if let Some(file_idx) = self.ctx.resolve_symbol_file_index(candidate) {
                    self.type_of_value_declaration_for_cross_file_symbol(
                        candidate,
                        candidate_symbol.value_declaration,
                        file_idx,
                    )
                } else {
                    TypeId::UNKNOWN
                };
            }
            if ty == TypeId::UNKNOWN || ty == TypeId::ERROR {
                ty = self.get_type_of_symbol(candidate);
            }
            if ty != TypeId::UNKNOWN && ty != TypeId::ERROR {
                return Some(ty);
            }
        }

        None
    }

    pub(crate) fn same_file_value_symbol_for_type_symbol(
        &self,
        type_sym_id: tsz_binder::SymbolId,
    ) -> Option<(tsz_binder::SymbolId, NodeIndex, usize)> {
        let type_symbol = self.get_symbol_globally(type_sym_id)?;
        if (type_symbol.flags & tsz_binder::symbol_flags::VALUE) != 0 {
            return None;
        }
        let file_idx = self.ctx.resolve_symbol_file_index(type_sym_id)?;
        let binder = self.ctx.get_binder_for_file(file_idx)?;
        for &candidate_id in binder
            .get_symbols()
            .find_all_by_name(&type_symbol.escaped_name)
        {
            if candidate_id == type_sym_id {
                continue;
            }
            let Some(candidate) = binder.get_symbol(candidate_id) else {
                continue;
            };
            if candidate.escaped_name != type_symbol.escaped_name
                || (candidate.flags & tsz_binder::symbol_flags::VALUE) == 0
                || (candidate.flags & tsz_binder::symbol_flags::ALIAS) != 0
                || candidate.import_module().is_some()
                || !candidate.value_declaration.is_some()
            {
                continue;
            }
            self.ctx.register_symbol_file_target(candidate_id, file_idx);
            return Some((candidate_id, candidate.value_declaration, file_idx));
        }
        None
    }

    /// Select the value-side declaration of a name-merged `TYPE_ALIAS` + value
    /// symbol.
    ///
    /// When a `TYPE_ALIAS` is declared before its merged value (e.g.
    /// `type Foo = ...; const Foo: any;`), the binder records the type-alias
    /// node as `value_declaration`. Typing that type-alias node in value
    /// position yields a possibly-undefined type. Return the first declaration
    /// whose kind is not a type-alias declaration so the actual value
    /// declaration drives value-position typing. Returns `None` when every
    /// declaration is a type-alias (or none can be read from the target arena),
    /// leaving the caller to fall back to the recorded `value_declaration`.
    pub(crate) fn value_declaration_skipping_type_alias(
        &self,
        target: &tsz_binder::Symbol,
        target_file_idx: usize,
    ) -> Option<NodeIndex> {
        // If the recorded value declaration is already a non-type-alias node,
        // it is the value side; keep it (matches the const-first merge order).
        let arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let is_type_alias_node = |decl: NodeIndex| {
            arena
                .get(decl)
                .is_some_and(|node| node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION)
        };
        if target.value_declaration.is_some() && !is_type_alias_node(target.value_declaration) {
            return Some(target.value_declaration);
        }
        target
            .declarations
            .iter()
            .copied()
            .find(|&decl| decl.is_some() && !is_type_alias_node(decl))
    }

    pub(crate) fn local_current_file_value_symbol_named(
        &self,
        name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        self.ctx
            .binder
            .get_symbols()
            .find_all_by_name(name)
            .iter()
            .copied()
            .find(|&candidate_id| {
                self.ctx
                    .binder
                    .get_symbol(candidate_id)
                    .is_some_and(|candidate| {
                        candidate.has_any_flags(symbol_flags::VALUE)
                            && candidate.value_declaration.is_some()
                            && (candidate.decl_file_idx == u32::MAX
                                || candidate.decl_file_idx == self.ctx.current_file_idx as u32)
                    })
            })
    }

    pub(super) fn has_recursive_alias_shape_for_flow_compare(&self, type_id: TypeId) -> bool {
        common_query::contains_lazy_or_recursive(self.ctx.types.as_type_database(), type_id)
    }

    /// Get the type of an identifier expression.
    ///
    /// This function resolves the type of an identifier by:
    /// 1. Looking up the symbol through the binder
    /// 2. Getting the declared type of the symbol
    /// 3. Checking for TDZ (temporal dead zone) violations
    /// 4. Checking definite assignment for block-scoped variables
    /// 5. Applying flow-based type narrowing
    ///
    /// ## Symbol Resolution:
    /// - Uses `resolve_identifier_symbol` to find the symbol
    /// - Checks for type-only aliases (error if used as value)
    /// - Validates that symbol has a value declaration
    ///
    /// ## TDZ Checking:
    /// - Static block TDZ: variable used in static block before declaration
    /// - Computed property TDZ: variable in computed property before declaration
    /// - Heritage clause TDZ: variable in extends/implements before declaration
    ///
    /// ## Definite Assignment:
    /// - Checks if variable is definitely assigned before use
    /// - Only applies to block-scoped variables without initializers
    /// - Skipped for parameters, ambient contexts, and captured variables
    ///
    /// ## Flow Narrowing:
    /// - If definitely assigned, applies type narrowing based on control flow
    /// - Refines union types based on typeof guards, null checks, etc.
    ///
    /// ## Intrinsic Names:
    /// - `undefined` → UNDEFINED type
    /// - `NaN` / `Infinity` → NUMBER type
    /// - `Symbol` → Symbol constructor type (if available in lib)
    ///
    /// ## Global Value Names:
    /// - Returns ANY for available globals (Array, Object, etc.)
    /// - Emits error for unavailable ES2015+ types
    ///
    /// ## Error Handling:
    /// - Returns ERROR for:
    ///   - Type-only aliases used as values
    ///   - Variables used before declaration (TDZ)
    ///   - Variables not definitely assigned
    ///   - Static members accessed without `this`
    ///   - `await` in default parameters
    ///   - Unresolved names (with "cannot find name" error)
    /// - Returns ANY for unresolved imports (TS2307 already emitted)
    pub(crate) fn get_type_of_identifier(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_identifier_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_identifier_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(ident) = self.ctx.arena.get_identifier(node) else {
            return TypeId::ERROR; // Missing identifier data - propagate error
        };

        let name = &ident.escaped_text;

        self.check_identifier_strict_mode_reserved(idx, name);

        if name == "arguments"
            && let Some(result) = self.arguments_identifier_type(idx, name)
        {
            return result;
        }

        // === CRITICAL FIX: Check type parameter scope FIRST ===
        // Type parameters in generic functions/classes/type aliases should be
        // resolved before checking any other scope. This is a common source of
        // TS2304 false positives.
        if let Some(result) = self.type_parameter_identifier_value(idx, name) {
            return result;
        }

        // Resolve via binder persistent scopes for stateless lookup.
        if let Some(sym_id) = self.resolve_identifier_symbol(idx) {
            let value_type = self.type_of_resolved_value_symbol(idx, request, sym_id, name);
            return self.project_declaration_boundary_value(sym_id, value_type);
        }

        self.resolve_unresolved_identifier(idx, name)
    }

    /// Project `any` to `unknown` across the declaration trust boundary when a
    /// value declared in an external declaration file (`.d.ts`, a default lib,
    /// or a `node_modules` package) is observed by sound user code.
    ///
    /// Off unless both `sound_mode` and `sound_declaration_projection` are set;
    /// a no-op for current-file/user-authored values. The polarity-aware type
    /// transform itself is owned by the solver
    /// (`tsz_solver::operations::declaration_projection`); the checker only
    /// supplies the trust-boundary policy (issue #8533).
    fn project_declaration_boundary_value(
        &self,
        sym_id: tsz_binder::SymbolId,
        value_type: TypeId,
    ) -> TypeId {
        if !self.ctx.compiler_options.sound_mode
            || !self.ctx.compiler_options.sound_declaration_projection
        {
            return value_type;
        }
        // Only sound user code observes the boundary: a declaration file reading
        // its own surfaces is not crossing into sound source.
        if self.ctx.is_declaration_file() {
            return value_type;
        }
        // Imported bindings are alias symbols declared in the importing file;
        // follow the alias to the symbol that actually declares the surface so
        // the boundary is judged by the *target* file, not the import site.
        let mut visited = AliasCycleTracker::new();
        let target_sym = self
            .resolve_alias_symbol(sym_id, &mut visited)
            .unwrap_or(sym_id);
        // The boundary is "this value's type is declared in an external
        // declaration file". Resolve from the order-independent declaring-file
        // index so the projection is schedule-stable.
        let declaring_file = self.ctx.resolve_symbol_declaring_file_index(target_sym);
        if !self.query_file_is_declaration_file(declaring_file) {
            return value_type;
        }
        tsz_solver::operations::declaration_projection::project_declaration_boundary(
            self.ctx.types,
            value_type,
            tsz_solver::operations::declaration_projection::Polarity::Covariant,
        )
    }

    /// TS1212: emit when a strict-mode reserved word is used in an expression.
    ///
    /// Declaration-site TS1212 is handled separately in variable/parameter
    /// checking. We emit here but do not return early — the identifier may
    /// still resolve as a value.
    fn check_identifier_strict_mode_reserved(&mut self, idx: NodeIndex, name: &str) {
        if crate::state_checking::is_strict_mode_reserved_name(name)
            && self.is_strict_mode_for_node(idx)
            && self.ctx.checking_computed_property_name.is_none()
        {
            self.emit_strict_mode_reserved_word_error(idx, name, true);
        }
    }

    /// Resolve a bare `arguments` reference, emitting the ES5/async/static-block
    /// diagnostics. Returns `Some(_)` when `arguments` resolves (or is rejected)
    /// and `None` when it should fall through to normal symbol resolution
    /// (e.g. a local variable named `arguments` shadows the built-in).
    fn arguments_identifier_type(&mut self, idx: NodeIndex, name: &str) -> Option<TypeId> {
        // Track that this function body uses `arguments` (for JS implicit rest params)
        self.ctx.js_body_uses_arguments = true;

        // TS2496: 'arguments' cannot be referenced in an arrow function in ES5.
        // Fires when `arguments` is inside an arrow that captures it from an outer
        // scope. Does NOT fire when `arguments` is a parameter of the immediate arrow
        // (e.g., `(arguments) => arguments`). tsc emits this and continues.
        if self.ctx.compiler_options.target.is_es5() && self.is_arguments_captured_by_arrow(idx) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                    idx,
                    diagnostic_messages::THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ARROW_FUNCTION_IN_ES5_CONSIDER_U,
                    diagnostic_codes::THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ARROW_FUNCTION_IN_ES5_CONSIDER_U,
                );
        }

        // TS2815: 'arguments' cannot be referenced in property initializers
        // or class static initialization blocks. Must check BEFORE regular
        // function body check because arrow functions are transparent.
        if self.is_arguments_in_class_initializer_or_static_block(idx) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                    idx,
                    diagnostic_messages::ARGUMENTS_CANNOT_BE_REFERENCED_IN_PROPERTY_INITIALIZERS_OR_CLASS_STATIC_INITIALI,
                    diagnostic_codes::ARGUMENTS_CANNOT_BE_REFERENCED_IN_PROPERTY_INITIALIZERS_OR_CLASS_STATIC_INITIALI,
                );
            return Some(TypeId::ERROR);
        }

        // Check if there's a local variable named "arguments" that shadows the built-in.
        // If so, fall through to normal resolution.
        let has_local_shadow = if self.is_in_regular_function_body(idx) {
            if let Some(sym_id) = self.resolve_identifier_symbol(idx) {
                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                    && !symbol.declarations.is_empty()
                {
                    let decl_node = symbol.declarations[0];
                    if let Some(current_fn) = self.find_enclosing_function(idx)
                        && let Some(decl_fn) = self.find_enclosing_function(decl_node)
                        && current_fn == decl_fn
                    {
                        trace!(
                            name = name,
                            idx = ?idx,
                            sym_id = ?sym_id,
                            "get_type_of_identifier: local 'arguments' variable shadows built-in IArguments"
                        );
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        // TS2522: 'arguments' cannot be referenced in an async function or
        // method in ES5. Arrow functions are transparent for `arguments`,
        // so this checks the nearest non-arrow function boundary.
        if !has_local_shadow
            && self.ctx.compiler_options.target.is_es5()
            && self.is_arguments_in_async_function_or_method(idx)
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                    idx,
                    diagnostic_messages::THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5,
                    diagnostic_codes::THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5,
                );
        }

        // If not shadowed by a local variable, resolve to the built-in IArguments type.
        // This handles both regular functions and arrow functions (which are transparent
        // for `arguments` — they capture from the enclosing regular function).
        // At global scope or in type contexts (interfaces, type aliases), `arguments`
        // is not valid and should fall through to normal resolution (emitting TS2304).
        if !has_local_shadow && self.has_enclosing_regular_function(idx) {
            let lib_binders = self.get_lib_binders();
            if let Some(iargs_sym) = self
                .ctx
                .binder
                .get_global_type_with_libs("IArguments", &lib_binders)
            {
                return Some(self.type_reference_symbol_type(iargs_sym));
            }
            return Some(TypeId::ANY);
        }
        None
    }

    /// Resolve an identifier that names an in-scope type parameter in value
    /// position. Returns `Some(_)` when the type parameter decides the result
    /// (TS2693 / heritage TS2304) and `None` when an outer value binding should
    /// take precedence (fall through to binder resolution).
    fn type_parameter_identifier_value(&mut self, idx: NodeIndex, name: &str) -> Option<TypeId> {
        let type_id = self.lookup_type_parameter(name)?;
        // `A` shadows the type parameter `A` in value position.
        let has_value_shadow = self
            .resolve_identifier_symbol(idx)
            .and_then(|sym_id| {
                self.ctx
                    .binder
                    .get_symbol(sym_id)
                    .map(|s| s.has_any_flags(tsz_binder::symbol_flags::VALUE))
            })
            .unwrap_or(false);
        if !has_value_shadow {
            // The closest binder symbol has no VALUE flag (it's the type parameter
            // itself). But type parameters only shadow in type contexts — in value
            // contexts, an outer-scope value binding (e.g., a class) should be
            // accessible. Check if there's a VALUE symbol with the same name by
            // re-resolving while skipping TYPE_PARAMETER-only symbols.
            let lib_binders = self.get_lib_binders();
            let has_outer_value = self
                .ctx
                .binder
                .resolve_identifier_with_filter(self.ctx.arena, idx, &lib_binders, |sym_id| {
                    self.ctx
                        .binder
                        .get_symbol_with_libs(sym_id, &lib_binders)
                        .is_some_and(|s| {
                            // Skip symbols that are ONLY type parameters.
                            // Accept VALUE symbols and non-type-only ALIAS symbols
                            // (e.g., `import * as E from "mod"` provides a runtime
                            // namespace object).
                            s.has_any_flags(tsz_binder::symbol_flags::VALUE)
                                || (s.has_any_flags(tsz_binder::symbol_flags::ALIAS)
                                    && !s.is_type_only)
                        })
                })
                .is_some();
            if has_outer_value {
                // Fall through to binder resolution — the outer value takes
                // precedence over the type parameter in expression context.
            } else {
                // In heritage expression positions (`class C<T> extends T {}`),
                // tsc reports TS2304 instead of TS2693 for type parameters.
                if self.is_direct_heritage_type_reference(idx) {
                    if self.is_heritage_type_only_context(idx) {
                        return Some(TypeId::ERROR);
                    }
                    // Route through boundary for TS2304/TS2552 with suggestion collection
                    self.report_not_found_at_boundary(
                        name,
                        idx,
                        crate::query_boundaries::name_resolution::NameLookupKind::Value,
                    );
                    return Some(TypeId::ERROR);
                }
                // TS2693: Type parameters cannot be used as values
                // Example: function f<T>() { return T; }  // Error: T is a type, not a value
                self.error_type_parameter_used_as_value(name, idx);
                return Some(type_id);
            }
        }
        // Fall through to binder resolution — the value symbol takes precedence
        None
    }
}

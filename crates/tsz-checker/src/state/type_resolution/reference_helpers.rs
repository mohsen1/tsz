//! Type reference resolution helpers: array types, simple type references,
//! type parameter extraction, and class instance type construction.

use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn get_reference_type_params_for_symbol(
        &mut self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> Vec<tsz_solver::TypeParamInfo> {
        let declared = self.extract_declared_type_params_for_reference_symbol(sym_id, expected_name);
        if !declared.is_empty() {
            return declared;
        }
        self.get_display_type_params_for_symbol(sym_id)
    }

    pub(crate) fn count_required_reference_type_params(
        &mut self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> usize {
        let declared = self.extract_declared_type_params_for_reference_symbol(sym_id, expected_name);
        if !declared.is_empty() {
            return declared.iter().filter(|param| param.default.is_none()).count();
        }
        self.count_required_type_params(sym_id)
    }

    pub(crate) fn symbol_has_declared_type_meaning(&self, sym_id: SymbolId) -> bool {
        let lib_binders = self.get_lib_binders();
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
            return false;
        };

        if (symbol.flags & symbol_flags::ALIAS) == 0 && (symbol.flags & symbol_flags::TYPE) != 0 {
            return true;
        }

        symbol.declarations.iter().copied().any(|decl_idx| {
            let arena = self
                .ctx
                .binder
                .get_arena_for_declaration(sym_id, decl_idx)
                .map_or(self.ctx.arena, |arena| arena.as_ref());
            arena.get(decl_idx).is_some_and(|node| {
                node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                    || node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || node.kind == syntax_kind_ext::ENUM_DECLARATION
            })
        })
    }

    /// Resolve `Array<T>`, `ReadonlyArray<T>`, or `ConcatArray<T>` without explicit type arguments.
    pub(crate) fn resolve_array_type_reference(
        &mut self,
        name: &str,
        type_name_idx: NodeIndex,
        type_ref: &tsz_parser::parser::node::TypeRefData,
    ) -> TypeId {
        let factory = self.ctx.types.factory();
        if let Some(type_id) = self.resolve_named_type_reference(name, type_name_idx) {
            return type_id;
        }
        if !self.ctx.has_lib_loaded() {
            self.error_cannot_find_global_type(name, type_name_idx);
            if let Some(args) = &type_ref.type_arguments {
                for &arg_idx in &args.nodes {
                    let _ = self.get_type_from_type_node(arg_idx);
                }
            }
            return TypeId::ERROR;
        }
        let elem_type = type_ref
            .type_arguments
            .as_ref()
            .and_then(|args| args.nodes.first().copied())
            .map_or(TypeId::ERROR, |idx| self.get_type_from_type_node(idx));
        let array_type = factory.array(elem_type);
        if name == "ReadonlyArray" {
            factory.readonly_type(array_type)
        } else {
            array_type
        }
    }

    /// Resolve a simple (non-array-like, non-primitive) type reference without type arguments.
    /// Handles generic validation, default type arguments, and error reporting.
    pub(crate) fn resolve_simple_type_reference(
        &mut self,
        idx: NodeIndex,
        type_name_idx: NodeIndex,
        name: &str,
        type_ref: &tsz_parser::parser::node::TypeRefData,
    ) -> TypeId {
        let factory = self.ctx.types.factory();
        if name != "Array" && name != "ReadonlyArray" && name != "ConcatArray" {
            match self.resolve_identifier_symbol_in_type_position(type_name_idx) {
                TypeSymbolResolution::Type(sym_id) => {
                    self.check_for_static_member_class_type_param_reference(sym_id, type_name_idx);
                    if self.ctx.has_lib_loaded() && self.ctx.symbol_is_from_lib(sym_id) {
                        self.prime_lib_type_params(name);
                    }
                    if self.symbol_is_namespace_only(sym_id) {
                        self.error_namespace_used_as_type_at(name, type_name_idx);
                        return TypeId::ERROR;
                    }
                    let type_params = self.get_reference_type_params_for_symbol(sym_id, name);
                    if !type_params.is_empty() {
                        self.ctx
                            .get_or_create_def_id_with_params(sym_id, type_params.clone());
                    }
                    // Use AST-level check first to avoid self-referential default
                    // resolution issues (e.g., `interface SelfRef<T = SelfRef> {}`).
                    let required_count = self
                        .count_required_type_params_from_ast(sym_id)
                        .filter(|_| !type_params.is_empty())
                        .unwrap_or_else(|| {
                            self.count_required_reference_type_params(sym_id, name)
                        });
                    if required_count > 0
                        // Skip TS2314 for self-references within the same type alias.
                        // TSC handles circular self-references (e.g. `type T1<X> = T1`)
                        // through its circularity detection path instead of emitting
                        // "Generic type requires N type argument(s)".
                        && !self.ctx.symbol_resolution_set.contains(&sym_id)
                    {
                        // tsc uses the original declaration name, not the local alias.
                        // e.g., `export type { A as B }` → `let d: B` reports 'A<T>', not 'B<T>'.
                        // Resolve through aliases to get the target symbol's name.
                        let resolved_name = {
                            let mut visited_aliases = Vec::new();
                            self.resolve_alias_symbol(sym_id, &mut visited_aliases)
                                .and_then(|target| {
                                    self.get_symbol_globally(target)
                                        .map(|s| s.escaped_name.clone())
                                })
                                .unwrap_or_else(|| name.to_string())
                        };
                        let display_name = Self::format_generic_display_name_with_interner(
                            &resolved_name,
                            &type_params,
                            self.ctx.types,
                        );
                        if required_count < type_params.len() {
                            // TS2707: Generic type 'X<T, U, V>' requires between N and M type arguments.
                            let min_str = required_count.to_string();
                            let max_str = type_params.len().to_string();
                            self.error_at_node_msg(
                                idx,
                                crate::diagnostics::diagnostic_codes::GENERIC_TYPE_REQUIRES_BETWEEN_AND_TYPE_ARGUMENTS,
                                &[&display_name, &min_str, &max_str],
                            );
                        } else {
                            self.error_generic_type_requires_type_arguments_at(
                                &display_name,
                                required_count,
                                idx,
                            );
                        }
                        // tsc returns errorType when a generic type is used without
                        // required type arguments. This prevents cascading errors
                        // like TS2454 on variables with erroneous type annotations.
                        return TypeId::ERROR;
                    }
                    // Apply default type arguments if no explicit args were provided
                    if type_ref
                        .type_arguments
                        .as_ref()
                        .is_none_or(|args| args.nodes.is_empty())
                    {
                        let has_defaults = type_params.iter().any(|p| p.default.is_some());
                        if has_defaults {
                            let default_args: Vec<TypeId> =
                                tsz_solver::resolve_default_type_args(self.ctx.types, &type_params);
                            let def_id = self.ctx.get_or_create_def_id(sym_id);
                            // Resolve the type alias body so its type params and body
                            // are registered in type_env. Without this, Application
                            // expansion via try_expand_application fails because
                            // resolve_lazy(def_id) returns None (body not registered).
                            // This is critical for cross-file generic constraints like
                            // `TBase extends Constructor` where Constructor<T = {}>.
                            let _ = self.get_type_of_symbol(sym_id);
                            let base_type_id = factory.lazy(def_id);
                            return factory.application(base_type_id, default_args);
                        }
                    }
                }
                TypeSymbolResolution::ValueOnly(sym_id) => {
                    self.report_wrong_meaning(
                        name,
                        type_name_idx,
                        sym_id,
                        crate::query_boundaries::name_resolution::NameLookupKind::Value,
                        crate::query_boundaries::name_resolution::NameLookupKind::Type,
                    );
                    return TypeId::ERROR;
                }
                TypeSymbolResolution::NotFound => {}
            }
        }

        // Create DefIds for type aliases (enables DefId-based resolution)
        if let TypeSymbolResolution::Type(sym_id) =
            self.resolve_identifier_symbol_in_type_position(type_name_idx)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.flags & symbol_flags::TYPE_ALIAS != 0
        {
            let _def_id = self.ctx.get_or_create_def_id(sym_id);
        }

        if let Some(type_id) = self.resolve_named_type_reference(name, type_name_idx) {
            return type_id;
        }
        if let Some((body_type, type_params)) = self.resolve_global_jsdoc_typedef_info(name) {
            if let Some(args) = &type_ref.type_arguments {
                let display_name = Self::format_generic_display_name_with_interner(
                    name,
                    &type_params,
                    self.ctx.types,
                );
                if !self.is_inside_type_parameter_declaration(type_name_idx)
                    && self.validate_jsdoc_type_reference_type_arguments_against_params(
                        &type_params,
                        args,
                        type_name_idx,
                        &display_name,
                    )
                {
                    return TypeId::ERROR;
                }

                let type_args: Vec<TypeId> = args
                    .nodes
                    .iter()
                    .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                    .collect();
                if !type_params.is_empty() && !type_args.is_empty() {
                    return tsz_solver::instantiate_generic(
                        self.ctx.types,
                        body_type,
                        &type_params,
                        &type_args,
                    );
                }
            } else {
                let required_count = type_params
                    .iter()
                    .filter(|param| param.default.is_none())
                    .count();
                if required_count > 0 {
                    let display_name = Self::format_generic_display_name_with_interner(
                        name,
                        &type_params,
                        self.ctx.types,
                    );
                    self.error_generic_type_requires_type_arguments_at(
                        &display_name,
                        required_count,
                        type_name_idx,
                    );
                    return TypeId::ERROR;
                }
            }
            return body_type;
        }
        if name == "await" {
            self.error_cannot_find_name_did_you_mean_at(name, "Awaited", type_name_idx);
            return TypeId::ERROR;
        }
        if self.is_known_global_type_name(name) {
            self.error_cannot_find_global_type(name, type_name_idx);
            return TypeId::ERROR;
        }
        if self.is_unresolved_import_symbol(type_name_idx) {
            return TypeId::ANY;
        }
        // Route through boundary for TS2304/TS2552 with spelling suggestions
        let _ = self.resolve_type_name_or_report(name, type_name_idx);
        TypeId::ERROR
    }

    /// Ensure a DefId has its type parameters cached and body registered before lowering.
    ///
    /// This is the stable-identity helper for the "prime `DefId` before `TypeLowering`"
    /// pattern.  It consolidates the ad hoc inline block that manually iterated
    /// declarations to find type parameters and then checked body registration.
    ///
    /// Steps:
    /// 1. Get or create a DefId for the symbol.
    /// 2. If type params are not yet cached, extract them from AST declarations
    ///    (via [`extract_declared_type_params_for_reference_symbol`]) and register.
    /// 3. For lib types, ensure the body is resolved so `resolve_lazy` succeeds.
    ///
    /// Returns the DefId ready for use in `Lazy(DefId)`.
    pub(crate) fn ensure_def_ready_for_lowering(
        &mut self,
        sym_id: SymbolId,
        name: &str,
    ) -> tsz_solver::def::DefId {
        let def_id = self.ctx.get_or_create_def_id(sym_id);

        // Step 2: extract and cache type parameters if not already cached.
        if self.ctx.get_def_type_params(def_id).is_none() {
            let params = self.extract_declared_type_params_for_reference_symbol(sym_id, name);
            if !params.is_empty() {
                self.ctx.insert_def_type_params(def_id, params);
            } else if !self.ctx.lib_contexts.is_empty() {
                // Not found in the file arena — try lib types which populates
                // both body and type params in the type environment.
                let _ = self.resolve_lib_type_by_name(name);
            }
        }

        // Step 3: ensure the body is registered in type_env for lib generic types
        // so that the solver's resolve_lazy can perform property access with
        // type parameter substitution.
        if self.ctx.get_def_type_params(def_id).is_some() && !self.ctx.lib_contexts.is_empty() {
            let has_body = self
                .ctx
                .type_env
                .try_borrow()
                .map(|env| env.get_def(def_id).is_some())
                .unwrap_or(false);
            if !has_body {
                let _ = self.resolve_lib_type_by_name(name);
            }
        }

        def_id
    }

    pub(crate) fn extract_declared_type_params_for_reference_symbol(
        &mut self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> Vec<tsz_solver::TypeParamInfo> {
        let Some(symbol) = self.get_symbol_globally(sym_id) else {
            return Vec::new();
        };
        let declarations = symbol.declarations.clone();
        let mixed_class_interface = (symbol.flags & symbol_flags::CLASS) != 0
            && (symbol.flags & symbol_flags::INTERFACE) != 0;

        for &decl_idx in &declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            if let Some(type_alias) = self.ctx.arena.get_type_alias(node) {
                if let Some(name_node) = self.ctx.arena.get(type_alias.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    && ident.escaped_text != expected_name
                {
                    continue;
                }
                let (params, updates) = self.push_type_parameters(&type_alias.type_parameters);
                self.pop_type_parameters(updates);
                if !params.is_empty() {
                    return params;
                }
            }

            if let Some(iface) = self.ctx.arena.get_interface(node) {
                if let Some(name_node) = self.ctx.arena.get(iface.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    && ident.escaped_text != expected_name
                {
                    continue;
                }
                let (params, updates) = self.push_type_parameters(&iface.type_parameters);
                self.pop_type_parameters(updates);
                if !params.is_empty() {
                    return params;
                }
            }

            if !mixed_class_interface && let Some(class) = self.ctx.arena.get_class(node) {
                if let Some(name_node) = self.ctx.arena.get(class.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    && ident.escaped_text != expected_name
                {
                    continue;
                }
                let (params, updates) = self.push_type_parameters(&class.type_parameters);
                self.pop_type_parameters(updates);
                if !params.is_empty() {
                    return params;
                }
            }
        }

        Vec::new()
    }

    pub(crate) fn symbol_is_namespace_only(&self, sym_id: SymbolId) -> bool {
        let lib_binders = self.get_lib_binders();
        if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
            if symbol.flags & symbol_flags::ALIAS != 0 {
                if self.symbol_has_declared_type_meaning(sym_id) {
                    return false;
                }

                let mut visited = Vec::new();
                let target_sym_id = self.resolve_alias_symbol(sym_id, &mut visited);

                if matches!(symbol.import_name.as_deref(), Some("*")) && target_sym_id.is_some() {
                    return true;
                }

                if let Some(target_sym_id) = target_sym_id
                    && target_sym_id != sym_id
                {
                    return self.symbol_is_namespace_only(target_sym_id);
                }

                // For module-level imports (`import X = require('...')` or
                // `import * as X from '...'`), when the alias can't be resolved,
                // the symbol may represent a module namespace. These have import_module
                // set and use either no import_name or the synthetic `*` marker
                // because they import the whole module namespace.
                //
                // Only flag as namespace-only when the target module IS known in our
                // exports table (so we know its shape) but doesn't have `export =`.
                // If the module has `export =`, resolve_alias_symbol would have succeeded
                // above. If the module isn't in our exports table at all (unresolved
                // cross-file reference), we can't assume it's namespace-only.
                if let Some(ref module_name) = symbol.import_module
                    && matches!(symbol.import_name.as_deref(), None | Some("*"))
                    && self
                        .ctx
                        .binder
                        .module_exports
                        .contains_key(module_name.as_str())
                {
                    return true;
                }
            }

            let is_namespace = (symbol.flags
                & (symbol_flags::MODULE
                    | symbol_flags::NAMESPACE_MODULE
                    | symbol_flags::VALUE_MODULE))
                != 0;
            let has_type = self.symbol_has_declared_type_meaning(sym_id);
            return is_namespace && !has_type;
        }
        false
    }

    pub(crate) fn should_resolve_recursive_type_alias(
        &self,
        sym_id: SymbolId,
        type_args: &tsz_parser::parser::NodeList,
    ) -> bool {
        if !self.ctx.symbol_resolution_set.contains(&sym_id) {
            return true;
        }
        if self.ctx.symbol_resolution_stack.last().copied() != Some(sym_id) {
            return true;
        }
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return true;
        };

        // Check if this is a type alias (original behavior)
        if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
            return self.type_args_match_alias_params(sym_id, type_args);
        }

        // For classes and interfaces, allow recursive references in type parameter constraints
        // Don't force eager resolution - this prevents false cycle detection for patterns like:
        // class C<T extends C<T>>
        // interface I<T extends I<T>>
        if symbol.flags & (symbol_flags::CLASS | symbol_flags::INTERFACE) != 0 {
            // Only resolve if we're not in a direct self-reference scenario
            // The symbol_resolution_stack check above handles direct recursion
            return false;
        }

        // For other symbol types, use type args matching
        self.type_args_match_alias_params(sym_id, type_args)
    }

    pub(crate) fn type_args_match_alias_params(
        &self,
        sym_id: SymbolId,
        type_args: &tsz_parser::parser::NodeList,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if symbol.flags & symbol_flags::TYPE_ALIAS == 0 {
            return false;
        }

        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE)
        };
        if decl_idx.is_none() {
            return false;
        }
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        let Some(type_alias) = self.ctx.arena.get_type_alias(node) else {
            return false;
        };
        let Some(type_params) = &type_alias.type_parameters else {
            return false;
        };
        if type_params.nodes.len() != type_args.nodes.len() {
            return false;
        }

        for (&param_idx, &arg_idx) in type_params.nodes.iter().zip(type_args.nodes.iter()) {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                return false;
            };
            let Some(param_name) = self
                .ctx
                .arena
                .get(param.name)
                .and_then(|node| self.ctx.arena.get_identifier(node))
                .map(|ident| ident.escaped_text.as_str())
            else {
                return false;
            };

            let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                return false;
            };
            if arg_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                let Some(arg_ref) = self.ctx.arena.get_type_ref(arg_node) else {
                    return false;
                };
                if arg_ref
                    .type_arguments
                    .as_ref()
                    .is_some_and(|list| !list.nodes.is_empty())
                {
                    return false;
                }
                let Some(arg_name_node) = self.ctx.arena.get(arg_ref.type_name) else {
                    return false;
                };
                let Some(arg_ident) = self.ctx.arena.get_identifier(arg_name_node) else {
                    return false;
                };
                if arg_ident.escaped_text != param_name {
                    return false;
                }
            } else if arg_node.kind == SyntaxKind::Identifier as u16 {
                let Some(arg_ident) = self.ctx.arena.get_identifier(arg_node) else {
                    return false;
                };
                if arg_ident.escaped_text != param_name {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    pub(crate) fn class_instance_type_from_symbol(&mut self, sym_id: SymbolId) -> Option<TypeId> {
        if let Some(&instance_type) = self.ctx.symbol_instance_types.get(&sym_id) {
            return Some(instance_type);
        }
        self.class_instance_type_with_params_from_symbol(sym_id)
            .map(|(instance_type, _)| instance_type)
    }

    pub(crate) fn class_instance_type_with_params_from_symbol(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE)
        };
        // When the primary declaration doesn't resolve to a class in the current
        // arena (e.g., class+interface merged symbol where value_declaration was
        // not propagated through program-level symbol merging), search all
        // declarations for a class node in the current arena.
        // Guard against NodeIndex collisions: verify the class name matches
        // the symbol name to avoid picking up an unrelated class from the arena.
        if decl_idx.is_none() || self.ctx.arena.get_class_at(decl_idx).is_none() {
            let expected_name = &symbol.escaped_name;
            for &d in &symbol.declarations {
                if d.is_some()
                    && let Some(class) = self.ctx.arena.get_class_at(d)
                    && self
                        .ctx
                        .arena
                        .get(class.name)
                        .and_then(|n| self.ctx.arena.get_identifier(n))
                        .is_some_and(|ident| ident.escaped_text.as_str() == expected_name)
                {
                    decl_idx = d;
                    break;
                }
            }
        }
        if decl_idx.is_none() {
            return None;
        }
        if let Some(class) = self.ctx.arena.get_class_at(decl_idx) {
            let canonical_sym = self.ctx.binder.get_node_symbol(decl_idx);
            let active_class_sym = canonical_sym.unwrap_or(sym_id);
            // Check if we're already resolving this class - return fallback to break cycle.
            // Return a Lazy(DefId) placeholder so that the parameter type remains
            // dynamically resolvable.  During class building the Lazy resolves to
            // the partial instance type via class_instance_type_cache; after
            // building completes it resolves to the final type via
            // symbol_instance_types.
            if self.ctx.class_instance_resolution_set.contains(&sym_id)
                || canonical_sym
                    .is_some_and(|sym| self.ctx.class_instance_resolution_set.contains(&sym))
            {
                let fallback = self.ctx.create_lazy_type_ref(active_class_sym);
                return Some((fallback, Vec::new()));
            }

            let (params, updates) = self.push_type_parameters(&class.type_parameters);
            if let Some(&instance_type) = self
                .ctx
                .symbol_instance_types
                .get(&sym_id)
                .or_else(|| self.ctx.symbol_instance_types.get(&active_class_sym))
            {
                self.pop_type_parameters(updates);
                return Some((instance_type, params));
            }

            let instance_type = self.get_class_instance_type(decl_idx, class);
            self.ctx.symbol_instance_types.insert(sym_id, instance_type);
            if active_class_sym != sym_id {
                self.ctx
                    .symbol_instance_types
                    .insert(active_class_sym, instance_type);
            }

            // Register the class instance type in both type environments
            // immediately so that Lazy(DefId) fallbacks (created by the recursion
            // guard above) can resolve via resolve_lazy during property access
            // checks and flow-analyzer narrowing.
            let def_id = self.ctx.get_or_create_def_id(active_class_sym);
            self.ctx
                .register_class_instance_in_envs(def_id, instance_type);

            self.pop_type_parameters(updates);
            return Some((instance_type, params));
        }

        // Cross-file fallback: class declaration is not in the current arena.
        // Delegate to a child checker with the symbol's arena.
        self.delegate_cross_arena_class_instance_type(sym_id)
    }

    /// Check if a type alias declaration has a mapped type body that
    /// unconditionally references the alias with the same type arguments
    /// (e.g., `type Circular<T> = {[P in keyof T]: Circular<T>}`).
    /// Used for TS2589 detection. Bounded recursion like
    /// `type DeepMap<T, R> = {[K in keyof T]: T[K] extends unknown[] ? DeepMap<T[K], R> : R}`
    /// does NOT trigger this because the recursive call uses different args.
    pub(crate) fn alias_has_self_referencing_mapped_body(
        &self,
        sym_id: SymbolId,
        decl_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
            return false;
        }
        let Some(type_alias) = self.ctx.arena.get_type_alias(node) else {
            return false;
        };
        let Some(_body_node) = self.ctx.arena.get(type_alias.type_node) else {
            return false;
        };

        // Get the alias name and type parameter names
        let sym_name = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .map(|s| s.escaped_name.clone())
            .unwrap_or_default();
        let param_names: Vec<String> = type_alias
            .type_parameters
            .as_ref()
            .map(|tpl| {
                tpl.nodes
                    .iter()
                    .filter_map(|&param_idx| {
                        let param_node = self.ctx.arena.get(param_idx)?;
                        let param = self.ctx.arena.get_type_parameter(param_node)?;
                        let name_node = self.ctx.arena.get(param.name)?;
                        let ident = self.ctx.arena.get_identifier(name_node)?;
                        Some(self.ctx.arena.resolve_identifier_text(ident).to_string())
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Check if the body contains a self-referencing mapped type (recursively)
        self.body_contains_self_referencing_mapped(type_alias.type_node, &sym_name, &param_names)
    }

    /// Recursively check if a type node contains a mapped type that references
    /// the alias with the same type arguments.
    fn body_contains_self_referencing_mapped(
        &self,
        node_idx: NodeIndex,
        name: &str,
        param_names: &[String],
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        // Check if this node is a mapped type with self-reference in template
        if node.kind == syntax_kind_ext::MAPPED_TYPE {
            if let Some(mapped) = self.ctx.arena.get_mapped_type(node) {
                if self.template_has_identity_self_ref(mapped.type_node, name, param_names) {
                    return true;
                }
            }
        }

        // Special case: index access type like `{ [P in K]: N<T, K> }[K]`
        // The object type is a mapped type, check if it self-references
        if node.kind == syntax_kind_ext::INDEXED_ACCESS_TYPE {
            if let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) {
                // Check the object type (which may be a mapped type)
                if self.body_contains_self_referencing_mapped(
                    indexed.object_type,
                    name,
                    param_names,
                ) {
                    return true;
                }
            }
        }

        // Recurse into children for union types, intersection types, etc.
        // Skip conditional types as they represent bounded recursion
        if node.kind != syntax_kind_ext::CONDITIONAL_TYPE {
            for child_idx in self.ctx.arena.get_children(node_idx) {
                if self.body_contains_self_referencing_mapped(child_idx, name, param_names) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a type node contains a type reference to `name` with type args
    /// that exactly match the given parameter names (identity recursion).
    /// Skips conditional type branches (they represent bounded recursion).
    fn template_has_identity_self_ref(
        &self,
        node_idx: NodeIndex,
        name: &str,
        param_names: &[String],
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        // Skip conditional type branches — they represent bounded recursion
        if node.kind == syntax_kind_ext::CONDITIONAL_TYPE {
            return false;
        }

        // Check type references
        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
        {
            // Check if the type name matches
            if let Some(name_node) = self.ctx.arena.get(type_ref.type_name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                && self.ctx.arena.resolve_identifier_text(ident) == name
            {
                // Check if type args are identity (same as param names)
                if let Some(args) = &type_ref.type_arguments
                    && args.nodes.len() == param_names.len()
                {
                    let is_identity =
                        args.nodes
                            .iter()
                            .zip(param_names.iter())
                            .all(|(&arg_idx, param_name)| {
                                self.ctx
                                    .arena
                                    .get(arg_idx)
                                    .and_then(|n| {
                                        if n.kind == syntax_kind_ext::TYPE_REFERENCE {
                                            let tr = self.ctx.arena.get_type_ref(n)?;
                                            let name_n = self.ctx.arena.get(tr.type_name)?;
                                            let id = self.ctx.arena.get_identifier(name_n)?;
                                            Some(
                                                self.ctx.arena.resolve_identifier_text(id)
                                                    == *param_name,
                                            )
                                        } else if n.kind == SyntaxKind::Identifier as u16 {
                                            let id = self.ctx.arena.get_identifier(n)?;
                                            Some(
                                                self.ctx.arena.resolve_identifier_text(id)
                                                    == *param_name,
                                            )
                                        } else {
                                            Some(false)
                                        }
                                    })
                                    .unwrap_or(false)
                            });
                    if is_identity {
                        return true;
                    }
                }
            }
        }

        // Recurse into children
        for child_idx in self.ctx.arena.get_children(node_idx) {
            if self.template_has_identity_self_ref(child_idx, name, param_names) {
                return true;
            }
        }
        false
    }
}

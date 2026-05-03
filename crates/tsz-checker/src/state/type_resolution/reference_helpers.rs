//! Type reference resolution helpers: array types, simple type references,
//! type parameter extraction, and class instance type construction.

use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::{NodeIndex, NodeList, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn get_reference_type_params_for_symbol(
        &mut self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> Vec<tsz_solver::TypeParamInfo> {
        let declared =
            self.extract_declared_type_params_for_reference_symbol(sym_id, expected_name);
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
        let declared =
            self.extract_declared_type_params_for_reference_symbol(sym_id, expected_name);
        if !declared.is_empty() {
            return declared
                .iter()
                .filter(|param| param.default.is_none())
                .count();
        }
        self.count_required_type_params(sym_id)
    }

    pub(crate) fn symbol_has_declared_type_meaning(&self, sym_id: SymbolId) -> bool {
        let lib_binders = self.get_lib_binders();
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
            return false;
        };

        if !symbol.has_any_flags(symbol_flags::ALIAS) && symbol.has_any_flags(symbol_flags::TYPE) {
            return true;
        }

        symbol.declarations.iter().copied().any(|decl_idx| {
            let arena = self
                .ctx
                .binder
                .arena_for_declaration_or(sym_id, decl_idx, self.ctx.arena);
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
                        .unwrap_or_else(|| self.count_required_reference_type_params(sym_id, name));
                    if required_count > 0 {
                        // Check if this is a class/interface symbol currently being resolved.
                        // For class/interface self references like `class A<T> { x: A }`
                        // and constraints like `class A<T extends A> {}`, tsc still emits
                        // TS2314 and treats the erroneous annotation as any-like. Type aliases
                        // keep the old resolution-set skip because tsc handles those through
                        // circularity detection.
                        let is_class_or_interface = self
                            .ctx
                            .binder
                            .get_symbol(sym_id)
                            .map(|s| s.has_any_flags(symbol_flags::CLASS | symbol_flags::INTERFACE))
                            .unwrap_or(false);
                        let should_emit_ts2314 = !self.ctx.symbol_resolution_set.contains(&sym_id)
                            || is_class_or_interface;
                        if should_emit_ts2314 {
                            // tsc uses the original declaration name, not the local alias.
                            // e.g., `export type { A as B }` → `let d: B` reports 'A<T>', not 'B<T>'.
                            // Resolve through aliases to get the target symbol's name.
                            let resolved_name = {
                                let mut visited_aliases = AliasCycleTracker::new();
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
                            // tsc's errorType is any-like here: downstream property access
                            // and return inference should not cascade from the bad annotation.
                            return TypeId::ANY;
                        }
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
                                crate::query_boundaries::common::resolve_default_type_args(
                                    self.ctx.types,
                                    &type_params,
                                );
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
            && symbol.has_any_flags(symbol_flags::TYPE_ALIAS)
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
                    return crate::query_boundaries::common::instantiate_generic(
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
        if self.has_special_missing_lib_type_diagnostic(name) {
            self.report_missing_lib_type_name(name, type_name_idx);
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
        let mixed_class_interface = symbol.has_any_flags(symbol_flags::CLASS)
            && symbol.has_any_flags(symbol_flags::INTERFACE);

        let mut merged: Vec<tsz_solver::TypeParamInfo> = Vec::new();
        let mut jsdoc_fallback: Option<Vec<tsz_solver::TypeParamInfo>> = None;
        for &decl_idx in &declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            let decl_params: Option<Vec<tsz_solver::TypeParamInfo>> = if let Some(type_alias) =
                self.ctx.arena.get_type_alias(node)
            {
                let name_matches = self
                    .ctx
                    .arena
                    .get(type_alias.name)
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .is_none_or(|ident| ident.escaped_text == expected_name);
                if name_matches {
                    let (params, updates) = self.push_type_parameters(&type_alias.type_parameters);
                    self.pop_type_parameters(updates);
                    Some(params)
                } else {
                    None
                }
            } else if let Some(iface) = self.ctx.arena.get_interface(node) {
                let name_matches = self
                    .ctx
                    .arena
                    .get(iface.name)
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .is_none_or(|ident| ident.escaped_text == expected_name);
                if name_matches {
                    let (params, updates) = self.push_type_parameters(&iface.type_parameters);
                    self.pop_type_parameters(updates);
                    Some(params)
                } else {
                    None
                }
            } else if !mixed_class_interface && let Some(class) = self.ctx.arena.get_class(node) {
                let name_matches = self
                    .ctx
                    .arena
                    .get(class.name)
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .is_none_or(|ident| ident.escaped_text == expected_name);
                if name_matches {
                    let (params, updates) = self.push_type_parameters(&class.type_parameters);
                    self.pop_type_parameters(updates);
                    if params.is_empty()
                        && self.is_js_file()
                        && let Some(jsdoc_params) =
                            self.jsdoc_template_type_params_for_class_decl(decl_idx)
                        && !jsdoc_params.is_empty()
                    {
                        jsdoc_fallback.get_or_insert(jsdoc_params);
                        None
                    } else {
                        Some(params)
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let Some(params) = decl_params else {
                continue;
            };
            if params.is_empty() {
                continue;
            }
            if merged.is_empty() {
                merged = params;
                continue;
            }
            // Merge defaults across declarations of a merged class/interface.
            // tsc spreads type-parameter defaults across all merged declarations:
            // a default specified on any declaration applies for the unsupplied
            // position. Only fill missing slots so the leftmost-with-default wins.
            for (slot, incoming) in merged.iter_mut().zip(params.iter()) {
                if slot.default.is_none() && incoming.default.is_some() {
                    *slot = tsz_solver::TypeParamInfo {
                        name: slot.name,
                        constraint: slot.constraint,
                        default: incoming.default,
                        is_const: slot.is_const,
                    };
                }
            }
        }
        if !merged.is_empty() {
            return merged;
        }
        if let Some(jsdoc_params) = jsdoc_fallback {
            return jsdoc_params;
        }
        Vec::new()
    }

    /// Read leading JSDoc on a JS class declaration and synthesize
    /// `TypeParamInfo` entries from `@template T` tags. Walks up to the
    /// wrapping `EXPORT_DECLARATION` so `export class Foo` still locates
    /// the JSDoc that sits before the `export` keyword.
    fn jsdoc_template_type_params_for_class_decl(
        &mut self,
        decl_idx: NodeIndex,
    ) -> Option<Vec<tsz_solver::TypeParamInfo>> {
        let sf = self.ctx.arena.source_files.first()?;
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let node = self.ctx.arena.get(decl_idx)?;
        let mut search_pos = node.pos;
        if let Some(ext) = self.ctx.arena.get_extended(decl_idx)
            && ext.parent.is_some()
            && let Some(parent) = self.ctx.arena.get(ext.parent)
            && parent.kind == syntax_kind_ext::EXPORT_DECLARATION
        {
            search_pos = parent.pos;
        }
        let jsdoc = self.try_leading_jsdoc(comments, search_pos, source_text)?;
        let names = Self::jsdoc_template_type_params(&jsdoc);
        if names.is_empty() {
            return None;
        }
        let mut params = Vec::with_capacity(names.len());
        for (name, is_const) in names {
            if name.is_empty() {
                continue;
            }
            params.push(tsz_solver::TypeParamInfo {
                name: self.ctx.types.intern_string(&name),
                constraint: None,
                default: None,
                is_const,
            });
        }
        if params.is_empty() {
            None
        } else {
            Some(params)
        }
    }

    pub(crate) fn symbol_is_namespace_only(&self, sym_id: SymbolId) -> bool {
        let mut visited = AliasCycleTracker::new();
        self.symbol_is_namespace_only_tracked(sym_id, &mut visited)
    }

    /// Cycle-aware variant of [`symbol_is_namespace_only`]. Accepts the caller's
    /// `visited_aliases` so that mutual recursion with [`Self::resolve_alias_symbol`]
    /// shares a single cycle-tracking vector. Without this, a helper that starts
    /// its own fresh `Vec::new()` would bypass the caller's protection and allow
    /// unbounded recursion across alias chains that form cycles only when viewed
    /// at the full mutual-recursion level.
    pub(crate) fn symbol_is_namespace_only_tracked(
        &self,
        sym_id: SymbolId,
        visited_aliases: &mut AliasCycleTracker,
    ) -> bool {
        let lib_binders = self.get_lib_binders();
        if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
            if symbol.has_any_flags(symbol_flags::ALIAS) {
                if self.symbol_has_declared_type_meaning(sym_id) {
                    return false;
                }

                let target_sym_id = self.resolve_alias_symbol(sym_id, visited_aliases);

                if matches!(symbol.import_name.as_deref(), Some("*")) && target_sym_id.is_some() {
                    if symbol.is_umd_export {
                        if let Some(target_sym_id) = target_sym_id
                            && target_sym_id != sym_id
                        {
                            return self
                                .symbol_is_namespace_only_tracked(target_sym_id, visited_aliases);
                        }
                        return false;
                    }
                    return true;
                }

                if let Some(target_sym_id) = target_sym_id
                    && target_sym_id != sym_id
                {
                    return self.symbol_is_namespace_only_tracked(target_sym_id, visited_aliases);
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

            let is_namespace = symbol.has_any_flags(
                symbol_flags::MODULE | symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE,
            );
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
        if symbol.has_any_flags(symbol_flags::TYPE_ALIAS) {
            return self.type_args_match_alias_params(sym_id, type_args);
        }

        // For classes and interfaces, allow recursive references in type parameter constraints
        // Don't force eager resolution - this prevents false cycle detection for patterns like:
        // class C<T extends C<T>>
        // interface I<T extends I<T>>
        if symbol.has_any_flags(symbol_flags::CLASS | symbol_flags::INTERFACE) {
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
        if !symbol.has_any_flags(symbol_flags::TYPE_ALIAS) {
            return false;
        }

        let decl_idx = symbol.primary_declaration().unwrap_or(NodeIndex::NONE);
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

    pub(crate) fn type_arg_nodes_contain_scoped_type_parameter_for_depth_check(
        &self,
        type_args: &NodeList,
    ) -> bool {
        type_args
            .nodes
            .iter()
            .copied()
            .any(|node_idx| self.type_node_contains_scoped_type_parameter_for_depth_check(node_idx))
    }

    fn type_node_contains_scoped_type_parameter_for_depth_check(
        &self,
        node_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if let Some(identifier) = self.ctx.arena.get_identifier(node)
            && self
                .ctx
                .type_parameter_scope
                .contains_key(&identifier.escaped_text)
        {
            return true;
        }
        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child_idx| {
                self.type_node_contains_scoped_type_parameter_for_depth_check(child_idx)
            })
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
        let mut decl_idx = symbol.primary_declaration().unwrap_or(NodeIndex::NONE);
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
            // Check cache but skip ERROR values — these can arise when
            // class_instance_type_cache is cleared during class statement
            // checking and re-computation hits the recursion guard.
            if let Some(&instance_type) = self
                .ctx
                .symbol_instance_types
                .get(&sym_id)
                .or_else(|| self.ctx.symbol_instance_types.get(&active_class_sym))
            {
                // Don't return ERROR from the cache — it may have been temporarily
                // stored by another code path (e.g., constructor type building's
                // save/restore cycle). Fall through to re-resolve from the
                // class_instance_type_cache which always has the correct final type.
                if instance_type != TypeId::ERROR {
                    self.pop_type_parameters(updates);
                    return Some((instance_type, params));
                }
            }

            let instance_type = self.get_class_instance_type(decl_idx, class);
            // Only cache and register if the result is valid. When
            // get_class_instance_type returns ERROR (e.g. due to re-entrant
            // class statement checking clearing class_instance_type_cache),
            // preserve any previously computed valid instance type rather
            // than overwriting it with ERROR.
            if instance_type != TypeId::ERROR {
                self.ctx.symbol_instance_types.insert(sym_id, instance_type);
                if active_class_sym != sym_id {
                    self.ctx
                        .symbol_instance_types
                        .insert(active_class_sym, instance_type);
                }

                // Register the class instance type in both type environments
                // immediately so that Lazy(DefId) fallbacks (created by the
                // recursion guard above) can resolve via resolve_lazy during
                // property access checks and flow-analyzer narrowing.
                let def_id = self.ctx.get_or_create_def_id(active_class_sym);
                self.ctx
                    .register_class_instance_in_envs(def_id, instance_type);
            }

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
        if node.kind == syntax_kind_ext::MAPPED_TYPE
            && let Some(mapped) = self.ctx.arena.get_mapped_type(node)
            && self.template_has_identity_self_ref(mapped.type_node, name, param_names)
        {
            return true;
        }

        // Special case: index access type like `{ [P in K]: N<T, K> }[K]`
        // The object type is a mapped type, check if it self-references
        if node.kind == syntax_kind_ext::INDEXED_ACCESS_TYPE
            && let Some(indexed) = self.ctx.arena.get_indexed_access_type(node)
        {
            // Check the object type (which may be a mapped type)
            if self.body_contains_self_referencing_mapped(indexed.object_type, name, param_names) {
                return true;
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

    /// Emit TS2615 for a circular mapped type application.
    ///
    /// tsc emits TS2615 alongside TS2589 when a type alias instantiation
    /// involves a mapped type whose property circularly references itself.
    pub(crate) fn emit_ts2615_for_circular_mapped_type(
        &mut self,
        error_node: NodeIndex,
        type_id: TypeId,
    ) {
        use crate::diagnostics::diagnostic_codes;

        // Try to extract the property name from the type application args.
        // Returns (unquoted_name, quoted_name) — tsc uses unquoted in the property
        // reference and quoted in the mapped type representation.
        // tsc only emits TS2615 for type alias applications when the mapped type
        // constraint resolves to a concrete string literal key (e.g., `"M"` in
        // `N<number, "M">`). When the constraint is `keyof T` resolving to
        // multiple keys, tsc omits TS2615 and only emits TS2589.
        let Some((prop_display, prop_in_mapped)) = self.extract_mapped_type_property_name(type_id)
        else {
            return;
        };

        // Build a simplified mapped type representation for the message.
        let mapped_str = format!("{{ [P in {prop_in_mapped}]: any; }}");

        let message = format!(
            "Type of property '{prop_display}' circularly references itself in mapped type '{mapped_str}'."
        );
        self.error_at_node(
            error_node,
            &message,
            diagnostic_codes::TYPE_OF_PROPERTY_CIRCULARLY_REFERENCES_ITSELF_IN_MAPPED_TYPE,
        );
    }

    /// Try to extract the property name from a circular mapped type application.
    /// Returns (`unquoted_name`, `quoted_name`) for use in the diagnostic message.
    fn extract_mapped_type_property_name(&self, type_id: TypeId) -> Option<(String, String)> {
        let (_base, args) =
            crate::query_boundaries::common::application_info(self.ctx.types, type_id)?;

        for &arg_id in &args {
            if let Some(atom) =
                crate::query_boundaries::common::string_literal_value(self.ctx.types, arg_id)
            {
                let name = self.ctx.types.resolve_atom(atom);
                return Some((name.to_string(), format!("\"{name}\"")));
            }
        }
        None
    }
}

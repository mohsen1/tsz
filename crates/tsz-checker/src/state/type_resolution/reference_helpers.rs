//! Type reference resolution helpers: array types, simple type references,
//! type parameter extraction, and class instance type construction.

use crate::query_boundaries::state::type_resolution as query;
use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::{NodeIndex, NodeList, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl CheckerState<'_> {
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

    pub(crate) fn symbol_has_declared_type_meaning_or_partner(&self, sym_id: SymbolId) -> bool {
        self.symbol_has_declared_type_meaning(sym_id)
            || self
                .ctx
                .alias_partner_reverse(self.ctx.binder, sym_id)
                .is_some_and(|partner_id| self.symbol_has_declared_type_meaning(partner_id))
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
                    if self.ctx.has_lib_loaded()
                        && (self.ctx.symbol_is_from_lib(sym_id)
                            || self.ctx.binder.lib_symbol_ids.contains(&sym_id))
                    {
                        self.prime_lib_type_params(name);
                    }
                    if self.symbol_is_namespace_only(sym_id) {
                        self.error_namespace_used_as_type_at(name, type_name_idx);
                        return TypeId::ERROR;
                    }
                    // A type alias that circularly references itself collapsed to
                    // a non-generic error type. A bare reference (no type args)
                    // resolves to that error type with no arity diagnostic; the
                    // TS2315 "not generic" for argument-bearing references is
                    // emitted by the type-reference validation path. This avoids
                    // a spurious TS2314 for the self-reference's missing args.
                    if self.type_reference_alias_collapsed_to_error(sym_id) {
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
                        let is_class_or_interface =
                            self.ctx.binder.get_symbol(sym_id).is_some_and(|s| {
                                s.has_any_flags(symbol_flags::CLASS | symbol_flags::INTERFACE)
                            });
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
                            // tsc renders a generic type *alias* by its bare name
                            // (`callback`), but a generic class/interface with its
                            // type parameters (`Array<T>`, `I<T>`) —
                            // `typeToString` writes the declared type parameters
                            // for the latter only. A re-export carries `ALIAS`
                            // (not `TYPE_ALIAS`), so `export type { A as B }` still
                            // resolves to its target's parameterized `A<T>`.
                            let is_bare_type_alias =
                                self.ctx.binder.get_symbol(sym_id).is_some_and(|s| {
                                    s.has_any_flags(symbol_flags::TYPE_ALIAS)
                                        && !s.has_any_flags(
                                            symbol_flags::CLASS | symbol_flags::INTERFACE,
                                        )
                                });
                            let display_name = if is_bare_type_alias {
                                resolved_name
                            } else {
                                Self::format_generic_display_name_with_interner(
                                    &resolved_name,
                                    &type_params,
                                    self.ctx.types,
                                )
                            };
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
                                query::resolve_default_type_args(self.ctx.types, &type_params);
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
                    } else if let Some(args) = &type_ref.type_arguments
                        && self
                            .ctx
                            .binder
                            .get_symbol(sym_id)
                            .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::ALIAS))
                    {
                        let mut visited_aliases = AliasCycleTracker::new();
                        let resolved_alias =
                            self.resolve_alias_symbol(sym_id, &mut visited_aliases);
                        let alias_target = resolved_alias
                            .filter(|&target_sym_id| target_sym_id != sym_id)
                            .or_else(|| self.resolve_import_alias_cross_file(sym_id));
                        if let Some(target_sym_id) = alias_target {
                            let target_is_class = self
                                .get_symbol_from_registered_file_target(target_sym_id)
                                .or_else(|| self.get_cross_file_symbol(target_sym_id))
                                .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::CLASS));
                            if target_is_class {
                                let (body_type, type_params) =
                                    self.type_reference_symbol_type_with_params(target_sym_id);
                                let type_args = args
                                    .nodes
                                    .iter()
                                    .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                                    .collect::<Vec<_>>();
                                if !type_params.is_empty() && !type_args.is_empty() {
                                    return query::instantiate_generic(
                                        self.ctx.types,
                                        body_type,
                                        &type_params,
                                        &type_args,
                                    );
                                }
                                return body_type;
                            }
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
                    return query::instantiate_generic(
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

    /// M9: a bare namespace-qualified reference (`ns.Alias`, no type arguments)
    /// whose target's type parameters ALL carry defaults — substitute the
    /// declared defaults, exactly as [`resolve_simple_type_reference`] does for
    /// an unqualified bare reference. Returns `None` when the target is not an
    /// all-defaulted generic, so the caller falls through to its existing
    /// qualified-name resolution (and its arity diagnostics).
    ///
    /// Gating on EVERY parameter having a default mirrors the simple-name path,
    /// whose fill runs only after the `required_count > 0` arity early-return: a
    /// mixed `<A, B = A>` reference (some required) must reach the arity
    /// diagnostic, not be silently filled. The member's simple `escaped_name`
    /// (not the dotted entity text) is used for parameter extraction so it
    /// matches the declaration by name instead of falling back to the
    /// collision-prone raw-`SymbolId` display path.
    pub(crate) fn qualified_bare_reference_default_fill(
        &mut self,
        sym_id: SymbolId,
        type_name_idx: NodeIndex,
    ) -> Option<TypeId> {
        let name = self
            .get_symbol_globally(sym_id)
            .map(|s| s.escaped_name.clone())
            .or_else(|| self.entity_name_text(type_name_idx))
            .unwrap_or_else(|| "<unknown>".to_string());
        let type_params = self.get_reference_type_params_for_symbol(sym_id, &name);
        if type_params.is_empty() || !type_params.iter().all(|p| p.default.is_some()) {
            return None;
        }
        self.ctx
            .get_or_create_def_id_with_params(sym_id, type_params.clone());
        let default_args = query::resolve_default_type_args(self.ctx.types, &type_params);
        let def_id = self.ctx.get_or_create_def_id(sym_id);
        // Register the alias body so `Application` expansion can resolve
        // `Lazy(def_id)` (mirrors the simple-name path).
        let _ = self.get_type_of_symbol(sym_id);
        let base_type_id = self.ctx.types.factory().lazy(def_id);
        Some(
            self.ctx
                .types
                .factory()
                .application(base_type_id, default_args),
        )
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
        let should_extract_params = self.ctx.get_def_type_params(def_id).is_none_or(|cached| {
            !cached.is_empty()
                && cached
                    .iter()
                    .all(|param| param.constraint.is_none() && param.default.is_none())
        });
        if should_extract_params {
            let params = self
                .extract_declared_type_params_for_reference_symbol(sym_id, name)
                .unwrap_or_default();
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
                .is_ok_and(|env| env.get_def(def_id).is_some());
            if !has_body {
                let _ = self.resolve_lib_type_by_name(name);
            }
        }

        def_id
    }

    /// Extract the type parameters declared by a referenced symbol, resolved
    /// against the symbol's own declaration arena and gated by `expected_name`.
    ///
    /// Returns `Some(params)` when a declaration whose name matches
    /// `expected_name` was located — even when the result is empty, which means
    /// the symbol is genuinely non-generic. Returns `None` only when no matching
    /// declaration could be found in any known arena.
    ///
    /// The `Some(empty)` vs `None` distinction matters: a non-generic symbol must
    /// not let callers fall back to the raw-`SymbolId`-keyed display/count paths,
    /// because raw `SymbolId` values collide across binders and that fallback can
    /// read an unrelated same-file generic's parameters (e.g. an imported
    /// non-generic alias inheriting a sibling generic's free type params).
    pub(crate) fn extract_declared_type_params_for_reference_symbol(
        &mut self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> Option<Vec<tsz_solver::TypeParamInfo>> {
        let mut effective_sym_id = sym_id;
        let mut effective_file_idx = None;
        let local_reference_sym_id =
            self.current_non_import_reference_symbol_id(sym_id, expected_name);
        let import_target = self
            .ctx
            .binder
            .file_locals
            .get(expected_name)
            .and_then(|alias_sym_id| self.ctx.binder.get_symbol(alias_sym_id))
            .or_else(|| self.ctx.binder.get_symbol(sym_id))
            .filter(|alias_symbol| self.reference_symbol_is_import_alias(alias_symbol))
            .and_then(|alias_symbol| {
                self.reference_import_alias_export_target(alias_symbol, expected_name)
            });
        if let Some((target_sym_id, target_idx)) = import_target {
            if let Some(target_idx) = target_idx {
                self.ctx
                    .register_symbol_file_target(target_sym_id, target_idx);
            }
            effective_sym_id = target_sym_id;
            effective_file_idx = target_idx;
        } else if let Some(local_sym_id) = local_reference_sym_id {
            effective_sym_id = local_sym_id;
        }

        let current_non_import = local_reference_sym_id == Some(effective_sym_id);
        let symbol = (if current_non_import {
            self.ctx.binder.get_symbol(effective_sym_id)
        } else {
            self.get_symbol_from_registered_file_target(effective_sym_id)
                .or_else(|| self.get_cross_file_symbol(effective_sym_id))
        })?;
        let declarations = symbol.declarations.clone();
        // When the reference resolved through an import/re-export chain, the
        // resolved target declaration legitimately carries its *own* name, which
        // can differ from both the use-site name and the use-site import name
        // when an intermediate re-export renamed it
        // (`export type { Original as Renamed } from './base'`). Accept that name
        // too. Gated on `import_target` so the raw-`SymbolId` fallback — which can
        // collide across binders — keeps its strict use-site name gate.
        let resolved_target_decl_name = import_target.map(|_| symbol.escaped_name.clone());
        let imported_decl_name = self
            .ctx
            .binder
            .file_locals
            .get(expected_name)
            .and_then(|alias_sym_id| self.ctx.binder.get_symbol(alias_sym_id))
            .or_else(|| self.ctx.binder.get_symbol(sym_id))
            .filter(|alias_symbol| self.reference_symbol_is_import_alias(alias_symbol))
            .and_then(|alias_symbol| alias_symbol.import_name().map(str::to_string));
        let mixed_class_interface = symbol.has_any_flags(symbol_flags::CLASS)
            && symbol.has_any_flags(symbol_flags::INTERFACE);

        if symbol.has_any_flags(symbol_flags::CLASS)
            && let Some(file_idx) =
                effective_file_idx.or_else(|| self.ctx.resolve_symbol_file_index(effective_sym_id))
            && !std::ptr::eq(self.ctx.get_arena_for_file(file_idx as u32), self.ctx.arena)
        {
            let decl_arena = self.ctx.get_arena_for_file(file_idx as u32);
            for &decl_idx in &declarations {
                if let Some(names) = Self::type_param_names_in_arena(
                    decl_arena,
                    symbol.flags,
                    decl_idx,
                    &symbol.escaped_name,
                ) && !names.is_empty()
                {
                    return Some(
                        names
                            .into_iter()
                            .map(|name| tsz_solver::TypeParamInfo {
                                name: self.ctx.types.intern_string(&name),
                                constraint: None,
                                default: None,
                                is_const: false,
                                origin: tsz_solver::TypeParamOrigin::User,
                            })
                            .collect(),
                    );
                }
            }
        }

        let mut merged: Vec<tsz_solver::TypeParamInfo> = Vec::new();
        let mut jsdoc_fallback: Option<Vec<tsz_solver::TypeParamInfo>> = None;
        // Track whether any declaration matched `expected_name`, and whether any
        // matched declaration is generic. A matched declaration that is
        // *syntactically* non-generic (no type-parameter list) proves the symbol
        // has no type parameters; one that carries a type-parameter list we failed
        // to extract here must keep the display fallback alive, so it counts as
        // generic.
        let mut matched_any_decl = false;
        let mut matched_generic_decl = false;
        for &decl_idx in &declarations {
            let cross_file_arena = if let Some(file_idx) = effective_file_idx.or_else(|| {
                if current_non_import {
                    None
                } else {
                    self.ctx.resolve_symbol_file_index(effective_sym_id)
                }
            }) && let Some(arena) = self
                .ctx
                .all_arenas
                .as_ref()
                .and_then(|arenas| arenas.get(file_idx).cloned())
                && !std::ptr::eq(arena.as_ref(), self.ctx.arena)
            {
                Some(arena)
            } else {
                None
            };
            let decl_arenas: Vec<(&NodeArena, bool)> = if let Some(arena) =
                cross_file_arena.as_deref()
            {
                vec![(arena, false)]
            } else {
                self.ctx
                    .binder
                    .declaration_arenas
                    .get(&(effective_sym_id, decl_idx))
                    .map(|arenas| {
                        arenas
                            .iter()
                            .map(|arena| {
                                (arena.as_ref(), std::ptr::eq(arena.as_ref(), self.ctx.arena))
                            })
                            .collect()
                    })
                    .or_else(|| {
                        self.ctx
                            .binder
                            .symbol_arenas
                            .get(&effective_sym_id)
                            .map(|arena| {
                                vec![(arena.as_ref(), std::ptr::eq(arena.as_ref(), self.ctx.arena))]
                            })
                    })
                    .unwrap_or_else(|| vec![(self.ctx.arena, true)])
            };

            for (decl_arena, is_current_arena) in decl_arenas {
                let Some(node) = decl_arena.get(decl_idx) else {
                    continue;
                };

                let decl_params: Option<Vec<tsz_solver::TypeParamInfo>> = if let Some(type_alias) =
                    decl_arena.get_type_alias(node)
                {
                    if let Some(name_node) = decl_arena.get(type_alias.name)
                        && let Some(ident) = decl_arena.get_identifier(name_node)
                        && !Self::reference_decl_name_is_accepted(
                            ident.escaped_text.as_str(),
                            expected_name,
                            imported_decl_name.as_deref(),
                            resolved_target_decl_name.as_deref(),
                        )
                    {
                        continue;
                    }
                    let params = if is_current_arena {
                        self.collect_current_arena_type_alias_params_with_resolved_refs(
                            decl_arena,
                            type_alias,
                            effective_sym_id,
                        )
                    } else {
                        let type_resolver = |node_idx: NodeIndex| {
                            decl_arena.get_identifier_text(node_idx).and_then(|name| {
                                self.resolve_declaration_file_type_symbol_for_lowering(
                                    name,
                                    effective_file_idx,
                                )
                                .map(|sym_id| sym_id.0)
                                .or_else(|| {
                                    (!self.declaration_file_type_shadow_for_lib_name(
                                        name,
                                        effective_file_idx,
                                    ))
                                    .then(|| {
                                        self.resolve_actual_lib_name_to_def_id_for_lowering(name)
                                    })
                                    .flatten()
                                    .or_else(|| {
                                        self.resolve_entity_name_text_to_def_id_for_lowering(name)
                                    })
                                    .and_then(|def_id| {
                                        self.ctx.def_to_symbol_id_with_fallback(def_id)
                                    })
                                    .map(|sym_id| sym_id.0)
                                })
                            })
                        };
                        let def_id_resolver = |node_idx: NodeIndex| {
                            decl_arena.get_identifier_text(node_idx).and_then(|name| {
                                self.resolve_declaration_file_type_def_id_for_lowering(
                                    name,
                                    effective_file_idx,
                                )
                                .or_else(|| {
                                    (!self.declaration_file_type_shadow_for_lib_name(
                                        name,
                                        effective_file_idx,
                                    ))
                                    .then(|| {
                                        self.resolve_actual_lib_name_to_def_id_for_lowering(name)
                                    })
                                    .flatten()
                                    .or_else(|| {
                                        self.resolve_entity_name_text_to_def_id_for_lowering(name)
                                    })
                                })
                            })
                        };
                        let value_resolver =
                            |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                        let name_resolver = |type_name: &str| {
                            self.resolve_declaration_file_type_def_id_for_lowering(
                                type_name,
                                effective_file_idx,
                            )
                            .or_else(|| {
                                (!self.declaration_file_type_shadow_for_lib_name(
                                    type_name,
                                    effective_file_idx,
                                ))
                                .then(|| {
                                    self.resolve_actual_lib_name_to_def_id_for_lowering(type_name)
                                })
                                .flatten()
                                .or_else(|| {
                                    self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
                                })
                            })
                        };
                        tsz_lowering::TypeLowering::with_hybrid_resolver(
                            decl_arena,
                            self.ctx.types,
                            &type_resolver,
                            &def_id_resolver,
                            &value_resolver,
                        )
                        .with_name_def_id_resolver(&name_resolver)
                        .prefer_name_def_id_resolution()
                        .collect_type_alias_type_parameters(type_alias)
                    };
                    Some(self.apply_omitted_defaults_to_cross_file_param_constraints(
                        decl_arena,
                        type_alias.type_parameters.as_ref(),
                        params,
                        effective_file_idx,
                    ))
                } else if let Some(iface) = decl_arena.get_interface(node) {
                    if let Some(name_node) = decl_arena.get(iface.name)
                        && let Some(ident) = decl_arena.get_identifier(name_node)
                        && !Self::reference_decl_name_is_accepted(
                            ident.escaped_text.as_str(),
                            expected_name,
                            imported_decl_name.as_deref(),
                            resolved_target_decl_name.as_deref(),
                        )
                    {
                        continue;
                    }
                    let params = if is_current_arena {
                        let (params, updates) = self.push_type_parameters(&iface.type_parameters);
                        self.pop_type_parameters(updates);
                        params
                    } else {
                        let type_resolver = |node_idx: NodeIndex| {
                            decl_arena.get_identifier_text(node_idx).and_then(|name| {
                                self.resolve_declaration_file_type_symbol_for_lowering(
                                    name,
                                    effective_file_idx,
                                )
                                .map(|sym_id| sym_id.0)
                                .or_else(|| {
                                    (!self.declaration_file_type_shadow_for_lib_name(
                                        name,
                                        effective_file_idx,
                                    ))
                                    .then(|| {
                                        self.resolve_actual_lib_name_to_def_id_for_lowering(name)
                                    })
                                    .flatten()
                                    .or_else(|| {
                                        self.resolve_entity_name_text_to_def_id_for_lowering(name)
                                    })
                                    .and_then(|def_id| {
                                        self.ctx.def_to_symbol_id_with_fallback(def_id)
                                    })
                                    .map(|sym_id| sym_id.0)
                                })
                            })
                        };
                        let def_id_resolver = |node_idx: NodeIndex| {
                            decl_arena.get_identifier_text(node_idx).and_then(|name| {
                                self.resolve_declaration_file_type_def_id_for_lowering(
                                    name,
                                    effective_file_idx,
                                )
                                .or_else(|| {
                                    (!self.declaration_file_type_shadow_for_lib_name(
                                        name,
                                        effective_file_idx,
                                    ))
                                    .then(|| {
                                        self.resolve_actual_lib_name_to_def_id_for_lowering(name)
                                    })
                                    .flatten()
                                    .or_else(|| {
                                        self.resolve_entity_name_text_to_def_id_for_lowering(name)
                                    })
                                })
                            })
                        };
                        let value_resolver =
                            |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                        let name_resolver = |type_name: &str| {
                            self.resolve_declaration_file_type_def_id_for_lowering(
                                type_name,
                                effective_file_idx,
                            )
                            .or_else(|| {
                                (!self.declaration_file_type_shadow_for_lib_name(
                                    type_name,
                                    effective_file_idx,
                                ))
                                .then(|| {
                                    self.resolve_actual_lib_name_to_def_id_for_lowering(type_name)
                                })
                                .flatten()
                                .or_else(|| {
                                    self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
                                })
                            })
                        };
                        tsz_lowering::TypeLowering::with_hybrid_resolver(
                            decl_arena,
                            self.ctx.types,
                            &type_resolver,
                            &def_id_resolver,
                            &value_resolver,
                        )
                        .with_name_def_id_resolver(&name_resolver)
                        .prefer_name_def_id_resolution()
                        .collect_merged_interface_type_parameters(&[(decl_idx, decl_arena)])
                    };
                    Some(self.apply_omitted_defaults_to_cross_file_param_constraints(
                        decl_arena,
                        iface.type_parameters.as_ref(),
                        params,
                        effective_file_idx,
                    ))
                } else if !mixed_class_interface && let Some(class) = decl_arena.get_class(node) {
                    if let Some(name_node) = decl_arena.get(class.name)
                        && let Some(ident) = decl_arena.get_identifier(name_node)
                        && !Self::reference_decl_name_is_accepted(
                            ident.escaped_text.as_str(),
                            expected_name,
                            imported_decl_name.as_deref(),
                            resolved_target_decl_name.as_deref(),
                        )
                    {
                        continue;
                    }
                    if is_current_arena {
                        let (params, updates) = self.push_type_parameters(&class.type_parameters);
                        self.pop_type_parameters(updates);
                        if !params.is_empty() {
                            Some(params)
                        } else if self.is_js_file()
                            && let Some(jsdoc_params) =
                                self.jsdoc_template_type_params_for_class_decl(decl_idx)
                            && !jsdoc_params.is_empty()
                        {
                            jsdoc_fallback.get_or_insert(jsdoc_params);
                            None
                        } else {
                            Some(params)
                        }
                    } else if let Some(type_parameters) = &class.type_parameters {
                        let type_resolver = |node_idx: NodeIndex| {
                            decl_arena.get_identifier_text(node_idx).and_then(|name| {
                                self.resolve_declaration_file_type_symbol_for_lowering(
                                    name,
                                    effective_file_idx,
                                )
                                .map(|sym_id| sym_id.0)
                                .or_else(|| {
                                    (!self.declaration_file_type_shadow_for_lib_name(
                                        name,
                                        effective_file_idx,
                                    ))
                                    .then(|| {
                                        self.resolve_actual_lib_name_to_def_id_for_lowering(name)
                                    })
                                    .flatten()
                                    .or_else(|| {
                                        self.resolve_entity_name_text_to_def_id_for_lowering(name)
                                    })
                                    .and_then(|def_id| {
                                        self.ctx.def_to_symbol_id_with_fallback(def_id)
                                    })
                                    .map(|sym_id| sym_id.0)
                                })
                            })
                        };
                        let def_id_resolver = |node_idx: NodeIndex| {
                            decl_arena.get_identifier_text(node_idx).and_then(|name| {
                                self.resolve_declaration_file_type_def_id_for_lowering(
                                    name,
                                    effective_file_idx,
                                )
                                .or_else(|| {
                                    (!self.declaration_file_type_shadow_for_lib_name(
                                        name,
                                        effective_file_idx,
                                    ))
                                    .then(|| {
                                        self.resolve_actual_lib_name_to_def_id_for_lowering(name)
                                    })
                                    .flatten()
                                    .or_else(|| {
                                        self.resolve_entity_name_text_to_def_id_for_lowering(name)
                                    })
                                })
                            })
                        };
                        let value_resolver =
                            |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                        let name_resolver = |type_name: &str| {
                            self.resolve_declaration_file_type_def_id_for_lowering(
                                type_name,
                                effective_file_idx,
                            )
                            .or_else(|| {
                                (!self.declaration_file_type_shadow_for_lib_name(
                                    type_name,
                                    effective_file_idx,
                                ))
                                .then(|| {
                                    self.resolve_actual_lib_name_to_def_id_for_lowering(type_name)
                                })
                                .flatten()
                                .or_else(|| {
                                    self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
                                })
                            })
                        };
                        let params = tsz_lowering::TypeLowering::with_hybrid_resolver(
                            decl_arena,
                            self.ctx.types,
                            &type_resolver,
                            &def_id_resolver,
                            &value_resolver,
                        )
                        .with_name_def_id_resolver(&name_resolver)
                        .prefer_name_def_id_resolution()
                        .collect_type_parameters(type_parameters);
                        Some(self.apply_omitted_defaults_to_cross_file_param_constraints(
                            decl_arena,
                            class.type_parameters.as_ref(),
                            params,
                            effective_file_idx,
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                };

                let Some(params) = decl_params else {
                    continue;
                };
                matched_any_decl = true;
                if params.is_empty() {
                    // The matched declaration produced no type parameters. Only
                    // treat the symbol as non-generic when the declaration is
                    // syntactically non-generic; if it carries a type-parameter
                    // list we merely failed to resolve, mark it generic so the
                    // display fallback stays available.
                    if Self::decl_node_is_syntactically_generic(decl_arena, node) {
                        matched_generic_decl = true;
                    }
                    continue;
                }
                matched_generic_decl = true;
                if merged.is_empty() {
                    merged = params;
                    continue;
                }
                // Merge constraints/defaults across declarations of a merged
                // class/interface. `tsc` makes a constraint/default specified on one
                // declaration visible to sibling declarations at the same positional
                // type-parameter slot. Only fill missing slots so the leftmost
                // declaration still owns explicit facts on that slot.
                for (slot, incoming) in merged.iter_mut().zip(params.iter()) {
                    if slot.constraint.is_none() && incoming.constraint.is_some() {
                        *slot = tsz_solver::TypeParamInfo {
                            name: slot.name,
                            constraint: incoming.constraint,
                            default: slot.default,
                            is_const: slot.is_const,
                            origin: slot.origin,
                        };
                    }
                    if slot.default.is_none() && incoming.default.is_some() {
                        *slot = tsz_solver::TypeParamInfo {
                            name: slot.name,
                            constraint: slot.constraint,
                            default: incoming.default,
                            is_const: slot.is_const,
                            origin: slot.origin,
                        };
                    }
                }
            }
        }
        if !merged.is_empty() {
            return Some(merged);
        }
        if let Some(jsdoc_params) = jsdoc_fallback {
            return Some(jsdoc_params);
        }
        if matched_any_decl && !matched_generic_decl {
            // Every matched declaration is syntactically non-generic: the symbol
            // genuinely has no type parameters. Returning `Some(empty)` keeps
            // callers off the raw-`SymbolId` display/count fallback that can leak a
            // colliding same-file symbol's parameters. When a matched declaration
            // carried an unresolved type-parameter list (`matched_generic_decl`),
            // fall through to `None` so the display path can still recover them.
            return Some(Vec::new());
        }
        None
    }

    /// Whether a declaration's own identifier `ident_text` matches one of the
    /// names that legitimately denote the referenced symbol: the use-site name,
    /// the use-site import name, or — when the reference was resolved through an
    /// import/re-export chain — the resolved target declaration's own name (which
    /// differs when an intermediate re-export renamed it). A non-match means the
    /// declaration belongs to a different symbol that merely shares a colliding
    /// raw `SymbolId`, so its type parameters must not be read.
    fn reference_decl_name_is_accepted(
        ident_text: &str,
        expected_name: &str,
        imported_decl_name: Option<&str>,
        resolved_target_decl_name: Option<&str>,
    ) -> bool {
        ident_text == expected_name
            || imported_decl_name == Some(ident_text)
            || resolved_target_decl_name == Some(ident_text)
    }

    /// Whether the declaration at `decl_idx` carries a non-empty type-parameter
    /// list (type alias, interface, or class). Used to distinguish a genuinely
    /// non-generic declaration from one whose type parameters merely failed to
    /// resolve in the current arena.
    fn decl_node_is_syntactically_generic(
        arena: &NodeArena,
        node: &tsz_parser::parser::node::Node,
    ) -> bool {
        arena
            .get_type_alias(node)
            .and_then(|alias| alias.type_parameters.as_ref())
            .or_else(|| {
                arena
                    .get_interface(node)
                    .and_then(|iface| iface.type_parameters.as_ref())
            })
            .or_else(|| {
                arena
                    .get_class(node)
                    .and_then(|class| class.type_parameters.as_ref())
            })
            .is_some_and(|type_parameters| !type_parameters.nodes.is_empty())
    }

    fn apply_omitted_defaults_to_cross_file_param_constraints(
        &mut self,
        decl_arena: &NodeArena,
        type_parameters: Option<&NodeList>,
        mut params: Vec<tsz_solver::TypeParamInfo>,
        effective_file_idx: Option<usize>,
    ) -> Vec<tsz_solver::TypeParamInfo> {
        let Some(type_parameters) = type_parameters else {
            return params;
        };
        for (i, &param_idx) in type_parameters.nodes.iter().enumerate() {
            let Some(param) = params.get_mut(i) else {
                break;
            };
            let Some(param_node) = decl_arena.get(param_idx) else {
                continue;
            };
            let Some(param_data) = decl_arena.get_type_parameter(param_node) else {
                continue;
            };
            if param_data.constraint == NodeIndex::NONE {
                continue;
            }
            if let Some(defaulted_constraint) = self
                .cross_file_omitted_default_constraint_reference(
                    decl_arena,
                    param_data.constraint,
                    effective_file_idx,
                )
            {
                param.constraint = Some(defaulted_constraint);
            }
        }
        params
    }

    fn cross_file_omitted_default_constraint_reference(
        &mut self,
        decl_arena: &NodeArena,
        constraint_idx: NodeIndex,
        effective_file_idx: Option<usize>,
    ) -> Option<TypeId> {
        let node = decl_arena.get(constraint_idx)?;
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = decl_arena.get_type_ref(node)?;
        if type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty())
        {
            return None;
        }

        let name = decl_arena.get_identifier_text(type_ref.type_name)?;
        let target_sym_id =
            self.resolve_declaration_file_type_symbol_for_lowering(name, effective_file_idx)?;
        // Cycle guard: expanding `target_sym_id`'s own defaults re-enters
        // `get_reference_type_params_for_symbol` below, which can recurse back to
        // a symbol already being expanded (self/mutually referential generic
        // registries — the fp-ts `URItoKind`/`Kind` HKT family). Leave the
        // constraint un-defaulted on re-entry instead of looping; the
        // `ref_type_params` cache populated by the outermost frame still yields
        // the fully-expanded form for non-cyclic references.
        if !self
            .ctx
            .omitted_default_constraint_stack
            .borrow_mut()
            .insert(target_sym_id.0)
        {
            return None;
        }
        let result = self.cross_file_omitted_default_constraint_reference_inner(
            name,
            target_sym_id,
            effective_file_idx,
        );
        self.ctx
            .omitted_default_constraint_stack
            .borrow_mut()
            .remove(&target_sym_id.0);
        result
    }

    fn cross_file_omitted_default_constraint_reference_inner(
        &mut self,
        name: &str,
        target_sym_id: SymbolId,
        effective_file_idx: Option<usize>,
    ) -> Option<TypeId> {
        let target_name = self
            .get_symbol_from_registered_file_target(target_sym_id)
            .or_else(|| self.get_cross_file_symbol(target_sym_id))
            .or_else(|| self.ctx.binder.get_symbol(target_sym_id))
            .map_or_else(|| name.to_string(), |symbol| symbol.escaped_name.clone());
        let type_params = self.get_reference_type_params_for_symbol(target_sym_id, &target_name);
        if type_params.is_empty() {
            return None;
        }
        let default_args = crate::query_boundaries::type_defaults::fill_application_defaults(
            self.ctx.types,
            &[],
            &type_params,
        )?;

        self.ensure_def_ready_for_lowering(target_sym_id, &target_name);
        let def_id = self
            .resolve_declaration_file_type_def_id_for_lowering(name, effective_file_idx)
            .unwrap_or_else(|| {
                self.ctx
                    .get_or_create_def_id_for_symbol_name(target_sym_id, &target_name)
            });
        let base = self.ctx.types.factory().lazy(def_id);
        Some(self.ctx.types.factory().application(base, default_args))
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
        let constraint_strs = Self::jsdoc_template_constraint_strings(&jsdoc);
        for (name, is_const, default_str) in names {
            if name.is_empty() {
                continue;
            }
            let default = default_str
                .as_deref()
                .and_then(|s| self.resolve_jsdoc_reference(s));
            let constraint = constraint_strs
                .get(&name)
                .and_then(|s| self.resolve_jsdoc_reference(s));
            let info = tsz_solver::TypeParamInfo {
                name: self.ctx.types.intern_string(&name),
                constraint,
                default,
                is_const,
                origin: tsz_solver::TypeParamOrigin::User,
            };
            let (_, stamped_info) = self.intern_jsdoc_type_param_for_owner_stamped(decl_idx, info);
            params.push(stamped_info);
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
                if self.symbol_has_declared_type_meaning_or_partner(sym_id) {
                    return false;
                }

                let target_sym_id = self.resolve_alias_symbol(sym_id, visited_aliases);

                if matches!(symbol.import_name(), Some("*")) && target_sym_id.is_some() {
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
                if let Some(module_name) = symbol.import_module()
                    && matches!(symbol.import_name(), None | Some("*"))
                    && self.ctx.binder.module_exports.contains_key(module_name)
                {
                    return true;
                }
            }

            let is_namespace = symbol.has_any_flags(
                symbol_flags::MODULE | symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE,
            );
            let has_type = self.symbol_has_declared_type_meaning_or_partner(sym_id);
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

    pub(crate) fn type_alias_symbol_contains_conditional_type(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if !symbol.has_any_flags(symbol_flags::TYPE_ALIAS) {
            return false;
        }

        symbol.declarations.iter().copied().any(|decl_idx| {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            if node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                return false;
            }
            let Some(type_alias) = self.ctx.arena.get_type_alias(node) else {
                return false;
            };
            self.ctx
                .arena
                .get(type_alias.type_node)
                .and_then(|node| self.ctx.arena.get_conditional_type(node))
                .is_some()
        })
    }

    pub(crate) fn type_alias_symbol_direct_conditional_branches_are_array_like(
        &self,
        sym_id: SymbolId,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if !symbol.has_any_flags(symbol_flags::TYPE_ALIAS) {
            return false;
        }

        symbol.declarations.iter().copied().any(|decl_idx| {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            let Some(type_alias) = self.ctx.arena.get_type_alias(node) else {
                return false;
            };
            let Some(body_node) = self.ctx.arena.get(type_alias.type_node) else {
                return false;
            };
            let Some(conditional) = self.ctx.arena.get_conditional_type(body_node) else {
                return false;
            };
            self.type_node_is_array_like_branch(conditional.true_type)
                && self.type_node_is_array_like_branch(conditional.false_type)
        })
    }

    fn type_node_is_array_like_branch(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if matches!(
            node.kind,
            syntax_kind_ext::ARRAY_TYPE | syntax_kind_ext::TUPLE_TYPE
        ) {
            return true;
        }
        if node.kind == syntax_kind_ext::TYPE_OPERATOR
            && let Some(operator) = self.ctx.arena.get_type_operator(node)
            && operator.operator == SyntaxKind::ReadonlyKeyword as u16
        {
            let Some(operand) = self.ctx.arena.get(operator.type_node) else {
                return false;
            };
            return matches!(
                operand.kind,
                syntax_kind_ext::ARRAY_TYPE | syntax_kind_ext::TUPLE_TYPE
            );
        }
        false
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

    pub(crate) fn type_node_contains_scoped_type_parameter_for_depth_check(
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
                .contains_key(identifier.escaped_text.as_str())
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
        if let Some(instance_type) = self.ctx.symbol_instance_types.get(&sym_id) {
            return Some(instance_type);
        }
        self.class_instance_type_with_params_from_symbol(sym_id)
            .map(|(instance_type, _)| instance_type)
    }

    pub(crate) fn class_instance_type_with_params_from_symbol(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        if let Some(instance_type) = self.ctx.symbol_instance_types.get(&sym_id)
            && !instance_type.is_any_unknown_or_error()
        {
            let params = self
                .class_instance_def_id_for_symbol(sym_id)
                .and_then(|def_id| self.ctx.get_def_type_params(def_id))
                .unwrap_or_default();
            return Some((instance_type, params));
        }
        if self.ctx.class_instance_resolution_set.contains(&sym_id) {
            let fallback = self.ctx.create_lazy_type_ref(sym_id);
            return Some((fallback, Vec::new()));
        }

        if self
            .get_symbol_from_registered_file_target(sym_id)
            .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::CLASS))
            && let Some(owner_file_idx) = self.ctx.resolve_symbol_file_index(sym_id)
            && owner_file_idx != self.ctx.current_file_idx
            && let Some(result) = self.delegate_cross_arena_class_instance_type(sym_id)
        {
            if self.file_index_is_declaration_file(owner_file_idx) {
                self.publish_delegated_class_instance_type(sym_id, result.0, &result.1);
            }
            return Some(result);
        }

        let symbol = self
            .get_symbol_from_registered_file_target(sym_id)
            .or_else(|| self.ctx.binder.get_symbol(sym_id))?;
        let mut decl_idx = symbol.primary_declaration().unwrap_or(NodeIndex::NONE);
        // When the primary declaration doesn't resolve to a class in the current
        // arena (e.g., class+interface merged symbol where value_declaration was
        // not propagated through program-level symbol merging), search all
        // declarations for a class node in the current arena.
        // Guard against NodeIndex collisions: check binder arena provenance and
        // verify the class name matches the symbol name to avoid picking up an
        // unrelated class from the arena.
        if decl_idx.is_none()
            || !self
                .ctx
                .declaration_is_local_to_current_arena(sym_id, decl_idx)
            || self.ctx.arena.get_class_at(decl_idx).is_none()
        {
            let expected_name = &symbol.escaped_name;
            for &d in &symbol.declarations {
                if d.is_some()
                    && self.ctx.declaration_is_local_to_current_arena(sym_id, d)
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
        if self
            .ctx
            .declaration_is_local_to_current_arena(sym_id, decl_idx)
            && let Some(class) = self.ctx.arena.get_class_at(decl_idx)
        {
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
                let partial_instance = self
                    .ctx
                    .class_instance_type_cache
                    .borrow()
                    .get(&decl_idx)
                    .copied();
                if let Some(partial_instance) = partial_instance
                    && partial_instance != TypeId::ERROR
                    && partial_instance != TypeId::ANY
                {
                    return Some((partial_instance, Vec::new()));
                }
                let self_sym = self.class_self_reference_symbol(class, active_class_sym);
                crate::class_type::note_class_self_reference_deferral();
                let fallback = self.ctx.create_lazy_type_ref(self_sym);
                return Some((fallback, Vec::new()));
            }

            let (params, updates) = self.push_type_parameters(&class.type_parameters);
            // Check cache but skip ERROR values — these can arise when
            // class_instance_type_cache is cleared during class statement
            // checking and re-computation hits the recursion guard.
            if let Some(instance_type) = self
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
                    let cached_has_construct_signature =
                        query::callable_shape_for_type(self.ctx.types, instance_type)
                            .is_some_and(|shape| !shape.construct_signatures.is_empty());
                    let class_cached = self
                        .ctx
                        .class_instance_type_cache
                        .borrow()
                        .get(&decl_idx)
                        .copied();
                    if cached_has_construct_signature
                        && let Some(class_cached) = class_cached
                        && class_cached != TypeId::ERROR
                        && class_cached != TypeId::ANY
                    {
                        self.pop_type_parameters(updates);
                        return Some((class_cached, params));
                    }
                    if !cached_has_construct_signature {
                        self.pop_type_parameters(updates);
                        return Some((instance_type, params));
                    }
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

    pub(crate) fn publish_delegated_class_instance_type(
        &mut self,
        sym_id: SymbolId,
        instance_type: TypeId,
        params: &[tsz_solver::TypeParamInfo],
    ) {
        if instance_type.is_any_unknown_or_error() {
            return;
        }

        self.publish_delegated_class_instance_symbol_type(sym_id, instance_type);
        self.publish_delegated_class_instance_env_type(sym_id, instance_type, params);
    }

    fn publish_delegated_class_instance_symbol_type(
        &mut self,
        sym_id: SymbolId,
        instance_type: TypeId,
    ) {
        if instance_type.is_any_unknown_or_error() {
            return;
        }

        self.ctx.symbol_instance_types.insert(sym_id, instance_type);
    }

    fn publish_delegated_class_instance_env_type(
        &mut self,
        sym_id: SymbolId,
        instance_type: TypeId,
        params: &[tsz_solver::TypeParamInfo],
    ) {
        if instance_type.is_any_unknown_or_error() {
            return;
        }

        let symbol_name = self
            .get_symbol_from_registered_file_target(sym_id)
            .or_else(|| self.get_cross_file_symbol(sym_id))
            .map(|symbol| symbol.escaped_name.clone());
        let def_id = symbol_name.as_deref().map_or_else(
            || self.ctx.get_or_create_def_id(sym_id),
            |name| self.ctx.get_or_create_def_id_for_symbol_name(sym_id, name),
        );
        if !params.is_empty() && self.ctx.get_def_type_params(def_id).is_none() {
            self.ctx.insert_def_type_params(def_id, params.to_vec());
        }
        self.ctx
            .definition_store
            .register_type_to_def(instance_type, def_id);
        self.ctx
            .register_class_instance_in_envs(def_id, instance_type);
    }

    fn class_instance_def_id_for_symbol(&self, sym_id: SymbolId) -> Option<tsz_solver::def::DefId> {
        let symbol_name = self
            .get_symbol_from_registered_file_target(sym_id)
            .or_else(|| self.get_cross_file_symbol(sym_id))
            .map(|symbol| symbol.escaped_name.as_str());
        Some(symbol_name.map_or_else(
            || self.ctx.get_or_create_def_id(sym_id),
            |name| self.ctx.get_or_create_def_id_for_symbol_name(sym_id, name),
        ))
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

    /// Returns `true` only when the body contains the pattern `{ [P in K]: Alias<K> }[K]`:
    /// a mapped type whose template is an identity self-reference, immediately indexed to
    /// extract a property.  That shape collapses the alias back to itself (infinite instantiation).
    /// A mapped type appearing directly in the body or as a union member is coinductively valid.
    fn body_contains_self_referencing_mapped(
        &self,
        node_idx: NodeIndex,
        name: &str,
        param_names: &[String],
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::INDEXED_ACCESS_TYPE
            && let Some(indexed) = self.ctx.arena.get_indexed_access_type(node)
            && let Some(obj_node) = self.ctx.arena.get(indexed.object_type)
            && obj_node.kind == syntax_kind_ext::MAPPED_TYPE
            && let Some(mapped) = self.ctx.arena.get_mapped_type(obj_node)
            && self.template_has_identity_self_ref(mapped.type_node, name, param_names)
        {
            return true;
        }

        // Skip MAPPED_TYPE (coinductively valid) and CONDITIONAL_TYPE (bounded recursion).
        if node.kind != syntax_kind_ext::CONDITIONAL_TYPE
            && node.kind != syntax_kind_ext::MAPPED_TYPE
        {
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
        let (_base, args) = query::get_application_info(self.ctx.types, type_id)?;

        for &arg_id in &args {
            if let Some(atom) = query::string_literal_value(self.ctx.types, arg_id) {
                let name = self.ctx.types.resolve_atom(atom);
                return Some((name.to_string(), format!("\"{name}\"")));
            }
        }
        None
    }
}

#[path = "reference_helpers_homomorphic.rs"]
mod reference_helpers_homomorphic;

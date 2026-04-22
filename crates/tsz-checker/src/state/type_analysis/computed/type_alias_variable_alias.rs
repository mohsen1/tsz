//! Continuation of `compute_type_of_symbol`: type alias, class property, variable,
//! and alias symbol resolution.

use crate::query_boundaries::common::{array_element_type, is_generic_type};
use crate::query_boundaries::flow as flow_boundary;
use crate::query_boundaries::state::type_environment;
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{TypeId, Visibility};

impl<'a> CheckerState<'a> {
    /// Continuation of `compute_type_of_symbol` for type alias, class property,
    /// variable, and alias symbol kinds.
    ///
    /// This is a pure code-motion split -- no logic changes.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn compute_type_of_symbol_type_alias_variable_alias(
        &mut self,
        sym_id: SymbolId,
        flags: u32,
        value_decl: NodeIndex,
        declarations: &[NodeIndex],
        import_module: &Option<String>,
        import_name: &Option<String>,
        escaped_name: &str,
        factory: &tsz_solver::TypeFactory<'_>,
    ) -> (TypeId, Vec<tsz_solver::TypeParamInfo>) {
        // Type alias - resolve using checker's get_type_from_type_node to properly resolve symbols
        if flags & symbol_flags::TYPE_ALIAS != 0 {
            // Compiler-provided intrinsic type aliases (e.g., `type BuiltinIteratorReturn = intrinsic`)
            // cannot be resolved from their body—the `intrinsic` keyword has no type semantics
            // on its own. Intercept known intrinsic names and resolve them directly.
            // No lib_contexts guard needed: BuiltinIteratorReturn is a compiler-defined name
            // and cross-arena child contexts may not carry lib_contexts forward.
            if escaped_name == "BuiltinIteratorReturn" {
                let ty = if self.ctx.compiler_options.strict_builtin_iterator_return {
                    TypeId::UNDEFINED
                } else {
                    TypeId::ANY
                };
                return (ty, Vec::new());
            }
            // When a type alias name collides with a global value declaration
            // (e.g., user-defined `type Proxy<T>` vs global `declare var Proxy`),
            // the merged symbol's value_declaration points to the var decl, not the
            // type alias. We must search declarations[] to find the actual type alias.
            let decl_idx = declarations
                .iter()
                .copied()
                .find(|&d| {
                    self.ctx
                        .arena
                        .get(d)
                        .and_then(|n| {
                            if n.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                                // Verify name matches to prevent NodeIndex collisions
                                let type_alias = self.ctx.arena.get_type_alias(n)?;
                                let name_node = self.ctx.arena.get(type_alias.name)?;
                                let ident = self.ctx.arena.get_identifier(name_node)?;
                                let name = self.ctx.arena.resolve_identifier_text(ident);
                                Some(name == escaped_name)
                            } else {
                                Some(false)
                            }
                        })
                        .unwrap_or(false)
                })
                .unwrap_or_else(|| {
                    if value_decl.is_some() {
                        value_decl
                    } else {
                        declarations.first().copied().unwrap_or(NodeIndex::NONE)
                    }
                });
            if decl_idx.is_some() {
                // When the type alias declaration was found in the user arena
                // (the `find` closure above checks `self.ctx.arena.get(d)`),
                // we MUST use the user arena for node lookup.  The `symbol_arenas`
                // fallback may point to a lib arena (e.g., when a user-defined
                // `type Proxy<T>` merges with the global `declare var Proxy`),
                // causing the user-arena NodeIndex to fail lookup in the lib arena
                // and incorrectly returning TypeId::UNKNOWN.
                let found_in_user_arena = self
                    .ctx
                    .arena
                    .get(decl_idx)
                    .and_then(|n| self.ctx.arena.get_type_alias(n))
                    .is_some();
                let decl_arena = if found_in_user_arena {
                    self.ctx.arena
                } else {
                    self.ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                        .and_then(|v| v.first())
                        .map(std::convert::AsRef::as_ref)
                        .or_else(|| {
                            self.ctx
                                .binder
                                .symbol_arenas
                                .get(&sym_id)
                                .map(std::convert::AsRef::as_ref)
                        })
                        .unwrap_or(self.ctx.arena)
                };

                let Some(node) = decl_arena.get(decl_idx) else {
                    return (TypeId::UNKNOWN, Vec::new());
                };
                let Some(type_alias) = decl_arena.get_type_alias(node) else {
                    return (TypeId::UNKNOWN, Vec::new());
                };

                let decl_binder = self
                    .ctx
                    .get_binder_for_arena(decl_arena)
                    .unwrap_or(self.ctx.binder);
                let has_cross_arena_metadata = !std::ptr::eq(decl_arena, self.ctx.arena)
                    || decl_binder.symbol_arenas.contains_key(&sym_id)
                    || decl_binder
                        .declaration_arenas
                        .contains_key(&(sym_id, decl_idx));

                // When a local type alias has no own type parameters but is
                // inside a generic function (e.g., `function foo<T>() { type X = T extends ... }`),
                // the enclosing function's type parameters must be in scope during lowering.
                // Push them before lowering and pop after.
                let enclosing_tp_updates = if type_alias.type_parameters.is_none() {
                    self.push_enclosing_type_params_for_node(decl_arena, decl_idx)
                } else {
                    Vec::new()
                };

                let (mut alias_type, params) = if has_cross_arena_metadata {
                    let mut result = self.lower_cross_arena_type_alias_declaration(
                        sym_id, decl_idx, decl_arena, type_alias,
                    );
                    if (result.0 == TypeId::ANY
                        || result.0 == TypeId::UNKNOWN
                        || result.0 == TypeId::ERROR)
                        && let Some(resolved) = self
                            .resolve_cross_arena_type_alias_body_with_checker(
                                decl_arena, sym_id, type_alias,
                            )
                        && resolved != TypeId::UNKNOWN
                        && resolved != TypeId::ERROR
                    {
                        result.0 = resolved;
                    }
                    // When a same-file type alias has cross-arena metadata but the
                    // declaration is in the current arena, resolve TypeQuery references
                    // with flow narrowing. Push type parameters into scope first so
                    // that type args in typeof expressions (e.g. `typeof Foo<U>`)
                    // can resolve them instead of emitting false TS2304.
                    if std::ptr::eq(decl_arena, self.ctx.arena) {
                        let (mut at, params) = result;
                        let (_, tp_updates) =
                            self.push_type_parameters(&type_alias.type_parameters);
                        at = self.resolve_type_queries_with_flow(at, type_alias.type_node);
                        self.pop_type_parameters(tp_updates);
                        (at, params)
                    } else {
                        result
                    }
                } else {
                    let (params, updates) = self.push_type_parameters(&type_alias.type_parameters);
                    self.prime_type_reference_params_in_alias_body(
                        decl_arena,
                        type_alias.type_node,
                    );
                    let mut alias_type = self.get_type_from_type_node(type_alias.type_node);
                    // Resolve TypeQuery references with flow narrowing while type
                    // parameters are still in scope. This prevents false TS2304
                    // errors for type params used as type arguments in typeof
                    // expressions (e.g. `type Alias<U> = typeof Foo<U>`).
                    if std::ptr::eq(decl_arena, self.ctx.arena) {
                        alias_type =
                            self.resolve_type_queries_with_flow(alias_type, type_alias.type_node);
                    }
                    self.pop_type_parameters(updates);
                    (alias_type, params)
                };

                // Pop enclosing type parameters that were pushed for local type aliases.
                self.pop_type_parameters(enclosing_tp_updates);

                // Eagerly evaluate non-generic type aliases whose body is a concrete
                // conditional type.  tsc resolves `type U = [any] extends [number] ? 1 : 0`
                // to `1` during alias resolution so that diagnostics print the resolved
                // type.  We do the same here: if the alias has no type parameters AND
                // the body is evaluable (Conditional, IndexAccess, Mapped, etc.) AND it
                // does not contain deferred type parameters, evaluate it now.
                if params.is_empty()
                    && crate::query_boundaries::common::is_conditional_type(
                        self.ctx.types,
                        alias_type,
                    )
                    && !crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        alias_type,
                    )
                {
                    alias_type = self.evaluate_type_with_env(alias_type);
                }

                // Check for invalid circular reference (TS2456)
                // A type alias circularly references itself if it resolves to itself
                // without structural wrapping (e.g., `type A = B; type B = A;`)
                //
                // Three detection paths:
                // 1. is_direct_circular_reference: same-file cycles via symbol_resolution_set
                // 2. circular_type_aliases: marked by a previous cycle member
                // 3. is_cross_file_circular_alias: cross-file cycles by following
                //    Lazy → body → Lazy chain through shared DefinitionStore
                // Suppress circularity checking when the type alias declaration
                // contains parse errors. tsc skips TS2456 for malformed
                // declarations (e.g. `type T1<in in> = T1`) where syntax errors
                // take priority over semantic circularity detection.
                // Check two signals:
                // 1. node_contains_any_parse_error (from parser error positions)
                // 2. Empty type parameter names (parser recovery creates empty
                //    identifiers when reserved words like `in` appear as names)
                let has_empty_tp_name =
                    type_alias.type_parameters.as_ref().is_some_and(|tp_list| {
                        tp_list.nodes.iter().any(|&tp_idx| {
                            self.ctx
                                .arena
                                .get(tp_idx)
                                .and_then(|tp_node| self.ctx.arena.get_type_parameter(tp_node))
                                .and_then(|tp| {
                                    self.ctx
                                        .arena
                                        .get(tp.name)
                                        .and_then(|n| self.ctx.arena.get_identifier(n))
                                })
                                .is_some_and(|ident| {
                                    self.ctx.arena.resolve_identifier_text(ident).is_empty()
                                })
                        })
                    });
                let decl_has_parse_error =
                    self.node_contains_any_parse_error(decl_idx) || has_empty_tp_name;
                let circularity_eligible = flags & (symbol_flags::ALIAS | symbol_flags::NAMESPACE)
                    == 0
                    && !decl_has_parse_error;
                // When the type alias has type parameters and the body is a bare
                // self-reference (simple type reference without type arguments),
                // TSC does NOT consider it circular.  The bare `T` in
                // `type T<out out> = T` refers to the generic type constructor and
                // goes through instantiation, so it is structurally wrapped.
                let generic_self_ref =
                    !params.is_empty() && self.is_simple_type_reference(type_alias.type_node);
                let is_non_generic_mapped_cycle = params.is_empty()
                    && self.is_non_generic_mapped_type_circular(sym_id, type_alias.type_node);
                let is_circular = circularity_eligible
                    && !generic_self_ref
                    && (self.is_direct_circular_reference(
                        sym_id,
                        alias_type,
                        type_alias.type_node,
                        false,
                    ) || self.ctx.circular_type_aliases.contains(&sym_id)
                        || (self.is_simple_type_reference(type_alias.type_node)
                            && self.is_cross_file_circular_alias(sym_id, alias_type))
                        || is_non_generic_mapped_cycle);
                if is_circular && !self.has_parse_errors() {
                    use crate::diagnostics::{
                        diagnostic_codes, diagnostic_messages, format_message,
                    };

                    // Mark this alias as circular so downstream checks (TS2313)
                    // can detect constraints referencing it.
                    self.ctx.circular_type_aliases.insert(sym_id);
                    // Also mark in the shared DefinitionStore for cross-file visibility.
                    if let Some(def_id) = self.ctx.get_existing_def_id(sym_id) {
                        self.ctx.definition_store.mark_circular_def(def_id);
                    }

                    // Suppress TS2456 when the file has parse errors — the
                    // circularity may be an artifact of parser recovery (e.g.,
                    // malformed type parameter lists).  tsc does the same:
                    // it skips semantic circularity errors when syntax errors
                    // are present.
                    // Suppress TS2456 when:
                    // 1. The file has parse errors (syntax errors take priority)
                    // 2. The type alias has an import alias partner — the apparent
                    //    circularity is caused by the name conflict (TS2440 will
                    //    be emitted instead during statement checking).
                    let has_import_partner = self
                        .ctx
                        .alias_partner_for(self.ctx.binder, sym_id)
                        .and_then(|partner_id| self.ctx.binder.get_symbol(partner_id))
                        .is_some_and(|partner| partner.flags & symbol_flags::ALIAS != 0);
                    // tsc's hasParseDiagnostics() checks ALL parse diagnostics
                    // (including grammar checks like TS1359) to suppress TS2456.
                    // Our has_parse_errors only tracks "real" syntax errors, so
                    // we also check all_parse_error_positions which includes
                    // non-suppressing parse errors like TS1359.
                    let file_has_any_parse_diag =
                        self.has_parse_errors() || !self.ctx.all_parse_error_positions.is_empty();
                    // Suppress TS2456 when the type alias body provides
                    // structural wrapping (array, tuple, object literal,
                    // function, etc.). That suppression must NOT apply to the
                    // mapped-type cycle form `type T = { [K in keyof T]: ... }`:
                    // TypeScript treats the alias reference in the mapped key
                    // space as a direct circularity and still emits TS2456.
                    // We check the local AST rather than a resolved type to
                    // avoid SymbolId/arena collisions during driver-mode runs.
                    let body_is_deferred =
                        self.alias_ast_is_deferred(sym_id) && !is_non_generic_mapped_cycle;
                    if !file_has_any_parse_diag && !has_import_partner && !body_is_deferred {
                        let name = escaped_name;
                        let message = format_message(
                            diagnostic_messages::TYPE_ALIAS_CIRCULARLY_REFERENCES_ITSELF,
                            &[name],
                        );
                        // Point at the type alias name, not the entire declaration
                        self.error_at_node(
                            type_alias.name,
                            &message,
                            diagnostic_codes::TYPE_ALIAS_CIRCULARLY_REFERENCES_ITSELF,
                        );
                    }
                    let def_id = self.ctx.get_or_create_def_id(sym_id);
                    if !params.is_empty() {
                        self.ctx.insert_def_type_params(def_id, params.clone());
                    }
                    self.ctx
                        .definition_store
                        .register_type_to_def(alias_type, def_id);
                    self.ctx.definition_store.set_body(def_id, alias_type);
                    // Preserve the raw alias body so downstream consumers can
                    // continue to reason about the surrounding type graph (for
                    // example, recursive base-type diagnostics) without
                    // collapsing the branch to ERROR or hiding the original
                    // meta-type structure behind a self-lazy placeholder.
                    return (alias_type, params);
                }

                // CRITICAL FIX: Always create DefId for type aliases, not just when they have type parameters
                // This enables Lazy type resolution via TypeResolver during narrowing operations
                let def_id = self.ctx.get_or_create_def_id(sym_id);

                // Cache type parameters for Application expansion (Priority 1 fix)
                // This enables ExtractState<NumberReducer> to expand correctly
                if !params.is_empty() {
                    self.ctx.insert_def_type_params(def_id, params.clone());
                }

                // Register the object shape so diagnostics can display the type alias
                // name (e.g., "Square") instead of the structural type (e.g.,
                // "{ size: number; kind: \"sq\" }"). Mirrors the interface path above.
                if let Some(shape) = type_environment::object_shape(self.ctx.types, alias_type) {
                    self.ctx.definition_store.set_instance_shape(def_id, shape);
                }

                // Return the params that were used during lowering - this ensures
                // type_env gets the same TypeIds as the type body
                return (alias_type, params);
            }
            return (TypeId::UNKNOWN, Vec::new());
        }

        // Class property declarations: resolve type from annotation or initializer.
        if flags & symbol_flags::PROPERTY != 0
            && let Some(node) = self.ctx.arena.get(value_decl)
            && node.kind == syntax_kind_ext::PROPERTY_DECLARATION
            && let Some(prop_decl) = self.ctx.arena.get_property_decl(node)
        {
            if prop_decl.type_annotation.is_some() {
                let annotation_type = self.get_type_from_type_node(prop_decl.type_annotation);
                return (annotation_type, Vec::new());
            }
            if let Some(jsdoc_type) = self.jsdoc_type_annotation_for_node(value_decl) {
                return (jsdoc_type, Vec::new());
            }
            if prop_decl.initializer.is_some() {
                let init_type = self.get_type_of_node(prop_decl.initializer);
                return (init_type, Vec::new());
            }
        }

        // Variable - get type from annotation or infer from initializer
        if flags & (symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE)
            != 0
        {
            let mut resolved_value_decl = value_decl;

            // When a function-scoped `var` redeclares a parameter (`function f(x: A) { var x: B; }`),
            // TypeScript keeps the parameter's original value surface for later identifier reads
            // and reports the mismatch through TS2403 instead of mutating the live symbol type.
            // Preserve that by preferring the merged parameter declaration for symbol-type reads.
            if let Some(param_decl) = declarations.iter().copied().find(|&decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::PARAMETER)
            }) {
                resolved_value_decl = param_decl;
            }

            // Symbols can point at wrappers (export declarations, variable statements, or
            // declaration lists). Normalize to the concrete VariableDeclaration node.
            if resolved_value_decl.is_some() {
                if let Some(node) = self.ctx.arena.get(resolved_value_decl)
                    && node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    && let Some(export_decl) = self.ctx.arena.get_export_decl(node)
                    && export_decl.export_clause.is_some()
                {
                    resolved_value_decl = export_decl.export_clause;
                }

                if let Some(node) = self.ctx.arena.get(resolved_value_decl) {
                    if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                        if let Some(var_stmt) = self.ctx.arena.get_variable(node) {
                            'find_decl_in_stmt: for &list_idx in &var_stmt.declarations.nodes {
                                let Some(list_node) = self.ctx.arena.get(list_idx) else {
                                    continue;
                                };
                                let Some(decl_list) = self.ctx.arena.get_variable(list_node) else {
                                    continue;
                                };
                                for &decl_idx in &decl_list.declarations.nodes {
                                    let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                                        continue;
                                    };
                                    let Some(var_decl) =
                                        self.ctx.arena.get_variable_declaration(decl_node)
                                    else {
                                        continue;
                                    };
                                    let Some(name_node) = self.ctx.arena.get(var_decl.name) else {
                                        continue;
                                    };
                                    let Some(ident) = self.ctx.arena.get_identifier(name_node)
                                    else {
                                        continue;
                                    };
                                    if ident.escaped_text == escaped_name {
                                        resolved_value_decl = decl_idx;
                                        break 'find_decl_in_stmt;
                                    }
                                }
                            }
                        }
                    } else if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                        && let Some(decl_list) = self.ctx.arena.get_variable(node)
                    {
                        for &decl_idx in &decl_list.declarations.nodes {
                            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                                continue;
                            };
                            let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
                            else {
                                continue;
                            };
                            let Some(name_node) = self.ctx.arena.get(var_decl.name) else {
                                continue;
                            };
                            let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                                continue;
                            };
                            if ident.escaped_text == escaped_name {
                                resolved_value_decl = decl_idx;
                                break;
                            }
                        }
                    }
                }
            }

            // When a variable is merged with a namespace (e.g., `namespace ns { ... }` +
            // `const ns: ns.Foo`), the binder's value_declaration may point to the
            // ModuleDeclaration node instead of the VariableDeclaration. Fall back to
            // searching declarations[] for the actual variable declaration.
            //
            // IMPORTANT: Skip this fallback when resolved_value_decl was deliberately
            // set to a PARAMETER node (by the redeclaration fix above). When a `var`
            // redeclares a constructor parameter property (e.g., `constructor(public p: number)
            // { var p: string; }`), the parameter's type annotation is the canonical type
            // for the symbol. Without this guard, the fallback finds the `var` declaration
            // and uses its (incompatible) type annotation instead.
            let is_parameter_node = self
                .ctx
                .arena
                .get(resolved_value_decl)
                .is_some_and(|node| node.kind == syntax_kind_ext::PARAMETER);
            if resolved_value_decl.is_some()
                && !is_parameter_node
                && self
                    .ctx
                    .arena
                    .get(resolved_value_decl)
                    .and_then(|n| self.ctx.arena.get_variable_declaration(n))
                    .is_none()
            {
                for &decl_idx in declarations {
                    if decl_idx.is_none() {
                        continue;
                    }
                    if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                        && self.ctx.arena.get_variable_declaration(decl_node).is_some()
                    {
                        resolved_value_decl = decl_idx;
                        break;
                    }
                }
            }

            if resolved_value_decl.is_some()
                && let Some(node) = self.ctx.arena.get(resolved_value_decl)
            {
                // Check if this is a variable declaration
                if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) {
                    // First try type annotation using type-node lowering (resolves through binder).
                    if var_decl.type_annotation.is_some() {
                        let annotation_type =
                            self.get_type_from_type_node(var_decl.type_annotation);
                        // `const k: unique symbol = Symbol()` — create a proper UniqueSymbol
                        // type using the variable's binder symbol as identity.
                        if annotation_type == TypeId::SYMBOL
                            && self.is_const_variable_declaration(resolved_value_decl)
                            && self.is_unique_symbol_type_annotation(var_decl.type_annotation)
                        {
                            return (
                                self.ctx
                                    .types
                                    .unique_symbol(tsz_solver::SymbolRef(sym_id.0)),
                                Vec::new(),
                            );
                        }
                        return (annotation_type, Vec::new());
                    }
                    if let Some(jsdoc_type) =
                        self.jsdoc_type_annotation_for_node(resolved_value_decl)
                    {
                        return (jsdoc_type, Vec::new());
                    }
                    if var_decl.initializer.is_some()
                        && self.is_const_variable_declaration(resolved_value_decl)
                    {
                        // In JS files, parenthesized expressions may carry JSDoc
                        // type casts (e.g., `/** @type {*} */(null)` → any).
                        // The cast type overrides the inner literal, so compute
                        // the full initializer type first and use it when it's
                        // `any` or `unknown` (assertion results).
                        if self.ctx.is_js_file()
                            && self.ctx.arena.get(var_decl.initializer).is_some_and(|n| {
                                n.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                            })
                        {
                            let init_type = self.get_type_of_node(var_decl.initializer);
                            if init_type == TypeId::ANY || init_type == TypeId::UNKNOWN {
                                return (init_type, Vec::new());
                            }
                        }
                        if let Some(literal_type) =
                            self.literal_type_from_initializer(var_decl.initializer)
                        {
                            let literal_type = if self.ctx.is_js_file() {
                                self.augment_object_type_with_define_properties(
                                    escaped_name,
                                    literal_type,
                                )
                            } else {
                                literal_type
                            };
                            return (literal_type, Vec::new());
                        }
                    }
                    // `const k = Symbol()` — infer unique symbol type.
                    // In TypeScript, const declarations initialized with Symbol() get
                    // a unique symbol type (typeof k), not the general `symbol` type.
                    if var_decl.initializer.is_some()
                        && self.is_const_variable_declaration(resolved_value_decl)
                        && self.is_symbol_call_initializer(var_decl.initializer)
                    {
                        return (
                            self.ctx
                                .types
                                .unique_symbol(tsz_solver::SymbolRef(sym_id.0)),
                            Vec::new(),
                        );
                    }
                    // Fall back to inferring from initializer
                    if var_decl.initializer.is_some() {
                        let mut inferred_type = self.get_type_of_node(var_decl.initializer);
                        // Eagerly evaluate Application types (e.g., merge<A, B>)
                        // to concrete types. Without this, long chains like
                        //   const o50 = merge(merge(merge(...)))
                        // store deeply-nested Application trees that cause O(2^N)
                        // traversal work in subsequent type operations (inference,
                        // contains_type_parameters, ensure_application_symbols_resolved).
                        if is_generic_type(self.ctx.types, inferred_type) {
                            inferred_type = self.evaluate_application_type(inferred_type);
                        }
                        inferred_type = self.augment_callable_type_with_expandos(
                            escaped_name,
                            sym_id,
                            inferred_type,
                        );
                        if self.ctx.is_js_file() {
                            inferred_type = self.augment_object_type_with_define_properties(
                                escaped_name,
                                inferred_type,
                            );
                        }
                        let init_is_direct_empty_array = self
                            .ctx
                            .arena
                            .get(var_decl.initializer)
                            .is_some_and(|init_node| {
                                init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                    && self
                                        .ctx
                                        .arena
                                        .get_literal_expr(init_node)
                                        .is_some_and(|lit| lit.elements.nodes.is_empty())
                            });
                        if init_is_direct_empty_array
                            && array_element_type(self.ctx.types, inferred_type)
                                == Some(TypeId::NEVER)
                        {
                            inferred_type = self.ctx.types.factory().array(TypeId::ANY);
                        }
                        // Literal Widening for mutable bindings (let/var):
                        // Only widen when the initializer is a "fresh" literal expression
                        // (direct literal in source code). Types from variable references,
                        // narrowing, or computed expressions are "non-fresh" and NOT widened.
                        // `let x = "div" as const` should have type "div", not string.
                        if !self.is_const_variable_declaration(resolved_value_decl)
                            && !self.is_const_assertion_initializer(var_decl.initializer)
                        {
                            let widened_type =
                                if self.is_fresh_literal_expression(var_decl.initializer) {
                                    self.widen_initializer_type_for_mutable_binding(inferred_type)
                                } else {
                                    inferred_type
                                };
                            // Route null/undefined widening through the flow observation boundary.
                            let final_type = flow_boundary::widen_null_undefined_to_any(
                                self.ctx.types,
                                widened_type,
                                self.ctx.strict_null_checks(),
                            );
                            return (final_type, Vec::new());
                        }
                        return (inferred_type, Vec::new());
                    }

                    // For-of/for-in loop variable: no syntactic initializer, but the
                    // type comes from the iterable expression.  Eagerly compute the
                    // element type so that definite-assignment analysis (TS2454) sees
                    // the real type instead of the fallback `any`.
                    if let Some(for_element_type) =
                        self.compute_for_in_of_variable_type(resolved_value_decl)
                    {
                        let widened = if !self.ctx.compiler_options.sound_mode {
                            crate::query_boundaries::common::widen_freshness(
                                self.ctx.types,
                                for_element_type,
                            )
                        } else {
                            for_element_type
                        };
                        return (widened, Vec::new());
                    }
                }
                // Check if this is a function parameter
                else if let Some(param) = self.ctx.arena.get_parameter(node) {
                    // Get type from annotation
                    if param.type_annotation.is_some() {
                        let mut type_id = self.get_type_from_type_node(param.type_annotation);
                        // Under strictNullChecks, optional parameters (?) include undefined
                        // in their type. E.g., `n?: number` has type `number | undefined`.
                        if param.question_token
                            && self.ctx.strict_null_checks()
                            && type_id != TypeId::ANY
                            && type_id != TypeId::UNKNOWN
                            && type_id != TypeId::ERROR
                        {
                            type_id = factory.union2(type_id, TypeId::UNDEFINED);
                        }
                        return (type_id, Vec::new());
                    }
                    // Check for inline JSDoc @type on the parameter itself
                    if let Some(jsdoc_type) =
                        self.jsdoc_type_annotation_for_node(resolved_value_decl)
                    {
                        return (jsdoc_type, Vec::new());
                    }
                    // In JS files, check the parent function's JSDoc for @param {Type} name.
                    // The @param tag lives on the function declaration, not on the parameter.
                    if self.is_js_file() {
                        let pname = self.parameter_name_for_error(param.name);
                        // Walk up the parent chain to find the enclosing function node
                        // AST structure: Parameter -> SyntaxList -> FunctionDeclaration
                        let mut current = resolved_value_decl;
                        for _ in 0..4 {
                            if let Some(ext) = self.ctx.arena.get_extended(current)
                                && ext.parent.is_some()
                            {
                                current = ext.parent;
                                if let Some(comment_start) =
                                    self.get_jsdoc_comment_pos_for_function(current)
                                    && let Some(func_jsdoc) = self.get_jsdoc_for_function(current)
                                    && let Some(jsdoc_type) = self
                                        .resolve_jsdoc_param_type_with_pos(
                                            &func_jsdoc,
                                            &pname,
                                            Some(comment_start),
                                        )
                                {
                                    return (jsdoc_type, Vec::new());
                                }
                            } else {
                                break;
                            }
                        }
                    }
                    if let Some(contextual_type) =
                        self.contextual_parameter_type_from_enclosing_function(resolved_value_decl)
                    {
                        return (contextual_type, Vec::new());
                    }
                    // Fall back to inferring from initializer (default value)
                    if param.initializer.is_some() {
                        return (self.get_type_of_node(param.initializer), Vec::new());
                    }
                }
            }
            // Binding element from variable declaration destructuring:
            // `let { a, ...rest } = expr` — resolve element type from initializer.
            if resolved_value_decl.is_some()
                && let Some(t) = self.resolve_binding_element_from_variable_initializer(
                    resolved_value_decl,
                    escaped_name,
                )
            {
                return (t, Vec::new());
            }
            if resolved_value_decl.is_some()
                && let Some(t) = self
                    .resolve_binding_element_from_annotated_param(resolved_value_decl, escaped_name)
            {
                return (t, Vec::new());
            }
            // Variable without type annotation or initializer gets implicit 'any'
            // This prevents cascading TS2571 errors
            return (TypeId::ANY, Vec::new());
        }

        // Alias - resolve the aliased type (import x = ns.member or ES6 imports)
        if flags & symbol_flags::ALIAS != 0 {
            if value_decl.is_some()
                && let Some(node) = self.ctx.arena.get(value_decl)
                && node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                && let Some(import) = self.ctx.arena.get_import_decl(node)
            {
                // Check for require() call FIRST — `import A = require('M')` must
                // resolve through module_exports, not qualified symbol resolution.
                // The qualified symbol path is for `import x = ns.member` patterns.
                if let Some(module_specifier) =
                    self.get_require_module_specifier(import.module_specifier)
                {
                    if let Some(module_type) = self.commonjs_module_value_type(
                        &module_specifier,
                        Some(self.ctx.current_file_idx),
                    ) {
                        return (module_type, Vec::new());
                    }

                    // Resolve the canonical export surface (module-specifier variants,
                    // cross-file tables, and export= member merging).
                    let exports_table = self.resolve_effective_module_exports(&module_specifier);

                    if let Some(exports_table) = exports_table {
                        // Create an object type with all the module's exports
                        use tsz_solver::PropertyInfo;
                        let module_is_non_module_entity = self
                            .ctx
                            .module_resolves_to_non_module_entity(&module_specifier);
                        let ordered_exports = self.ordered_namespace_export_entries(&exports_table);
                        // Record cross-file symbol targets so delegate_cross_arena_symbol_resolution
                        // can find the correct arena for symbols from ambient modules.
                        for &(name, sym_id) in &ordered_exports {
                            self.record_cross_file_symbol_if_needed(
                                sym_id,
                                name,
                                &module_specifier,
                            );
                        }
                        let exports_table_target =
                            ordered_exports.iter().find_map(|(_, export_sym_id)| {
                                self.ctx.resolve_symbol_file_index(*export_sym_id)
                            });
                        let mut export_equals_type =
                            exports_table.get("export=").map(|export_equals_sym| {
                                let export_equals_type = self.get_type_of_symbol(export_equals_sym);
                                self.widen_type_for_display(export_equals_type)
                            });
                        // Handle `export { X as "module.exports" }` as export-equals.
                        if export_equals_type.is_none()
                            && let Some(module_exports_sym) = exports_table.get("module.exports")
                        {
                            let me_type = self.get_type_of_symbol(module_exports_sym);
                            export_equals_type = Some(self.widen_type_for_display(me_type));
                        }
                        let surface = exports_table_target
                            .map(|target_idx| self.resolve_js_export_surface(target_idx))
                            .or_else(|| {
                                self.resolve_js_export_surface_for_module(
                                    &module_specifier,
                                    Some(self.ctx.current_file_idx),
                                )
                            });
                        let mut props: Vec<PropertyInfo> = if surface
                            .as_ref()
                            .is_some_and(|s| s.has_commonjs_exports)
                        {
                            let mut named_exports = surface
                                .as_ref()
                                .map(|s| s.named_exports.clone())
                                .unwrap_or_default();
                            Self::normalize_namespace_export_declaration_order(&mut named_exports);
                            if let Some(surface_direct_type) =
                                surface.as_ref().and_then(|s| s.direct_export_type)
                            {
                                export_equals_type = Some(surface_direct_type);
                            }
                            named_exports
                        } else {
                            let mut props: Vec<PropertyInfo> = Vec::new();
                            for &(name, sym_id) in &ordered_exports {
                                if self.should_skip_namespace_export_name(
                                    &exports_table,
                                    name,
                                    sym_id,
                                ) {
                                    continue;
                                }
                                // Skip type-only, wildcard-type-only, value-less, and
                                // transitively type-only exports (e.g., re-exported from
                                // a module that uses `export type { X }`).
                                if self.is_type_only_export_symbol(sym_id)
                                    || self
                                        .is_export_from_type_only_wildcard(&module_specifier, name)
                                    || self.export_symbol_has_no_value(sym_id)
                                    || self.is_export_type_only_from_file(
                                        &module_specifier,
                                        name,
                                        None,
                                    )
                                {
                                    continue;
                                }
                                let mut prop_type = self.get_type_of_symbol(sym_id);
                                prop_type = self.apply_module_augmentations(
                                    &module_specifier,
                                    name,
                                    prop_type,
                                );
                                let declaration_order = if name == "default" {
                                    1
                                } else {
                                    props.len() as u32 + 2
                                };
                                let name_atom = self.ctx.types.intern_string(name);
                                props.push(PropertyInfo {
                                    name: name_atom,
                                    type_id: prop_type,
                                    write_type: prop_type,
                                    optional: false,
                                    readonly: false,
                                    is_method: false,
                                    is_class_prototype: false,
                                    visibility: Visibility::Public,
                                    parent_id: None,
                                    declaration_order,
                                    is_string_named: false,
                                });
                            }
                            props
                        };

                        if !module_is_non_module_entity {
                            for aug_name in
                                self.collect_module_augmentation_names(&module_specifier)
                            {
                                let name_atom = self.ctx.types.intern_string(&aug_name);
                                if props.iter().any(|p| p.name == name_atom) {
                                    continue;
                                }
                                props.push(PropertyInfo {
                                    name: name_atom,
                                    type_id: TypeId::ANY,
                                    write_type: TypeId::ANY,
                                    optional: false,
                                    readonly: false,
                                    is_method: false,
                                    is_class_prototype: false,
                                    visibility: Visibility::Public,
                                    parent_id: None,
                                    declaration_order: 0,
                                    is_string_named: false,
                                });
                            }
                        }
                        Self::normalize_namespace_export_declaration_order(&mut props);
                        let namespace_has_no_runtime_props = props.is_empty();
                        let namespace_type = factory.object(props);
                        // Store display name for error messages: TSC shows namespace
                        // types as `typeof import("module")` in diagnostics.
                        let display_module_name = self.resolve_namespace_display_module_name(
                            &exports_table,
                            &module_specifier,
                        );
                        self.ctx
                            .namespace_module_names
                            .insert(namespace_type, display_module_name);
                        if let Some(export_equals_type) = export_equals_type {
                            if module_is_non_module_entity || namespace_has_no_runtime_props {
                                return (export_equals_type, Vec::new());
                            }
                            return (
                                factory.intersection2(export_equals_type, namespace_type),
                                Vec::new(),
                            );
                        }

                        return (namespace_type, Vec::new());
                    }
                    // Use unified JS export surface for CommonJS fallback
                    if let Some(surface) = self.resolve_js_export_surface_for_module(
                        &module_specifier,
                        Some(self.ctx.current_file_idx),
                    ) && surface.has_commonjs_exports
                    {
                        let display_name =
                            self.imported_namespace_display_module_name(&module_specifier);
                        if let Some(type_id) =
                            surface.to_type_id_with_display_name(self, Some(display_name))
                        {
                            return (type_id, Vec::new());
                        }
                    }
                    self.emit_module_not_found_error(&module_specifier, value_decl);
                    return (TypeId::ANY, Vec::new());
                }
                // Not a require() call — try qualified symbol resolution
                // for `import x = ns.member` patterns.
                if let Some(target_sym) = self.resolve_qualified_symbol(import.module_specifier) {
                    // When the target has both TYPE_ALIAS and VALUE flags
                    // (e.g., `import X = NS.Foo` where NS has both `type Foo = ...` and
                    // `const Foo = ...`), cache the type alias body for type contexts and
                    // return the value type. This mirrors the TYPE_ALIAS + ALIAS merge
                    // logic used for ES6 imports.
                    let target_info = self
                        .get_symbol_globally(target_sym)
                        .map(|s| (s.flags, s.value_declaration));
                    if let Some((tflags, vd)) = target_info
                        && tflags & symbol_flags::TYPE_ALIAS != 0
                        && tflags & symbol_flags::VALUE != 0
                    {
                        let ta_type = self.get_type_of_symbol(target_sym);
                        self.ctx.import_type_alias_types.insert(sym_id, ta_type);
                        let val_type = self.type_of_value_declaration_for_symbol(target_sym, vd);
                        return (val_type, Vec::new());
                    }
                    return (self.get_type_of_symbol(target_sym), Vec::new());
                }
                // Namespace import failed to resolve
                // Check for TS2694 (Namespace has no exported member) or TS2304 (Cannot find name)
                // This happens when: import Alias = NS.NotExported (where NotExported is not exported)

                // 1. Check for TS2694 (Namespace has no exported member)
                // But suppress when the left part resolves to a pure interface that
                // shadows an outer namespace which has the member (tsc uses namespace
                // meaning for import-equals entity name resolution).
                let suppress_ts2694 =
                    self.check_import_qualified_shadows_namespace(import.module_specifier);
                if !suppress_ts2694
                    && self.report_type_query_missing_member(import.module_specifier)
                {
                    return (TypeId::ERROR, Vec::new());
                }

                // 2. Check for TS2304 (Cannot find name) for the left-most part
                if let Some(missing_idx) = self.missing_type_query_left(import.module_specifier) {
                    // Suppress if it's an unresolved import (TS2307 already emitted)
                    if !self.is_unresolved_import_symbol(missing_idx)
                        && let Some(name) = self.entity_name_text(missing_idx)
                    {
                        // Route through boundary for TS2304/TS2552 with suggestion collection
                        self.report_not_found_at_boundary(
                            &name,
                            missing_idx,
                            crate::query_boundaries::name_resolution::NameLookupKind::Value,
                        );
                    }
                    return (TypeId::ERROR, Vec::new());
                }

                // Return ERROR for other cases to prevent cascading errors
                return (TypeId::ERROR, Vec::new());
            }

            // Synthetic `export default ...` aliases point directly at the exported
            // declaration node (often an anonymous class/function). They are real
            // value symbols, not unresolved imports, so compute the declaration type
            // instead of falling through to the generic alias-any path.
            if import_module.is_none()
                && value_decl.is_some()
                && let Some(node) = self.ctx.arena.get(value_decl)
                && !matches!(
                    node.kind,
                    syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                        | syntax_kind_ext::IMPORT_SPECIFIER
                        | syntax_kind_ext::EXPORT_SPECIFIER
                        | syntax_kind_ext::IMPORT_CLAUSE
                        | syntax_kind_ext::NAMESPACE_IMPORT
                )
            {
                return (
                    self.type_of_value_declaration_for_symbol(sym_id, value_decl),
                    Vec::new(),
                );
            }

            let has_local_non_import_declaration = declarations.iter().copied().any(|decl_idx| {
                if self.ctx.binder.node_symbols.get(&decl_idx.0) != Some(&sym_id) {
                    return false;
                }
                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    return false;
                };
                if matches!(
                    node.kind,
                    syntax_kind_ext::IMPORT_SPECIFIER
                        | syntax_kind_ext::EXPORT_SPECIFIER
                        | syntax_kind_ext::IMPORT_CLAUSE
                        | syntax_kind_ext::NAMESPACE_IMPORT
                        | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                ) {
                    return false;
                }
                if node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                    return true;
                }
                self.ctx
                    .arena
                    .get_export_decl(node)
                    .is_some_and(|export_decl| {
                        export_decl.module_specifier.is_none() || export_decl.is_default_export
                    })
            });

            if import_module.is_some() && has_local_non_import_declaration && value_decl.is_some() {
                return (
                    self.type_of_value_declaration_for_symbol(sym_id, value_decl),
                    Vec::new(),
                );
            }

            // Handle ES6 named imports (import { X } from './module')
            // Use the import_module field to resolve to the actual export
            // Check if this symbol has import tracking metadata

            // For ES6 imports with import_module set, resolve using module_exports
            if let Some(module_name) = import_module {
                let import_source_file_idx = self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .and_then(|symbol| {
                        (symbol.decl_file_idx != u32::MAX).then_some(symbol.decl_file_idx as usize)
                    })
                    .or(Some(self.ctx.current_file_idx));

                // Check if this is a shorthand ambient module (declare module "foo" without body)
                // Imports from shorthand ambient modules are typed as `any`
                if self
                    .ctx
                    .binder
                    .shorthand_ambient_modules
                    .contains(module_name)
                {
                    return (TypeId::ANY, Vec::new());
                }

                // Check if this is a namespace import (import * as ns) or
                // namespace re-export (export * as ns from 'mod').
                // Namespace imports have import_name = None, namespace
                // re-exports have import_name = Some("*").
                if import_name.is_none() || import_name.as_deref() == Some("*") {
                    if let Some(json_namespace_type) = self
                        .json_module_namespace_type_for_module(module_name, import_source_file_idx)
                    {
                        return (json_namespace_type, Vec::new());
                    }

                    // This is a namespace import: import * as ns from 'module'
                    // Create an object type containing all module exports

                    // Guard: if we're already computing this module's namespace type,
                    // we've hit a circular module import (e.g. prop-types <-> react).
                    // Return `any` to break the cycle, matching tsc's behavior for
                    // circular module references.
                    if self
                        .ctx
                        .module_namespace_resolution_set
                        .contains(module_name)
                    {
                        return (TypeId::ANY, Vec::new());
                    }
                    self.ctx
                        .module_namespace_resolution_set
                        .insert(module_name.to_string());

                    // For cross-file symbols (e.g., `export * as ns from './b'` in
                    // another file), the module_name is relative to the declaring file,
                    // not the current file. Use the symbol's declaring file for resolution.
                    let declaring_file_idx = self.ctx.resolve_symbol_file_index(sym_id);
                    let exports_table = self.resolve_effective_module_exports_from_file(
                        module_name,
                        declaring_file_idx,
                    );
                    if let Some(exports_table) = exports_table {
                        let ordered_exports = self.ordered_namespace_export_entries(&exports_table);
                        // Record cross-file symbol targets for all symbols in the table
                        for &(name, sym_id) in &ordered_exports {
                            self.record_cross_file_symbol_if_needed(sym_id, name, module_name);
                        }

                        use tsz_solver::PropertyInfo;
                        let module_is_non_module_entity =
                            self.ctx.module_resolves_to_non_module_entity(module_name);
                        let exports_table_target =
                            ordered_exports.iter().find_map(|(_, export_sym_id)| {
                                self.ctx.resolve_symbol_file_index(*export_sym_id)
                            });
                        let mut export_equals_type =
                            exports_table.get("export=").map(|export_equals_sym| {
                                let export_equals_type = self.get_type_of_symbol(export_equals_sym);
                                self.widen_type_for_display(export_equals_type)
                            });
                        // TypeScript allows `export { X as "module.exports" }` in ESM.
                        // This acts like `export = X` for CJS interop: the namespace
                        // import gets construct/call signatures from X.
                        if export_equals_type.is_none()
                            && let Some(module_exports_sym) = exports_table.get("module.exports")
                        {
                            let me_type = self.get_type_of_symbol(module_exports_sym);
                            export_equals_type = Some(self.widen_type_for_display(me_type));
                        }
                        let surface = exports_table_target
                            .map(|target_idx| self.resolve_js_export_surface(target_idx))
                            .or_else(|| {
                                self.resolve_js_export_surface_for_module(
                                    module_name,
                                    declaring_file_idx,
                                )
                            });
                        let mut props: Vec<PropertyInfo> = if surface
                            .as_ref()
                            .is_some_and(|s| s.has_commonjs_exports)
                        {
                            let mut named_exports = surface
                                .as_ref()
                                .map(|s| s.named_exports.clone())
                                .unwrap_or_default();
                            Self::normalize_namespace_export_declaration_order(&mut named_exports);
                            if export_equals_type.is_none() {
                                export_equals_type =
                                    surface.as_ref().and_then(|s| s.direct_export_type);
                            }
                            named_exports
                        } else {
                            let mut props: Vec<PropertyInfo> = Vec::new();
                            for &(name, export_sym_id) in &ordered_exports {
                                if self.should_skip_namespace_export_name(
                                    &exports_table,
                                    name,
                                    export_sym_id,
                                ) {
                                    continue;
                                }
                                // Skip type-only exports (`export type { A }`), exports
                                // reached through `export type *` wildcards, exports
                                // that are intrinsically type-only (type aliases, interfaces
                                // without merged values), and transitively type-only
                                // exports (re-exported from a `export type` chain).
                                if self.is_type_only_export_symbol(export_sym_id)
                                    || self.is_export_from_type_only_wildcard(module_name, name)
                                    || self.export_symbol_has_no_value(export_sym_id)
                                    || self.is_export_type_only_from_file(
                                        module_name,
                                        name,
                                        declaring_file_idx,
                                    )
                                {
                                    continue;
                                }
                                let mut prop_type = self.get_type_of_symbol(export_sym_id);

                                // Rule #44: Apply module augmentations to each exported type
                                prop_type =
                                    self.apply_module_augmentations(module_name, name, prop_type);

                                let declaration_order = if name == "default" {
                                    1
                                } else {
                                    props.len() as u32 + 2
                                };
                                let name_atom = self.ctx.types.intern_string(name);
                                props.push(PropertyInfo {
                                    name: name_atom,
                                    type_id: prop_type,
                                    write_type: prop_type,
                                    optional: false,
                                    readonly: false,
                                    is_method: false,
                                    is_class_prototype: false,
                                    visibility: Visibility::Public,
                                    parent_id: None,
                                    declaration_order,
                                    is_string_named: false,
                                });
                            }
                            props
                        };

                        self.append_export_equals_import_type_namespace_props(
                            module_name,
                            declaring_file_idx,
                            &exports_table,
                            &mut props,
                        );

                        // Add augmentation declarations that introduce entirely new names.
                        // If the target resolves to a non-module export= value, these names
                        // are invalid and should not be surfaced on the namespace.
                        if !module_is_non_module_entity {
                            for aug_name in self.collect_module_augmentation_names(module_name) {
                                let name_atom = self.ctx.types.intern_string(&aug_name);
                                if props.iter().any(|p| p.name == name_atom) {
                                    continue;
                                }
                                props.push(PropertyInfo {
                                    name: name_atom,
                                    // Cross-file augmentation declarations may live in a different
                                    // arena; use `any` here to preserve namespace member visibility.
                                    type_id: TypeId::ANY,
                                    write_type: TypeId::ANY,
                                    optional: false,
                                    readonly: false,
                                    is_method: false,
                                    is_class_prototype: false,
                                    visibility: Visibility::Public,
                                    parent_id: None,
                                    declaration_order: 0,
                                    is_string_named: false,
                                });
                            }
                        }

                        if let Some(source_idx) = declaring_file_idx.or(import_source_file_idx)
                            && let Some(umd_name) =
                                self.resolve_umd_namespace_name_for_module(module_name, source_idx)
                        {
                            for (name, member_sym_id) in
                                self.collect_namespace_exports_across_binders(&umd_name)
                            {
                                let name_atom = self.ctx.types.intern_string(&name);
                                if props.iter().any(|p| p.name == name_atom) {
                                    continue;
                                }

                                let prop_type = self.get_type_of_symbol(member_sym_id);
                                props.push(PropertyInfo {
                                    name: name_atom,
                                    type_id: prop_type,
                                    write_type: prop_type,
                                    optional: false,
                                    readonly: false,
                                    is_method: false,
                                    is_class_prototype: false,
                                    visibility: Visibility::Public,
                                    parent_id: None,
                                    declaration_order: 0,
                                    is_string_named: false,
                                });
                            }
                        }

                        let is_import_equals_alias =
                            self.ctx.arena.get(value_decl).is_some_and(|node| {
                                node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                            });

                        let allow_namespace_default = self
                            .source_file_import_uses_system_default_namespace_fallback(module_name)
                            || (!self.ctx.compiler_options.module.is_node_module()
                                && self.ctx.allow_synthetic_default_imports()
                                && !is_import_equals_alias
                                && export_equals_type.is_some())
                            || (self.ctx.compiler_options.module.is_node_module()
                                && self.ctx.file_is_esm == Some(true)
                                && !self.module_is_esm(module_name)
                                && self.module_can_use_synthetic_default_import(module_name));

                        // Namespace imports in CJS-fallback mode need a synthetic
                        // required `default` that points to the module object surface.
                        // This must be present even when there is no explicit `export =`,
                        // and it must override an existing `default` property exported
                        // as a regular value (e.g. `exports.default = "x"`), so that
                        // `import * as ns from "./m.cjs"; ns.default.a` is valid.
                        if allow_namespace_default {
                            let default_atom = self.ctx.types.intern_string("default");
                            let can_use_cjs_namespace_default =
                                self.module_can_use_synthetic_default_import(module_name);
                            let has_named_default_prop =
                                props.iter().any(|p| p.name == default_atom);
                            let synthetic_default_type =
                                if can_use_cjs_namespace_default && has_named_default_prop {
                                    let mut synthetic_props = props.clone();
                                    Self::normalize_namespace_export_declaration_order(
                                        &mut synthetic_props,
                                    );
                                    Some(factory.object(synthetic_props))
                                } else {
                                    export_equals_type.or_else(|| {
                                        can_use_cjs_namespace_default.then(|| {
                                            let mut synthetic_props = props.clone();
                                            Self::normalize_namespace_export_declaration_order(
                                                &mut synthetic_props,
                                            );
                                            factory.object(synthetic_props)
                                        })
                                    })
                                };
                            if let Some(eq_type) = synthetic_default_type {
                                if let Some(existing_default) =
                                    props.iter_mut().find(|p| p.name == default_atom)
                                {
                                    existing_default.type_id = eq_type;
                                    existing_default.write_type = eq_type;
                                    existing_default.optional = false;
                                    existing_default.readonly = false;
                                } else {
                                    props.push(PropertyInfo {
                                        name: default_atom,
                                        type_id: eq_type,
                                        write_type: eq_type,
                                        optional: false,
                                        readonly: false,
                                        is_method: false,
                                        is_class_prototype: false,
                                        visibility: Visibility::Public,
                                        parent_id: None,
                                        declaration_order: 1,
                                        is_string_named: false,
                                    });
                                }
                            }
                        }

                        Self::normalize_namespace_export_declaration_order(&mut props);
                        let namespace_has_no_runtime_props = props.is_empty();
                        let namespace_type = factory.object(props);
                        // Store display name for error messages: TSC shows namespace
                        // types as `typeof import("module")` in diagnostics.
                        let preserve_namespace_display =
                            !(module_is_non_module_entity && allow_namespace_default);
                        if preserve_namespace_display {
                            self.ctx.namespace_module_names.insert(
                                namespace_type,
                                self.imported_namespace_display_module_name(module_name),
                            );
                        }
                        self.ctx.module_namespace_resolution_set.remove(module_name);
                        if let Some(export_equals_type) = export_equals_type {
                            if module_is_non_module_entity || namespace_has_no_runtime_props {
                                // For namespace imports of `export =` non-module values:
                                // - Callable/constructable types (functions, classes): wrap
                                //   with `{ default: value }` under allowSyntheticDefaultImports
                                //   so that `ns.default()` works (tsc behavior).
                                // - Non-callable primitives (`number | undefined`, etc.):
                                //   return the export= type directly so narrowing works
                                //   (e.g., `if (b0) x = b0` narrows `number | undefined`).
                                let is_object_like =
                                    crate::query_boundaries::dispatch::is_object_like_type(
                                        self.ctx.types,
                                        export_equals_type,
                                    );
                                let is_callable_like =
                                    crate::query_boundaries::common::is_callable_type(
                                        self.ctx.types,
                                        export_equals_type,
                                    );
                                if allow_namespace_default && (is_object_like || is_callable_like) {
                                    return (namespace_type, Vec::new());
                                }
                                return (export_equals_type, Vec::new());
                            }
                            return (
                                factory.intersection2(export_equals_type, namespace_type),
                                Vec::new(),
                            );
                        }

                        return (namespace_type, Vec::new());
                    }
                    // Module not found - emit TS2307 error and return ANY
                    // TypeScript treats unresolved imports as `any` to avoid cascading errors
                    self.ctx.module_namespace_resolution_set.remove(module_name);
                    self.emit_module_not_found_error(module_name, value_decl);
                    return (TypeId::ANY, Vec::new());
                }

                // This is a named import: import { X } from 'module'
                // Use import_name if set (for renamed imports), otherwise use escaped_name
                let export_name = import_name.as_deref().unwrap_or(escaped_name);

                if export_name == "default"
                    && let Some(json_type) =
                        self.json_module_type_for_module(module_name, import_source_file_idx)
                {
                    return (json_type, Vec::new());
                }

                // For default imports from CommonJS `export =` modules in JS/checkJs,
                // prefer the direct CommonJS export surface type when available.
                // This avoids collapsing the imported binding to `any` when the
                // synthetic `export=` symbol has an imprecise type.
                if export_name == "default"
                    && let Some(surface) = self.resolve_js_export_surface_for_module(
                        module_name,
                        Some(self.ctx.current_file_idx),
                    )
                    && surface.has_commonjs_exports
                    && let Some(direct_export_type) = surface.direct_export_type
                    && direct_export_type != TypeId::ANY
                    && direct_export_type != TypeId::UNKNOWN
                    && direct_export_type != TypeId::ERROR
                {
                    let direct_export_type = crate::query_boundaries::common::widen_literal_type(
                        self.ctx.types,
                        direct_export_type,
                    );
                    let direct_export_type =
                        self.widen_fresh_object_literal_properties_for_display(direct_export_type);
                    return (direct_export_type, Vec::new());
                }

                // In node16/nodenext, when an ESM file default-imports a CJS module,
                // the default binding is the entire module namespace (module.exports),
                // not the "default" export. This matches tsc's behavior where Node.js
                // ESM-CJS interop wraps the CJS module.
                let is_node_esm_importing_cjs = export_name == "default"
                    && self.ctx.compiler_options.module.is_node_module()
                    && self.ctx.file_is_esm == Some(true)
                    && !self.module_is_esm(module_name);

                if export_name == "default"
                    && (self.source_file_import_uses_system_default_namespace_fallback(module_name)
                        || is_node_esm_importing_cjs)
                {
                    if self
                        .ctx
                        .module_namespace_resolution_set
                        .contains(module_name)
                    {
                        return (TypeId::ANY, Vec::new());
                    }
                    self.ctx
                        .module_namespace_resolution_set
                        .insert(module_name.to_string());

                    if let Some(exports_table) = self.resolve_effective_module_exports(module_name)
                    {
                        let ordered_exports = self.ordered_namespace_export_entries(&exports_table);
                        if exports_table.has("export=")
                            && let Some(export_eq_sym) = exports_table.get("export=")
                        {
                            let export_eq_type = self.get_type_of_symbol(export_eq_sym);
                            let export_eq_type =
                                crate::query_boundaries::common::widen_literal_type(
                                    self.ctx.types,
                                    export_eq_type,
                                );
                            let export_eq_type = self
                                .widen_fresh_object_literal_properties_for_display(export_eq_type);
                            self.ctx.module_namespace_resolution_set.remove(module_name);
                            return (export_eq_type, Vec::new());
                        }

                        use tsz_solver::PropertyInfo;
                        let mut props: Vec<PropertyInfo> = Vec::new();
                        for &(name, export_sym_id) in &ordered_exports {
                            if self.should_skip_namespace_export_name(
                                &exports_table,
                                name,
                                export_sym_id,
                            ) {
                                continue;
                            }
                            let declaration_order = if name == "default" {
                                1
                            } else {
                                props.len() as u32 + 2
                            };
                            let prop_type = self.get_type_of_symbol(export_sym_id);
                            let name_atom = self.ctx.types.intern_string(name);
                            props.push(PropertyInfo {
                                name: name_atom,
                                type_id: prop_type,
                                write_type: prop_type,
                                optional: false,
                                readonly: false,
                                is_method: false,
                                is_class_prototype: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                                declaration_order,
                                is_string_named: false,
                            });
                        }
                        Self::normalize_namespace_export_declaration_order(&mut props);
                        let module_type = factory.object(props);
                        self.ctx.namespace_module_names.insert(
                            module_type,
                            self.imported_namespace_display_module_name(module_name),
                        );
                        self.ctx.module_namespace_resolution_set.remove(module_name);
                        return (module_type, Vec::new());
                    }

                    self.ctx.module_namespace_resolution_set.remove(module_name);
                }

                // Check if the module exists first (for proper error differentiation)
                let module_exists = self
                    .ctx
                    .module_exports_contains_module(self.ctx.binder, module_name)
                    || self.module_exists_cross_file(module_name);
                if self.is_ambient_module_match(module_name) && !module_exists {
                    return (TypeId::ANY, Vec::new());
                }

                // CommonJS object-literal/property exports are the concrete runtime
                // export surface. A module augmentation may introduce a duplicate
                // symbol with the same name, but it must not replace the JS export's
                // value type.
                if let Some(result) = self.resolve_js_export_named_type(
                    module_name,
                    export_name,
                    Some(self.ctx.current_file_idx),
                ) {
                    return (result, Vec::new());
                }

                // First, try local binder's module_exports
                let cross_file_result = self.resolve_cross_file_export(module_name, export_name);
                let export_sym_id = cross_file_result
                    .or_else(|| {
                        self.ctx
                            .binder
                            .module_exports
                            .get(module_name)
                            .and_then(|exports_table| exports_table.get(export_name))
                    })
                    .or_else(|| {
                        self.resolve_named_export_via_export_equals(module_name, export_name)
                    })
                    .or_else(|| {
                        let mut visited_aliases = AliasCycleTracker::new();
                        self.resolve_reexported_member_symbol(
                            module_name,
                            export_name,
                            &mut visited_aliases,
                        )
                    });

                if let Some(export_sym_id) = export_sym_id {
                    // Detect cross-file SymbolIds: the driver copies target file's
                    // module_exports into the local binder, so SymbolIds may be from
                    // another binder. Check if the SymbolId maps to the expected name
                    // in the current binder — if not, it's from another file.
                    self.record_cross_file_symbol_if_needed(
                        export_sym_id,
                        export_name,
                        module_name,
                    );

                    // TYPE_ALIAS + ALIAS merge: cache type alias body for type contexts,
                    // return namespace type for value contexts.
                    if let Some(alias_id) =
                        self.ctx.alias_partner_for(self.ctx.binder, export_sym_id)
                    {
                        let ta_type = self.get_type_of_symbol(export_sym_id);
                        self.ctx.import_type_alias_types.insert(sym_id, ta_type);
                        self.record_cross_file_symbol_if_needed(alias_id, export_name, module_name);
                        let mut result = self.get_type_of_symbol(alias_id);
                        result = self.apply_module_augmentations(module_name, export_name, result);
                        if export_name == "default" {
                            result = self.widen_type_for_display(result);
                        }
                        let should_cache_on_export_symbol =
                            self.get_symbol_globally(export_sym_id).is_none_or(|sym| {
                                (sym.flags & symbol_flags::TYPE) == 0
                                    || (sym.flags & symbol_flags::VALUE) == 0
                            });
                        if should_cache_on_export_symbol {
                            self.ctx.symbol_types.insert(export_sym_id, result);
                        }
                        return (result, Vec::new());
                    }

                    // When the export symbol has both INTERFACE and VALUE
                    // flags (e.g., `interface MyFunction` + `export const
                    // MyFunction`, or `interface MyMixin` + `export function
                    // MyMixin`), `get_type_of_symbol` returns the interface
                    // type because INTERFACE is checked first. For import
                    // aliases (value position), we need the variable/function
                    // type so the imported binding is callable/constructable.
                    let mut result = if let Some(sym) = self.get_symbol_globally(export_sym_id) {
                        let has_interface = sym.flags & symbol_flags::INTERFACE != 0;
                        let has_value = sym.flags
                            & (symbol_flags::FUNCTION_SCOPED_VARIABLE
                                | symbol_flags::BLOCK_SCOPED_VARIABLE
                                | symbol_flags::FUNCTION)
                            != 0;
                        if has_interface && has_value && sym.value_declaration.is_some() {
                            let vd = sym.value_declaration;
                            let vd_type =
                                self.type_of_value_declaration_for_symbol(export_sym_id, vd);
                            if vd_type != TypeId::UNKNOWN && vd_type != TypeId::ERROR {
                                vd_type
                            } else {
                                self.get_type_of_symbol(export_sym_id)
                            }
                        } else {
                            self.get_type_of_symbol(export_sym_id)
                        }
                    } else {
                        self.get_type_of_symbol(export_sym_id)
                    };
                    result = self.apply_module_augmentations(module_name, export_name, result);
                    if export_name == "default" {
                        result = crate::query_boundaries::common::widen_literal_type(
                            self.ctx.types,
                            result,
                        );
                        result = self.widen_fresh_object_literal_properties_for_display(result);
                    }
                    let should_cache_on_export_symbol =
                        self.get_symbol_globally(export_sym_id).is_none_or(|sym| {
                            (sym.flags & symbol_flags::TYPE) == 0
                                || (sym.flags & symbol_flags::VALUE) == 0
                        });
                    if should_cache_on_export_symbol {
                        self.ctx.symbol_types.insert(export_sym_id, result);
                    }
                    return (result, Vec::new());
                }

                // Module augmentations can introduce named exports that don't appear
                // in the base module export table. Treat those names as resolvable.
                if self
                    .ctx
                    .binder
                    .module_augmentations
                    .get(module_name)
                    .is_some_and(|augs| augs.iter().any(|aug| aug.name == *export_name))
                {
                    let mut result = TypeId::ANY;
                    result = self.apply_module_augmentations(module_name, export_name, result);
                    return (result, Vec::new());
                }

                // If the module resolved externally but isn't part of the program,
                // skip export member validation (treat as `any`).
                let has_exports_table = self
                    .ctx
                    .module_exports_contains_module(self.ctx.binder, module_name)
                    || self.resolve_effective_module_exports(module_name).is_some();
                if module_exists
                    && !has_exports_table
                    && self.ctx.resolve_import_target(module_name).is_none()
                {
                    return (TypeId::ANY, Vec::new());
                }

                // Export not found - emit appropriate error based on what's missing
                if module_exists {
                    // Module exists but export not found
                    if export_name == "default" {
                        let has_export_equals = self.module_has_export_equals(module_name);

                        if has_export_equals {
                            self.emit_no_default_export_error(module_name, value_decl, false);
                            return (TypeId::ERROR, Vec::new());
                        }

                        let uses_system_namespace_default = self
                            .source_file_import_uses_system_default_namespace_fallback(module_name);

                        // For missing default exports, check_imported_members already
                        // emits TS1192 for positional default imports (`import X from "mod"`).
                        // Don't emit a duplicate TS2305 here — just return ERROR and let
                        // the import checker handle the diagnostic unless the module
                        // transform itself provides a namespace-shaped default.
                        if !self.ctx.allow_synthetic_default_imports()
                            && !uses_system_namespace_default
                        {
                            tracing::debug!(
                                "default export missing and allowSyntheticDefaultImports is false, returning ERROR (TS1192 handled by import checker)"
                            );
                            return (TypeId::ERROR, Vec::new());
                        }

                        // For default imports without a default export, only
                        // synthesize a namespace fallback for CommonJS-shaped
                        // modules. Pure ESM modules must still report TS1192.
                        if uses_system_namespace_default || is_node_esm_importing_cjs {
                            // Same circular module guard as namespace imports above
                            if self
                                .ctx
                                .module_namespace_resolution_set
                                .contains(module_name)
                            {
                                return (TypeId::ANY, Vec::new());
                            }
                            self.ctx
                                .module_namespace_resolution_set
                                .insert(module_name.to_string());

                            // Create a namespace type from all module exports
                            let exports_table = self.resolve_effective_module_exports(module_name);

                            if let Some(exports_table) = exports_table {
                                let ordered_exports =
                                    self.ordered_namespace_export_entries(&exports_table);
                                use tsz_solver::PropertyInfo;
                                let mut props: Vec<PropertyInfo> = Vec::new();
                                for &(name, export_sym_id) in &ordered_exports {
                                    if self.should_skip_namespace_export_name(
                                        &exports_table,
                                        name,
                                        export_sym_id,
                                    ) {
                                        continue;
                                    }
                                    let declaration_order = if name == "default" {
                                        1
                                    } else {
                                        props.len() as u32 + 2
                                    };
                                    let prop_type = self.get_type_of_symbol(export_sym_id);
                                    let name_atom = self.ctx.types.intern_string(name);
                                    props.push(PropertyInfo {
                                        name: name_atom,
                                        type_id: prop_type,
                                        write_type: prop_type,
                                        optional: false,
                                        readonly: false,
                                        is_method: false,
                                        is_class_prototype: false,
                                        visibility: Visibility::Public,
                                        parent_id: None,
                                        declaration_order,
                                        is_string_named: false,
                                    });
                                }
                                Self::normalize_namespace_export_declaration_order(&mut props);
                                let module_type = factory.object(props);
                                self.ctx.namespace_module_names.insert(
                                    module_type,
                                    self.imported_namespace_display_module_name(module_name),
                                );
                                self.ctx.module_namespace_resolution_set.remove(module_name);
                                return (module_type, Vec::new());
                            }
                            self.ctx.module_namespace_resolution_set.remove(module_name);
                        }
                        return (TypeId::ERROR, Vec::new());
                    } else {
                        // TS2305/TS2614: Module has no exported member.
                        // Before emitting, try a type-level resolution for `export =`
                        // modules where the member may be a key of a mapped type.
                        if export_name != "*" {
                            let found_via_export_equals_type = self
                                .try_resolve_named_export_via_export_equals_type(
                                    module_name,
                                    export_name,
                                );
                            if let Some(prop_type) = found_via_export_equals_type {
                                return (prop_type, Vec::new());
                            }
                            let import_specifier_decl = declarations.iter().copied().find(|&decl| {
                                self.ctx.arena.get(decl).is_some_and(|node| {
                                    node.kind == tsz_parser::parser::syntax_kind_ext::IMPORT_SPECIFIER
                                })
                            });
                            if self.ctx.arena.get(value_decl).is_some_and(|node| {
                                node.kind == tsz_parser::parser::syntax_kind_ext::IMPORT_SPECIFIER
                            }) {
                                return (TypeId::ERROR, Vec::new());
                            }
                            self.emit_no_exported_member_error(
                                module_name,
                                export_name,
                                import_specifier_decl.unwrap_or(value_decl),
                            );
                        }
                    }
                } else {
                    // Module not found at all - emit TS2307
                    self.emit_module_not_found_error(module_name, value_decl);
                }
                return (TypeId::ERROR, Vec::new());
            }

            // Unresolved alias - return ANY to prevent cascading TS2571 errors
            return (TypeId::ANY, Vec::new());
        }

        // Fallback: return ANY for unresolved symbols to prevent cascading errors
        // The actual "cannot find" error should already be emitted elsewhere
        (TypeId::ANY, Vec::new())
    }
}

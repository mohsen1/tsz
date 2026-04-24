//! JS constructor instance type synthesis for `new` expressions.
//!
//! Split from `complex.rs` to keep files under the 2000-LOC guard.
//! Contains:
//! - `synthesize_js_constructor_instance_type` — builds an instance type from
//!   `this.prop = value` assignments in JS constructor functions
//! - `collect_generic_constructor_this_properties` — scans generic constructor bodies
//! - `extract_generic_this_assignment` — extracts name/type from `this.prop = rhs`

use super::complex_constructors::PrototypeMembers;
use crate::query_boundaries::type_computation::complex as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Synthesize an instance type for a JS constructor function.
    /// Given `function Foo(x) { this.a = x; this.b = "hello"; }`, collects the
    /// `this.prop = value` assignments and builds an object type `{ a: T, b: string }`.
    /// Also scans `Foo.prototype.m = function() { this.y = ... }` patterns for
    /// properties assigned in prototype methods (typed as `T | undefined`).
    /// Returns `None` if the target is not a plain function or has no this-property assignments.
    pub(crate) fn synthesize_js_constructor_instance_type(
        &mut self,
        expr_idx: NodeIndex,
        constructor_type: TypeId,
        arg_types: &[TypeId],
    ) -> Option<TypeId> {
        use rustc_hash::FxHashMap;
        use tsz_binder::symbol_flags;
        use tsz_solver::PropertyInfo;

        // Resolve the function symbol from the new expression target
        let expr_node = self.ctx.arena.get(expr_idx)?;
        let expr_kind = expr_node.kind;
        let callable_symbol = query::callable_shape_for_type(self.ctx.types, constructor_type)
            .and_then(|shape| shape.symbol);
        let sym_id = if expr_kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            self.ctx
                .binder
                .resolve_identifier(self.ctx.arena, expr_idx)
                .or_else(|| self.ctx.binder.get_node_symbol(expr_idx))
                .or(callable_symbol)
        } else {
            self.ctx
                .binder
                .get_node_symbol(expr_idx)
                .or_else(|| {
                    self.ctx
                        .arena
                        .get_function(expr_node)
                        .and_then(|func| func.name.into_option())
                        .and_then(|name_idx| {
                            self.ctx
                                .binder
                                .resolve_identifier(self.ctx.arena, name_idx)
                                .or_else(|| self.ctx.binder.get_node_symbol(name_idx))
                        })
                })
                .or_else(|| {
                    self.ctx
                        .arena
                        .get_variable_declaration(expr_node)
                        .and_then(|decl| {
                            self.ctx
                                .binder
                                .resolve_identifier(self.ctx.arena, decl.name)
                                .or_else(|| self.ctx.binder.get_node_symbol(decl.name))
                        })
                })
                .or(callable_symbol)
                .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, expr_idx))
        };

        // For anonymous function expressions (e.g., `exports.A = function() { this.x = 1; }`),
        // no symbol may exist. Fall back to using the expression node directly as a function.
        let (func, func_name_str, _func_node_idx) = if let Some(sym_id) = sym_id {
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            let value_decl = self
                .checked_js_constructor_value_declaration(
                    sym_id,
                    symbol.value_declaration,
                    &symbol.declarations,
                )
                .unwrap_or(symbol.value_declaration);
            let node = self.ctx.arena.get(value_decl)?;

            // Only handle plain JS function constructors (not classes).
            if symbol.has_any_flags(symbol_flags::CLASS)
                && !self.declaration_is_checked_js_constructor_value_declaration(sym_id, value_decl)
            {
                return None;
            }

            if let Some(func) = self.ctx.arena.get_function(node) {
                let func_name = self
                    .ctx
                    .arena
                    .get(func.name)
                    .and_then(|n| self.ctx.arena.get_identifier(n))
                    .map(|ident| ident.escaped_text.clone());
                (func, func_name, value_decl)
            } else if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) {
                let init_node = self.ctx.arena.get(var_decl.initializer)?;
                if init_node.kind != tsz_parser::parser::syntax_kind_ext::FUNCTION_EXPRESSION {
                    return None;
                }
                let func = self.ctx.arena.get_function(init_node)?;
                let func_name = self
                    .ctx
                    .arena
                    .get(func.name)
                    .and_then(|n| self.ctx.arena.get_identifier(n))
                    .map(|ident| ident.escaped_text.clone())
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get(var_decl.name)
                            .and_then(|n| self.ctx.arena.get_identifier(n))
                            .map(|ident| ident.escaped_text.clone())
                    });
                (func, func_name, var_decl.initializer)
            } else {
                return None;
            }
        } else if expr_kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_EXPRESSION
            || expr_kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION
        {
            // Direct function expression/declaration without a symbol (e.g., anonymous
            // function expression in `exports.X = function() { ... }`).
            let func = self.ctx.arena.get_function(expr_node)?;
            let func_name = func
                .name
                .into_option()
                .and_then(|name_idx| {
                    self.ctx
                        .arena
                        .get(name_idx)
                        .and_then(|n| self.ctx.arena.get_identifier(n))
                })
                .map(|ident| ident.escaped_text.clone());
            (func, func_name, expr_idx)
        } else {
            return None;
        };

        let body_idx = func.body;
        if body_idx.is_none() {
            return None;
        }
        let func_node_idx = self.ctx.arena.parent_of(body_idx);

        // Build effective template/parameter data for JS generic constructors.
        let func_shape = crate::query_boundaries::common::function_shape_for_type(
            self.ctx.types,
            constructor_type,
        );
        let mut effective_type_params: Vec<tsz_solver::TypeParamInfo> = func_shape
            .as_ref()
            .map(|shape| shape.type_params.clone())
            .unwrap_or_default();
        let mut effective_param_types: Vec<TypeId> = func_shape
            .as_ref()
            .map(|shape| shape.params.iter().map(|param| param.type_id).collect())
            .unwrap_or_default();
        let mut fallback_param_type_map: Option<FxHashMap<String, TypeId>> = None;

        if effective_type_params.is_empty() || effective_param_types.is_empty() {
            let fallback = self.js_class_body_param_type_map(body_idx);

            if effective_param_types.is_empty() {
                effective_param_types = func
                    .parameters
                    .nodes
                    .iter()
                    .map(|&param_idx| {
                        self.ctx
                            .arena
                            .get(param_idx)
                            .and_then(|param_node| self.ctx.arena.get_parameter(param_node))
                            .and_then(|param| self.ctx.arena.get(param.name))
                            .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                            .and_then(|ident| fallback.get(&ident.escaped_text).copied())
                            .unwrap_or(TypeId::ANY)
                    })
                    .collect();
            }

            if effective_type_params.is_empty() {
                let mut seen = rustc_hash::FxHashSet::default();
                for &param_type in fallback.values() {
                    if let Some(tp_info) =
                        crate::query_boundaries::common::type_param_info(self.ctx.types, param_type)
                        && seen.insert(tp_info.name)
                    {
                        effective_type_params.push(tp_info);
                    }
                }
            }

            fallback_param_type_map = Some(fallback);
        }

        let is_generic = !effective_type_params.is_empty();

        let mut properties: FxHashMap<tsz_common::interner::Atom, PropertyInfo> =
            FxHashMap::default();
        let mut scope_restore: Vec<(String, Option<TypeId>)> = Vec::new();

        if is_generic {
            // We cannot use collect_js_constructor_this_properties here because
            // get_type_of_node resolves types in the call site context, not the
            // function declaration context.
            let param_type_map: FxHashMap<String, TypeId> = if let Some(shape) = func_shape.as_ref()
                && !shape.params.is_empty()
            {
                func.parameters
                    .nodes
                    .iter()
                    .zip(shape.params.iter())
                    .filter_map(|(&param_idx, param_info)| {
                        let param_node = self.ctx.arena.get(param_idx)?;
                        let param_data = self.ctx.arena.get_parameter(param_node)?;
                        let name_node = self.ctx.arena.get(param_data.name)?;
                        let ident = self.ctx.arena.get_identifier(name_node)?;
                        Some((ident.escaped_text.clone(), param_info.type_id))
                    })
                    .collect()
            } else {
                fallback_param_type_map
                    .clone()
                    .unwrap_or_else(|| self.js_class_body_param_type_map(body_idx))
            };

            // Push type params into scope for @type resolution
            let factory = self.ctx.types.factory();
            for tp in &effective_type_params {
                let name = self.ctx.types.resolve_atom(tp.name);
                let ty = factory.type_param(*tp);
                let previous = self.ctx.type_parameter_scope.insert(name.clone(), ty);
                scope_restore.push((name, previous));
            }

            // Scan the constructor body for this-property patterns
            self.collect_generic_constructor_this_properties(
                body_idx,
                &param_type_map,
                &mut properties,
                sym_id,
            );
        } else {
            if !arg_types.is_empty()
                && let Some(func_node_idx) = func_node_idx
                && let Some(jsdoc) = self.get_jsdoc_for_function(func_node_idx)
            {
                let template_names: rustc_hash::FxHashSet<String> =
                    Self::jsdoc_template_type_params(&jsdoc)
                        .into_iter()
                        .map(|(name, _)| name)
                        .collect();
                if !template_names.is_empty() {
                    let jsdoc_param_names: Vec<String> = Self::extract_jsdoc_param_names(&jsdoc)
                        .into_iter()
                        .map(|(name, _)| name)
                        .collect();
                    let mut inserted_templates = rustc_hash::FxHashSet::default();

                    for (pi, &param_idx) in func.parameters.nodes.iter().enumerate() {
                        let Some(&arg_ty) = arg_types.get(pi) else {
                            continue;
                        };
                        let Some(param) = self.ctx.arena.get_parameter_at(param_idx) else {
                            continue;
                        };
                        let pname =
                            self.effective_jsdoc_param_name(param.name, &jsdoc_param_names, pi);
                        let Some(type_expr) = Self::extract_jsdoc_param_type_string(&jsdoc, &pname)
                        else {
                            continue;
                        };
                        let normalized = type_expr
                            .trim()
                            .trim_end_matches('=')
                            .trim_start_matches("...")
                            .trim()
                            .to_string();
                        if !template_names.contains(&normalized)
                            || !inserted_templates.insert(normalized.clone())
                        {
                            continue;
                        }

                        let widened_arg =
                            crate::query_boundaries::common::widen_type(self.ctx.types, arg_ty);
                        let previous = self
                            .ctx
                            .type_parameter_scope
                            .insert(normalized.clone(), widened_arg);
                        scope_restore.push((normalized, previous));
                    }
                }
            }

            // Non-generic: use standard property collection
            self.collect_js_constructor_this_properties(body_idx, &mut properties, sym_id, true);
        }

        for (name, previous) in scope_restore {
            if let Some(prev) = previous {
                self.ctx.type_parameter_scope.insert(name, prev);
            } else {
                self.ctx.type_parameter_scope.remove(&name);
            }
        }

        // Also scan Foo.prototype.m = ... patterns for:
        // 1. Method bindings (added directly as instance properties)
        // 2. this.prop assignments inside prototype methods (typed as T | undefined)
        let mut has_prototype_evidence = false;
        if let Some(ref func_name_s) = func_name_str
            && let Some(sym_id) = sym_id
        {
            let symbol = self.ctx.binder.get_symbol(sym_id);
            let value_decl = symbol.map(|s| s.value_declaration).unwrap_or(expr_idx);
            let PrototypeMembers {
                method_bindings,
                this_props,
                has_evidence: prototype_evidence,
            } = self.collect_prototype_members_and_this_properties(value_decl, func_name_s, sym_id);
            has_prototype_evidence = prototype_evidence;

            // Add prototype methods as instance properties
            for (name, prop) in method_bindings {
                properties.entry(name).or_insert(prop);
            }

            // Add this-properties from prototype methods (with | undefined)
            for (name, mut prop) in this_props {
                let factory = self.ctx.types.factory();
                let widened_prop_type = factory.union2(prop.type_id, TypeId::UNDEFINED);
                if let Some(existing) = properties.get_mut(&name) {
                    if existing.write_type == TypeId::ANY {
                        existing.type_id = factory.union2(existing.type_id, widened_prop_type);
                    }
                } else {
                    prop.type_id = widened_prop_type;
                    prop.write_type = prop.type_id;
                    properties.insert(name, prop);
                }
            }

            for (name, prop) in self.collect_define_property_bindings_on_function_prototype(
                value_decl,
                func_name_s,
                sym_id,
            ) {
                has_prototype_evidence = true;
                properties.entry(name).or_insert(prop);
            }
        }

        for prop in properties.values_mut() {
            if prop.write_type == TypeId::ANY
                && (prop.type_id == TypeId::NULL || prop.type_id == TypeId::UNDEFINED)
            {
                prop.type_id = TypeId::ANY;
            }
        }

        if properties.is_empty() {
            if has_prototype_evidence {
                if let Some(sym_id) = sym_id {
                    let brand_name = self
                        .ctx
                        .types
                        .intern_string(&format!("__js_ctor_brand_{}", sym_id.0));
                    properties.insert(
                        brand_name,
                        PropertyInfo {
                            name: brand_name,
                            type_id: TypeId::UNKNOWN,
                            write_type: TypeId::UNKNOWN,
                            optional: false,
                            readonly: false,
                            is_method: false,
                            is_class_prototype: false,
                            visibility: tsz_solver::Visibility::Public,
                            parent_id: Some(sym_id),
                            declaration_order: 0,
                            is_string_named: false,
                        },
                    );
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }

        // Build an object type from the collected properties
        let props: Vec<PropertyInfo> = properties.into_values().collect();
        let factory = self.ctx.types.factory();
        let instance_type = factory.object(props);

        // If the constructor function has template type params, instantiate the
        // instance type by inferring type arguments from the actual call arguments.
        if !effective_type_params.is_empty() && !arg_types.is_empty() {
            let mut type_args = Vec::with_capacity(effective_type_params.len());
            for tp in &effective_type_params {
                let tp_id = self.ctx.types.factory().type_param(*tp);
                let mut inferred = None;
                for (i, &param_type) in effective_param_types.iter().enumerate() {
                    if param_type == tp_id
                        && let Some(&arg_ty) = arg_types.get(i)
                    {
                        // Widen literal types (e.g., 1 → number)
                        // since non-const type params don't preserve literals
                        inferred = Some(crate::query_boundaries::common::widen_type(
                            self.ctx.types,
                            arg_ty,
                        ));
                        break;
                    }
                }
                type_args.push(inferred.unwrap_or(TypeId::UNKNOWN));
            }
            let instantiated = crate::query_boundaries::common::instantiate_generic(
                self.ctx.types,
                instance_type,
                &effective_type_params,
                &type_args,
            );
            return Some(instantiated);
        }

        Some(instance_type)
    }

    /// Collect this-property assignments from a generic JS constructor function body.
    ///
    /// Unlike `collect_js_constructor_this_properties`, this uses the function shape's
    /// parameter types (correctly resolved during function type creation) instead of
    /// re-evaluating types via `get_type_of_node` (which would resolve in the wrong scope).
    /// Also handles bare `this.prop` expressions with `@type` JSDoc annotations.
    fn collect_generic_constructor_this_properties(
        &mut self,
        body_idx: NodeIndex,
        param_type_map: &rustc_hash::FxHashMap<String, TypeId>,
        properties: &mut rustc_hash::FxHashMap<
            tsz_common::interner::Atom,
            tsz_solver::PropertyInfo,
        >,
        parent_sym: Option<tsz_binder::SymbolId>,
    ) {
        use tsz_scanner::SyntaxKind;
        use tsz_solver::{PropertyInfo, Visibility};

        let top_level_stmts: Vec<NodeIndex> = {
            let Some(body_node) = self.ctx.arena.get(body_idx) else {
                return;
            };
            let Some(block) = self.ctx.arena.get_block(body_node) else {
                return;
            };
            block.statements.nodes.clone()
        };
        let mut stmts = Vec::new();
        for stmt_idx in top_level_stmts {
            self.collect_nested_js_this_assignment_statements(stmt_idx, &mut stmts);
        }
        let this_aliases = self.collect_this_aliases(&stmts);

        for &stmt_idx in &stmts {
            if let Some((prop_name, rhs_idx, is_private, report_idx)) =
                self.extract_this_property_assignment(stmt_idx, &this_aliases)
            {
                if is_private {
                    continue;
                }

                let rhs_type = if let Some(jsdoc_type) = self.js_statement_declared_type(stmt_idx) {
                    jsdoc_type
                } else {
                    let Some(rhs_node) = self.ctx.arena.get(rhs_idx) else {
                        continue;
                    };
                    if rhs_node.kind == SyntaxKind::Identifier as u16 {
                        if let Some(rhs_ident) = self.ctx.arena.get_identifier(rhs_node) {
                            param_type_map
                                .get(rhs_ident.escaped_text.as_str())
                                .copied()
                                .unwrap_or_else(|| self.get_type_of_node(rhs_idx))
                        } else {
                            self.get_type_of_node(rhs_idx)
                        }
                    } else {
                        self.get_type_of_node(rhs_idx)
                    }
                };

                if rhs_type == TypeId::UNDEFINED
                    || rhs_type == TypeId::VOID
                    || self.js_assignment_rhs_is_void_zero(rhs_idx)
                {
                    if let Some(parent_sym) = parent_sym
                        && let Some(symbol) = self.ctx.binder.get_symbol(parent_sym)
                    {
                        self.error_at_node(
                            report_idx,
                            &format!(
                                "Property '{prop_name}' does not exist on type '{}'.",
                                symbol.escaped_name
                            ),
                            crate::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        );
                    }
                    continue;
                }

                let name_atom = self.ctx.types.intern_string(&prop_name);
                properties.entry(name_atom).or_insert(PropertyInfo {
                    name: name_atom,
                    type_id: rhs_type,
                    write_type: rhs_type,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: parent_sym,
                    declaration_order: 0,
                    is_string_named: false,
                });
                continue;
            }

            if let Some((prop_name, is_private, _report_idx)) =
                self.extract_jsdoc_this_property_declaration(stmt_idx, &this_aliases)
            {
                if is_private {
                    continue;
                }
                if let Some(jsdoc_type) = self.js_statement_declared_type(stmt_idx) {
                    if jsdoc_type == TypeId::UNDEFINED {
                        continue;
                    }
                    let name_atom = self.ctx.types.intern_string(&prop_name);
                    properties.entry(name_atom).or_insert(PropertyInfo {
                        name: name_atom,
                        type_id: jsdoc_type,
                        write_type: jsdoc_type,
                        optional: false,
                        readonly: false,
                        is_method: false,
                        is_class_prototype: false,
                        visibility: Visibility::Public,
                        parent_id: parent_sym,
                        declaration_order: 0,
                        is_string_named: false,
                    });
                }
            }
        }
    }

    /// Extract a this-property assignment's name and type for generic constructor context.
    ///
    /// For `this.prop = rhs`:
    /// - If rhs is an identifier matching a parameter name, uses the function shape's type
    /// - If there's a @type JSDoc annotation, uses that
    /// - Otherwise falls back to `get_type_of_node` for the rhs
    fn extract_generic_this_assignment(
        &mut self,
        lhs_idx: NodeIndex,
        rhs_idx: NodeIndex,
        param_type_map: &rustc_hash::FxHashMap<String, TypeId>,
        stmt_idx: NodeIndex,
        this_aliases: &[String],
    ) -> Option<(String, TypeId, NodeIndex)> {
        use tsz_scanner::SyntaxKind;

        let lhs_node = self.ctx.arena.get(lhs_idx)?;
        if lhs_node.kind != tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.ctx.arena.get_access_expr(lhs_node)?;
        let obj_node = self.ctx.arena.get(access.expression)?;
        let is_this_or_alias = if obj_node.kind == SyntaxKind::ThisKeyword as u16 {
            true
        } else if obj_node.kind == SyntaxKind::Identifier as u16 {
            self.ctx
                .arena
                .get_identifier(obj_node)
                .is_some_and(|ident| {
                    this_aliases
                        .iter()
                        .any(|alias| alias == &ident.escaped_text)
                })
        } else {
            false
        };
        if !is_this_or_alias {
            return None;
        }
        let name_node = self.ctx.arena.get(access.name_or_argument)?;
        let ident = self.ctx.arena.get_identifier(name_node)?;
        let prop_name = ident.escaped_text.clone();

        // Determine type: @type annotation > param type > get_type_of_node
        let type_id = if let Some(jsdoc_type) = self.js_statement_declared_type(stmt_idx) {
            jsdoc_type
        } else {
            // Check if RHS is a parameter identifier
            let rhs_node = self.ctx.arena.get(rhs_idx)?;
            if rhs_node.kind == SyntaxKind::Identifier as u16 {
                if let Some(rhs_ident) = self.ctx.arena.get_identifier(rhs_node) {
                    if let Some(&param_type) = param_type_map.get(&rhs_ident.escaped_text) {
                        param_type
                    } else {
                        self.get_type_of_node(rhs_idx)
                    }
                } else {
                    self.get_type_of_node(rhs_idx)
                }
            } else {
                self.get_type_of_node(rhs_idx)
            }
        };

        Some((prop_name, type_id, access.name_or_argument))
    }
}

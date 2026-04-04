use crate::query_boundaries::common::{callable_shape_for_type, function_shape_for_type};
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{ObjectShape, PropertyInfo, TypeId, Visibility};

impl<'a> CheckerState<'a> {
    fn infer_symbol_type_for_file(
        &mut self,
        target_file_idx: usize,
        sym_id: tsz_binder::SymbolId,
    ) -> TypeId {
        if target_file_idx == self.ctx.current_file_idx {
            return self.get_type_of_symbol(sym_id);
        }

        let Some(all_arenas) = self.ctx.all_arenas.clone() else {
            return TypeId::ANY;
        };
        let Some(all_binders) = self.ctx.all_binders.clone() else {
            return TypeId::ANY;
        };
        let Some(arena) = all_arenas.get(target_file_idx) else {
            return TypeId::ANY;
        };
        let Some(binder) = all_binders.get(target_file_idx) else {
            return TypeId::ANY;
        };
        let Some(source_file) = arena.source_files.first() else {
            return TypeId::ANY;
        };

        let mut checker = Box::new(CheckerState::with_parent_cache(
            arena.as_ref(),
            binder.as_ref(),
            self.ctx.types,
            source_file.file_name.clone(),
            self.ctx.compiler_options.clone(),
            self,
        ));
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        checker.ctx.copy_cross_file_state_from(&self.ctx);
        checker.ctx.current_file_idx = target_file_idx;
        self.ctx.copy_symbol_file_targets_to(&mut checker.ctx);

        let ty = checker.get_type_of_symbol(sym_id);
        self.ctx.merge_symbol_file_targets_from(&checker.ctx);
        ty
    }

    fn infer_getter_return_type_for_file(
        &mut self,
        target_file_idx: usize,
        body_idx: NodeIndex,
    ) -> TypeId {
        if target_file_idx == self.ctx.current_file_idx {
            return self.infer_getter_return_type(body_idx);
        }

        let Some(all_arenas) = self.ctx.all_arenas.clone() else {
            return TypeId::ANY;
        };
        let Some(all_binders) = self.ctx.all_binders.clone() else {
            return TypeId::ANY;
        };
        let Some(arena) = all_arenas.get(target_file_idx) else {
            return TypeId::ANY;
        };
        let Some(binder) = all_binders.get(target_file_idx) else {
            return TypeId::ANY;
        };
        let Some(source_file) = arena.source_files.first() else {
            return TypeId::ANY;
        };

        let mut checker = Box::new(CheckerState::with_parent_cache(
            arena.as_ref(),
            binder.as_ref(),
            self.ctx.types,
            source_file.file_name.clone(),
            self.ctx.compiler_options.clone(),
            self,
        ));
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        checker.ctx.copy_cross_file_state_from(&self.ctx);
        checker.ctx.current_file_idx = target_file_idx;
        self.ctx.copy_symbol_file_targets_to(&mut checker.ctx);

        let ty = checker.infer_getter_return_type(body_idx);
        self.ctx.merge_symbol_file_targets_from(&checker.ctx);
        ty
    }

    pub(crate) fn constant_define_property_name_in_file(
        &self,
        target_file_idx: usize,
        arena: &tsz_parser::parser::NodeArena,
        idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                arena.get_literal(node).map(|lit| lit.text.clone())
            }
            syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = arena.get_parenthesized(node)?;
                self.constant_define_property_name_in_file(target_file_idx, arena, paren.expression)
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let ident = arena.get_identifier(node)?;
                let binder = self.ctx.get_binder_for_file(target_file_idx)?;
                let sym_id = binder.file_locals.get(&ident.escaped_text)?;
                let symbol = binder.get_symbol(sym_id)?;
                let decl_node = arena.get(symbol.value_declaration)?;
                let var_decl = arena.get_variable_declaration(decl_node)?;
                self.constant_define_property_name_in_file(
                    target_file_idx,
                    arena,
                    var_decl.initializer,
                )
            }
            _ => None,
        }
    }

    pub(crate) fn define_property_info_from_descriptor(
        &mut self,
        target_file_idx: usize,
        arena: &tsz_parser::parser::NodeArena,
        name: &str,
        descriptor_expr: NodeIndex,
        declaration_order: u32,
    ) -> Option<PropertyInfo> {
        let descriptor = self.resolve_define_property_descriptor_object_literal(
            target_file_idx,
            arena,
            descriptor_expr,
        )?;
        let mut value_type = None;
        let mut getter_type = None;
        let mut setter_type = None;
        let mut has_value = false;
        let mut writable_true = false;
        let mut has_setter = false;

        for &element_idx in &descriptor.elements.nodes {
            let Some(element_node) = arena.get(element_idx) else {
                continue;
            };
            match element_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(prop) = arena.get_property_assignment(element_node) else {
                        continue;
                    };
                    let Some(prop_name) =
                        crate::types_domain::queries::core::get_literal_property_name(
                            arena, prop.name,
                        )
                    else {
                        continue;
                    };
                    match prop_name.as_str() {
                        "value" => {
                            has_value = true;
                            let value = self.infer_commonjs_export_rhs_type(
                                target_file_idx,
                                prop.initializer,
                                None,
                            );
                            value_type = Some(crate::query_boundaries::common::widen_type(
                                self.ctx.types,
                                crate::query_boundaries::common::widen_freshness(
                                    self.ctx.types,
                                    value,
                                ),
                            ));
                        }
                        "writable" => {
                            writable_true = arena
                                .get(prop.initializer)
                                .is_some_and(|init| init.kind == SyntaxKind::TrueKeyword as u16);
                        }
                        "get" => {
                            let getter_fn = self.infer_commonjs_export_rhs_type(
                                target_file_idx,
                                prop.initializer,
                                None,
                            );
                            getter_type = function_shape_for_type(self.ctx.types, getter_fn)
                                .map(|shape| shape.return_type)
                                .or(getter_type)
                                .or(Some(TypeId::ANY));
                        }
                        "set" => {
                            has_setter = true;
                            if let Some(init_node) = arena.get(prop.initializer)
                                && let Some(func) = arena.get_function(init_node)
                                && let Some(&first_param) = func.parameters.nodes.first()
                                && let Some(binder) = self.ctx.get_binder_for_file(target_file_idx)
                                && let Some(&param_sym) = binder.node_symbols.get(&first_param.0)
                            {
                                setter_type = Some(
                                    self.infer_symbol_type_for_file(target_file_idx, param_sym),
                                );
                            } else {
                                setter_type.get_or_insert(TypeId::ANY);
                            }
                        }
                        _ => {}
                    }
                }
                syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = arena.get_method_decl(element_node) else {
                        continue;
                    };
                    let Some(prop_name) =
                        crate::types_domain::queries::core::get_literal_property_name(
                            arena,
                            method.name,
                        )
                    else {
                        continue;
                    };
                    let contextual_method_type = (prop_name.as_str() == "set")
                        .then_some(getter_type)
                        .flatten();
                    let method_type = self.infer_commonjs_descriptor_method_type(
                        target_file_idx,
                        element_idx,
                        contextual_method_type.map(|ty| {
                            self.ctx
                                .types
                                .factory()
                                .function(tsz_solver::FunctionShape::new(
                                    vec![tsz_solver::ParamInfo::unnamed(ty)],
                                    TypeId::VOID,
                                ))
                        }),
                    );
                    let Some(shape) = function_shape_for_type(self.ctx.types, method_type) else {
                        continue;
                    };
                    match prop_name.as_str() {
                        "get" => {
                            getter_type = Some(shape.return_type);
                        }
                        "set" => {
                            has_setter = true;
                            setter_type = shape
                                .params
                                .first()
                                .map(|param| param.type_id)
                                .or(getter_type)
                                .or(Some(TypeId::ANY));
                        }
                        _ => {}
                    }
                }
                syntax_kind_ext::GET_ACCESSOR => {
                    let Some(accessor) = arena.get_accessor(element_node) else {
                        getter_type.get_or_insert(TypeId::ANY);
                        continue;
                    };
                    let inferred = if accessor.type_annotation.is_some() {
                        self.get_type_from_type_node(accessor.type_annotation)
                    } else {
                        self.infer_getter_return_type_for_file(target_file_idx, accessor.body)
                    };
                    getter_type = Some(inferred);
                }
                syntax_kind_ext::SET_ACCESSOR => {
                    has_setter = true;
                    let Some(accessor) = arena.get_accessor(element_node) else {
                        setter_type.get_or_insert(TypeId::ANY);
                        continue;
                    };
                    let Some(&first_param) = accessor.parameters.nodes.first() else {
                        setter_type.get_or_insert(TypeId::ANY);
                        continue;
                    };
                    let Some(binder) = self.ctx.get_binder_for_file(target_file_idx) else {
                        setter_type.get_or_insert(TypeId::ANY);
                        continue;
                    };
                    let Some(&param_sym) = binder.node_symbols.get(&first_param.0) else {
                        setter_type.get_or_insert(TypeId::ANY);
                        continue;
                    };
                    setter_type = Some(self.infer_symbol_type_for_file(target_file_idx, param_sym));
                }
                _ => {}
            }
        }

        let has_getter = getter_type.is_some();
        let has_accessor_descriptor = has_getter || has_setter;
        let has_data_descriptor = has_value || writable_true;

        // TSC treats malformed or mixed descriptors on exports as permissive
        // open-world properties rather than surfacing readonly/missing-member
        // behavior at import sites.
        if (!has_accessor_descriptor && !has_data_descriptor)
            || (has_accessor_descriptor && has_data_descriptor)
            || (writable_true && !has_value && !has_accessor_descriptor)
        {
            return Some(PropertyInfo {
                name: self.ctx.types.intern_string(name),
                type_id: TypeId::ANY,
                write_type: TypeId::ANY,
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

        if has_setter && setter_type == Some(TypeId::ANY) && getter_type.is_some() {
            setter_type = getter_type;
        }

        let writable = has_setter || (has_value && writable_true);
        let precise_setter_type =
            setter_type.filter(|&ty| ty != TypeId::ANY && ty != TypeId::UNKNOWN);
        let read_type = value_type
            .or(getter_type)
            .or(setter_type)
            .unwrap_or(TypeId::ANY);
        let write_type = if writable {
            precise_setter_type
                .or(getter_type)
                .or(value_type)
                .unwrap_or(read_type)
        } else {
            read_type
        };

        Some(PropertyInfo {
            name: self.ctx.types.intern_string(name),
            type_id: read_type,
            write_type,
            optional: false,
            readonly: !writable,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order,
            is_string_named: false,
        })
    }

    pub(crate) fn augment_object_type_with_define_properties(
        &mut self,
        root_name: &str,
        base_type: TypeId,
    ) -> TypeId {
        let Some(source_file) = self.ctx.arena.source_files.get(self.ctx.current_file_idx) else {
            return base_type;
        };

        let mut props = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(stmt) = self.ctx.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let Some(call_node) = self.ctx.arena.get(stmt.expression) else {
                continue;
            };
            if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
                continue;
            }
            let Some(call) = self.ctx.arena.get_call_expr(call_node) else {
                continue;
            };
            let Some(callee_node) = self.ctx.arena.get(call.expression) else {
                continue;
            };
            if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                continue;
            }
            let Some(callee) = self.ctx.arena.get_access_expr(callee_node) else {
                continue;
            };
            let is_define_property = self
                .ctx
                .arena
                .get_identifier_at(callee.expression)
                .is_some_and(|ident| ident.escaped_text == "Object")
                && self
                    .ctx
                    .arena
                    .get_identifier_at(callee.name_or_argument)
                    .is_some_and(|ident| ident.escaped_text == "defineProperty");
            if !is_define_property {
                continue;
            }

            let Some(args) = &call.arguments else {
                continue;
            };
            if args.nodes.len() < 3 {
                continue;
            }
            let Some(target_ident) = self
                .ctx
                .arena
                .get(args.nodes[0])
                .and_then(|node| self.ctx.arena.get_identifier(node))
            else {
                continue;
            };
            if target_ident.escaped_text != root_name {
                continue;
            }

            let Some(name) = self.constant_define_property_name_in_file(
                self.ctx.current_file_idx,
                self.ctx.arena,
                args.nodes[1],
            ) else {
                continue;
            };

            if let Some(prop) = self.define_property_info_from_descriptor(
                self.ctx.current_file_idx,
                self.ctx.arena,
                &name,
                args.nodes[2],
                props.len() as u32,
            ) {
                props.push(prop);
            }
        }

        if props.is_empty() {
            return base_type;
        }

        let base_shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, base_type)
                .map(|shape| shape.as_ref().clone())
                .or_else(|| {
                    let widened =
                        crate::query_boundaries::common::widen_freshness(self.ctx.types, base_type);
                    crate::query_boundaries::common::object_shape_for_type(self.ctx.types, widened)
                        .map(|shape| shape.as_ref().clone())
                });
        if let Some(shape) = base_shape {
            let mut merged_props = shape.properties;
            for prop in props {
                if let Some(existing) = merged_props
                    .iter_mut()
                    .find(|existing| existing.name == prop.name)
                {
                    *existing = prop;
                } else {
                    merged_props.push(prop);
                }
            }

            if shape.string_index.is_some() || shape.number_index.is_some() {
                return self.ctx.types.factory().object_with_index(ObjectShape {
                    flags: shape.flags,
                    properties: merged_props,
                    string_index: shape.string_index,
                    number_index: shape.number_index,
                    symbol: shape.symbol,
                });
            }

            return self.ctx.types.factory().object_with_flags_and_symbol(
                merged_props,
                shape.flags,
                shape.symbol,
            );
        }

        let define_property_type = self.ctx.types.factory().object(props);
        if matches!(base_type, TypeId::UNKNOWN | TypeId::ERROR) {
            define_property_type
        } else {
            self.ctx
                .types
                .factory()
                .intersection2(base_type, define_property_type)
        }
    }

    pub(super) fn upgrade_commonjs_export_constructor_type(
        &mut self,
        rhs_expr: NodeIndex,
        rhs_type: TypeId,
    ) -> TypeId {
        let Some(instance_type) =
            self.synthesize_js_constructor_instance_type(rhs_expr, rhs_type, &[])
        else {
            return rhs_type;
        };

        if let Some(func) = function_shape_for_type(self.ctx.types, rhs_type) {
            if func.is_constructor {
                return rhs_type;
            }

            let call_sig = tsz_solver::CallSignature {
                type_params: func.type_params.clone(),
                params: func.params.clone(),
                this_type: func.this_type,
                return_type: func.return_type,
                type_predicate: func.type_predicate,
                is_method: func.is_method,
            };
            let construct_sig = tsz_solver::CallSignature {
                return_type: instance_type,
                ..call_sig.clone()
            };
            let symbol = self.resolve_identifier_symbol_without_tracking(rhs_expr);
            return self
                .ctx
                .types
                .factory()
                .callable(tsz_solver::CallableShape {
                    call_signatures: vec![call_sig],
                    construct_signatures: vec![construct_sig],
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                    symbol,
                    is_abstract: false,
                });
        }

        let Some(shape) = callable_shape_for_type(self.ctx.types, rhs_type) else {
            return rhs_type;
        };
        if !shape.construct_signatures.is_empty() || shape.call_signatures.is_empty() {
            return rhs_type;
        }

        let mut new_shape = shape.as_ref().clone();
        let mut construct_sig = new_shape.call_signatures[0].clone();
        construct_sig.return_type = instance_type;
        new_shape.construct_signatures.push(construct_sig);
        if new_shape.symbol.is_none() {
            new_shape.symbol = self.resolve_identifier_symbol_without_tracking(rhs_expr);
        }
        self.ctx.types.factory().callable(new_shape)
    }

    pub(crate) fn direct_commonjs_module_export_assignment_rhs(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let expr_idx = arena.skip_parenthesized(expr_idx);
        let expr_node = arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = arena.get_binary_expr(expr_node)?;
        if binary.operator_token != SyntaxKind::EqualsToken as u16 {
            return None;
        }
        let left_idx = arena.skip_parenthesized(binary.left);
        let left_node = arena.get(left_idx)?;

        if left_node.kind == SyntaxKind::Identifier as u16
            && arena
                .get_identifier(left_node)
                .is_some_and(|ident| ident.escaped_text == "exports")
        {
            return Some(binary.right);
        }

        if left_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

        let left_access = arena.get_access_expr(left_node)?;
        let target_idx = arena.skip_parenthesized(left_access.expression);
        arena
            .get_identifier_at(target_idx)
            .is_some_and(|ident| ident.escaped_text == "module")
            .then_some(())
            .and_then(|_| {
                arena
                    .get_identifier_at(left_access.name_or_argument)
                    .filter(|ident| ident.escaped_text == "exports")
            })?;
        Some(binary.right)
    }

    /// Extract the body statements of an IIFE (Immediately Invoked Function Expression).
    /// Recognizes patterns like `(function() { ... })()` and `(function() { ... }.call(this))`.
    pub(super) fn get_iife_body_statements(
        arena: &tsz_parser::parser::NodeArena,
        call_idx: NodeIndex,
    ) -> Option<&[NodeIndex]> {
        let call_node = arena.get(call_idx)?;
        if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = arena.get_call_expr(call_node)?;
        // Unwrap parentheses: `(function() { ... })()`
        let callee_idx = arena.skip_parenthesized(call.expression);
        let callee_node = arena.get(callee_idx)?;
        if callee_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION {
            return None;
        }
        let func = arena.get_function(callee_node)?;
        let body_node = arena.get(func.body)?;
        let block = arena.get_block(body_node)?;
        Some(&block.statements.nodes)
    }

    fn augment_namespace_props_with_direct_assignment_exports_for_file(
        &mut self,
        target_file_idx: usize,
        props: &mut Vec<tsz_solver::PropertyInfo>,
        start_statement_ordinal: Option<usize>,
    ) {
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32).clone();
        let Some(source_file) = target_arena.source_files.first() else {
            return;
        };
        let export_aliases = Self::collect_commonjs_export_aliases_in_arena(&target_arena);
        let mut pending_props: FxHashMap<String, Vec<(NodeIndex, Option<String>)>> =
            FxHashMap::default();
        let mut ordered_names: Vec<String> = Vec::new();

        // Collect statements to scan: top-level statements + IIFE body statements.
        // In CommonJS files, `exports.X = value` inside IIFEs like `(function() { ... })()`
        // are valid export declarations (tsc recognizes them regardless of scope).
        let mut all_stmts: Vec<NodeIndex> = Vec::new();
        for (stmt_ordinal, &stmt_idx) in source_file.statements.nodes.iter().enumerate() {
            if start_statement_ordinal.is_some_and(|start| stmt_ordinal < start) {
                continue;
            }
            all_stmts.push(stmt_idx);
            // Check if this statement is an IIFE and extract its body statements
            if let Some(stmt_node) = target_arena.get(stmt_idx)
                && stmt_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
                && let Some(stmt) = target_arena.get_expression_statement(stmt_node)
                && let Some(iife_stmts) =
                    Self::get_iife_body_statements(&target_arena, stmt.expression)
            {
                all_stmts.extend_from_slice(iife_stmts);
            }
        }

        for stmt_idx in &all_stmts {
            let Some(stmt_node) = target_arena.get(*stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(stmt) = target_arena.get_expression_statement(stmt_node) else {
                continue;
            };
            self.collect_direct_commonjs_assignment_exports(
                &target_arena,
                stmt.expression,
                &mut pending_props,
                &mut ordered_names,
                &export_aliases,
            );
        }

        for name_text in ordered_names {
            let name_atom = self.ctx.types.intern_string(&name_text);
            let Some(assignments) = pending_props.remove(&name_text) else {
                continue;
            };
            for (rhs_expr, expando_root) in assignments {
                let rhs_type = self
                    .commonjs_string_literal_rhs_type(target_file_idx, rhs_expr)
                    .unwrap_or_else(|| {
                        self.infer_commonjs_export_rhs_type(
                            target_file_idx,
                            rhs_expr,
                            expando_root.as_deref(),
                        )
                    });
                if rhs_type == TypeId::UNDEFINED {
                    continue;
                }
                if let Some(existing) = props.iter_mut().find(|prop| prop.name == name_atom) {
                    existing.type_id = rhs_type;
                    existing.write_type = rhs_type;
                    existing.optional = false;
                    existing.readonly = false;
                    continue;
                }
                props.push(tsz_solver::PropertyInfo {
                    name: name_atom,
                    type_id: rhs_type,
                    write_type: rhs_type,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: props.len() as u32,
                    is_string_named: false,
                });
            }
        }
    }

    pub(super) fn resolve_define_property_descriptor_object_literal(
        &self,
        target_file_idx: usize,
        arena: &tsz_parser::parser::NodeArena,
        descriptor_expr: NodeIndex,
    ) -> Option<tsz_parser::parser::node::LiteralExprData> {
        let descriptor_node = arena.get(descriptor_expr)?;
        if descriptor_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return arena.get_literal_expr(descriptor_node).cloned();
        }

        let ident = arena.get_identifier(descriptor_node)?;
        let binder = self.ctx.get_binder_for_file(target_file_idx)?;
        let sym_id = binder.file_locals.get(&ident.escaped_text)?;
        let symbol = binder.get_symbol(sym_id)?;
        let decl_idx = symbol.value_declaration;
        let decl_node = arena.get(decl_idx)?;
        let var_decl = arena.get_variable_declaration(decl_node)?;
        let init_node = arena.get(var_decl.initializer)?;
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        arena.get_literal_expr(init_node).cloned()
    }

    fn augment_namespace_props_with_define_property_exports_for_file(
        &mut self,
        target_file_idx: usize,
        props: &mut Vec<tsz_solver::PropertyInfo>,
        start_statement_ordinal: Option<usize>,
    ) {
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32).clone();
        let Some(source_file) = target_arena.source_files.first() else {
            return;
        };

        let mut pending = Vec::new();
        for (stmt_ordinal, &stmt_idx) in source_file.statements.nodes.iter().enumerate() {
            if start_statement_ordinal.is_some_and(|start| stmt_ordinal < start) {
                continue;
            }
            let Some(stmt_node) = target_arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(stmt) = target_arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let Some(call_node) = target_arena.get(stmt.expression) else {
                continue;
            };
            if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
                continue;
            }
            let Some(call) = target_arena.get_call_expr(call_node) else {
                continue;
            };
            let Some(callee_node) = target_arena.get(call.expression) else {
                continue;
            };
            if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                continue;
            }
            let Some(access) = target_arena.get_access_expr(callee_node) else {
                continue;
            };
            let is_object_define_property = target_arena
                .get_identifier_at(access.expression)
                .is_some_and(|ident| ident.escaped_text == "Object")
                && target_arena
                    .get_identifier_at(access.name_or_argument)
                    .is_some_and(|ident| ident.escaped_text == "defineProperty");
            if !is_object_define_property {
                continue;
            }

            let Some(args) = &call.arguments else {
                continue;
            };
            if args.nodes.len() < 3 {
                continue;
            }

            let target_expr = args.nodes[0];
            let name_expr = args.nodes[1];
            let descriptor_expr = args.nodes[2];

            let is_exports_target = target_arena
                .get_identifier_at(target_expr)
                .is_some_and(|ident| ident.escaped_text == "exports");
            let is_module_exports_target =
                target_arena.get(target_expr).is_some_and(|target_node| {
                    if target_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                        return false;
                    }
                    let Some(target_access) = target_arena.get_access_expr(target_node) else {
                        return false;
                    };
                    target_arena
                        .get_identifier_at(target_access.expression)
                        .is_some_and(|ident| ident.escaped_text == "module")
                        && target_arena
                            .get_identifier_at(target_access.name_or_argument)
                            .is_some_and(|ident| ident.escaped_text == "exports")
                });
            if !is_exports_target && !is_module_exports_target {
                continue;
            }

            let Some(name) = self.constant_define_property_name_in_file(
                target_file_idx,
                &target_arena,
                name_expr,
            ) else {
                continue;
            };
            let name_atom = self.ctx.types.intern_string(&name);
            if props.iter().any(|prop| prop.name == name_atom) {
                continue;
            }
            pending.push((name, descriptor_expr));
        }

        for (name, descriptor_expr) in pending {
            let Some(prop) = self.define_property_info_from_descriptor(
                target_file_idx,
                &target_arena,
                &name,
                descriptor_expr,
                props.len() as u32,
            ) else {
                continue;
            };
            props.push(prop);
        }
    }

    pub(crate) fn augment_namespace_props_with_commonjs_exports_for_file_after(
        &mut self,
        target_file_idx: usize,
        props: &mut Vec<tsz_solver::PropertyInfo>,
        start_statement_ordinal: Option<usize>,
    ) {
        self.augment_namespace_props_with_direct_assignment_exports_for_file(
            target_file_idx,
            props,
            start_statement_ordinal,
        );
        self.augment_namespace_props_with_define_property_exports_for_file(
            target_file_idx,
            props,
            start_statement_ordinal,
        );
    }

    pub(crate) fn commonjs_module_value_type(
        &mut self,
        module_name: &str,
        source_file_idx: Option<usize>,
    ) -> Option<TypeId> {
        let factory = self.ctx.types.factory();

        if let Some(json_type) = self.json_module_type_for_module(module_name, source_file_idx) {
            return Some(json_type);
        }

        let target_file_idx = source_file_idx
            .and_then(|file_idx| {
                self.ctx
                    .resolve_import_target_from_file(file_idx, module_name)
            })
            .or_else(|| self.ctx.resolve_import_target(module_name));

        let exports_table = self
            .resolve_effective_module_exports_from_file(module_name, source_file_idx)
            .or_else(|| {
                let target_file_idx = target_file_idx?;
                let target_file_name = self
                    .ctx
                    .get_arena_for_file(target_file_idx as u32)
                    .source_files
                    .first()
                    .map(|source_file| source_file.file_name.clone())?;
                self.resolve_effective_module_exports(&target_file_name)
            });

        if let Some(exports_table) = exports_table {
            let module_is_non_module_entity =
                self.ctx.module_resolves_to_non_module_entity(module_name);
            for (name, &sym_id) in exports_table.iter() {
                self.record_cross_file_symbol_if_needed(sym_id, name, module_name);
            }
            let exports_table_target = exports_table
                .iter()
                .find_map(|(_, &sym_id)| self.ctx.resolve_symbol_file_index(sym_id));

            let mut export_equals_type = exports_table
                .get("export=")
                .map(|export_equals_sym| self.get_type_of_symbol(export_equals_sym));
            let augment_target = source_file_idx
                .and_then(|src_idx| {
                    self.ctx
                        .resolve_import_target_from_file(src_idx, module_name)
                })
                .or(target_file_idx)
                .or(exports_table_target);
            let surface =
                augment_target.map(|target_idx| self.resolve_js_export_surface(target_idx));
            if let Some(surface) = surface.as_ref()
                && surface.has_commonjs_exports
            {
                let display_name = self.imported_namespace_display_module_name(module_name);
                return surface.to_type_id_with_display_name(self, Some(display_name));
            }
            let mut props: Vec<PropertyInfo> = if surface
                .as_ref()
                .is_some_and(|s| s.has_commonjs_exports)
            {
                let mut named_exports = surface
                    .as_ref()
                    .map(|s| s.named_exports.clone())
                    .unwrap_or_default();
                for (order, prop) in named_exports.iter_mut().enumerate() {
                    prop.declaration_order = order as u32;
                }
                if let Some(surface_direct_type) =
                    surface.as_ref().and_then(|s| s.direct_export_type)
                {
                    export_equals_type = Some(surface_direct_type);
                }
                named_exports
            } else {
                let mut props: Vec<PropertyInfo> = Vec::new();
                for (name, &sym_id) in exports_table.iter() {
                    if name == "export="
                        || self.is_type_only_export_symbol(sym_id)
                        || self.is_export_from_type_only_wildcard(module_name, name)
                        || self.export_symbol_has_no_value(sym_id)
                        || self.is_export_type_only_from_file(module_name, name, source_file_idx)
                    {
                        continue;
                    }

                    let mut prop_type = self.get_type_of_symbol(sym_id);
                    prop_type = self.apply_module_augmentations(module_name, name, prop_type);
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
                        declaration_order: props.len() as u32,
                        is_string_named: false,
                    });
                }
                props
            };

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

            let has_named_props = !props.is_empty();
            let namespace_type = has_named_props.then(|| {
                let namespace_type = factory.object(props);
                let preserve_namespace_display =
                    !(module_is_non_module_entity && self.ctx.allow_synthetic_default_imports());
                if preserve_namespace_display {
                    let display_module_name =
                        self.resolve_namespace_display_module_name(&exports_table, module_name);
                    self.ctx
                        .namespace_module_names
                        .insert(namespace_type, display_module_name);
                }
                namespace_type
            });
            if let Some(export_equals_type) = export_equals_type {
                if module_is_non_module_entity {
                    if self.ctx.allow_synthetic_default_imports() {
                        return Some(namespace_type.unwrap_or(export_equals_type));
                    }
                    return Some(export_equals_type);
                }
                return Some(
                    namespace_type
                        .map(|namespace_type| {
                            factory.intersection2(export_equals_type, namespace_type)
                        })
                        .unwrap_or(export_equals_type),
                );
            }

            return namespace_type;
        }

        // Use the unified JS export surface for the no-export-table fallback.
        // This synthesizes module.exports, exports.foo, Object.defineProperty,
        // and prototype assignments through one authority.
        if let Some(surface) =
            self.resolve_js_export_surface_for_module(module_name, source_file_idx)
            && surface.has_commonjs_exports
        {
            let display_name = self.imported_namespace_display_module_name(module_name);
            return surface.to_type_id_with_display_name(self, Some(display_name));
        }

        None
    }
}

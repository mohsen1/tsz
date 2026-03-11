use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;
use tsz_solver::Visibility;

impl<'a> CheckerState<'a> {
    fn collect_direct_commonjs_assignment_exports(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        expr_idx: NodeIndex,
        pending_props: &mut FxHashMap<String, NodeIndex>,
        ordered_names: &mut Vec<String>,
    ) {
        let Some(expr_node) = arena.get(expr_idx) else {
            return;
        };
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return;
        }
        let Some(binary) = arena.get_binary_expr(expr_node) else {
            return;
        };
        if binary.operator_token != tsz_scanner::SyntaxKind::EqualsToken as u16 {
            return;
        }

        let Some(left_node) = arena.get(binary.left) else {
            return;
        };
        if left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(left_access) = arena.get_access_expr(left_node)
        {
            let direct_exports_name = arena
                .get_identifier_at(left_access.expression)
                .and_then(|ident| {
                    (ident.escaped_text == "exports").then(|| {
                        arena
                            .get_identifier_at(left_access.name_or_argument)
                            .map(|name| name.escaped_text.to_string())
                    })
                })
                .flatten();
            let module_exports_name = arena.get(left_access.expression).and_then(|target_node| {
                if target_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    return None;
                }
                let target_access = arena.get_access_expr(target_node)?;
                let is_module_exports = arena
                    .get_identifier_at(target_access.expression)
                    .is_some_and(|ident| ident.escaped_text == "module")
                    && arena
                        .get_identifier_at(target_access.name_or_argument)
                        .is_some_and(|ident| ident.escaped_text == "exports");
                is_module_exports.then(|| {
                    arena
                        .get_identifier_at(left_access.name_or_argument)
                        .map(|name| name.escaped_text.to_string())
                })?
            });
            if let Some(name_text) = direct_exports_name.or(module_exports_name) {
                if !pending_props.contains_key(&name_text) {
                    ordered_names.push(name_text.clone());
                }
                pending_props.insert(name_text, binary.right);
            }
        }

        self.collect_direct_commonjs_assignment_exports(
            arena,
            binary.right,
            pending_props,
            ordered_names,
        );
    }

    fn infer_commonjs_export_rhs_type(
        &mut self,
        target_file_idx: usize,
        rhs_expr: NodeIndex,
    ) -> TypeId {
        if target_file_idx == self.ctx.current_file_idx {
            if let Some(literal_type) = self.literal_type_from_initializer(rhs_expr) {
                return literal_type;
            }
            return self.get_type_of_node(rhs_expr);
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
        checker.ctx.all_arenas = Some(all_arenas.clone());
        checker.ctx.all_binders = Some(all_binders.clone());
        checker.ctx.resolved_module_paths = self.ctx.resolved_module_paths.clone();
        checker.ctx.module_specifiers = self.ctx.module_specifiers.clone();
        checker.ctx.current_file_idx = target_file_idx;
        if !self.ctx.cross_file_symbol_targets.borrow().is_empty() {
            *checker.ctx.cross_file_symbol_targets.borrow_mut() =
                self.ctx.cross_file_symbol_targets.borrow().clone();
        }

        let ty = checker
            .literal_type_from_initializer(rhs_expr)
            .unwrap_or_else(|| checker.get_type_of_node(rhs_expr));
        for (&sym_id, &target_idx) in checker.ctx.cross_file_symbol_targets.borrow().iter() {
            self.ctx
                .cross_file_symbol_targets
                .borrow_mut()
                .insert(sym_id, target_idx);
        }
        ty
    }

    fn augment_namespace_props_with_direct_assignment_exports_for_file(
        &mut self,
        target_file_idx: usize,
        props: &mut Vec<tsz_solver::PropertyInfo>,
    ) {
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let Some(source_file) = target_arena.source_files.first() else {
            return;
        };
        let mut pending_props: FxHashMap<String, NodeIndex> = FxHashMap::default();
        let mut ordered_names: Vec<String> = Vec::new();

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = target_arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(stmt) = target_arena.get_expression_statement(stmt_node) else {
                continue;
            };
            self.collect_direct_commonjs_assignment_exports(
                target_arena,
                stmt.expression,
                &mut pending_props,
                &mut ordered_names,
            );
        }

        for name_text in ordered_names {
            let name_atom = self.ctx.types.intern_string(&name_text);
            if props.iter().any(|prop| prop.name == name_atom) {
                continue;
            }
            let Some(rhs_expr) = pending_props.get(&name_text).copied() else {
                continue;
            };
            let rhs_type = self.infer_commonjs_export_rhs_type(target_file_idx, rhs_expr);
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
            });
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

    pub(super) fn augment_namespace_props_with_define_property_exports(
        &mut self,
        module_name: &str,
        source_file_idx: Option<usize>,
        props: &mut Vec<tsz_solver::PropertyInfo>,
    ) {
        let target_file_idx = source_file_idx
            .and_then(|file_idx| {
                self.ctx
                    .resolve_import_target_from_file(file_idx, module_name)
            })
            .or_else(|| self.ctx.resolve_import_target(module_name));
        let Some(target_file_idx) = target_file_idx else {
            return;
        };

        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let Some(source_file) = target_arena.source_files.first() else {
            return;
        };

        for &stmt_idx in &source_file.statements.nodes {
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

            let Some(name_node) = target_arena.get(name_expr) else {
                continue;
            };
            if name_node.kind != tsz_scanner::SyntaxKind::StringLiteral as u16 {
                continue;
            }
            let Some(name_lit) = target_arena.get_literal(name_node) else {
                continue;
            };
            let name_atom = self.ctx.types.intern_string(&name_lit.text);
            if props.iter().any(|prop| prop.name == name_atom) {
                continue;
            }

            let Some(descriptor) = self.resolve_define_property_descriptor_object_literal(
                target_file_idx,
                target_arena,
                descriptor_expr,
            ) else {
                continue;
            };

            let mut has_value = false;
            let mut writable_true = false;
            let mut has_getter = false;
            let mut has_setter = false;

            for &element_idx in &descriptor.elements.nodes {
                let Some(element_node) = target_arena.get(element_idx) else {
                    continue;
                };
                match element_node.kind {
                    syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                        let Some(prop) = target_arena.get_property_assignment(element_node) else {
                            continue;
                        };
                        let Some(prop_name) =
                            crate::types_domain::queries::core::get_literal_property_name(
                                target_arena,
                                prop.name,
                            )
                        else {
                            continue;
                        };

                        match prop_name.as_str() {
                            "value" => {
                                has_value = true;
                            }
                            "writable" => {
                                writable_true =
                                    target_arena.get(prop.initializer).is_some_and(|init| {
                                        init.kind == tsz_scanner::SyntaxKind::TrueKeyword as u16
                                    });
                            }
                            _ => {}
                        }
                    }
                    syntax_kind_ext::GET_ACCESSOR => {
                        has_getter = true;
                    }
                    syntax_kind_ext::SET_ACCESSOR => {
                        has_setter = true;
                    }
                    _ => {}
                }
            }

            let readonly = !has_value || !writable_true || has_getter || has_setter;
            props.push(tsz_solver::PropertyInfo {
                name: name_atom,
                type_id: TypeId::ANY,
                write_type: TypeId::ANY,
                optional: false,
                readonly,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: props.len() as u32,
            });
        }
    }

    pub(crate) fn augment_namespace_props_with_commonjs_exports_for_file(
        &mut self,
        target_file_idx: usize,
        props: &mut Vec<tsz_solver::PropertyInfo>,
    ) {
        self.augment_namespace_props_with_direct_assignment_exports_for_file(
            target_file_idx,
            props,
        );
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let Some(source_file) = target_arena.source_files.first() else {
            return;
        };
        let module_name = source_file.file_name.clone();
        self.augment_namespace_props_with_define_property_exports(
            &module_name,
            Some(target_file_idx),
            props,
        );
    }
}

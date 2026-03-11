use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;
use tsz_solver::Visibility;

impl<'a> CheckerState<'a> {
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
}

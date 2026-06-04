impl<'a> NamespaceES5Transformer<'a> {
    /// Transform a variable statement in namespace. When `force_export` is true, the variable
    /// is always treated as exported (used for `export { variable }` wrappers).
    fn transform_variable_in_namespace(
        &self,
        ns_name: &str,
        var_idx: NodeIndex,
        force_export: bool,
    ) -> Option<IRNode> {
        let var_data = self.arena.get_variable_at(var_idx)?;
        if self.arena.is_declare(&var_data.modifiers) {
            return None;
        }

        let is_exported = force_export
            || self
                .arena
                .has_modifier(&var_data.modifiers, SyntaxKind::ExportKeyword);

        if let Some((env_name, using_async)) = self.active_namespace_using_env.borrow().clone() {
            let source_flags = self
                .arena
                .get(var_idx)
                .map_or(0, |node| self.variable_statement_source_using_flags(node));
            let mut decls = Vec::new();
            let mut temps = Vec::new();
            for &decl_list_idx in &var_data.declarations.nodes {
                let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                    continue;
                };
                let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                    continue;
                };
                let flags = source_flags
                    | decl_list.declarations.nodes.iter().fold(
                        decl_list_node.flags as u32,
                        |flags, &decl_idx| {
                            flags | self.arena.get_variable_declaration_flags(decl_idx)
                        },
                    );
                if flags & node_flags::USING == 0 {
                    continue;
                }
                for &decl_idx in &decl_list.declarations.nodes {
                    let Some(decl) = self.arena.get_variable_declaration_at(decl_idx) else {
                        continue;
                    };
                    let Some(name) = get_identifier_text(self.arena, decl.name) else {
                        continue;
                    };
                    let initializer = if decl.initializer.is_some() {
                        let converter = AstToIr::new(self.arena);
                        let expr = converter.convert_expression(decl.initializer);
                        temps.extend(converter.take_hoisted_temps());
                        expr
                    } else {
                        IRNode::void_0()
                    };
                    decls.push(IRNode::VarDecl {
                        name: name.into(),
                        initializer: Some(Box::new(IRNode::CallExpr {
                            callee: Box::new(IRNode::RuntimeHelper(
                                "__addDisposableResource".into(),
                            )),
                            arguments: vec![
                                IRNode::id(env_name.clone()),
                                initializer,
                                IRNode::BooleanLiteral(using_async),
                            ],
                        })),
                    });
                }
            }
            if !decls.is_empty() {
                self.hoisted_temps.borrow_mut().extend(temps);
                return Some(IRNode::Sequence(decls));
            }
        }

        if is_exported {
            // For exported variables, emit directly as namespace property assignments:
            // `Namespace.X = initializer;` instead of `var X = initializer; Namespace.X = X;`
            let (decls, temps) =
                convert_exported_variable_declarations(self.arena, &var_data.declarations, ns_name);
            self.hoisted_temps.borrow_mut().extend(temps);
            if decls.is_empty() {
                None
            } else {
                Some(IRNode::Sequence(decls))
            }
        } else {
            if self.variable_statement_has_binding_pattern(var_idx) {
                return Some(IRNode::ASTRef(var_idx));
            }

            let empty_decl_keyword =
                self.declaration_keyword_from_var_declarations(&var_data.declarations);
            let (decls, temps) = convert_variable_declarations(
                self.arena,
                &var_data.declarations,
                empty_decl_keyword,
            );
            self.hoisted_temps.borrow_mut().extend(temps);
            Some(IRNode::Sequence(decls))
        }
    }

    fn variable_statement_has_binding_pattern(&self, var_idx: NodeIndex) -> bool {
        let Some(var_data) = self.arena.get_variable_at(var_idx) else {
            return false;
        };

        var_data.declarations.nodes.iter().any(|&decl_list_idx| {
            self.arena
                .get_variable_at(decl_list_idx)
                .is_some_and(|decl_list| {
                    decl_list.declarations.nodes.iter().any(|&decl_idx| {
                        let Some(decl) = self.arena.get_variable_declaration_at(decl_idx) else {
                            return false;
                        };
                        self.arena.get(decl.name).is_some_and(|name| {
                            name.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                || name.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        })
                    })
                })
        })
    }

    /// Transform an enum in namespace. When `force_export` is true, the enum
    /// is always treated as exported (used for `export { enum }` wrappers).
    fn transform_enum_in_namespace(
        &self,
        ns_name: &str,
        enum_idx: NodeIndex,
        force_export: bool,
    ) -> Option<IRNode> {
        let is_exported = force_export || {
            let enum_node = self.arena.get(enum_idx)?;
            let enum_data = self.arena.get_enum(enum_node)?;
            self.arena
                .has_modifier(&enum_data.modifiers, SyntaxKind::ExportKeyword)
        };

        let mut enum_ir = transform_enum_to_ir(self.arena, enum_idx)?;
        let invalid_namespace_static = self.has_invalid_namespace_static_modifier(enum_idx);

        // For exported enums, fold the namespace export into the IIFE closing:
        // `(Color = A.Color || (A.Color = {}))` instead of separate `A.Color = Color;`
        if is_exported
            && let IRNode::EnumIIFE {
                namespace_export, ..
            } = &mut enum_ir
        {
            *namespace_export = Some(ns_name.to_string().into());
        }
        if invalid_namespace_static
            && let IRNode::EnumIIFE {
                invalid_namespace_static,
                ..
            } = &mut enum_ir
        {
            *invalid_namespace_static = true;
        }

        Some(enum_ir)
    }

    /// Core implementation for nested namespace transforms. When `force_export` is true,
    /// the namespace is always treated as exported (used for `export { namespace }` wrappers).
    fn transform_nested_namespace_core(
        &self,
        parent_ns: &str,
        ns_idx: NodeIndex,
        should_declare_var: bool,
        force_export: bool,
    ) -> Option<IRNode> {
        let ns_data = self.arena.get_module_at(ns_idx)?;

        // Skip ambient nested namespaces
        if self
            .arena
            .has_modifier(&ns_data.modifiers, SyntaxKind::DeclareKeyword)
        {
            return None;
        }

        let name_parts = self.flatten_module_name(ns_data.name)?;
        if name_parts.is_empty() {
            return None;
        }

        let is_exported = force_export
            || self
                .arena
                .has_modifier(&ns_data.modifiers, SyntaxKind::ExportKeyword);

        // Transform body
        let mut body = self.transform_namespace_body(ns_data.body, &name_parts);
        self.rewrite_const_enum_accesses(&mut body, &name_parts);

        // Skip non-instantiated namespaces (only contain types).
        if !body.iter().any(|n| !is_comment_node(n)) && !self.has_value_declarations(ns_data.body) {
            return None;
        }

        // Detect collision: rename the IIFE parameter (e.g., A -> A_1) when a
        // top-level member OR any nested-scope binding (e.g. a parameter
        // `function f(A) {}`) shadows the innermost namespace name.
        let innermost_name = name_parts.last().map_or("", |s| s.as_str());
        let nested_conflict = self.namespace_body_has_nested_binding(ns_data.body, innermost_name);
        let param_name = detect_and_apply_param_rename_with_extra(
            &mut body,
            innermost_name,
            nested_conflict,
            || self.next_renamed_iife_param(innermost_name),
        );

        let name = name_parts.first().cloned().unwrap_or_default();

        Some(IRNode::NamespaceIIFE {
            name: name.into(),
            name_parts: name_parts.into_iter().map(Into::into).collect(),
            body,
            is_exported,
            attach_to_exports: is_exported && self.is_commonjs,
            commonjs_export_names: self
                .commonjs_export_names
                .iter()
                .cloned()
                .map(Into::into)
                .collect(),
            system_export_names: Vec::new(),
            should_declare_var,
            // Nested namespaces live directly inside a `MODULE_BLOCK`, which is a
            // scope top, so their hoisted binding is never reset.
            hoist_var_void_zero: false,
            default_export_merge: false,
            parent_name: is_exported.then(|| parent_ns.to_string().into()),
            param_name: param_name.map(Into::into),
            skip_sequence_indent: true, // Nested namespace IIFEs need to skip indent when in sequence
            trailing_comment: self
                .extract_namespace_trailing_comment(ns_data.body)
                .map(Into::into),
            invalid_namespace_static: self.has_invalid_namespace_static_modifier(ns_idx),
        })
    }
}

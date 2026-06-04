impl<'a> ES5ClassTransformer<'a> {
    /// Emit `__decorate` calls for decorated members inside the IIFE body.
    pub(super) fn emit_member_decorator_ir(&self, body: &mut Vec<IRNode>, class_idx: NodeIndex) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return;
        };

        // Track accessor names that have already been emitted so that
        // getter/setter pairs produce only one __decorate call (the first one).
        let mut emitted_accessor_names = std::collections::HashSet::<String>::new();

        // `tsc` emits per-member `__decorate` calls for all decorated
        // instance/prototype members (in declaration order) before any decorated
        // static members (in declaration order).
        let ordered_members = self.legacy_member_decorate_order_es5(&class_data.members.nodes);

        for &member_idx in &ordered_members {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            enum MemberMeta {
                Property {
                    type_annotation: NodeIndex,
                },
                Method {
                    parameters: NodeList,
                    return_type: NodeIndex,
                    async_returns_promise: bool,
                },
                Accessor {
                    name: NodeIndex,
                    is_static: bool,
                },
            }

            let (modifiers, name_idx, is_property, is_accessor, meta) = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    // Skip overload signatures (no body) — decorators on overloads
                    // are not emitted as __decorate targets
                    if !method.body.is_some() {
                        continue;
                    }
                    let has_async_modifier = self
                        .arena
                        .has_modifier(&method.modifiers, SyntaxKind::AsyncKeyword);
                    let has_generator_asterisk = method.asterisk_token
                        || crate::transforms::emit_utils::source_header_has_async_generator_asterisk(
                            self.source_text,
                            member_node.pos,
                            self.arena
                                .get(method.body)
                                .map_or(member_node.end, |body| body.pos),
                        );
                    let meta = MemberMeta::Method {
                        parameters: method.parameters.clone(),
                        return_type: method.type_annotation,
                        async_returns_promise: has_async_modifier && !has_generator_asterisk,
                    };
                    (&method.modifiers, method.name, false, false, meta)
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    let is_auto_accessor = self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword);
                    let meta = MemberMeta::Property {
                        type_annotation: prop.type_annotation,
                    };
                    (&prop.modifiers, prop.name, !is_auto_accessor, false, meta)
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (
                        &accessor.modifiers,
                        accessor.name,
                        false,
                        true,
                        MemberMeta::Accessor {
                            name: accessor.name,
                            is_static: self.arena.is_static(&accessor.modifiers),
                        },
                    )
                }
                _ => continue,
            };

            let is_static = self.arena.is_static(modifiers);

            // Member-level decorators. Accessors merge the member decorators from
            // the whole getter/setter pair into the single `__decorate` call.
            let decorators = if is_accessor {
                self.collect_accessor_member_decorators_es5(
                    &class_data.members.nodes,
                    name_idx,
                    is_static,
                )
            } else {
                self.collect_decorators_from_modifiers(modifiers)
            };

            // Parameter decorators. Methods use their own parameters; accessors
            // merge the parameter decorators from the whole getter/setter pair
            // (the setter's `@dec param`) into the accessor's single call.
            let param_decorators: Vec<(usize, Vec<NodeIndex>)> = if is_accessor {
                self.collect_accessor_param_decorators_es5(
                    &class_data.members.nodes,
                    name_idx,
                    is_static,
                )
            } else {
                match &meta {
                    MemberMeta::Method { parameters, .. } => {
                        self.collect_param_decorators_es5(parameters)
                    }
                    _ => Vec::new(),
                }
            };

            if decorators.is_empty() && param_decorators.is_empty() {
                continue;
            }

            let member_name = get_identifier_text(self.arena, name_idx);
            let Some(member_name) = member_name else {
                continue;
            };
            if member_name.is_empty() {
                continue;
            }

            // For getter/setter pairs, tsc emits only one __decorate call
            // for the first accessor of the pair. Skip the second; its parameter
            // decorators were already merged in above.
            if is_accessor && !emitted_accessor_names.insert(member_name.clone()) {
                continue;
            }

            let mut dec_strs = self.render_decorator_expressions(&decorators);
            // Add __param entries for parameter decorators
            for (param_idx, param_decs) in &param_decorators {
                for dec_idx in param_decs {
                    let dec_str = self.render_single_decorator_expression(*dec_idx);
                    if let Some(dec_str) = dec_str {
                        dec_strs.push(format!(
                            "{}({param_idx}, {dec_str})",
                            self.helper_name("__param")
                        ));
                    }
                }
            }
            let target_str = if is_static {
                self.class_name.clone()
            } else {
                format!("{}.prototype", self.class_name)
            };
            let desc_str = if is_property { "void 0" } else { "null" };

            // Collect metadata strings if emit_decorator_metadata is enabled
            let metadata_strs: Vec<String> = if self.emit_decorator_metadata {
                match &meta {
                    MemberMeta::Property { type_annotation } => {
                        let serialized = serialize_type_for_metadata(self.arena, *type_annotation);
                        vec![format!(
                            "{}(\"design:type\", {serialized})",
                            self.helper_name("__metadata")
                        )]
                    }
                    MemberMeta::Method {
                        parameters,
                        return_type,
                        async_returns_promise,
                    } => {
                        let param_types = serialize_param_types(self.arena, parameters);
                        let ret_type = if return_type.is_some() {
                            serialize_type_for_metadata(self.arena, *return_type)
                        } else if *async_returns_promise {
                            "Promise".to_string()
                        } else {
                            "void 0".to_string()
                        };
                        vec![
                            format!(
                                "{}(\"design:type\", Function)",
                                self.helper_name("__metadata")
                            ),
                            format!(
                                "{}(\"design:paramtypes\", [{param_types}])",
                                self.helper_name("__metadata")
                            ),
                            format!(
                                "{}(\"design:returntype\", {ret_type})",
                                self.helper_name("__metadata")
                            ),
                        ]
                    }
                    MemberMeta::Accessor { name, is_static } => {
                        self.accessor_metadata_strings(&class_data.members.nodes, *name, *is_static)
                    }
                }
            } else {
                Vec::new()
            };

            // Format matching tsc:
            // __decorate([\n        dec1,\n        dec2\n    ], target, "name", desc)
            // Note: first line indent is handled by the body emitter's write_indent().
            // Continuation lines after \n need absolute indentation from column 0.
            // The indent_base accounts for nesting (e.g., namespace IIFE body).
            let inner_indent = "    ".repeat((self.indent_base + 2) as usize);
            let outer_indent = "    ".repeat((self.indent_base + 1) as usize);
            let total_entries = dec_strs.len() + metadata_strs.len();
            let mut raw = String::new();
            raw.push_str(&self.helper_name("__decorate"));
            raw.push_str("([");
            for (i, dec_str) in dec_strs.iter().enumerate() {
                raw.push('\n');
                raw.push_str(&inner_indent);
                raw.push_str(dec_str);
                if i + 1 < total_entries {
                    raw.push(',');
                }
            }
            for (i, meta_str) in metadata_strs.iter().enumerate() {
                raw.push('\n');
                raw.push_str(&inner_indent);
                raw.push_str(meta_str);
                if dec_strs.len() + i + 1 < total_entries {
                    raw.push(',');
                }
            }
            raw.push('\n');
            raw.push_str(&outer_indent);
            raw.push_str("], ");
            raw.push_str(&target_str);
            raw.push_str(", \"");
            raw.push_str(&member_name);
            raw.push_str("\", ");
            raw.push_str(desc_str);
            raw.push(')');

            body.push(IRNode::ExpressionStatement(Box::new(IRNode::Raw(
                raw.into(),
            ))));
        }
    }

    /// Emit `ClassName = __decorate([dec1, ...], ClassName)` for class-level decorators.
    /// When `emit_decorator_metadata` is enabled and the class has a constructor,
    /// also includes `__metadata("design:paramtypes", [...])` in the decorator array.
    pub(super) fn emit_class_decorator_ir(&self, body: &mut Vec<IRNode>, class_idx: NodeIndex) {
        let dec_strs = self.render_decorator_expressions(&self.class_decorators);
        if dec_strs.is_empty() {
            return;
        }

        // Collect constructor parameter decorators (__param entries).
        // tsc includes these in the class-level __decorate call between
        // class decorators and __metadata entries.
        let mut param_strs: Vec<String> = Vec::new();
        let mut metadata_strs: Vec<String> = Vec::new();
        if let Some(class_node) = self.arena.get(class_idx)
            && let Some(class_data) = self.arena.get_class(class_node)
        {
            for &member_idx in &class_data.members.nodes {
                if let Some(member_node) = self.arena.get(member_idx)
                    && member_node.kind == syntax_kind_ext::CONSTRUCTOR
                    && let Some(ctor) = self.arena.get_constructor(member_node)
                {
                    // Collect __param entries for constructor parameter decorators
                    let all_param_decs = self.collect_param_decorators_es5(&ctor.parameters);
                    for (param_idx, decs) in &all_param_decs {
                        for dec_idx in decs {
                            if let Some(dec_str) = self.render_single_decorator_expression(*dec_idx)
                            {
                                param_strs.push(format!(
                                    "{}({param_idx}, {dec_str})",
                                    self.helper_name("__param")
                                ));
                            }
                        }
                    }

                    // Build constructor paramtypes metadata if emit_decorator_metadata is enabled
                    if self.emit_decorator_metadata {
                        let param_types = serialize_param_types(self.arena, &ctor.parameters);
                        metadata_strs.push(format!(
                            "{}(\"design:paramtypes\", [{param_types}])",
                            self.helper_name("__metadata")
                        ));
                    }
                    break;
                }
            }
        }

        // Format matching tsc:
        // ClassName = __decorate([\n        dec1,\n        __param(0, dec),\n        __metadata(...)\n    ], ClassName)
        let inner_indent = "    ".repeat((self.indent_base + 2) as usize);
        let outer_indent = "    ".repeat((self.indent_base + 1) as usize);
        let total_entries = dec_strs.len() + param_strs.len() + metadata_strs.len();
        let mut raw = String::new();
        raw.push_str(&self.class_name);
        raw.push_str(" = ");
        if let Some(alias) = self.class_self_reference_alias.as_ref() {
            raw.push_str(alias);
            raw.push_str(" = ");
        }
        raw.push_str(&self.helper_name("__decorate"));
        raw.push_str("([");
        let mut written = 0;
        for dec_str in &dec_strs {
            raw.push('\n');
            raw.push_str(&inner_indent);
            raw.push_str(dec_str);
            written += 1;
            if written < total_entries {
                raw.push(',');
            }
        }
        for param_str in &param_strs {
            raw.push('\n');
            raw.push_str(&inner_indent);
            raw.push_str(param_str);
            written += 1;
            if written < total_entries {
                raw.push(',');
            }
        }
        for meta_str in &metadata_strs {
            raw.push('\n');
            raw.push_str(&inner_indent);
            raw.push_str(meta_str);
            written += 1;
            if written < total_entries {
                raw.push(',');
            }
        }
        raw.push('\n');
        raw.push_str(&outer_indent);
        raw.push_str("], ");
        raw.push_str(&self.class_name);
        raw.push(')');

        body.push(IRNode::ExpressionStatement(Box::new(IRNode::Raw(
            raw.into(),
        ))));
    }

    /// Emit `ClassName = __decorate([__param(0, dec), ...], ClassName)` for constructor
    /// parameter decorators when there are no class-level decorators. tsc emits this
    /// at the class level when a constructor parameter has a decorator.
    pub(super) fn emit_ctor_param_decorator_ir(
        &self,
        body: &mut Vec<IRNode>,
        class_idx: NodeIndex,
    ) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return;
        };

        // Find the constructor and collect its parameter decorators
        let mut all_param_decs: Vec<(usize, Vec<NodeIndex>)> = Vec::new();
        for &member_idx in &class_data.members.nodes {
            if let Some(member_node) = self.arena.get(member_idx)
                && member_node.kind == syntax_kind_ext::CONSTRUCTOR
                && let Some(ctor) = self.arena.get_constructor(member_node)
            {
                all_param_decs = self.collect_param_decorators_es5(&ctor.parameters);
                break;
            }
        }

        if all_param_decs.is_empty() {
            return;
        }

        // Build __param(index, dec) strings
        let mut param_strs: Vec<String> = Vec::new();
        for (param_idx, decs) in &all_param_decs {
            for dec_idx in decs {
                if let Some(dec_str) = self.render_single_decorator_expression(*dec_idx) {
                    param_strs.push(format!(
                        "{}({param_idx}, {dec_str})",
                        self.helper_name("__param")
                    ));
                }
            }
        }

        if param_strs.is_empty() {
            return;
        }

        // Build constructor paramtypes metadata if emit_decorator_metadata is enabled
        let metadata_strs: Vec<String> = if self.emit_decorator_metadata {
            let mut meta = Vec::new();
            for &member_idx in &class_data.members.nodes {
                if let Some(member_node) = self.arena.get(member_idx)
                    && member_node.kind == syntax_kind_ext::CONSTRUCTOR
                    && let Some(ctor) = self.arena.get_constructor(member_node)
                {
                    let param_types = serialize_param_types(self.arena, &ctor.parameters);
                    meta.push(format!(
                        "{}(\"design:paramtypes\", [{param_types}])",
                        self.helper_name("__metadata")
                    ));
                    break;
                }
            }
            meta
        } else {
            Vec::new()
        };

        let inner_indent = "    ".repeat((self.indent_base + 2) as usize);
        let outer_indent = "    ".repeat((self.indent_base + 1) as usize);
        let total_entries = param_strs.len() + metadata_strs.len();
        let mut raw = String::new();
        raw.push_str(&self.class_name);
        raw.push_str(" = ");
        raw.push_str(&self.helper_name("__decorate"));
        raw.push_str("([");
        for (i, param_str) in param_strs.iter().enumerate() {
            raw.push('\n');
            raw.push_str(&inner_indent);
            raw.push_str(param_str);
            if i + 1 < total_entries {
                raw.push(',');
            }
        }
        for (i, meta_str) in metadata_strs.iter().enumerate() {
            raw.push('\n');
            raw.push_str(&inner_indent);
            raw.push_str(meta_str);
            if param_strs.len() + i + 1 < total_entries {
                raw.push(',');
            }
        }
        raw.push('\n');
        raw.push_str(&outer_indent);
        raw.push_str("], ");
        raw.push_str(&self.class_name);
        raw.push(')');

        body.push(IRNode::ExpressionStatement(Box::new(IRNode::Raw(
            raw.into(),
        ))));
    }
}

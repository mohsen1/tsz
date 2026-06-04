impl<'a> DeclarationEmitter<'a> {
    /// Whether the *value* meaning of `sym_id` is a class constructor that was
    /// merged into a same-named namespace/module.
    ///
    /// This is the classic ambient pattern `namespace N { class N {} } export = N`
    /// (e.g. `@types/node`'s `events`): the exported symbol is the namespace,
    /// but its self-named member is a class, so a reference to `N` as a value is
    /// a class constructor. tsc emits `extends N` directly here rather than
    /// synthesizing a `_base` const. The match is structural (member name equals
    /// the namespace's own name); no specific identifier is hard-coded.
    fn symbol_value_meaning_is_class_constructor(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> bool {
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };
        if symbol.flags & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE) == 0 {
            return false;
        }
        let Some(exports) = symbol.exports.as_ref() else {
            return false;
        };
        let Some(self_member) = exports.get(symbol.escaped_name.as_str()) else {
            return false;
        };
        if self_member == sym_id {
            return false;
        }
        self.symbol_is_class_constructor(self_member, binder)
    }

    pub(in crate::declaration_emitter) fn js_extends_entity_reference_has_any_annotation(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        self.source_is_js_file
            && self
                .js_extends_entity_reference_declared_type_text(expr_idx)
                .is_some_and(|type_text| {
                    self.jsdoc_type_text_for_declaration_emit(&type_text).trim() == "any"
                })
    }

    pub(in crate::declaration_emitter) fn js_extends_entity_reference_declared_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        self.reference_declared_type_annotation_text(expr_idx)
            .or_else(|| self.value_reference_initializer_asserted_type_text(expr_idx))
    }

    fn value_reference_initializer_asserted_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let binder = self.binder?;
        let raw_sym_id = self.value_reference_symbol(expr_idx)?;
        let sym_id = self
            .resolve_portability_import_alias(raw_sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_declaration_symbol(raw_sym_id, binder));

        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let decl_idx = Self::annotation_bearing_declaration_from_arena(source_arena, decl_idx)
                .unwrap_or(decl_idx);
            let decl_node = source_arena.get(decl_idx)?;
            let initializer = source_arena
                .get_variable_declaration(decl_node)
                .and_then(|decl| decl.initializer.is_some().then_some(decl.initializer))
                .or_else(|| {
                    source_arena
                        .get_property_decl(decl_node)
                        .and_then(|decl| decl.initializer.is_some().then_some(decl.initializer))
                })?;

            if std::ptr::eq(source_arena, self.arena) {
                return self
                    .explicit_asserted_type_text(initializer)
                    .or_else(|| self.jsdoc_type_text_for_node(initializer));
            }

            let asserted_type =
                Self::explicit_asserted_type_node_from_arena(source_arena, initializer)?;
            let type_text =
                self.type_annotation_text_from_arena_node(source_arena, asserted_type)?;
            Some(self.jsdoc_type_text_for_declaration_emit(&type_text))
        })
    }

    pub(in crate::declaration_emitter) fn js_variable_dependency_is_synthetic_class_extends_alias_source(
        &self,
        name_idx: NodeIndex,
    ) -> bool {
        if !self.source_is_js_file
            || !self.public_api_filter_enabled()
            || self
                .get_identifier_text(name_idx)
                .is_some_and(|name| self.public_api_type_surface_contains_typeof_name(&name))
        {
            return false;
        }
        let Some(source_sym_id) = self.declaration_name_symbol(name_idx) else {
            return false;
        };
        let source_name = self.get_identifier_text(name_idx);

        self.arena
            .nodes
            .iter()
            .enumerate()
            .any(|(class_idx, class_node)| {
                if class_node.kind != syntax_kind_ext::CLASS_DECLARATION {
                    return false;
                }
                let Some(class) = self.arena.get_class(class_node) else {
                    return false;
                };
                let Ok(class_idx) = u32::try_from(class_idx) else {
                    return false;
                };
                let class_idx = NodeIndex(class_idx);
                let wrapped_export = self.arena.nodes.iter().any(|node| {
                    self.arena
                        .get_export_decl(node)
                        .is_some_and(|export| export.export_clause == class_idx)
                });
                let parent_export = self
                    .arena
                    .get_extended(class_idx)
                    .map(|ext| ext.parent)
                    .is_some_and(|parent| self.statement_has_effective_export(parent));
                if !self.statement_has_effective_export(class_idx)
                    && !wrapped_export
                    && !parent_export
                    && !self.should_emit_public_api_dependency(class.name)
                {
                    return false;
                }
                let Some(heritage) = class.heritage_clauses.as_ref() else {
                    return false;
                };
                let Some((type_idx, expr_idx)) = self.non_nameable_extends_heritage_type(heritage)
                else {
                    return false;
                };
                let reference_name = self.nameable_constructor_expression_text(expr_idx);
                self.js_entity_extends_needs_synthetic_alias(type_idx)
                    && (self.value_reference_symbol(expr_idx) == Some(source_sym_id)
                        || (source_name.is_some() && reference_name == source_name))
            })
    }

    fn declaration_name_symbol(&self, name_idx: NodeIndex) -> Option<SymbolId> {
        let binder = self.binder?;
        binder.node_symbols.get(&name_idx.0).copied().or_else(|| {
            self.get_identifier_text(name_idx)
                .and_then(|name| binder.file_locals.get(&name))
        })
    }

    pub(in crate::declaration_emitter) fn initializer_is_new_expression(
        &self,
        initializer: NodeIndex,
    ) -> bool {
        let initializer = self.skip_parenthesized_non_null_and_comma(initializer);
        self.arena
            .get(initializer)
            .is_some_and(|node| node.kind == syntax_kind_ext::NEW_EXPRESSION)
    }

    pub(in crate::declaration_emitter) fn new_expression_constructor_is_class_like(
        &self,
        initializer: NodeIndex,
    ) -> bool {
        let initializer = self.skip_parenthesized_non_null_and_comma(initializer);
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return false;
        }
        let Some(new_expr) = self.arena.get_call_expr(init_node) else {
            return false;
        };
        let Some(expr_idx) = self.skip_parenthesized_expression(new_expr.expression) else {
            return false;
        };
        if self
            .nameable_constructor_expression_text(expr_idx)
            .is_none()
        {
            return false;
        }
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(sym_id) = self.value_reference_symbol(expr_idx) else {
            return false;
        };
        let sym_id = self.resolve_portability_symbol(sym_id, binder);
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };
        (symbol.flags & symbol_flags::CLASS) != 0
            || symbol.declarations.iter().copied().any(|decl_idx| {
                self.arena.get(decl_idx).is_some_and(|decl_node| {
                    decl_node.kind == syntax_kind_ext::CLASS_DECLARATION
                        || decl_node.kind == syntax_kind_ext::CLASS_EXPRESSION
                })
            })
    }

    pub(in crate::declaration_emitter) fn emit_direct_symbol_dependency_for_type(
        &mut self,
        type_id: tsz_solver::TypeId,
    ) {
        let Some(binder) = self.binder else {
            return;
        };
        let Some(interner) = self.type_interner else {
            return;
        };
        let Some(type_cache) = self.type_cache.as_ref() else {
            return;
        };

        let symbol_id = tsz_solver::visitor::lazy_def_id(interner, type_id)
            .and_then(|def_id| type_cache.def_to_symbol.get(&def_id).copied())
            .or_else(|| {
                tsz_solver::visitor::object_shape_id(interner, type_id)
                    .or_else(|| tsz_solver::visitor::object_with_index_shape_id(interner, type_id))
                    .and_then(|shape_id| interner.object_shape(shape_id).symbol)
            })
            .or_else(|| {
                tsz_solver::visitor::callable_shape_id(interner, type_id)
                    .and_then(|shape_id| interner.callable_shape(shape_id).symbol)
            });
        let Some(symbol_id) = symbol_id else {
            return;
        };
        if !self.emitted_synthetic_dependency_symbols.insert(symbol_id) {
            return;
        }

        let Some(symbol) = binder.symbols.get(symbol_id) else {
            return;
        };
        let Some(decl_idx) = symbol.declarations.first().copied() else {
            return;
        };
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };
        let wrapped_export = self.arena.nodes.iter().any(|node| {
            self.arena
                .get_export_decl(node)
                .is_some_and(|export| export.export_clause == decl_idx)
        });
        let has_effective_export = self.statement_has_effective_export(decl_idx)
            || self
                .arena
                .get_extended(decl_idx)
                .map(|ext| ext.parent)
                .is_some_and(|parent| self.statement_has_effective_export(parent))
            || wrapped_export;

        let saved_emit_public_api_only = self.emit_public_api_only;
        match decl_node.kind {
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                if has_effective_export {
                    self.emitted_synthetic_dependency_symbols.remove(&symbol_id);
                } else {
                    self.emit_public_api_only = false;
                    self.emit_interface_declaration(decl_idx);
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                let should_emit = saved_emit_public_api_only
                    && !has_effective_export
                    && self.arena.get_class(decl_node).is_some_and(|class| {
                        !self
                            .arena
                            .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
                    });
                if should_emit {
                    self.emit_public_api_only = false;
                    self.emit_class_declaration(decl_idx);
                } else {
                    self.emitted_synthetic_dependency_symbols.remove(&symbol_id);
                }
            }
            _ => {
                self.emitted_synthetic_dependency_symbols.remove(&symbol_id);
            }
        }
        self.emit_public_api_only = saved_emit_public_api_only;
    }

    pub(crate) fn emit_synthetic_class_extends_alias_if_needed(
        &mut self,
        class_name: NodeIndex,
        heritage: Option<&NodeList>,
        is_default_export: bool,
    ) -> Option<String> {
        let type_id = self.synthetic_class_extends_alias_type_id(heritage)?;
        self.retain_direct_type_symbols_for_public_api(type_id);
        if self.used_symbols.is_none() {
            self.emit_direct_symbol_dependency_for_type(type_id);
        }
        let alias_name = if is_default_export {
            "default_base".to_string()
        } else {
            let class_name = self.get_identifier_text(class_name)?;
            format!("{class_name}_base")
        };

        self.write_indent();
        if self.should_emit_declare_keyword(false) {
            self.write("declare ");
        }
        self.write("const ");
        self.write(&alias_name);
        self.write(": ");
        let type_text = self.print_synthetic_class_extends_alias_type(type_id);
        let source_type_text = self
            .synthetic_class_extends_alias_source_type_text(heritage)
            .or_else(|| {
                let heritage = heritage?;
                let (_, expr_idx) = self.non_nameable_extends_heritage_type(heritage)?;
                self.js_extends_entity_reference_declared_type_text(expr_idx)
                    .map(|type_text| self.jsdoc_type_text_for_declaration_emit(&type_text))
            });
        let prefer_source_text = type_text == "never"
            || source_type_text.as_ref().is_some_and(|source_text| {
                source_text.contains(" & ")
                    || (Self::is_constructor_object_type_text(source_text)
                        && Self::type_text_has_conditional_infer_surface(&type_text))
            });
        let type_text = if prefer_source_text {
            source_type_text.unwrap_or(type_text)
        } else {
            type_text
        };
        self.write(&type_text);
        self.write(";");
        self.write_line();
        self.emitted_non_exported_declaration = true;

        Some(alias_name)
    }

    fn enum_member_literal_initializer_value(
        interner: &tsz_solver::construction::TypeInterner,
        type_id: tsz_solver::types::TypeId,
    ) -> Option<tsz_solver::types::LiteralValue> {
        let (_def_id, member_type) = tsz_solver::visitor::enum_components(interner, type_id)?;
        tsz_solver::visitor::literal_value(interner, member_type)
    }

    fn call_expression_has_matching_primitive_literal_argument(
        &self,
        initializer: NodeIndex,
        type_text: &str,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }
        let Some(call) = self.arena.get_call_expr(init_node) else {
            return false;
        };
        call.arguments.as_ref().is_some_and(|args| {
            args.nodes.iter().copied().any(|arg| {
                self.primitive_literal_argument_type_text(arg).as_deref() == Some(type_text)
            })
        })
    }
}

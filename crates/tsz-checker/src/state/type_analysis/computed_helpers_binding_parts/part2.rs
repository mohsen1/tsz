impl<'a> CheckerState<'a> {
    /// Resolve a single `TYPE_QUERY` node with flow narrowing applied.
    ///
    /// For simple identifiers (e.g. `typeof c`), resolves the identifier's type
    /// using `get_type_of_node` which applies control-flow narrowing. For other
    /// forms, falls back to the standard `get_type_from_type_query`.
    fn resolve_type_query_with_flow(&mut self, tq_idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(tq_idx) else {
            return self.get_type_from_type_query(tq_idx);
        };
        let Some(type_query) = self.ctx.arena.get_type_query(node) else {
            return self.get_type_from_type_query(tq_idx);
        };

        let expr_name = type_query.expr_name;
        let Some(expr_node) = self.ctx.arena.get(expr_name) else {
            return self.get_type_from_type_query(tq_idx);
        };

        // Only apply flow-aware resolution for simple identifiers
        if expr_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return self.get_type_from_type_query(tq_idx);
        }

        // Resolve the identifier's type with flow narrowing enabled.
        let expr_type = self.get_type_of_node_with_request(expr_name, &TypingRequest::NONE);

        // If we got a useful type (not ANY/ERROR), use it.
        // Otherwise fall back to the standard non-flow path.
        if expr_type != TypeId::ANY && expr_type != TypeId::ERROR {
            expr_type
        } else {
            self.get_type_from_type_query(tq_idx)
        }
    }

    /// Recursively collect `TYPE_QUERY` node indices from a type node subtree.
    fn collect_type_query_nodes(&self, idx: NodeIndex, out: &mut Vec<NodeIndex>) {
        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::TYPE_QUERY {
            out.push(idx);
            return;
        }

        if node.kind == syntax_kind_ext::TYPE_LITERAL {
            if let Some(data) = self.ctx.arena.get_type_literal(node) {
                for &member_idx in &data.members.nodes {
                    self.collect_type_query_nodes(member_idx, out);
                }
            }
            return;
        }

        if node.kind == syntax_kind_ext::INDEX_SIGNATURE {
            if let Some(data) = self.ctx.arena.get_index_signature(node)
                && data.type_annotation.is_some()
            {
                self.collect_type_query_nodes(data.type_annotation, out);
            }
            return;
        }

        if node.kind == syntax_kind_ext::PROPERTY_SIGNATURE
            || node.kind == syntax_kind_ext::PROPERTY_DECLARATION
        {
            if let Some(data) = self.ctx.arena.get_property_decl(node)
                && data.type_annotation.is_some()
            {
                self.collect_type_query_nodes(data.type_annotation, out);
            }
            return;
        }

        if node.kind == syntax_kind_ext::UNION_TYPE
            || node.kind == syntax_kind_ext::INTERSECTION_TYPE
        {
            if let Some(data) = self.ctx.arena.get_composite_type(node) {
                for &member_idx in &data.types.nodes {
                    self.collect_type_query_nodes(member_idx, out);
                }
            }
            return;
        }

        if node.kind == syntax_kind_ext::ARRAY_TYPE
            && let Some(data) = self.ctx.arena.get_array_type(node)
        {
            self.collect_type_query_nodes(data.element_type, out);
        }
    }

    /// Check if a symbol is a type-only export (excludable from namespace value type).
    pub(crate) fn is_type_only_export_symbol(&self, sym_id: SymbolId) -> bool {
        let symbol = self.get_cross_file_symbol(sym_id);
        let Some(symbol) = symbol else {
            return false;
        };
        if !symbol.is_type_only {
            return false;
        }
        if symbol.has_any_flags(symbol_flags::ALIAS) && symbol.has_any_flags(symbol_flags::VALUE) {
            return false;
        }
        true
    }

    /// Check if an export symbol has no value component (type-only).
    pub(crate) fn export_symbol_has_no_value(&self, sym_id: SymbolId) -> bool {
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders));
        let Some(symbol) = symbol else {
            return false;
        };

        let flags = symbol.flags;
        if (flags & symbol_flags::VALUE) != 0 {
            if (flags & symbol_flags::VALUE_MODULE) != 0
                && (flags & symbol_flags::NAMESPACE_MODULE) != 0
                && self.is_module_uninstantiated(sym_id)
            {
                return true;
            }
            return false;
        }
        if (flags & symbol_flags::NAMESPACE_MODULE) != 0 {
            return true;
        }
        if (flags & symbol_flags::TYPE) != 0 {
            return true;
        }
        if flags & symbol_flags::ALIAS != 0 {
            let mut visited = AliasCycleTracker::new();
            if let Some(target) = self.resolve_alias_symbol(sym_id, &mut visited) {
                let target_sym = self
                    .get_cross_file_symbol(target)
                    .or_else(|| self.ctx.binder.get_symbol_with_libs(target, &lib_binders));
                if let Some(target_sym) = target_sym {
                    let tf = target_sym.flags;
                    if (tf & symbol_flags::VALUE) != 0 {
                        if (tf & symbol_flags::VALUE_MODULE) != 0
                            && (tf & symbol_flags::NAMESPACE_MODULE) != 0
                            && self.is_module_uninstantiated(target)
                        {
                            return true;
                        }
                        return false;
                    }
                    if (tf & symbol_flags::NAMESPACE_MODULE) != 0 {
                        return true;
                    }
                    return (tf & symbol_flags::TYPE) != 0;
                }
            }
        }
        false
    }

    fn is_module_uninstantiated(&self, sym_id: SymbolId) -> bool {
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders));
        let Some(symbol) = symbol else {
            return false;
        };
        let Some(exports) = &symbol.exports else {
            return true;
        };
        for (_, &export_sym_id) in exports.iter() {
            let export_sym = self.get_cross_file_symbol(export_sym_id).or_else(|| {
                self.ctx
                    .binder
                    .get_symbol_with_libs(export_sym_id, &lib_binders)
            });
            let Some(export_sym) = export_sym else {
                continue;
            };
            let ef = export_sym.flags;
            if (ef & (symbol_flags::VALUE & !symbol_flags::VALUE_MODULE)) != 0 {
                return false;
            }
            if (ef & symbol_flags::VALUE_MODULE) != 0
                && !self.is_module_uninstantiated(export_sym_id)
            {
                return false;
            }
        }
        true
    }

    /// Check if a named export was reached through a `export type *` wildcard chain.
    pub(crate) fn is_export_from_type_only_wildcard(
        &self,
        module_name: &str,
        export_name: &str,
    ) -> bool {
        let Some(target_file_idx) = self.ctx.resolve_import_target(module_name) else {
            return false;
        };
        let Some(target_binder) = self.ctx.get_binder_for_file(target_file_idx) else {
            return false;
        };
        let target_file_name = self
            .ctx
            .get_arena_for_file(target_file_idx as u32)
            .source_files
            .first()
            .map(|sf| sf.file_name.as_str());
        let Some(file_name) = target_file_name else {
            return false;
        };
        if let Some((sym_id, true)) =
            target_binder.resolve_import_with_reexports_type_only(file_name, export_name)
        {
            if let Some(sym) = target_binder.symbols.get(sym_id)
                && sym.has_any_flags(symbol_flags::ALIAS)
                && sym.has_any_flags(symbol_flags::VALUE)
            {
                return false;
            }
            true
        } else {
            false
        }
    }

    pub(crate) fn report_private_identifier_outside_class(
        &mut self,
        name_idx: NodeIndex,
        property_name: &str,
        object_type: TypeId,
        object_expr: NodeIndex,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        let class_name = self.get_private_identifier_declaring_class_name(
            object_type,
            object_expr,
            property_name,
        );
        let message = format_message(
            diagnostic_messages::PROPERTY_IS_NOT_ACCESSIBLE_OUTSIDE_CLASS_BECAUSE_IT_HAS_A_PRIVATE_IDENTIFIER,
            &[property_name, &class_name],
        );
        self.error_at_node(
            name_idx,
            &message,
            diagnostic_codes::PROPERTY_IS_NOT_ACCESSIBLE_OUTSIDE_CLASS_BECAUSE_IT_HAS_A_PRIVATE_IDENTIFIER,
        );
    }

    pub(crate) fn report_private_identifier_shadowed(
        &mut self,
        name_idx: NodeIndex,
        property_name: &str,
        object_type: TypeId,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        let type_string = self
            .get_class_display_name_from_type(object_type)
            .unwrap_or_else(|| self.format_type_diagnostic(object_type));
        let message = format_message(
            diagnostic_messages::THE_PROPERTY_CANNOT_BE_ACCESSED_ON_TYPE_WITHIN_THIS_CLASS_BECAUSE_IT_IS_SHADOWED,
            &[property_name, &type_string],
        );
        self.error_at_node(
            name_idx,
            &message,
            diagnostic_codes::THE_PROPERTY_CANNOT_BE_ACCESSED_ON_TYPE_WITHIN_THIS_CLASS_BECAUSE_IT_IS_SHADOWED,
        );
    }

    /// Returns true if `sym_id` is a merged interface+value symbol.
    pub(crate) fn is_merged_interface_value_symbol(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let flags = symbol.flags;
        (flags & symbol_flags::INTERFACE) != 0 && (flags & symbol_flags::VALUE) != 0
    }
}

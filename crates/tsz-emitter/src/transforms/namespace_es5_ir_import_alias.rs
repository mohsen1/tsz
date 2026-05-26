use super::*;

impl<'a> NamespaceES5Transformer<'a> {
    pub(super) fn namespace_statement_erases_runtime(&self, member_idx: NodeIndex) -> bool {
        let Some(member_node) = self.arena.get(member_idx) else {
            return true;
        };

        match member_node.kind {
            k if k == syntax_kind_ext::EXPORT_DECLARATION => self
                .arena
                .get_export_decl(member_node)
                .is_none_or(|export_data| {
                    self.namespace_statement_erases_runtime(export_data.export_clause)
                }),
            k if k == syntax_kind_ext::INTERFACE_DECLARATION
                || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                || k == syntax_kind_ext::IMPORT_DECLARATION
                || k == syntax_kind_ext::NAMED_EXPORTS =>
            {
                true
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => self
                .arena
                .get_import_decl_at(member_idx)
                .is_none_or(|import| {
                    self.import_equals_uses_external_module_ref(member_idx)
                        || !self.import_equals_target_has_runtime_value(
                            member_idx,
                            import.module_specifier,
                        )
                }),
            _ => false,
        }
    }

    pub(super) fn import_equals_uses_external_module_ref(&self, import_idx: NodeIndex) -> bool {
        let Some(import) = self.arena.get_import_decl_at(import_idx) else {
            return false;
        };
        self.arena.get(import.module_specifier).is_some_and(|node| {
            node.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE
                || node.kind == SyntaxKind::StringLiteral as u16
        })
    }

    pub(super) fn transform_import_equals_in_namespace(
        &self,
        ns_name: &str,
        import_idx: NodeIndex,
    ) -> Option<IRNode> {
        let import = self.arena.get_import_decl_at(import_idx)?;
        if !self.import_equals_target_has_runtime_value(import_idx, import.module_specifier) {
            return None;
        }

        let alias = get_identifier_text(self.arena, import.import_clause)?;
        if !self.import_equals_alias_is_referenced_after_node(import_idx, import) {
            return None;
        }

        let target_expr = AstToIr::new(self.arena).convert_expression(import.module_specifier);
        let is_exported = self
            .arena
            .has_modifier(&import.modifiers, SyntaxKind::ExportKeyword);

        if is_exported {
            Some(IRNode::NamespaceExport {
                namespace: ns_name.to_string().into(),
                name: alias.into(),
                value: Box::new(target_expr),
            })
        } else {
            Some(IRNode::VarDecl {
                name: alias.into(),
                initializer: Some(Box::new(target_expr)),
            })
        }
    }

    pub(super) fn transform_import_equals_exported(
        &self,
        ns_name: &str,
        import_idx: NodeIndex,
    ) -> Option<IRNode> {
        let import = self.arena.get_import_decl_at(import_idx)?;
        let alias = get_identifier_text(self.arena, import.import_clause)?;

        if !self.import_equals_target_has_runtime_value(import_idx, import.module_specifier) {
            return None;
        }

        let target_expr = AstToIr::new(self.arena).convert_expression(import.module_specifier);

        Some(IRNode::NamespaceExport {
            namespace: ns_name.to_string().into(),
            name: alias.into(),
            value: Box::new(target_expr),
        })
    }

    fn import_equals_target_has_runtime_value(
        &self,
        import_idx: NodeIndex,
        target_idx: NodeIndex,
    ) -> bool {
        let Some(target_parts) = collect_qualified_name_parts(self.arena, target_idx) else {
            return true;
        };

        let namespace_parts = self.containing_namespace_parts(import_idx);
        if !namespace_parts.is_empty() {
            let mut relative_parts = namespace_parts;
            relative_parts.extend(target_parts.iter().cloned());
            if let Some(has_runtime) = entity_path_has_runtime_value(self.arena, &relative_parts) {
                return has_runtime;
            }
        }

        entity_path_has_runtime_value(self.arena, &target_parts).unwrap_or(true)
    }

    fn import_equals_alias_is_referenced_after_node(
        &self,
        import_idx: NodeIndex,
        import: &tsz_parser::parser::node::ImportDeclData,
    ) -> bool {
        let Some(alias) = get_identifier_text(self.arena, import.import_clause) else {
            return true;
        };
        let Some(source_text) = self.source_text else {
            return true;
        };
        let Some(import_node) = self.arena.get(import_idx) else {
            return true;
        };
        let full_haystack = self.source_after_import_equals(import_node, import);
        let haystack = if let Some(scope_end) = self.namespace_import_scope_end(import_idx) {
            let full_start_in_source = source_text.len().saturating_sub(full_haystack.len());
            let scope_end = scope_end as usize;
            if scope_end <= full_start_in_source {
                ""
            } else {
                let end_in_full = scope_end - full_start_in_source;
                &full_haystack[..end_in_full.min(full_haystack.len())]
            }
        } else {
            full_haystack
        };
        let stripped = crate::import_usage::strip_type_only_content(haystack);
        crate::import_usage::contains_identifier_occurrence_before_shadow(&stripped, &alias)
    }

    fn source_after_import_equals(
        &self,
        import_node: &Node,
        import: &tsz_parser::parser::node::ImportDeclData,
    ) -> &'a str {
        let Some(source_text) = self.source_text else {
            return "";
        };
        let mut start = self
            .arena
            .get(import.module_specifier)
            .map_or(import_node.end as usize, |module_node| {
                module_node.end as usize
            });
        start = start.min(source_text.len());
        let bytes = source_text.as_bytes();
        while start < bytes.len() {
            match bytes[start] {
                b'\n' => {
                    start += 1;
                    break;
                }
                b'\r' => {
                    start += 1;
                    if start < bytes.len() && bytes[start] == b'\n' {
                        start += 1;
                    }
                    break;
                }
                _ => start += 1,
            }
        }
        &source_text[start..]
    }

    fn namespace_import_scope_end(&self, import_idx: NodeIndex) -> Option<u32> {
        let block_idx = self.containing_module_block(import_idx)?;
        let block_node = self.arena.get(block_idx)?;
        let block = self.arena.get_module_block(block_node)?;
        let statements = block.statements.as_ref()?;
        let last_stmt = statements
            .nodes
            .last()
            .and_then(|last_idx| self.arena.get(*last_idx))?;
        Some(self.find_code_end_of_erased_stmt(last_stmt.pos, last_stmt.end))
    }

    fn containing_module_block(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = self.arena.parent_of(node_idx).unwrap_or(NodeIndex::NONE);
        while current != NodeIndex::NONE {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::MODULE_BLOCK {
                return Some(current);
            }
            current = self.arena.parent_of(current).unwrap_or(NodeIndex::NONE);
        }
        None
    }

    fn containing_namespace_parts(&self, node_idx: NodeIndex) -> Vec<String> {
        let mut groups = Vec::new();
        let mut current = self.arena.parent_of(node_idx).unwrap_or(NodeIndex::NONE);

        while current != NodeIndex::NONE {
            let Some(node) = self.arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module) = self.arena.get_module(node)
                && let Some(parts) = self.flatten_module_name(module.name)
            {
                groups.push(parts);
            }
            current = self.arena.parent_of(current).unwrap_or(NodeIndex::NONE);
        }

        groups.reverse();
        groups.into_iter().flatten().collect()
    }
}

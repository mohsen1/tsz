impl<'a> CodeActionProvider<'a> {
    // -------------------------------------------------------------------------
    // Import removal
    // -------------------------------------------------------------------------

    fn sorted_named_import_specifier_replacement(
        &self,
        import_idx: NodeIndex,
    ) -> Option<(u32, u32, String)> {
        let import_node = self.arena.get(import_idx)?;
        let import_decl = self.arena.get_import_decl(import_node)?;
        let clause_node = self.arena.get(import_decl.import_clause)?;
        let clause = self.arena.get_import_clause(clause_node)?;
        let named_node = self.arena.get(clause.named_bindings)?;
        if named_node.kind != syntax_kind_ext::NAMED_IMPORTS {
            return None;
        }
        let named = self.arena.get_named_imports(named_node)?;
        if named.elements.nodes.len() < 2 {
            return None;
        }

        let named_text = self
            .source
            .get(named_node.pos as usize..named_node.end as usize)?;
        let open_rel = named_text.find('{')?;
        let close_rel = named_text.rfind('}')?;
        if close_rel <= open_rel {
            return None;
        }
        let inner_start = named_node.pos + open_rel as u32 + 1;
        let inner_end = named_node.pos + close_rel as u32;

        let open_pos = self.line_map.offset_to_position(inner_start, self.source);
        let close_pos = self.line_map.offset_to_position(inner_end, self.source);
        if open_pos.line != close_pos.line {
            return None;
        }

        let mut entries = Vec::with_capacity(named.elements.nodes.len());
        for &specifier_idx in &named.elements.nodes {
            let specifier_node = self.arena.get(specifier_idx)?;
            let specifier = self.arena.get_specifier(specifier_node)?;
            let import_idx = if specifier.property_name.is_some() {
                specifier.property_name
            } else {
                specifier.name
            };
            let local_idx = if specifier.name.is_some() {
                specifier.name
            } else {
                specifier.property_name
            };
            let import_name = self.arena.get_identifier_text(import_idx)?;
            let local_name = self.arena.get_identifier_text(local_idx)?;
            let mut rendered = String::new();
            if specifier.is_type_only {
                rendered.push_str("type ");
            }
            if import_name == local_name {
                rendered.push_str(import_name);
            } else {
                rendered.push_str(&format!("{import_name} as {local_name}"));
            }
            entries.push((specifier.is_type_only, local_name.to_string(), rendered));
        }

        entries.sort_by(
            |(left_is_type_only, left, _), (right_is_type_only, right, _)| {
                let type_order = self.organize_imports_type_order.as_deref();
                let left_group = match type_order {
                    Some("last") if *left_is_type_only => 1,
                    Some("first") if !*left_is_type_only => 1,
                    _ => 0,
                };
                let right_group = match type_order {
                    Some("last") if *right_is_type_only => 1,
                    Some("first") if !*right_is_type_only => 1,
                    _ => 0,
                };
                left_group.cmp(&right_group).then_with(|| {
                    if !self.organize_imports_ignore_case {
                        return left.cmp(right);
                    }
                    let left_folded = left.to_ascii_lowercase();
                    let right_folded = right.to_ascii_lowercase();
                    left_folded.cmp(&right_folded)
                })
            },
        );
        let replacement = format!(
            " {} ",
            entries
                .iter()
                .map(|(_, _, rendered)| rendered.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        let original = self.source.get(inner_start as usize..inner_end as usize)?;
        if original == replacement {
            return None;
        }

        Some((inner_start, inner_end, replacement))
    }

    fn get_module_specifier(&self, import_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(import_idx)?;
        let import_decl = self.arena.get_import_decl(node)?;
        let spec_idx = import_decl.module_specifier;
        let text = self.arena.get_literal_text(spec_idx)?;
        Some(text.to_string())
    }

    pub(super) fn missing_import_quickfixes(
        &self,
        root: NodeIndex,
        diag: &LspDiagnostic,
        candidates: &[ImportCandidate],
    ) -> Vec<CodeAction> {
        let code = match diag.code {
            Some(code) => code,
            None => return Vec::new(),
        };
        if code != tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME
            && code != tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAMESPACE
        {
            return Vec::new();
        }

        let Some((missing_name, usage)) = self.diagnostic_identifier_usage(diag) else {
            return Vec::new();
        };

        let mut actions = Vec::new();
        for candidate in candidates {
            if candidate.local_name != missing_name {
                continue;
            }
            if usage == ImportUsage::Value && candidate.is_type_only {
                continue;
            }

            let mut resolved = candidate.clone();
            // Use `import type` when the identifier is only used in a type position
            // (type annotations, implements clauses, etc.). For value usage, use a
            // regular import so the symbol is available at runtime.
            resolved.is_type_only = usage == ImportUsage::Type;

            let Some(edits) = self.build_import_edit(root, &resolved) else {
                continue;
            };

            let mut changes = FxHashMap::default();
            changes.insert(self.file_name.clone(), edits);

            let title = format!(
                "Import '{}' from '{}'",
                candidate.local_name, candidate.module_specifier
            );
            actions.push(CodeAction {
                title,
                kind: CodeActionKind::QuickFix,
                edit: Some(WorkspaceEdit { changes }),
                is_preferred: false,
                data: Some(serde_json::json!({
                    "fixName": "import",
                    "fixId": "fixMissingImport",
                    "fixAllDescription": "Add all missing imports"
                })),
            });
        }

        actions
    }
}

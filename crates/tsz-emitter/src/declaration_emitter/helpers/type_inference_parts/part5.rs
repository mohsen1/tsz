use super::*;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn qualify_ambient_module_exported_names_in_text(
        &self,
        source_arena: &NodeArena,
        module_specifier: &str,
        text: &str,
        excluded_names: &[String],
    ) -> String {
        let Some(source_file) = self.arena_source_file(source_arena) else {
            return text.to_string();
        };

        let mut replacements = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            self.collect_ambient_module_export_replacements(
                source_arena,
                stmt_idx,
                module_specifier,
                excluded_names,
                &mut replacements,
            );
        }

        Self::replace_whole_words_in_text(text, &replacements)
    }

    pub(in crate::declaration_emitter) fn collect_ambient_module_export_replacements(
        &self,
        source_arena: &NodeArena,
        module_idx: NodeIndex,
        module_specifier: &str,
        excluded_names: &[String],
        replacements: &mut Vec<(String, String)>,
    ) {
        let Some(module_node) = source_arena.get(module_idx) else {
            return;
        };
        let Some(module) = source_arena.get_module(module_node) else {
            return;
        };

        let Some(name_node) = source_arena.get(module.name) else {
            return;
        };
        if name_node.kind != SyntaxKind::StringLiteral as u16 {
            return;
        }
        let Some(literal) = source_arena.get_literal(name_node) else {
            return;
        };
        if literal.text != module_specifier {
            return;
        }

        let Some(body_node) = source_arena.get(module.body) else {
            return;
        };
        if source_arena.get_module(body_node).is_some() {
            self.collect_ambient_module_export_replacements(
                source_arena,
                module.body,
                module_specifier,
                excluded_names,
                replacements,
            );
            return;
        }

        let Some(block) = source_arena.get_module_block(body_node) else {
            return;
        };
        let Some(statements) = block.statements.as_ref() else {
            return;
        };

        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = source_arena.get(stmt_idx) else {
                continue;
            };
            let export_name = if let Some(decl) = source_arena.get_interface(stmt_node) {
                Some(decl.name)
            } else if let Some(decl) = source_arena.get_type_alias(stmt_node) {
                Some(decl.name)
            } else if let Some(decl) = source_arena.get_class(stmt_node) {
                Some(decl.name)
            } else if let Some(decl) = source_arena.get_enum(stmt_node) {
                Some(decl.name)
            } else {
                source_arena.get_function(stmt_node).map(|decl| decl.name)
            }
            .and_then(|name_idx| self.identifier_text_from_arena(source_arena, name_idx));

            let Some(export_name) = export_name else {
                continue;
            };
            if excluded_names.iter().any(|name| name == &export_name) {
                continue;
            }
            let qualified = format!("import(\"{module_specifier}\").{export_name}");
            replacements.push((export_name, qualified));
        }
    }
}

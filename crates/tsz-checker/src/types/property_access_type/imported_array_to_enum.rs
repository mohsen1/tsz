//! Literal member recovery for imported `arrayToEnum` results.

use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn imported_array_to_enum_member_literal_type(
        &self,
        base_expr: NodeIndex,
        member_name_idx: NodeIndex,
    ) -> Option<TypeId> {
        let member_node = self.ctx.arena.get(member_name_idx)?;
        let property_name = if member_node.kind == SyntaxKind::Identifier as u16 {
            self.ctx
                .arena
                .get_identifier(member_node)
                .map(|ident| ident.escaped_text.clone())?
        } else if member_node.kind == SyntaxKind::StringLiteral as u16 {
            self.ctx
                .arena
                .get_literal(member_node)
                .map(|lit| lit.text.clone())?
        } else {
            return None;
        };

        let base_expr = self.ctx.arena.skip_parenthesized_and_assertions(base_expr);
        let base_node = self.ctx.arena.get(base_expr)?;
        if base_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let base_name = self
            .ctx
            .arena
            .get_identifier(base_node)?
            .escaped_text
            .clone();
        let base_sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, base_expr)?;
        let (mut target_sym_id, mut target_file_idx) = if let Some((sym_id, file_idx)) = self
            .value_import_target_symbol_named(base_expr, &base_name)
            .or_else(|| self.imported_value_target_symbol(base_sym_id))
        {
            (sym_id, Some(file_idx))
        } else if let Some(sym_id) = self.ctx.resolve_import_alias_and_register(base_sym_id) {
            (sym_id, self.ctx.resolve_symbol_file_index(sym_id))
        } else {
            (base_sym_id, self.ctx.resolve_symbol_file_index(base_sym_id))
        };
        let mut target_symbol = if let Some(file_idx) = target_file_idx {
            self.ctx
                .get_binder_for_file(file_idx)?
                .get_symbol(target_sym_id)?
        } else {
            self.get_cross_file_symbol(target_sym_id)?
        };
        if target_symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE == 0
            && let Some((value_sym_id, _, file_idx)) =
                self.same_file_value_symbol_for_type_symbol(target_sym_id)
        {
            target_sym_id = value_sym_id;
            target_file_idx = Some(file_idx);
            target_symbol = self
                .ctx
                .get_binder_for_file(file_idx)?
                .get_symbol(target_sym_id)?;
        }
        if target_symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE == 0 {
            return None;
        }

        let file_idx =
            target_file_idx.or_else(|| self.ctx.resolve_symbol_file_index(target_sym_id))?;
        let arena = self.ctx.get_arena_for_file(file_idx as u32);
        let mut value_decl = if target_symbol.value_declaration.is_some() {
            target_symbol.value_declaration
        } else {
            target_symbol.primary_declaration()?
        };
        let mut value_node = arena.get(value_decl)?;
        if value_node.kind == SyntaxKind::Identifier as u16 {
            value_decl = arena.get_extended(value_decl)?.parent;
            value_node = arena.get(value_decl)?;
        }
        if value_node.kind != syntax_kind_ext::VARIABLE_DECLARATION
            || !arena.is_const_variable_declaration(value_decl)
        {
            return None;
        }

        let variable = arena.get_variable_declaration(value_node)?;
        let initializer = arena.skip_parenthesized_and_assertions(variable.initializer);
        let call_node = arena.get(initializer)?;
        if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = arena.get_call_expr(call_node)?;
        let callee_name = crate::symbols_domain::name_text::expression_name_text_in_arena(
            arena,
            call.expression,
        )?;
        if callee_name != "arrayToEnum" && !callee_name.ends_with(".arrayToEnum") {
            return None;
        }

        let first_arg = call.arguments.as_ref()?.nodes.first().copied()?;
        let arg = arena.skip_parenthesized_and_assertions(first_arg);
        let arg_node = arena.get(arg)?;
        if arg_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }
        let array = arena.get_literal_expr(arg_node)?;
        for &element in &array.elements.nodes {
            let element = arena.skip_parenthesized_and_assertions(element);
            let element_node = arena.get(element)?;
            if (element_node.kind == SyntaxKind::StringLiteral as u16
                || element_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16)
                && let Some(lit) = arena.get_literal(element_node)
                && lit.text == property_name
            {
                return Some(self.ctx.types.literal_string(&lit.text));
            }
        }

        None
    }

    fn value_import_target_symbol_named(
        &self,
        idx: NodeIndex,
        name: &str,
    ) -> Option<(tsz_binder::SymbolId, usize)> {
        let mut current = idx;
        let mut guard = 0u32;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            guard += 1;
            if guard > 4096 {
                return None;
            }
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
            if let Some(node) = self.ctx.arena.get(current)
                && (node.kind == syntax_kind_ext::SOURCE_FILE
                    || node.kind == syntax_kind_ext::MODULE_BLOCK)
            {
                break;
            }
        }

        let root = self.ctx.arena.get(current)?;
        if root.kind != syntax_kind_ext::SOURCE_FILE && root.kind != syntax_kind_ext::MODULE_BLOCK {
            return None;
        }

        for stmt_idx in self.ctx.arena.get_children(current) {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }
            let Some(import_decl) = self.ctx.arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(clause_node) = self.ctx.arena.get(import_decl.import_clause) else {
                continue;
            };
            let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
                continue;
            };
            if clause.is_type_only || clause.named_bindings.is_none() {
                continue;
            }
            let Some(named_bindings_node) = self.ctx.arena.get(clause.named_bindings) else {
                continue;
            };
            if named_bindings_node.kind != syntax_kind_ext::NAMED_IMPORTS {
                continue;
            }
            let Some(named_imports) = self.ctx.arena.get_named_imports(named_bindings_node) else {
                continue;
            };
            for &specifier_idx in &named_imports.elements.nodes {
                let Some(specifier_node) = self.ctx.arena.get(specifier_idx) else {
                    continue;
                };
                let Some(specifier) = self.ctx.arena.get_specifier(specifier_node) else {
                    continue;
                };
                if specifier.is_type_only
                    || specifier.name.is_none()
                    || self.ctx.arena.get_identifier_text(specifier.name) != Some(name)
                {
                    continue;
                }
                let export_name = if specifier.property_name.is_some() {
                    self.ctx
                        .arena
                        .get_identifier_text(specifier.property_name)?
                        .to_string()
                } else {
                    name.to_string()
                };
                let module_specifier = self.import_module_specifier_text(import_decl)?;
                if self.is_export_type_only_across_binders(&module_specifier, &export_name) {
                    continue;
                }
                if let Some(target) =
                    self.exported_block_scoped_value_symbol(&module_specifier, &export_name)
                {
                    return Some(target);
                }
            }
        }

        None
    }

    fn imported_value_target_symbol(
        &self,
        alias_sym_id: tsz_binder::SymbolId,
    ) -> Option<(tsz_binder::SymbolId, usize)> {
        let alias = self.ctx.binder.get_symbol(alias_sym_id)?;
        if alias.flags & symbol_flags::ALIAS == 0 {
            return None;
        }

        let module_specifier = alias.import_module.as_ref()?;
        let import_name = alias.import_name.as_deref().unwrap_or(&alias.escaped_name);
        self.exported_block_scoped_value_symbol(module_specifier, import_name)
    }

    fn exported_block_scoped_value_symbol(
        &self,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<(tsz_binder::SymbolId, usize)> {
        let target_idx = self
            .ctx
            .resolve_import_target_from_file(self.ctx.current_file_idx, module_specifier)?;
        let target_binder = self.ctx.get_binder_for_file(target_idx)?;

        for &candidate_id in target_binder.get_symbols().find_all_by_name(export_name) {
            let Some(candidate) = target_binder.get_symbol(candidate_id) else {
                continue;
            };
            if candidate.escaped_name == export_name
                && candidate.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0
                && candidate.flags & symbol_flags::ALIAS == 0
                && candidate.import_module.is_none()
                && candidate.value_declaration.is_some()
                && candidate.is_exported
            {
                self.ctx
                    .register_symbol_file_target(candidate_id, target_idx);
                return Some((candidate_id, target_idx));
            }
        }

        None
    }

    fn import_module_specifier_text(
        &self,
        import_decl: &tsz_parser::parser::node::ImportDeclData,
    ) -> Option<String> {
        let spec_node = self.ctx.arena.get(import_decl.module_specifier)?;
        let literal = self.ctx.arena.get_literal(spec_node)?;
        Some(literal.text.clone())
    }
}

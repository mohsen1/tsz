//! Keyof type-alias display recovery for assignability diagnostics.

use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn keyof_type_alias_body_display(&mut self, ty: TypeId) -> Option<String> {
        if let Some(def_id) = self
            .ctx
            .definition_store
            .find_type_alias_by_body(ty)
            .or_else(|| {
                let def_id = self.ctx.definition_store.find_def_for_type(ty)?;
                let def = self.ctx.definition_store.get(def_id)?;
                (def.kind == tsz_solver::def::DefKind::TypeAlias).then_some(def_id)
            })
        {
            return self.keyof_type_alias_definition_display(def_id);
        }

        self.ctx
            .definition_store
            .all_type_alias_defs()
            .into_iter()
            .find_map(|def_id| {
                let def = self.ctx.definition_store.get(def_id)?;
                if !def.type_params.is_empty() {
                    return None;
                }
                let body = def.body?;
                crate::query_boundaries::common::keyof_inner_type(self.ctx.types, body)?;
                let evaluated = self.evaluate_type_for_assignability(body);
                (evaluated == ty || self.relation_boolean_guard_mutual(evaluated, ty))
                    .then_some(def_id)
            })
            .and_then(|def_id| self.keyof_type_alias_definition_display(def_id))
    }

    pub(crate) fn keyof_type_alias_definition_display(
        &mut self,
        def_id: tsz_solver::def::DefId,
    ) -> Option<String> {
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind != tsz_solver::def::DefKind::TypeAlias || !def.type_params.is_empty() {
            return None;
        }
        let body = def.body?;
        let inner = crate::query_boundaries::common::keyof_inner_type(self.ctx.types, body)?;
        if let Some(alias_name) = self.lookup_type_alias_name_for_display(inner) {
            return Some(format!("keyof {alias_name}"));
        }
        if let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, inner)
            && let Some(sym_id) = shape.symbol
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            return Some(format!("keyof {}", symbol.escaped_name));
        }
        None
    }

    pub(in crate::error_reporter) fn keyof_type_alias_annotation_display_for_expression(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        if let Some(type_node_idx) = self.declared_assignment_type_annotation_node(expr_idx)
            && let Some(display) = self.keyof_type_alias_annotation_node_display(type_node_idx)
        {
            return Some(display);
        }
        let annotation = self.declared_type_annotation_text_for_expression(expr_idx)?;
        self.keyof_type_alias_annotation_display(&annotation)
    }

    fn declared_assignment_type_annotation_node(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self
            .resolve_identifier_symbol(expr_idx)
            .or_else(|| self.ctx.binder.node_symbols.get(&expr_idx.0).copied())?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut declarations = Vec::new();
        if symbol.value_declaration.is_some() {
            declarations.push(symbol.value_declaration);
        }
        declarations.extend(symbol.declarations.iter().copied());

        declarations.into_iter().find_map(|decl_idx| {
            let decl_idx = if self
                .ctx
                .arena
                .get(decl_idx)
                .is_some_and(|node| node.kind == tsz_scanner::SyntaxKind::Identifier as u16)
            {
                self.ctx
                    .arena
                    .get_extended(decl_idx)
                    .map(|ext| ext.parent)
                    .filter(|parent| parent.is_some())
                    .unwrap_or(decl_idx)
            } else {
                decl_idx
            };
            let decl = self.ctx.arena.get(decl_idx)?;
            if let Some(param) = self.ctx.arena.get_parameter(decl)
                && param.type_annotation.is_some()
            {
                return Some(param.type_annotation);
            }
            if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl)
                && var_decl.type_annotation.is_some()
            {
                return Some(var_decl.type_annotation);
            }
            if let Some(prop_decl) = self.ctx.arena.get_property_decl(decl)
                && prop_decl.type_annotation.is_some()
            {
                return Some(prop_decl.type_annotation);
            }
            None
        })
    }

    fn keyof_type_alias_annotation_node_display(
        &mut self,
        type_node_idx: NodeIndex,
    ) -> Option<String> {
        let type_node = self.ctx.arena.get(type_node_idx)?;
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = self.ctx.arena.get_type_ref(type_node)?;
        let sym_id = match self.resolve_qualified_symbol_in_type_position(type_ref.type_name) {
            TypeSymbolResolution::Type(sym_id) | TypeSymbolResolution::ValueOnly(sym_id) => sym_id,
            TypeSymbolResolution::NotFound => return None,
        };
        let def_id = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .and_then(|file_idx| {
                self.ctx
                    .definition_store
                    .lookup_by_symbol(sym_id.0, file_idx as u32)
            })
            .or_else(|| self.ctx.definition_store.find_def_by_symbol(sym_id.0))?;
        self.keyof_type_alias_definition_display(def_id)
    }

    pub(in crate::error_reporter) fn keyof_type_alias_annotation_display(
        &mut self,
        annotation: &str,
    ) -> Option<String> {
        let name = simple_or_namespace_member_name(annotation.trim())?;
        if name != annotation.trim() {
            return None;
        }
        let name_atom = self.ctx.types.intern_string(name);
        self.ctx
            .definition_store
            .find_defs_by_name(name_atom)?
            .into_iter()
            .find_map(|def_id| {
                let def = self.ctx.definition_store.get(def_id)?;
                (def.kind == tsz_solver::def::DefKind::TypeAlias
                    && def.type_params.is_empty()
                    && def.name == name_atom)
                    .then_some(def_id)
            })
            .and_then(|def_id| self.keyof_type_alias_definition_display(def_id))
            .or_else(|| self.keyof_type_alias_textual_definition_display(name))
    }

    fn keyof_type_alias_textual_definition_display(&mut self, name: &str) -> Option<String> {
        let source = self.ctx.arena.source_files.first()?.text.as_ref();
        let pattern = format!("type {name} = keyof ");
        let start = source.rfind(&pattern)? + pattern.len();
        let rest = &source[start..];
        let end = rest
            .char_indices()
            .find_map(|(idx, ch)| {
                (idx > 0 && matches!(ch, ';' | '\n' | '\r' | ',' | ')' | '{')).then_some(idx)
            })
            .unwrap_or(rest.len());
        let operand = rest[..end].trim();
        if operand.is_empty()
            || operand.contains('|')
            || operand.contains('&')
            || operand.contains('[')
            || operand.contains('{')
            || operand.contains("=>")
        {
            return None;
        }
        Some(format!(
            "keyof {}",
            self.format_annotation_like_type(operand)
        ))
    }
}

fn simple_or_namespace_member_name(display: &str) -> Option<&str> {
    if display.starts_with("typeof ")
        || display.starts_with("import(")
        || display.contains('<')
        || display.contains('[')
        || display.contains(' ')
    {
        return None;
    }
    let name = display.rsplit_once('.').map_or(display, |(_, short)| short);
    let mut chars = name.chars();
    let first = chars.next()?;
    if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
        return None;
    }
    chars
        .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
        .then_some(name)
}

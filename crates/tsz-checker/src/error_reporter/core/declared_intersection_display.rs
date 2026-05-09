//! Declared intersection annotation display for diagnostics.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn declared_intersection_annotation_display_for_expression(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let annotation_idx =
            self.declared_current_arena_annotation_node_for_expression(expr_idx)?;
        self.format_declared_intersection_annotation_node(annotation_idx)
    }

    fn declared_current_arena_annotation_node_for_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        if let Some(parent) = self
            .ctx
            .arena
            .get_extended(expr_idx)
            .map(|extended| extended.parent)
            .filter(|parent| parent.is_some())
            && let Some(annotation) =
                self.annotation_node_from_declaration_containing_name(parent, expr_idx)
        {
            return Some(annotation);
        }

        let sym_id = self
            .resolve_identifier_symbol(expr_idx)
            .or_else(|| self.ctx.binder.node_symbols.get(&expr_idx.0).copied())?;
        let symbol = self.get_cross_file_symbol(sym_id)?;
        let owner_binder = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            .or_else(|| {
                self.ctx
                    .binder
                    .symbol_arenas
                    .get(&sym_id)
                    .and_then(|arena| self.ctx.get_binder_for_arena(arena))
            })
            .unwrap_or(self.ctx.binder);
        let fallback_arena = if symbol.decl_file_idx != u32::MAX {
            self.ctx.get_arena_for_file(symbol.decl_file_idx)
        } else {
            owner_binder
                .symbol_arenas
                .get(&sym_id)
                .map(std::convert::AsRef::as_ref)
                .unwrap_or(self.ctx.arena)
        };

        let mut declarations: Vec<(NodeIndex, &tsz_parser::NodeArena)> = Vec::new();
        let mut push_declaration = |decl_idx: NodeIndex| {
            if decl_idx.is_none() {
                return;
            }

            let mut pushed = false;
            if let Some(arenas) = owner_binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for arena in arenas {
                    let arena = arena.as_ref();
                    if arena.get(decl_idx).is_none() {
                        continue;
                    }
                    let key = (decl_idx, arena as *const tsz_parser::NodeArena);
                    if declarations.iter().all(|(existing_idx, existing_arena)| {
                        (
                            *existing_idx,
                            *existing_arena as *const tsz_parser::NodeArena,
                        ) != key
                    }) {
                        declarations.push((decl_idx, arena));
                    }
                    pushed = true;
                }
            }

            if !pushed && fallback_arena.get(decl_idx).is_some() {
                let key = (decl_idx, fallback_arena as *const tsz_parser::NodeArena);
                if declarations.iter().all(|(existing_idx, existing_arena)| {
                    (
                        *existing_idx,
                        *existing_arena as *const tsz_parser::NodeArena,
                    ) != key
                }) {
                    declarations.push((decl_idx, fallback_arena));
                }
            }
        };

        push_declaration(symbol.value_declaration);
        for &decl_idx in &symbol.declarations {
            push_declaration(decl_idx);
        }

        declarations.into_iter().find_map(|(decl_idx, decl_arena)| {
            if !std::ptr::eq(decl_arena, self.ctx.arena) {
                return None;
            }

            let decl_idx = if decl_arena
                .get(decl_idx)
                .is_some_and(|decl| decl.kind == tsz_scanner::SyntaxKind::Identifier as u16)
            {
                let parent = decl_arena
                    .get_extended(decl_idx)
                    .map(|extended| extended.parent)
                    .unwrap_or(NodeIndex::NONE);
                let parent_node = decl_arena.get(parent);
                if parent.is_some()
                    && parent_node.is_some_and(|node| {
                        decl_arena.get_variable_declaration(node).is_some()
                            || decl_arena.get_parameter(node).is_some()
                            || decl_arena.get_property_decl(node).is_some()
                    })
                {
                    parent
                } else {
                    decl_idx
                }
            } else {
                decl_idx
            };

            self.annotation_node_from_declaration(decl_idx)
        })
    }

    fn annotation_node_from_declaration_containing_name(
        &self,
        decl_idx: NodeIndex,
        name_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let decl = self.ctx.arena.get(decl_idx)?;
        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl)
            && var_decl.name == name_idx
            && var_decl.type_annotation.is_some()
        {
            return Some(var_decl.type_annotation);
        }
        if let Some(param) = self.ctx.arena.get_parameter(decl)
            && param.name == name_idx
            && param.type_annotation.is_some()
        {
            return Some(param.type_annotation);
        }
        if let Some(prop_decl) = self.ctx.arena.get_property_decl(decl)
            && prop_decl.name == name_idx
            && prop_decl.type_annotation.is_some()
        {
            return Some(prop_decl.type_annotation);
        }
        None
    }

    fn annotation_node_from_declaration(&self, decl_idx: NodeIndex) -> Option<NodeIndex> {
        let decl = self.ctx.arena.get(decl_idx)?;
        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl)
            && var_decl.type_annotation.is_some()
        {
            return Some(var_decl.type_annotation);
        }
        if let Some(param) = self.ctx.arena.get_parameter(decl)
            && param.type_annotation.is_some()
        {
            return Some(param.type_annotation);
        }
        if let Some(prop_decl) = self.ctx.arena.get_property_decl(decl)
            && prop_decl.type_annotation.is_some()
        {
            return Some(prop_decl.type_annotation);
        }
        None
    }

    fn format_declared_intersection_annotation_node(
        &mut self,
        annotation_idx: NodeIndex,
    ) -> Option<String> {
        let mut annotation_idx = annotation_idx;
        while self.ctx.arena.get(annotation_idx).is_some_and(|node| {
            node.kind == tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_TYPE
        }) {
            annotation_idx = self
                .ctx
                .arena
                .get_wrapped_type_at(annotation_idx)?
                .type_node;
        }

        let node = self.ctx.arena.get(annotation_idx)?;
        if node.kind != tsz_parser::parser::syntax_kind_ext::INTERSECTION_TYPE {
            return None;
        }

        let member_nodes = self.ctx.arena.get_composite_type(node)?.types.nodes.clone();
        if member_nodes.len() < 2 {
            return None;
        }

        let mut members = Vec::with_capacity(member_nodes.len());
        let mut saw_type_literal_member = false;
        for member_node in member_nodes {
            let member_node_kind = self.ctx.arena.get(member_node).map(|node| node.kind);
            saw_type_literal_member |=
                member_node_kind == Some(tsz_parser::parser::syntax_kind_ext::TYPE_LITERAL);
            let was_parenthesized =
                member_node_kind == Some(tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_TYPE);
            let member_type = self.get_type_from_type_node(member_node);
            let display = self.format_type_for_assignability_message(member_type);
            if member_node_kind == Some(tsz_parser::parser::syntax_kind_ext::TYPE_LITERAL)
                && !display.trim_start().starts_with('{')
            {
                return None;
            }
            if was_parenthesized {
                members.push(format!("({display})"));
            } else {
                members.push(display);
            }
        }
        if !saw_type_literal_member {
            return None;
        }
        Some(members.join(" & "))
    }
}

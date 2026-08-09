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
        let member_nodes = self.declared_intersection_annotation_member_nodes(annotation_idx)?;

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

    /// The member type nodes of `annotation_idx` when it is (after peeling
    /// wrapping parentheses) a written `IntersectionType` with at least two
    /// members, shared by [`Self::format_declared_intersection_annotation_node`]
    /// and [`Self::declared_intersection_member_types_for_expression`] so the
    /// peel-and-validate walk is not re-spelled between a display-oriented
    /// caller and a type-oriented one.
    fn declared_intersection_annotation_member_nodes(
        &self,
        annotation_idx: NodeIndex,
    ) -> Option<Vec<NodeIndex>> {
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

        let members = self.ctx.arena.get_composite_type(node)?.types.nodes.clone();
        (members.len() >= 2).then_some(members)
    }

    /// The written intersection that reduced a property-access receiver to
    /// `never`, recovered from the receiver's *declared* annotation and
    /// followed through any type-alias references.
    ///
    /// Returns the evaluated member types (for conflict detection) paired with
    /// the display `tsc` uses in the `{0}` slot of the `TS18031`/`TS18032`
    /// elaboration:
    ///
    /// - `None` display — the annotation is a *directly written* intersection
    ///   (`declare const c: A & B`); the caller renders the members
    ///   structurally (`A & B`), byte-for-byte as before this alias support.
    /// - `Some(name)` — the intersection is reached through one or more type
    ///   aliases (`type C = A & B; declare const c: C`); `tsc` names the alias
    ///   whose body *is* the intersection (`C`, `Pair<"y">`), not the members
    ///   and not an outer alias that merely forwards to it (`type D = C` still
    ///   displays `C`), so the walk keeps the innermost naming alias.
    ///
    /// Unlike [`Self::declared_intersection_annotation_display_for_expression`],
    /// this carries no type-literal display gate: a conflict-detection query
    /// over the members is just as valid when every member is a plain
    /// interface/type-alias reference as when one is a type literal. Returns
    /// `None` for every case the caller must silently accept (no explicit
    /// annotation, not an intersection, fewer than two members, a cross-arena
    /// or multi-declaration alias) rather than guessing — the primary
    /// diagnostic is unaffected either way, so under-covering never produces a
    /// wrong message.
    pub(in crate::error_reporter) fn declared_never_intersection_for_expression(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<(Vec<tsz_solver::TypeId>, Option<String>)> {
        let annotation_idx =
            self.declared_current_arena_annotation_node_for_expression(expr_idx)?;
        self.recover_never_intersection_from_annotation(annotation_idx)
    }

    /// Walk the receiver annotation to the written intersection, hopping through
    /// single-declaration type-alias references (`C` -> `A & B`, `D` -> `C` ->
    /// `A & B`, `Pair<"y">` -> `{ kind: T } & { kind: "x" }`). Bounded by the
    /// per-alias `visited` cycle guard and a hard hop cap so a pathological
    /// alias chain cannot spin.
    fn recover_never_intersection_from_annotation(
        &mut self,
        annotation_idx: NodeIndex,
    ) -> Option<(Vec<tsz_solver::TypeId>, Option<String>)> {
        /// Alias hops to follow before giving up. Deep enough for any realistic
        /// forwarding chain; the `visited` set already stops true cycles.
        const MAX_ALIAS_HOPS: usize = 16;

        let mut visited: Vec<tsz_binder::SymbolId> = Vec::new();
        let mut current = annotation_idx;
        // `None` until the first alias hop; each hop overwrites it, so the last
        // hop before the intersection — the alias that directly wraps it — wins.
        let mut alias_display: Option<String> = None;

        for _ in 0..MAX_ALIAS_HOPS {
            current = self.skip_parenthesized_type_node(current);
            let node = self.ctx.arena.get(current)?;
            let kind = node.kind;

            if kind == tsz_parser::parser::syntax_kind_ext::INTERSECTION_TYPE {
                let member_nodes = self.declared_intersection_annotation_member_nodes(current)?;
                let members = member_nodes
                    .iter()
                    .map(|&member_node| self.get_type_from_type_node(member_node))
                    .collect();
                return Some((members, alias_display));
            }

            if kind == tsz_parser::parser::syntax_kind_ext::TYPE_REFERENCE {
                let type_name = self.ctx.arena.get_type_ref(node)?.type_name;
                let alias_body = self.type_reference_alias_body(type_name, &mut visited)?;
                alias_display = self.format_alias_reference_display(current);
                current = alias_body;
                continue;
            }

            return None;
        }
        None
    }

    /// Peel wrapping `ParenthesizedType` nodes (`(A & B)`, `((C))`) so the walk
    /// sees the intersection or type reference underneath.
    fn skip_parenthesized_type_node(&self, mut idx: NodeIndex) -> NodeIndex {
        while self.ctx.arena.get(idx).is_some_and(|node| {
            node.kind == tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_TYPE
        }) {
            match self.ctx.arena.get_wrapped_type_at(idx) {
                Some(wrapped) => idx = wrapped.type_node,
                None => break,
            }
        }
        idx
    }

    /// `tsc`'s `{0}` display for an alias reference that names a never-reduced
    /// intersection: the alias name, plus its written type arguments rendered
    /// through the type printer so literals normalize the same way the rest of
    /// the message does (`Pair<'y'>` -> `Pair<"y">`).
    fn format_alias_reference_display(&mut self, reference_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(reference_idx)?;
        let type_ref = self.ctx.arena.get_type_ref(node)?;
        let name = self.entity_name_text(type_ref.type_name)?;
        let Some(type_arguments) = type_ref.type_arguments.clone() else {
            return Some(name);
        };
        if type_arguments.nodes.is_empty() {
            return Some(name);
        }
        let rendered = type_arguments
            .nodes
            .iter()
            .map(|&arg_idx| {
                let arg_type = self.get_type_from_type_node(arg_idx);
                self.format_type(arg_type)
            })
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!("{name}<{rendered}>"))
    }
}

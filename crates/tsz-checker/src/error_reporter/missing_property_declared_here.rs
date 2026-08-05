use crate::diagnostics::{
    Diagnostic, DiagnosticRelatedInformation, diagnostic_codes, diagnostic_messages, format_message,
};
use crate::state::CheckerState;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Build tsc's `'{0}' is declared here.` (TS2728) pointer for the single
    /// unmatched property of a `TS2741` failure.
    ///
    /// tsc's `reportUnmatchedProperty` attaches this related entry only on the
    /// one-missing-property form: the multi-property forms (`TS2739`/`TS2740`)
    /// carry no pointer at all. The anchor is the *name node* of the property's
    /// own declaration, in whichever file declares it — so a property declared
    /// in an imported module points across files.
    ///
    /// `owner_candidates` are the target types the caller already has in hand
    /// (the relation target and the evaluated member target); the first one
    /// that resolves to a declaring symbol owning a member with this name wins.
    /// When no candidate resolves — an anonymous object type with no symbol,
    /// for instance — no pointer is produced, which leaves output exactly as it
    /// was rather than guessing at a declaration.
    pub(super) fn missing_property_declared_here_related(
        &mut self,
        owner_candidates: &[TypeId],
        property_name: tsz_common::interner::Atom,
        property_display: &str,
    ) -> Option<DiagnosticRelatedInformation> {
        let property = self.ctx.types.resolve_atom(property_name);
        owner_candidates
            .iter()
            .find_map(|&owner| {
                self.declared_here_related_for_owner(owner, &property, property_display)
            })
            .or_else(|| {
                owner_candidates.iter().find_map(|&owner| {
                    self.declared_here_related_for_anonymous_property(
                        owner,
                        &property,
                        property_display,
                    )
                })
            })
    }

    /// Fallback for an owner with no declaring symbol at all — an anonymous
    /// object type such as a type literal, reached directly or through a
    /// type alias / indexed access that resolves to one.
    ///
    /// `declared_here_related_for_owner` above declines immediately for these:
    /// `resolve_type_to_symbol_id` and `type_shape_symbol` both bottom out in
    /// `ObjectShape.symbol`, and a type literal never gets one (tsz's binder
    /// mints no symbol for `TYPE_LITERAL` nodes). The property itself still
    /// carries its own declaration span when it came from
    /// `get_type_from_type_literal`, so this reads that directly instead of
    /// walking a symbol's member table.
    ///
    /// Known gap (#16443 follow-up): `declared_location` is deliberately
    /// identity-exempt in `PropertyInfo`'s `Eq`/`Hash` (it is diagnostic-only,
    /// not structural), so the type interner's structural hash-consing can
    /// return an earlier-cached copy of an equal-shaped object that carries no
    /// location — e.g. an interface member whose own type is a type literal,
    /// reached again through an indexed access. Direct type-alias, inline, and
    /// parameter-position type literals are unaffected; this only declines
    /// (never anchors wrong) when it happens.
    fn declared_here_related_for_anonymous_property(
        &self,
        owner: TypeId,
        property: &str,
        property_display: &str,
    ) -> Option<DiagnosticRelatedInformation> {
        let info = crate::query_boundaries::common::find_property_by_str(
            self.ctx.types.as_type_database(),
            owner,
            property,
        )?;
        let (start, length, file) =
            self.declared_here_member_anchor(info.declared_location, property, None)?;
        Some(Diagnostic::related_pointer(
            diagnostic_codes::IS_DECLARED_HERE,
            file.unwrap_or_else(|| self.ctx.file_name.clone()),
            start,
            length,
            format_message(diagnostic_messages::IS_DECLARED_HERE, &[property_display]),
        ))
    }

    /// Resolve `owner` to its declaring symbol, find the member declaration
    /// with this name inside that symbol's own declaration, and anchor there.
    ///
    /// The symbol is read out of the binder that *declares* it, not the
    /// checking file's own binder. Per-file binders hand out raw `SymbolId`s
    /// from `0`, so an imported target's id names an unrelated local symbol —
    /// or nothing at all — in `self.ctx.binder`, and the walk below then finds
    /// no member list to anchor in. This is why a target declared in another
    /// file produced no pointer at all.
    fn declared_here_related_for_owner(
        &mut self,
        owner: TypeId,
        property: &str,
        property_display: &str,
    ) -> Option<DiagnosticRelatedInformation> {
        let owner_symbol = self.ctx.resolve_type_to_symbol_id(owner).or_else(|| {
            crate::query_boundaries::common::type_shape_symbol(self.ctx.types, owner)
        })?;
        let declaring_file_idx = self.ctx.resolve_symbol_file_index(owner_symbol);
        let binder = declaring_file_idx
            .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            .filter(|binder| binder.get_symbol(owner_symbol).is_some())
            .unwrap_or(self.ctx.binder);
        let symbol = binder.get_symbol(owner_symbol)?;
        let locations: Vec<tsz_binder::StableLocation> =
            std::iter::once(symbol.stable_value_declaration)
                .chain(symbol.stable_declarations.iter().copied())
                .filter(tsz_binder::StableLocation::is_known)
                .collect();
        let member_locations: Vec<tsz_binder::StableLocation> = symbol
            .members
            .as_ref()
            .and_then(|members| members.get(property))
            .and_then(|member_id| binder.get_symbol(member_id))
            .map(|member| {
                std::iter::once(member.stable_value_declaration)
                    .chain(member.stable_declarations.iter().copied())
                    .filter(tsz_binder::StableLocation::is_known)
                    .collect()
            })
            .unwrap_or_default();
        for location in locations {
            let Some((start, length, file)) =
                self.declared_here_anchor(location, property, declaring_file_idx)
            else {
                continue;
            };
            return Some(Diagnostic::related_pointer(
                diagnostic_codes::IS_DECLARED_HERE,
                file.unwrap_or_else(|| self.ctx.file_name.clone()),
                start,
                length,
                format_message(diagnostic_messages::IS_DECLARED_HERE, &[property_display]),
            ));
        }
        for location in member_locations {
            let Some((start, length, file)) =
                self.declared_here_member_anchor(location, property, declaring_file_idx)
            else {
                continue;
            };
            return Some(Diagnostic::related_pointer(
                diagnostic_codes::IS_DECLARED_HERE,
                file.unwrap_or_else(|| self.ctx.file_name.clone()),
                start,
                length,
                format_message(diagnostic_messages::IS_DECLARED_HERE, &[property_display]),
            ));
        }
        None
    }

    /// `(start, length, file)` for a member symbol's own declaration, used when
    /// the owner's declaration does not carry a member list the walk above can
    /// read.
    ///
    /// A `StableLocation` resolves by `(pos, end)` against whichever arena the
    /// stamped file index names, falling back to `declaring_file_idx` — the
    /// owning symbol's own file — when the location carries no file index. The
    /// node it lands on is still only trusted here when it really is a member
    /// declaration whose written name is the property being reported. Anything
    /// else declines rather than anchoring a pointer at an unrelated span.
    fn declared_here_member_anchor(
        &self,
        location: tsz_binder::StableLocation,
        property: &str,
        declaring_file_idx: Option<usize>,
    ) -> Option<(u32, u32, Option<String>)> {
        let (decl_idx, arena) = self
            .ctx
            .node_at_stable_location_in_file(location, declaring_file_idx)?;
        let name_idx = Self::member_name_node(arena, decl_idx)?;
        if crate::types_domain::queries::core::get_literal_property_name(arena, name_idx)
            .is_none_or(|name| name != property)
        {
            return None;
        }
        let anchor_idx = Self::member_anchor_for_kind(arena, decl_idx, name_idx);
        let (start, length) = Self::anchor_span(arena, anchor_idx)?;
        Some((start, length, Self::arena_file_name(arena, anchor_idx)))
    }

    /// `(start, length, file)` of the pointer anchor for `property` inside the
    /// declaration at `location`, in that declaration's own arena.
    fn declared_here_anchor(
        &self,
        location: tsz_binder::StableLocation,
        property: &str,
        declaring_file_idx: Option<usize>,
    ) -> Option<(u32, u32, Option<String>)> {
        let (decl_idx, arena) = self
            .ctx
            .node_at_stable_location_in_file(location, declaring_file_idx)?;
        let members = Self::declaration_member_list(arena, decl_idx)?;
        let anchor_idx = Self::member_anchor_node(arena, &members, property)?;
        let (start, length) = Self::anchor_span(arena, anchor_idx)?;
        Some((start, length, Self::arena_file_name(arena, anchor_idx)))
    }

    /// The member list a type's declaration owns: interfaces and classes carry
    /// theirs directly, a type alias carries its body's when that body is a
    /// type literal.
    fn declaration_member_list(
        arena: &NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<tsz_parser::parser::NodeList> {
        use tsz_parser::parser::syntax_kind_ext;

        let node = arena.get(decl_idx)?;
        if node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
            return Some(arena.get_interface(node)?.members.clone());
        }
        if node.kind == syntax_kind_ext::CLASS_DECLARATION
            || node.kind == syntax_kind_ext::CLASS_EXPRESSION
        {
            return Some(arena.get_class(node)?.members.clone());
        }
        if node.kind == syntax_kind_ext::TYPE_LITERAL {
            return Some(arena.get_type_literal(node)?.members.clone());
        }
        if node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
            let body_idx = arena.get_type_alias(node)?.type_node;
            if body_idx.is_some() && body_idx != decl_idx {
                return Self::declaration_member_list(arena, body_idx);
            }
        }
        None
    }

    /// The member in `members` named `property`, resolved to the node tsc
    /// underlines for it.
    fn member_anchor_node(
        arena: &NodeArena,
        members: &tsz_parser::parser::NodeList,
        property: &str,
    ) -> Option<NodeIndex> {
        for &member_idx in &members.nodes {
            let Some(name_idx) = Self::member_name_node(arena, member_idx) else {
                continue;
            };
            let Some(name) =
                crate::types_domain::queries::core::get_literal_property_name(arena, name_idx)
            else {
                continue;
            };
            if name != property {
                continue;
            }
            return Some(Self::member_anchor_for_kind(arena, member_idx, name_idx));
        }
        None
    }

    /// The name node of a property-like member declaration.
    ///
    /// An interface/type-literal member (`PROPERTY_SIGNATURE`/
    /// `METHOD_SIGNATURE`) stores its name on `SignatureData`; a class member
    /// (`PROPERTY_DECLARATION`/`METHOD_DECLARATION`) stores it on the
    /// distinct `PropertyDeclData`/`MethodDeclData` instead, so each kind
    /// needs its own accessor rather than one shared `get_signature` call.
    fn member_name_node(arena: &NodeArena, member_idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        let node = arena.get(member_idx)?;
        let name = if node.kind == syntax_kind_ext::PROPERTY_SIGNATURE
            || node.kind == syntax_kind_ext::METHOD_SIGNATURE
        {
            arena.get_signature(node)?.name
        } else if node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
            arena.get_property_decl(node)?.name
        } else if node.kind == syntax_kind_ext::METHOD_DECLARATION {
            arena.get_method_decl(node)?.name
        } else {
            return None;
        };
        name.is_some().then_some(name)
    }

    /// Span of an anchor node, narrowed to the token as written.
    ///
    /// A node's `end` runs to the start of the next token, so an identifier
    /// name node measured as `end - pos` swallows the `:` that follows it. The
    /// same narrowing `normalized_anchor_span` performs for the current arena
    /// is done here against the declaration's own arena: an identifier is its
    /// escaped text, a string-literal name is its text plus the two quotes it
    /// was written with, and anything else keeps the node span.
    fn anchor_span(arena: &NodeArena, anchor_idx: NodeIndex) -> Option<(u32, u32)> {
        use tsz_scanner::SyntaxKind;

        let node = arena.get(anchor_idx)?;
        let start = node.pos;
        if (node.kind == SyntaxKind::Identifier as u16
            || node.kind == SyntaxKind::PrivateIdentifier as u16)
            && let Some(identifier) = arena.get_identifier(node)
        {
            return Some((start, identifier.escaped_text.len() as u32));
        }
        if node.kind == SyntaxKind::StringLiteral as u16
            && let Some(name) =
                crate::types_domain::queries::core::get_literal_property_name(arena, anchor_idx)
        {
            return Some((start, name.len() as u32 + 2));
        }
        Some((start, node.end.saturating_sub(start)))
    }

    /// The node tsc underlines for a member: a property points at its *name*
    /// (`y` in `y: number;`), an interface/type-literal method signature at
    /// the whole member (`run(): void;`) — but a *class* method declaration
    /// points at its name only (`run` in `run(): void {}`), pinned against
    /// `typescript@7.0.2`.
    fn member_anchor_for_kind(
        arena: &NodeArena,
        member_idx: NodeIndex,
        name_idx: NodeIndex,
    ) -> NodeIndex {
        use tsz_parser::parser::syntax_kind_ext;

        match arena.get(member_idx).map(|node| node.kind) {
            Some(kind) if kind == syntax_kind_ext::METHOD_SIGNATURE => member_idx,
            _ => name_idx,
        }
    }

    /// File name owning `idx` in `arena`, walked from the node itself so a
    /// cross-file declaration reports its own file rather than the file the
    /// primary diagnostic lives in.
    fn arena_file_name(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
        let mut current = idx;
        while current.is_some() {
            let node = arena.get(current)?;
            if let Some(source_file) = arena.get_source_file(node) {
                return Some(source_file.file_name.clone());
            }
            let ext = arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }
        None
    }
}

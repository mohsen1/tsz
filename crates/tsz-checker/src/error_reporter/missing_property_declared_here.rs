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
    ///
    /// A target that is an *anonymous* object type resolves to no symbol at all,
    /// and cannot be made to: `ObjectShape.symbol` participates in the
    /// interner's `Hash`/`PartialEq` (nominal discrimination for classes), so
    /// giving a type literal its own symbol would stop `{ a: string }` written
    /// twice from being one `TypeId`. Nor can the declaration be carried on the
    /// interned shape itself — excluded from identity it would be shared by
    /// every structurally identical literal in the program, and the second
    /// occurrence would anchor its pointer into the first one's file.
    ///
    /// So an anonymous target's declaration is recovered from the *annotation
    /// written at the failure site* instead, which is per-occurrence by
    /// construction. `anchor_idx` is the node the primary diagnostic is
    /// reported at; [`Self::target_annotation_node`] walks to the annotation of
    /// the binding being assigned, stopping at function and statement
    /// boundaries so an unrelated outer annotation is never attributed here.
    /// When neither route resolves, no pointer is produced, which leaves output
    /// exactly as it was rather than guessing at a declaration.
    pub(super) fn missing_property_declared_here_related(
        &mut self,
        owner_candidates: &[TypeId],
        anchor_idx: NodeIndex,
        property_name: tsz_common::interner::Atom,
        property_display: &str,
    ) -> Option<DiagnosticRelatedInformation> {
        let property = self.ctx.types.resolve_atom(property_name);
        let from_symbol = owner_candidates.iter().find_map(|&owner| {
            self.declared_here_related_for_owner(owner, &property, property_display)
        });
        if from_symbol.is_some() {
            return from_symbol;
        }
        self.declared_here_related_from_annotation(anchor_idx, &property, property_display)
    }

    /// Anchor recovered from the type annotation the failing assignment was
    /// declared with, for targets that resolve to no binder symbol.
    ///
    /// This is a purely syntactic walk over annotation *nodes*, which is the
    /// only sound route for an anonymous object type: the type is interned
    /// structurally and therefore shared across every occurrence of the same
    /// shape, while the annotation node is the one this assignment was actually
    /// written against. The same reasoning already backs
    /// `target_annotation_denotes_intersection`.
    ///
    /// The property name is still validated against the written member list by
    /// [`Self::member_anchor_node`] before any anchor is produced, so an
    /// annotation that does not declare this property declines rather than
    /// pointing somewhere plausible-but-wrong.
    fn declared_here_related_from_annotation(
        &mut self,
        anchor_idx: NodeIndex,
        property: &str,
        property_display: &str,
    ) -> Option<DiagnosticRelatedInformation> {
        let annotation_idx = self.target_annotation_node(anchor_idx)?;
        let (start, length, file) = self.annotation_property_anchor(annotation_idx, property, 0)?;
        Some(Diagnostic::related_pointer(
            diagnostic_codes::IS_DECLARED_HERE,
            file.unwrap_or_else(|| self.ctx.file_name.clone()),
            start,
            length,
            format_message(diagnostic_messages::IS_DECLARED_HERE, &[property_display]),
        ))
    }

    /// `(start, length, file)` for `property`'s declaration inside the type
    /// annotation node `idx`, following the annotation's own syntax.
    ///
    /// Parentheses are peeled, a type literal is anchored directly, a type
    /// reference hands off to the symbol route (which owns cross-file arena
    /// resolution), and an indexed access resolves its key against the object
    /// type's written members and continues into that member's type node.
    fn annotation_property_anchor(
        &mut self,
        idx: NodeIndex,
        property: &str,
        depth: u32,
    ) -> Option<(u32, u32, Option<String>)> {
        use tsz_parser::parser::syntax_kind_ext;

        if depth > Self::ANNOTATION_WALK_MAX_DEPTH {
            return None;
        }
        let node = self.ctx.arena.get(idx)?;
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
            let inner = self.ctx.arena.get_wrapped_type(node)?.type_node;
            return self.annotation_property_anchor(inner, property, depth + 1);
        }
        if node.kind == syntax_kind_ext::TYPE_LITERAL {
            let arena = self.ctx.arena;
            let members = Self::declaration_member_list(arena, idx)?;
            let anchor_idx = Self::member_anchor_node(arena, &members, property)?;
            let (start, length) = Self::anchor_span(arena, anchor_idx)?;
            return Some((start, length, Self::arena_file_name(arena, anchor_idx)));
        }
        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            let name_idx = self.ctx.arena.get_type_ref(node)?.type_name;
            let owner_symbol = self.type_position_symbol(name_idx)?;
            let (locations, declaring_file_idx) = self.owner_declaration_locations(owner_symbol)?;
            let from_declaration = locations.into_iter().find_map(|location| {
                self.declared_here_anchor(location, property, declaring_file_idx)
            });
            if from_declaration.is_some() {
                return from_declaration;
            }
            // An alias chain (`type Indirect = Zed`) declares no member list of
            // its own, so the walk above finds nothing to anchor in. Continue
            // through the alias body, which is itself annotation syntax.
            let body_idx = self.local_type_alias_body(owner_symbol)?;
            return self.annotation_property_anchor(body_idx, property, depth + 1);
        }
        if node.kind == syntax_kind_ext::INDEXED_ACCESS_TYPE {
            let data = self.ctx.arena.get_indexed_access_type(node)?;
            let (object_idx, index_idx) = (data.object_type, data.index_type);
            let key = self.written_index_key(index_idx)?;
            let member_type_idx = self.annotation_member_type_node(object_idx, &key, depth)?;
            return self.annotation_property_anchor(member_type_idx, property, depth + 1);
        }
        None
    }

    /// The written type node of the member named `key` on the type annotation
    /// `idx` denotes, when that member is declared in the checking file.
    ///
    /// An indexed access whose object type is declared in another file declines
    /// here rather than reading a foreign arena's `NodeIndex` against the
    /// current one — per-file arenas make a raw index from another file name an
    /// unrelated node.
    fn annotation_member_type_node(
        &mut self,
        idx: NodeIndex,
        key: &str,
        depth: u32,
    ) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        if depth > Self::ANNOTATION_WALK_MAX_DEPTH {
            return None;
        }
        let node = self.ctx.arena.get(idx)?;
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
            let inner = self.ctx.arena.get_wrapped_type(node)?.type_node;
            return self.annotation_member_type_node(inner, key, depth + 1);
        }
        let declaration_idx = if node.kind == syntax_kind_ext::TYPE_LITERAL {
            idx
        } else if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            let name_idx = self.ctx.arena.get_type_ref(node)?.type_name;
            let owner_symbol = self.type_position_symbol(name_idx)?;
            self.local_declaration_node(owner_symbol)?
        } else {
            return None;
        };
        let members = Self::declaration_member_list(self.ctx.arena, declaration_idx)?;
        for &member_idx in &members.nodes {
            let Some(name_idx) = Self::member_name_node(self.ctx.arena, member_idx) else {
                continue;
            };
            let Some(name) = crate::types_domain::queries::core::get_literal_property_name(
                self.ctx.arena,
                name_idx,
            ) else {
                continue;
            };
            if name != key {
                continue;
            }
            let member_node = self.ctx.arena.get(member_idx)?;
            let type_node = self.ctx.arena.get_signature(member_node)?.type_annotation;
            return type_node.is_some().then_some(type_node);
        }
        None
    }

    /// The declaration node of `owner_symbol` when it is written in the file
    /// being checked, for the annotation walk's arena-local steps.
    ///
    /// A symbol declared in another file declines here rather than handing a
    /// foreign arena's `NodeIndex` to the current arena — per-file arenas hand
    /// out indices from `0`, so a raw index from elsewhere names an unrelated
    /// node. The cross-file case is served by the symbol route, which resolves
    /// its own arena from the location.
    fn local_declaration_node(&self, owner_symbol: tsz_binder::SymbolId) -> Option<NodeIndex> {
        let symbol = self.ctx.binder.get_symbol(owner_symbol)?;
        let location = std::iter::once(symbol.stable_value_declaration)
            .chain(symbol.stable_declarations.iter().copied())
            .find(tsz_binder::StableLocation::is_known)?;
        let (decl_idx, arena) = self.ctx.node_at_stable_location(location)?;
        std::ptr::eq(arena, self.ctx.arena).then_some(decl_idx)
    }

    /// The body type node of a type alias declared in the file being checked,
    /// used to continue the annotation walk through an alias chain.
    fn local_type_alias_body(&self, owner_symbol: tsz_binder::SymbolId) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        let decl_idx = self.local_declaration_node(owner_symbol)?;
        let node = self.ctx.arena.get(decl_idx)?;
        if node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
            return None;
        }
        let body_idx = self.ctx.arena.get_type_alias(node)?.type_node;
        (body_idx.is_some() && body_idx != decl_idx).then_some(body_idx)
    }

    /// The literal key an indexed-access annotation was written with
    /// (`T["inner"]`), or `None` for a computed or non-literal index.
    fn written_index_key(&self, index_idx: NodeIndex) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;

        let node = self.ctx.arena.get(index_idx)?;
        if node.kind != syntax_kind_ext::LITERAL_TYPE {
            return None;
        }
        let literal_idx = self.ctx.arena.get_literal_type(node)?.literal;
        crate::types_domain::queries::core::get_literal_property_name(self.ctx.arena, literal_idx)
    }

    /// The symbol a type-position name node denotes, ignoring value-only
    /// resolutions (an `import =` alias used as a bare type, for instance).
    fn type_position_symbol(&self, name_idx: NodeIndex) -> Option<tsz_binder::SymbolId> {
        match self.resolve_identifier_symbol_in_type_position_without_tracking(name_idx) {
            crate::symbol_resolver::TypeSymbolResolution::Type(symbol_id) => Some(symbol_id),
            _ => None,
        }
    }

    /// Bound on the annotation walk, which follows user-written type syntax and
    /// must terminate on a pathological nesting rather than recurse forever.
    const ANNOTATION_WALK_MAX_DEPTH: u32 = 8;

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
        let (locations, _) = self.owner_declaration_locations(owner_symbol)?;
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

    /// The declaration locations of `owner_symbol` paired with the file index
    /// they resolve against.
    ///
    /// The symbol is read out of the binder that *declares* it, not the
    /// checking file's own binder, for the reason given on
    /// [`Self::declared_here_related_for_owner`].
    fn owner_declaration_locations(
        &self,
        owner_symbol: tsz_binder::SymbolId,
    ) -> Option<(Vec<tsz_binder::StableLocation>, Option<usize>)> {
        let declaring_file_idx = self.ctx.resolve_symbol_file_index(owner_symbol);
        let binder = declaring_file_idx
            .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            .filter(|binder| binder.get_symbol(owner_symbol).is_some())
            .unwrap_or(self.ctx.binder);
        let symbol = binder.get_symbol(owner_symbol)?;
        let locations = std::iter::once(symbol.stable_value_declaration)
            .chain(symbol.stable_declarations.iter().copied())
            .filter(tsz_binder::StableLocation::is_known)
            .collect();
        Some((locations, declaring_file_idx))
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

use crate::diagnostics::{
    Diagnostic, DiagnosticRelatedInformation, diagnostic_codes, diagnostic_messages, format_message,
};
use crate::state::CheckerState;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_solver::TypeId;

/// Alias edges followed from the reported target before giving up.
///
/// One hop covers `import { X } from "./hub"` reaching a hub's own
/// `export { X } from "./dep"`, which is the shape tsc's `resolveAlias`
/// terminates on. A small bound above that leaves room for an alias whose
/// target is itself locally re-aliased, while keeping a cyclic re-export graph
/// from spinning; the equal-hop check below catches a self-edge immediately.
const MAX_ALIAS_HOPS: usize = 4;

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
        owner_candidates.iter().find_map(|&owner| {
            self.declared_here_related_for_owner(owner, &property, property_display)
        })
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
        let (start, length, file) = self.member_declaration_anchor_for_owner(owner, property)?;
        Some(Diagnostic::related_pointer(
            diagnostic_codes::IS_DECLARED_HERE,
            file.unwrap_or_else(|| self.ctx.file_name.clone()),
            start,
            length,
            format_message(diagnostic_messages::IS_DECLARED_HERE, &[property_display]),
        ))
    }

    /// `(start, length, file)` of the anchor tsc underlines for `property`'s own
    /// declaration on `owner`, or `None` when `owner` does not resolve to a
    /// declaration carrying that member.
    ///
    /// Shared by every pointer that names a member declaration — TS2728's
    /// `'x' is declared here.` and TS6500's `The expected type comes from
    /// property 'x' …` anchor identically, so the walk lives here once rather
    /// than being re-derived per diagnostic. Only the code, message, and
    /// name rendering differ between them, and those belong to the caller.
    pub(super) fn member_declaration_anchor_for_owner(
        &mut self,
        owner: TypeId,
        property: &str,
    ) -> Option<(u32, u32, Option<String>)> {
        let owner_symbol = self.ctx.resolve_type_to_symbol_id(owner).or_else(|| {
            crate::query_boundaries::common::type_shape_symbol(self.ctx.types, owner)
        })?;
        let mut symbol_id = owner_symbol;
        let mut declaring_file_idx = self.ctx.resolve_symbol_file_index(symbol_id);
        for _ in 0..MAX_ALIAS_HOPS {
            if let Some(anchor) = self.member_declaration_anchor_for_declared_symbol(
                symbol_id,
                declaring_file_idx,
                property,
            ) {
                return Some(anchor);
            }
            let (next_symbol, next_file_idx) =
                self.alias_target_symbol(symbol_id, declaring_file_idx)?;
            if next_symbol == symbol_id && declaring_file_idx == Some(next_file_idx) {
                return None;
            }
            symbol_id = next_symbol;
            declaring_file_idx = Some(next_file_idx);
        }
        None
    }

    /// The `(start, length, file)` anchor for a symbol that is expected to own
    /// the member list directly, with no alias following.
    ///
    /// Returns the raw anchor rather than a built diagnostic because both
    /// `TS2728` and `TS6500` underline the same span and differ only in code,
    /// message, and name rendering — those belong to the caller.
    fn member_declaration_anchor_for_declared_symbol(
        &mut self,
        owner_symbol: tsz_binder::SymbolId,
        declaring_file_idx: Option<usize>,
        property: &str,
    ) -> Option<(u32, u32, Option<String>)> {
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
            return Some((start, length, file));
        }
        for location in member_locations {
            let Some((start, length, file)) =
                self.declared_here_member_anchor(location, property, declaring_file_idx)
            else {
                continue;
            };
            return Some((start, length, file));
        }
        None
    }

    /// Follow one alias edge from `symbol_id` to the symbol the alias names in
    /// the module it imports from, returning it with the file index that
    /// declares it.
    ///
    /// A target reached through `export { X } from "./dep"` resolves to the
    /// *hub's own export specifier*, which owns no member list, so the walk
    /// above declines. tsc's `resolveAlias` follows the alias to the original
    /// declaration; the pointer must anchor there and never on the re-export
    /// clause.
    ///
    /// The hop is deliberately made through the module graph — module specifier
    /// resolved from the *alias's own* file, then the exported **name** looked
    /// up in the target module's public surface — and not by following a raw
    /// `SymbolId`. Per-file binders mint colliding ids from `0`, so both the
    /// hub's specifier and the original declaration are `SymbolId(0)` in their
    /// own binders and a symbol-level alias resolution is a measured no-op
    /// (#16415). `resolve_export_in_target_file` is program-aware and already
    /// walks named and wildcard re-export edges, so a chain of hubs terminates
    /// in one hop.
    fn alias_target_symbol(
        &self,
        symbol_id: tsz_binder::SymbolId,
        declaring_file_idx: Option<usize>,
    ) -> Option<(tsz_binder::SymbolId, usize)> {
        let source_file_idx = declaring_file_idx?;
        let binder = self.ctx.get_binder_for_file(source_file_idx)?;
        let symbol = binder.get_symbol(symbol_id)?;
        let locations: Vec<tsz_binder::StableLocation> =
            std::iter::once(symbol.stable_value_declaration)
                .chain(symbol.stable_declarations.iter().copied())
                .filter(tsz_binder::StableLocation::is_known)
                .collect();
        for location in locations {
            let Some((decl_idx, arena)) = self
                .ctx
                .node_at_stable_location_in_file(location, Some(source_file_idx))
            else {
                continue;
            };
            let Some((imported_name, module_specifier)) = Self::alias_import_edge(arena, decl_idx)
            else {
                continue;
            };
            let Some(target_idx) = self
                .ctx
                .resolve_import_target_from_file(source_file_idx, &module_specifier)
            else {
                continue;
            };
            let mut visited = rustc_hash::FxHashSet::default();
            let Some(target_symbol) =
                self.ctx
                    .resolve_export_in_target_file(target_idx, &imported_name, &mut visited)
            else {
                continue;
            };
            // The resolver registers the terminal symbol's own file; keep
            // `target_idx` only as the fallback for a same-file answer.
            let target_file_idx = self
                .ctx
                .resolve_symbol_file_index(target_symbol)
                .filter(|&idx| {
                    self.ctx
                        .get_binder_for_file(idx)
                        .is_some_and(|binder| binder.get_symbol(target_symbol).is_some())
                })
                .unwrap_or(target_idx);
            // Guard the hop the same way the anchor walk guards its span: a
            // resolved id whose symbol does not exist in the file it was
            // resolved to, or names something else, is a collision, not a
            // target. `default` is exempt — the exported name and the declared
            // name legitimately differ there.
            let names_the_target = self
                .ctx
                .get_binder_for_file(target_file_idx)
                .and_then(|binder| binder.get_symbol(target_symbol))
                .is_some_and(|symbol| {
                    imported_name == "default" || symbol.escaped_name == imported_name
                });
            if !names_the_target {
                continue;
            }
            return Some((target_symbol, target_file_idx));
        }
        None
    }

    /// `(imported name, module specifier)` for an alias declaration that names
    /// exactly one export of another module.
    ///
    /// A named specifier imports its `property_name` when it was renamed
    /// (`export { Cross as Renamed }` names `Cross` in the target module) and
    /// its own name otherwise; a default import clause names `default`. A
    /// namespace import binds the whole module rather than one export and is
    /// deliberately not an edge here.
    fn alias_import_edge(arena: &NodeArena, decl_idx: NodeIndex) -> Option<(String, String)> {
        use tsz_parser::parser::syntax_kind_ext;

        let node = arena.get(decl_idx)?;
        let imported_name = if node.kind == syntax_kind_ext::IMPORT_SPECIFIER
            || node.kind == syntax_kind_ext::EXPORT_SPECIFIER
        {
            let specifier = arena.get_specifier(node)?;
            Self::identifier_text(arena, specifier.property_name)
                .or_else(|| Self::identifier_text(arena, specifier.name))?
        } else if node.kind == syntax_kind_ext::IMPORT_CLAUSE {
            "default".to_string()
        } else {
            return None;
        };
        Some((
            imported_name,
            Self::alias_module_specifier(arena, decl_idx)?,
        ))
    }

    /// The module specifier of the import/export declaration `decl_idx` sits
    /// inside, as written.
    fn alias_module_specifier(arena: &NodeArena, decl_idx: NodeIndex) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = decl_idx;
        while current.is_some() {
            let node = arena.get(current)?;
            let specifier_idx = if node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                arena.get_import_decl(node)?.module_specifier
            } else if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                arena.get_export_decl(node)?.module_specifier
            } else {
                let ext = arena.get_extended(current)?;
                if ext.parent.is_none() {
                    break;
                }
                current = ext.parent;
                continue;
            };
            return arena
                .get(specifier_idx)
                .and_then(|node| arena.get_literal(node))
                .map(|literal| literal.text.clone());
        }
        None
    }

    /// Escaped text of an identifier node, when `idx` is one.
    fn identifier_text(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
        arena
            .get(idx)
            .and_then(|node| arena.get_identifier(node))
            .map(|identifier| identifier.escaped_text.to_string())
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

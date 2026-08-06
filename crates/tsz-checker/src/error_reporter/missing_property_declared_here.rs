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

    /// Property names, outermost first, naming the object-literal members the
    /// failure at `anchor_idx` sits inside — the syntactic path from the
    /// annotated binding down to the failing literal.
    ///
    /// Bounded by the same ancestor walk [`Self::target_annotation_node`] uses,
    /// and stopping at the same declaration and function boundaries, so the two
    /// always describe one assignment. Returns empty for a failure that is not
    /// inside an object literal at all (an array element, a call argument),
    /// which declines the pointer rather than guessing.
    pub(super) fn contextual_property_path(&self, anchor_idx: NodeIndex) -> Vec<String> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut path: Vec<String> = Vec::new();
        let mut current = anchor_idx;
        let mut guard = 0u32;
        while current.is_some() {
            guard += 1;
            if guard > 32 {
                break;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                || node.kind == syntax_kind_ext::PARAMETER
                || node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || node.kind == syntax_kind_ext::ARROW_FUNCTION
                || node.kind == syntax_kind_ext::METHOD_DECLARATION
                || node.kind == syntax_kind_ext::BLOCK
                || node.kind == syntax_kind_ext::SOURCE_FILE
            {
                break;
            }
            let name_idx = if node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                self.ctx.arena.get_property_assignment(node).map(|p| p.name)
            } else if node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                self.ctx.arena.get_shorthand_property(node).map(|p| p.name)
            } else {
                None
            };
            if let Some(name_idx) = name_idx {
                match crate::types_domain::queries::core::get_literal_property_name(
                    self.ctx.arena,
                    name_idx,
                ) {
                    Some(name) => path.push(name),
                    // A computed or otherwise non-literal member name cannot be
                    // matched against the written annotation by name. Abandon
                    // the whole path rather than skipping the level, which would
                    // anchor one member too shallow.
                    None => return Vec::new(),
                }
            }
            let Some(parent) = self.ctx.arena.get_extended(current).map(|ext| ext.parent) else {
                break;
            };
            current = parent;
        }
        path.reverse();
        path
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
        // A failure inside a *nested* object literal names a property of an
        // inner member type, not of the annotation itself: `oq` is no member of
        // `Outer` in `const r: Outer = { inner: { op: 1 } }`. Resolving the leaf
        // name straight against the annotation therefore declines, and tsc still
        // reports the pointer — `reportUnmatchedProperty` draws no distinction
        // between an inner and an outer literal. The missing step is the path,
        // which the object-literal syntax at the failure site supplies; each
        // member is validated against the written annotation before the next is
        // taken, so an annotation that does not declare the path declines here
        // exactly as it does at the top level.
        let path = self.contextual_property_path(anchor_idx);
        let anchor = if path.is_empty() {
            self.annotation_property_anchor(annotation_idx, property, 0)
        } else {
            let mut member_type_idx = annotation_idx;
            let mut walked = Some(());
            for key in &path {
                match self.annotation_member_type_node(member_type_idx, key, 0) {
                    Some(next) => member_type_idx = next,
                    None => {
                        walked = None;
                        break;
                    }
                }
            }
            walked.and_then(|()| self.annotation_property_anchor(member_type_idx, property, 0))
        };
        let (start, length, file) = anchor?;
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
    /// Parentheses are peeled and a type literal is anchored directly, both in
    /// the checking file's own arena. A type *reference* is handed to
    /// [`Self::member_declaration_anchor_following_aliases`], which owns
    /// cross-file arena resolution and re-export following — an annotation
    /// naming a re-exported alias must reach the original declaration exactly
    /// as a resolved target does. An indexed access resolves its written key
    /// against the object type's members and continues into that member's type
    /// node.
    pub(super) fn annotation_property_anchor(
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
            let type_ref = self.ctx.arena.get_type_ref(node)?;
            let name_idx = type_ref.type_name;
            let type_arguments = type_ref
                .type_arguments
                .as_ref()
                .map(|list| list.nodes.clone());
            if let Some(owner_symbol) = self.type_position_symbol(name_idx) {
                if let Some(anchor) =
                    self.member_declaration_anchor_following_aliases(owner_symbol, property)
                {
                    return Some(anchor);
                }
                // An alias chain (`type Indirect = Zed`) declares no member list of
                // its own, so the walk above finds nothing to anchor in. Continue
                // through the alias body, which is itself annotation syntax.
                if let Some(body_idx) = self.local_type_alias_body(owner_symbol)
                    && let Some(anchor) =
                        self.annotation_property_anchor(body_idx, property, depth + 1)
                {
                    return Some(anchor);
                }
            }
            // The reference itself declares no `property`, so look in what it is
            // parameterized by: `Array<T>` and `ReadonlyArray<T>` hold the written
            // element shape in a type argument exactly as `T[]` holds it in its
            // element type, and `tsc` anchors there identically.
            //
            // This is deliberately not keyed to the global array types. The
            // question the walk answers is "where is `property` written?", and a
            // type argument is the only place left to look once the reference's
            // own members and alias body have declined — so any generic wrapper
            // is served by the same rule, and a user-declared `Array` needs no
            // special case. See the anti-hardcoding gate in `.claude/CLAUDE.md`.
            return self.unique_type_argument_anchor(type_arguments?.as_slice(), property, depth);
        }
        if node.kind == syntax_kind_ext::INDEXED_ACCESS_TYPE {
            let data = self.ctx.arena.get_indexed_access_type(node)?;
            let (object_idx, index_idx) = (data.object_type, data.index_type);
            let key = self.written_index_key(index_idx)?;
            let member_type_idx = self.annotation_member_type_node(object_idx, &key, depth)?;
            return self.annotation_property_anchor(member_type_idx, property, depth + 1);
        }
        if node.kind == syntax_kind_ext::ARRAY_TYPE {
            // `T[]` and `T` describe the same element shape at every index, so
            // tsc anchors a missing-property pointer for an array-element
            // literal inside the element type exactly as it would for a bare
            // `T`-typed member. The array itself contributes no path segment
            // (`contextual_property_path` already skips over
            // `ARRAY_LITERAL_EXPRESSION` without pushing a name), so this is a
            // plain descent into the element type, not a new path step.
            let element_idx = self.ctx.arena.get_array_type(node)?.element_type;
            return self.annotation_property_anchor(element_idx, property, depth + 1);
        }
        if node.kind == syntax_kind_ext::TYPE_OPERATOR {
            let data = self.ctx.arena.get_type_operator(node)?;
            // `readonly T[]` describes the same element shape as `T[]`, so the
            // operator contributes no path segment and the walk descends into
            // its operand. `keyof`/`unique` are *not* transparent this way —
            // `keyof T` denotes T's keys, not T — so they decline here.
            if data.operator != tsz_scanner::SyntaxKind::ReadonlyKeyword as u16 {
                return None;
            }
            let operand_idx = data.type_node;
            return self.annotation_property_anchor(operand_idx, property, depth + 1);
        }
        None
    }

    /// The anchor for `property` in exactly one of `type_arguments`.
    ///
    /// Declines when two arguments both declare `property`: the walk has no
    /// basis to pick between them, and pointing at the wrong one is worse than
    /// omitting the pointer, which is what every other declining path here does.
    fn unique_type_argument_anchor(
        &mut self,
        type_arguments: &[NodeIndex],
        property: &str,
        depth: u32,
    ) -> Option<(u32, u32, Option<String>)> {
        let mut found: Option<(u32, u32, Option<String>)> = None;
        for &argument_idx in type_arguments {
            let Some(anchor) = self.annotation_property_anchor(argument_idx, property, depth + 1)
            else {
                continue;
            };
            if found.is_some() {
                return None;
            }
            found = Some(anchor);
        }
        found
    }

    /// The written type node of the member named `key` on the type annotation
    /// `idx` denotes, when that member is declared in the checking file.
    ///
    /// An indexed access whose object type is declared in another file declines
    /// here rather than reading a foreign arena's `NodeIndex` against the
    /// current one — per-file arenas make a raw index from another file name an
    /// unrelated node.
    pub(super) fn annotation_member_type_node(
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
    pub(super) fn local_declaration_node(
        &self,
        owner_symbol: tsz_binder::SymbolId,
    ) -> Option<NodeIndex> {
        let symbol = self.ctx.binder.get_symbol(owner_symbol)?;
        let location = std::iter::once(symbol.stable_value_declaration)
            .chain(symbol.stable_declarations.iter().copied())
            .find(tsz_binder::StableLocation::is_known)?;
        let (decl_idx, arena) = self.ctx.node_at_stable_location(location)?;
        std::ptr::eq(arena, self.ctx.arena).then_some(decl_idx)
    }

    /// The body type node of a type alias declared in the file being checked,
    /// used to continue the annotation walk through an alias chain.
    pub(super) fn local_type_alias_body(
        &self,
        owner_symbol: tsz_binder::SymbolId,
    ) -> Option<NodeIndex> {
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
    pub(super) fn type_position_symbol(&self, name_idx: NodeIndex) -> Option<tsz_binder::SymbolId> {
        match self.resolve_identifier_symbol_in_type_position_without_tracking(name_idx) {
            crate::symbol_resolver::TypeSymbolResolution::Type(symbol_id) => Some(symbol_id),
            _ => None,
        }
    }

    /// Bound on the annotation walk, which follows user-written type syntax and
    /// must terminate on a pathological nesting rather than recurse forever.
    pub(super) const ANNOTATION_WALK_MAX_DEPTH: u32 = 8;

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
        self.member_declaration_anchor_following_aliases(owner_symbol, property)
    }

    /// The member-declaration anchor for `owner_symbol`, following alias edges
    /// to the original declaration when the symbol itself owns no member list.
    ///
    /// Shared by the two entry points that reach a declaring symbol: the type's
    /// own resolution (which serves both `TS2728` and `TS6500`), and a type
    /// *reference* written in an annotation. All of them need the same
    /// re-export following, and all must carry the file index forward from the
    /// hop rather than re-deriving it — the id and the file it is valid in are
    /// one unit, since per-file binders mint colliding ids from `0`.
    fn member_declaration_anchor_following_aliases(
        &mut self,
        owner_symbol: tsz_binder::SymbolId,
        property: &str,
    ) -> Option<(u32, u32, Option<String>)> {
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
    pub(super) fn declaration_member_list(
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
    pub(super) fn member_name_node(arena: &NodeArena, member_idx: NodeIndex) -> Option<NodeIndex> {
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
    pub(super) fn anchor_span(arena: &NodeArena, anchor_idx: NodeIndex) -> Option<(u32, u32)> {
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
    pub(super) fn arena_file_name(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
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

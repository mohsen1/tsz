//! tsc's `The expected type comes from the return type of this signature.`
//! (`TS6502`) pointer.
//!
//! Structural rule, oracle-pinned against `typescript@7.0.2`
//! (`scripts/conformance/oracle.sh --strict --pretty`): when an object-literal
//! member's value is an expression-bodied arrow and the elaboration drilled to
//! the arrow's *body*, the `TS2322` reported there carries a pointer at the
//! declaration of the callable the expected return type came from — never at
//! the member's name, which is the sibling `TS6500` anchor.
//!
//! ```text
//! interface Ret { cb: () => string; }
//! const rt: Ret = { cb: () => 6 };
//!
//! a.ts:2:29 - error TS2322: Type 'number' is not assignable to type 'string'.
//!   a.ts:1:21 - The expected type comes from the return type of this signature.
//!     1 interface Ret { cb: () => string; }
//!                           ~~~~~~~~~~~~
//! ```
//!
//! The anchor is the *signature declaration*, which is why the two member
//! shapes underline different text: a property signature annotated with a
//! function type points at the **annotation** (`() => string`), while a method
//! signature has no separate annotation node and points at the **whole member**
//! (`m(): string;`, trailing semicolon included — the same span `TS2728`
//! already uses for a method signature).
//!
//! Deliberately reached from syntax and not from the type. The expected return
//! type is carried by an interned [`tsz_solver::types::FunctionShape`], which
//! has no declaration and cannot be given one: excluded from identity it would
//! be shared by every structurally identical signature in the program and the
//! second occurrence would anchor into the first one's file. That is the same
//! soundness wall that sank tsz#16454's `PropertyInfo::declared_location`
//! route, so the owner's *written* member list is the only per-occurrence
//! source of an anchor here.
//!
//! The owner-candidate route above needs a binder symbol, which an *anonymous*
//! owner never has — a type literal mints none at all (tsz#16443), so a nested
//! member (`interface Outer { inner: { cb: () => string } }`), an inline
//! annotation (`const x: { cb: () => string } = ...`) and a type-alias-to-type-
//! literal owner all declined. When every candidate declines, the same
//! annotation walk `missing_property_declared_here_related` uses for `TS2728`
//! and `expected_type_from_property_related` uses for `TS6500` recovers the
//! anchor from the object-literal syntax at the failure site instead, and then
//! narrows from the member to its signature exactly as the symbol route does.
//!
//! That walk also owns the *alias hop* the symbol route cannot take: for
//! `type Fn = () => string; interface Ref { cb: Fn }` tsc anchors inside `Fn`'s
//! body, not at `Fn` and not at `cb`. Following an alias means reading a
//! declaration's `NodeIndex`, which is only sound in the arena that minted it,
//! so the hop lives here — where every step is already checking-file-local —
//! rather than in the cross-arena symbol route.
//!
//! Shapes that still decline rather than guess, each pinned by a test:
//!
//! * a call argument (`take(() => 7)`) — the expected type comes from a
//!   parameter, not from an owner's member list, so there is no annotation to
//!   walk and no owner to resolve;
//! * an annotation that does not declare the written path, which declines at
//!   the hop that fails rather than anchoring one member too shallow.

use crate::diagnostics::{Diagnostic, diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Attach the `TS6502` pointer to every `TS2322` reported inside
    /// `body_idx`'s span since `since`.
    ///
    /// Called only from the arrow-body drill: that frame is the one tsc reports
    /// at, and a diagnostic that already carries a location pointer got it from
    /// a deeper frame that tsc's `!issuedElaboration` guard would have let keep
    /// it, so it is left alone exactly as the `TS6500` attach does.
    ///
    /// `annotation_anchor_idx` is the failing member's *name* node, not
    /// `body_idx`. The annotation fallback walks the object-literal ancestry
    /// outward, and that walk stops at the first function boundary it meets —
    /// starting it at the arrow's body would break on the arrow itself and see
    /// no path at all.
    pub(crate) fn attach_expected_type_from_return_pointer(
        &mut self,
        since: usize,
        owner_candidates: &[TypeId],
        property_name: &str,
        body_idx: NodeIndex,
        annotation_anchor_idx: NodeIndex,
    ) {
        if self.ctx.diagnostics.len() <= since {
            return;
        }
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };
        let (body_pos, body_end) = (body_node.pos, body_node.end);
        let Some(related) = self.expected_type_from_return_related(
            owner_candidates,
            property_name,
            annotation_anchor_idx,
        ) else {
            return;
        };
        for diagnostic in self.ctx.diagnostics.iter_mut().skip(since) {
            if diagnostic.code != diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE {
                continue;
            }
            if diagnostic.start < body_pos || diagnostic.start >= body_end {
                continue;
            }
            if diagnostic
                .related_information
                .iter()
                .any(|entry| entry.is_location_pointer())
            {
                continue;
            }
            diagnostic.related_information.push(related.clone());
        }
    }

    /// Build the `TS6502` entry for `property_name` on the first of
    /// `owner_candidates` whose written declaration carries that member with a
    /// signature to point at, falling back to the annotation syntax at
    /// `annotation_anchor_idx` for an owner that resolves to no binder symbol.
    fn expected_type_from_return_related(
        &mut self,
        owner_candidates: &[TypeId],
        property_name: &str,
        annotation_anchor_idx: NodeIndex,
    ) -> Option<crate::diagnostics::DiagnosticRelatedInformation> {
        let anchor = owner_candidates
            .iter()
            .find_map(|&owner| self.member_signature_anchor_for_owner(owner, property_name))
            .or_else(|| {
                self.return_signature_anchor_from_annotation(annotation_anchor_idx, property_name)
            })?;
        let (start, length, file) = anchor;
        Some(Diagnostic::related_pointer(
            diagnostic_codes::THE_EXPECTED_TYPE_COMES_FROM_THE_RETURN_TYPE_OF_THIS_SIGNATURE,
            file.unwrap_or_else(|| self.ctx.file_name.clone()),
            start,
            length,
            format_message(
                diagnostic_messages::THE_EXPECTED_TYPE_COMES_FROM_THE_RETURN_TYPE_OF_THIS_SIGNATURE,
                &[],
            ),
        ))
    }

    /// `(start, length, file)` recovered from the annotation the failing
    /// assignment was written with, for an owner with no binder symbol.
    ///
    /// `contextual_property_path(anchor_idx)` walks the object-literal ancestry
    /// from the failing member's *name* node outward, so the returned path ends
    /// with `property_name` itself — this member's value is what failed. Walking
    /// every hop but the last through the annotation's member types lands on the
    /// type node that actually declares `property_name`, exactly as
    /// `expected_type_from_property_anchor_from_annotation` does for `TS6500`;
    /// only the last step differs, narrowing from the member to its signature.
    ///
    /// Requiring the path to end in `property_name` is what keeps the walk
    /// honest: a path that describes a different member than the one that
    /// failed means the ancestry and the reported property disagree, and the
    /// pointer declines rather than anchoring on whichever member the annotation
    /// happens to declare under that name.
    fn return_signature_anchor_from_annotation(
        &mut self,
        anchor_idx: NodeIndex,
        property_name: &str,
    ) -> Option<(u32, u32, Option<String>)> {
        let annotation_idx = self.target_annotation_node(anchor_idx)?;
        let path = self.contextual_property_path(anchor_idx);
        if path.last().map(String::as_str) != Some(property_name) {
            return None;
        }
        let mut owner_idx = annotation_idx;
        for key in &path[..path.len() - 1] {
            owner_idx = self.annotation_member_type_node(owner_idx, key, 0)?;
        }
        self.annotation_signature_anchor(owner_idx, property_name, 0)
    }

    /// `(start, length, file)` of the signature declared for `property` inside
    /// the type annotation node `idx`, following the annotation's own syntax.
    ///
    /// The `TS6502` counterpart of
    /// [`Self::annotation_property_anchor`]: same node kinds, same alias
    /// continuation, same depth bound — it differs only in what it anchors on
    /// once the member is found, the member's *signature* rather than its name.
    ///
    /// Every step stays in the checking file's own arena. A type reference to a
    /// symbol declared elsewhere declines here rather than reading a foreign
    /// arena's `NodeIndex` against the current one; that case is the symbol
    /// route's, which resolves its arena from the location.
    fn annotation_signature_anchor(
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
            return self.annotation_signature_anchor(inner, property, depth + 1);
        }
        if node.kind == syntax_kind_ext::TYPE_LITERAL {
            return self.declared_member_signature_anchor(idx, property, depth);
        }
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let name_idx = self.ctx.arena.get_type_ref(node)?.type_name;
        let owner_symbol = self.type_position_symbol(name_idx)?;
        if let Some(declaration_idx) = self.local_declaration_node(owner_symbol)
            && let Some(anchor) =
                self.declared_member_signature_anchor(declaration_idx, property, depth)
        {
            return Some(anchor);
        }
        // An alias chain (`type Ind = Zed`) declares no member list of its own,
        // so the step above finds nothing to anchor in. Continue through the
        // alias body, which is itself annotation syntax.
        let body_idx = self.local_type_alias_body(owner_symbol)?;
        self.annotation_signature_anchor(body_idx, property, depth + 1)
    }

    /// `(start, length, file)` of the signature `property` is declared with on
    /// the declaration node `declaration_idx`, when that declaration owns a
    /// written member list carrying it.
    fn declared_member_signature_anchor(
        &mut self,
        declaration_idx: NodeIndex,
        property: &str,
        depth: u32,
    ) -> Option<(u32, u32, Option<String>)> {
        let members = Self::declaration_member_list(self.ctx.arena, declaration_idx)?;
        let member_idx = Self::named_member_node(self.ctx.arena, &members, property)?;
        self.member_signature_anchor(member_idx, depth)
    }

    /// `(start, length, file)` of the signature `member_idx` declares.
    ///
    /// [`Self::return_type_anchor_for_member`] answers this for a member whose
    /// signature is written directly on it. It declines for one wrapped in
    /// parentheses (`cb: (() => string)`), which tsc still underlines *inside*
    /// the parentheses, so the annotation is peeled here before giving up.
    fn member_signature_anchor(
        &mut self,
        member_idx: NodeIndex,
        depth: u32,
    ) -> Option<(u32, u32, Option<String>)> {
        use tsz_parser::parser::syntax_kind_ext;

        if let Some(anchor_idx) = Self::return_type_anchor_for_member(self.ctx.arena, member_idx) {
            let (start, length) = Self::anchor_span(self.ctx.arena, anchor_idx)?;
            return Some((
                start,
                length,
                Self::arena_file_name(self.ctx.arena, anchor_idx),
            ));
        }
        let node = self.ctx.arena.get(member_idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
            return None;
        }
        let annotation_idx = self.ctx.arena.get_signature(node)?.type_annotation;
        self.callable_type_node_anchor(annotation_idx, depth)
    }

    /// `(start, length, file)` of the function or constructor type `idx`
    /// denotes, peeling parentheses.
    ///
    /// Anything else — a union, an object type, a type reference — yields no
    /// signature of its own for tsc to point at, so it declines. A reference is
    /// deliberately *not* followed here: the arrow-body drill this pointer hangs
    /// off never fires for an alias-annotated member in the first place (see the
    /// `type_reference_annotation_*` test), so a hop at this site would be
    /// unreachable complexity on a diagnostic path.
    fn callable_type_node_anchor(
        &mut self,
        idx: NodeIndex,
        depth: u32,
    ) -> Option<(u32, u32, Option<String>)> {
        use tsz_parser::parser::syntax_kind_ext;

        if depth > Self::ANNOTATION_WALK_MAX_DEPTH {
            return None;
        }
        let node = self.ctx.arena.get(idx)?;
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
            let inner = self.ctx.arena.get_wrapped_type(node)?.type_node;
            return self.callable_type_node_anchor(inner, depth + 1);
        }
        if node.kind != syntax_kind_ext::FUNCTION_TYPE
            && node.kind != syntax_kind_ext::CONSTRUCTOR_TYPE
        {
            return None;
        }
        let (start, length) = Self::anchor_span(self.ctx.arena, idx)?;
        Some((start, length, Self::arena_file_name(self.ctx.arena, idx)))
    }

    /// `(start, length, file)` of the signature declaration for `property` on
    /// `owner`, in whichever arena declares it.
    ///
    /// Declines for an owner that resolves to no symbol, for a member this
    /// owner does not declare, and for a member whose signature is not written
    /// at the declaration — the three cases that would otherwise anchor a
    /// pointer at a span tsc does not underline.
    fn member_signature_anchor_for_owner(
        &mut self,
        owner: TypeId,
        property: &str,
    ) -> Option<(u32, u32, Option<String>)> {
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
        locations.into_iter().find_map(|location| {
            let (decl_idx, arena) = self
                .ctx
                .node_at_stable_location_in_file(location, declaring_file_idx)?;
            let members = Self::declaration_member_list(arena, decl_idx)?;
            let member_idx = Self::named_member_node(arena, &members, property)?;
            let anchor_idx = Self::return_type_anchor_for_member(arena, member_idx)?;
            let (start, length) = Self::anchor_span(arena, anchor_idx)?;
            Some((start, length, Self::arena_file_name(arena, anchor_idx)))
        })
    }

    /// The member of `members` written with the name `property`.
    fn named_member_node(
        arena: &NodeArena,
        members: &tsz_parser::parser::NodeList,
        property: &str,
    ) -> Option<NodeIndex> {
        members.nodes.iter().copied().find(|&member_idx| {
            Self::member_name_node(arena, member_idx)
                .and_then(|name_idx| {
                    crate::types_domain::queries::core::get_literal_property_name(arena, name_idx)
                })
                .is_some_and(|name| name == property)
        })
    }

    /// The node tsc underlines as "this signature" for `member_idx`.
    ///
    /// A method signature *is* the signature, so it is underlined whole. A
    /// property signature carries one only when its annotation is written as a
    /// function or constructor type; any other annotation (a type reference, a
    /// union, an omitted one) declines, because the signature tsc points at is
    /// then not this node.
    fn return_type_anchor_for_member(
        arena: &NodeArena,
        member_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        let node = arena.get(member_idx)?;
        if node.kind == syntax_kind_ext::METHOD_SIGNATURE {
            return Some(member_idx);
        }
        if node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
            return None;
        }
        let annotation_idx = arena.get_signature(node)?.type_annotation;
        let annotation = arena.get(annotation_idx)?;
        (annotation.kind == syntax_kind_ext::FUNCTION_TYPE
            || annotation.kind == syntax_kind_ext::CONSTRUCTOR_TYPE)
            .then_some(annotation_idx)
    }
}

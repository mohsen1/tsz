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
//! Shapes that decline rather than guess, each pinned by a test:
//!
//! * a member annotated with a type *reference* (`cb: Fn`) — tsc anchors inside
//!   the alias body, which needs the alias hop this walk does not take;
//! * a call argument (`take(() => 7)`) — the expected type comes from a
//!   parameter, not from an owner's member list;
//! * an anonymous owner, which resolves to no binder symbol at all (tsz#16443).

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
    pub(crate) fn attach_expected_type_from_return_pointer(
        &mut self,
        since: usize,
        owner_candidates: &[TypeId],
        property_name: &str,
        body_idx: NodeIndex,
    ) {
        if self.ctx.diagnostics.len() <= since {
            return;
        }
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };
        let (body_pos, body_end) = (body_node.pos, body_node.end);
        let Some(related) = self.expected_type_from_return_related(owner_candidates, property_name)
        else {
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
    /// signature to point at.
    fn expected_type_from_return_related(
        &mut self,
        owner_candidates: &[TypeId],
        property_name: &str,
    ) -> Option<crate::diagnostics::DiagnosticRelatedInformation> {
        owner_candidates.iter().find_map(|&owner| {
            let (start, length, file) = self.member_signature_anchor_for_owner(owner, property_name)?;
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
        })
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

use crate::diagnostics::{Diagnostic, diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Attach tsc's `The expected type comes from property '{0}' which is
    /// declared here on type '{1}'` (TS6500) pointer to every `TS2322` pushed
    /// since `since`.
    ///
    /// tsc's `elaborateElementwise` adds this related entry to the diagnostic it
    /// just reported for an object-literal member, pointing at the *target*
    /// property's own declaration. It is the elaboration's own leaf report that
    /// carries the pointer: when the recursion drilled further (a nested
    /// literal, an array element, an arrow body) the inner frame already owns
    /// the user's attention and tsc's `!issuedElaboration` guard suppresses the
    /// outer one. Callers therefore invoke this only around a leaf emit, never
    /// around a recursive elaboration that returned `true`.
    ///
    /// `owner_candidates` are the target types the caller already has in hand;
    /// the first that resolves to a declaration owning a member with this name
    /// wins. When none does — an anonymous shape with no symbol, an index
    /// signature, a synthesized member — nothing is attached and output is left
    /// exactly as it was rather than guessing at a declaration.
    /// Run `emit` — an object-literal member's *leaf* report — and attach the
    /// TS6500 pointer to whatever `TS2322` it produced.
    ///
    /// Wrapping the emit rather than editing each emitter keeps the pointer tied
    /// to the one decision that governs it: this frame, not a deeper one, is the
    /// frame tsc reported at.
    pub(crate) fn with_expected_type_from_property_pointer(
        &mut self,
        owner_candidates: &[TypeId],
        property_name: &str,
        emit: impl FnOnce(&mut Self),
    ) {
        let before = self.ctx.diagnostics.len();
        emit(self);
        self.attach_expected_type_from_property_pointer(before, owner_candidates, property_name);
    }

    pub(crate) fn attach_expected_type_from_property_pointer(
        &mut self,
        since: usize,
        owner_candidates: &[TypeId],
        property_name: &str,
    ) {
        if self.ctx.diagnostics.len() <= since {
            return;
        }
        let Some(related) =
            self.expected_type_from_property_related(owner_candidates, property_name)
        else {
            return;
        };
        for diagnostic in self.ctx.diagnostics.iter_mut().skip(since) {
            if diagnostic.code != diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE {
                continue;
            }
            // A leaf report that already carries a location pointer got it from
            // a deeper frame; tsc keeps that one rather than stacking a second.
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

    /// Build the TS6500 related entry for `property_name` on the first of
    /// `owner_candidates` whose declaration carries that member.
    ///
    /// The anchor is the same one TS2728 uses (the member's name node, or the
    /// whole member for an interface method signature), but the *name* is
    /// rendered differently: tsc formats TS6500's operand through
    /// `symbolToString`, so a string-literal member reads `property 'two-part'`
    /// where TS2728 reads `'"two-part"' is declared here.`. Passing the interned
    /// property name — not the primary diagnostic's display string — is what
    /// keeps the two apart.
    fn expected_type_from_property_related(
        &mut self,
        owner_candidates: &[TypeId],
        property_name: &str,
    ) -> Option<crate::diagnostics::DiagnosticRelatedInformation> {
        owner_candidates.iter().find_map(|&owner| {
            let (start, length, file) =
                self.member_declaration_anchor_for_owner(owner, property_name)?;
            let owner_display = self.format_type_for_assignability_message(owner);
            Some(Diagnostic::related_pointer(
                diagnostic_codes::THE_EXPECTED_TYPE_COMES_FROM_PROPERTY_WHICH_IS_DECLARED_HERE_ON_TYPE,
                file.unwrap_or_else(|| self.ctx.file_name.clone()),
                start,
                length,
                format_message(
                    diagnostic_messages::THE_EXPECTED_TYPE_COMES_FROM_PROPERTY_WHICH_IS_DECLARED_HERE_ON_TYPE,
                    &[property_name, &owner_display],
                ),
            ))
        })
    }
}

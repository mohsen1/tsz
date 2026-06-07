use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext::PROPERTY_SIGNATURE;

/// The `readonly` / optional modifier signature of a single property
/// declaration. Two declarations of the same property must carry identical
/// flags or `tsc` reports TS2687.
#[derive(Clone, Copy, PartialEq, Eq)]
struct PropertyModifierFlags {
    readonly: bool,
    optional: bool,
}

impl<'a> CheckerState<'a> {
    /// TS2687: "All declarations of '{0}' must have identical modifiers."
    ///
    /// `tsc` raises this whenever two or more property declarations resolve to
    /// the same member name but disagree on the `readonly` or optional (`?`)
    /// modifier. It is independent of the duplicate-identifier (TS2300) and
    /// same-type (TS2717) diagnostics: it fires even when the names are computed
    /// (so TS2300 is suppressed) and even when the declared types are identical
    /// (so TS2717 is absent).
    ///
    /// Targeting mirrors `tsc`: the first declaration is the reference; every
    /// later declaration whose flags differ from it is flagged, and the
    /// reference itself is flagged once if any later declaration differs. So
    /// `readonly a; a; readonly a` flags the first two (the third matches the
    /// reference) while `a; readonly a; readonly a` flags all three.
    ///
    /// `member_nodes` is the property-signature member nodes that share the
    /// canonical `name`, in source order. Callers pass the same group of
    /// declarations they already detected as duplicates, so computed names that
    /// resolve to the same value share a group.
    pub(crate) fn report_property_modifier_disagreements(
        &mut self,
        name: &str,
        member_nodes: &[NodeIndex],
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if member_nodes.len() < 2 {
            return;
        }

        let Some(reference) = self.property_signature_modifier_flags(member_nodes[0]) else {
            return;
        };

        // Collect the declarations that disagree with the reference before
        // emitting, so the flag reads (`&self`) don't interleave with the error
        // emission (`&mut self`). The reference is flagged once if any later
        // declaration differs.
        let mut nodes_to_flag: Vec<NodeIndex> = Vec::new();
        for &member_idx in &member_nodes[1..] {
            let Some(flags) = self.property_signature_modifier_flags(member_idx) else {
                continue;
            };
            if flags != reference {
                if nodes_to_flag.is_empty() {
                    nodes_to_flag.push(member_nodes[0]);
                }
                nodes_to_flag.push(member_idx);
            }
        }
        if nodes_to_flag.is_empty() {
            return;
        }

        let message = crate::diagnostics::format_message(
            diagnostic_messages::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_MODIFIERS,
            &[name],
        );
        for member_idx in nodes_to_flag {
            let name_node = self
                .property_signature_name_node(member_idx)
                .unwrap_or(member_idx);
            self.error_at_node(
                name_node,
                &message,
                diagnostic_codes::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_MODIFIERS,
            );
        }
    }

    /// Extract the `readonly` / optional modifier flags of a property signature,
    /// or `None` when `member_idx` is not a property signature.
    fn property_signature_modifier_flags(
        &self,
        member_idx: NodeIndex,
    ) -> Option<PropertyModifierFlags> {
        let node = self.ctx.arena.get(member_idx)?;
        if node.kind != PROPERTY_SIGNATURE {
            return None;
        }
        let sig = self.ctx.arena.get_signature(node)?;
        Some(PropertyModifierFlags {
            readonly: self.has_readonly_modifier(&sig.modifiers),
            optional: sig.question_token,
        })
    }

    /// The name node of a property signature, used to anchor the diagnostic.
    fn property_signature_name_node(&self, member_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(member_idx)?;
        if node.kind != PROPERTY_SIGNATURE {
            return None;
        }
        let sig = self.ctx.arena.get_signature(node)?;
        sig.name.is_some().then_some(sig.name)
    }
}

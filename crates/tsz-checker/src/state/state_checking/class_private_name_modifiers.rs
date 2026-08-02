//! Modifier grammar for class members named by a private identifier
//! (`#name`): `TS18010` and `TS18019`.
//!
//! `tsc` reports these two codes from `checkGrammarModifiers`, which walks a
//! member's modifier list **in source order** and **returns at the first
//! error**. A member therefore reports at most one modifier-grammar
//! diagnostic, and *which* one it reports depends on the order the modifiers
//! were written in:
//!
//! ```text
//! abstract class C { abstract static #x: number; }   // TS18019 at `abstract`
//! abstract class C { static abstract #x: number; }   // TS1243  at `abstract`
//! ```
//!
//! Both spellings declare the same member; only the walk order differs. The
//! same rule explains why an `abstract` member of a *non*-abstract class never
//! reports `TS18019` (the `TS1244`/`TS1253` container check fires first and
//! returns), and why `declare abstract #x` reports one `TS18019` rather than
//! two.
//!
//! This module owns the private-identifier arm of that walk. Modifier errors
//! that preempt it are reported elsewhere — container-abstractness in
//! `class.rs`, modifier ordering (`TS1029`) and modifier pairs (`TS1243`) in
//! the parser — so this walk stops without reporting when it reaches a
//! modifier whose own error `tsc` would have emitted first.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::state::CheckerState;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

/// Which private-identifier diagnostic a modifier walk settled on, if any.
enum PrivateNameModifierError {
    /// `TS18010`, anchored at an accessibility modifier.
    Accessibility(NodeIndex),
    /// `TS18019`, anchored at `abstract`/`declare` (carried for the message).
    Incompatible(NodeIndex, &'static str),
    /// A different modifier error preempts the private-identifier check and is
    /// reported by another owner; report nothing here.
    PreemptedElsewhere,
}

impl CheckerState<'_> {
    /// Report `TS18010`/`TS18019` for a private-named class member, following
    /// `tsc`'s source-ordered, first-error-wins modifier walk.
    ///
    /// `member_kind` is the member's `SyntaxKind`; `is_abstract_class` is the
    /// containing class's own abstractness, which decides whether an
    /// `abstract` modifier reaches the private-identifier check at all.
    pub(super) fn check_private_name_modifier_grammar(
        &mut self,
        member_idx: NodeIndex,
        member_kind: u16,
        modifiers: &Option<NodeList>,
        is_abstract_class: bool,
    ) {
        match self.walk_private_name_modifiers(member_kind, modifiers, is_abstract_class) {
            Some(PrivateNameModifierError::Accessibility(mod_idx)) => {
                self.error_at_node(
                    mod_idx,
                    diagnostic_messages::AN_ACCESSIBILITY_MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                    diagnostic_codes::AN_ACCESSIBILITY_MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                );
            }
            Some(PrivateNameModifierError::Incompatible(mod_idx, keyword)) => {
                self.error_at_node_msg(
                    mod_idx,
                    diagnostic_codes::MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                    &[keyword],
                );
            }
            Some(PrivateNameModifierError::PreemptedElsewhere) => {}
            None => self.report_jsdoc_accessibility_on_private_name(member_idx),
        }
    }

    /// The walk itself: returns the first modifier outcome, or `None` when no
    /// modifier in the list interacts with the private identifier.
    fn walk_private_name_modifiers(
        &self,
        member_kind: u16,
        modifiers: &Option<NodeList>,
        is_abstract_class: bool,
    ) -> Option<PrivateNameModifierError> {
        let Some(mods) = modifiers else {
            return None;
        };
        // `declare` is only a valid class-element modifier on a property; on a
        // method or accessor it is TS1031 ("cannot appear on class elements of
        // this kind"), which tsc reports instead of TS18019.
        let declare_is_valid_here = member_kind == syntax_kind_ext::PROPERTY_DECLARATION;
        let mut seen_static = false;

        for &mod_idx in &mods.nodes {
            let Some(mod_node) = self.ctx.arena.get(mod_idx) else {
                continue;
            };
            let kind = mod_node.kind;
            if kind == SyntaxKind::PublicKeyword as u16
                || kind == SyntaxKind::PrivateKeyword as u16
                || kind == SyntaxKind::ProtectedKeyword as u16
            {
                // An accessibility modifier must precede `static`; written
                // after it, tsc reports the ordering error (TS1029, owned by
                // the parser) and returns before reaching TS18010.
                return Some(if seen_static {
                    PrivateNameModifierError::PreemptedElsewhere
                } else {
                    PrivateNameModifierError::Accessibility(mod_idx)
                });
            } else if kind == SyntaxKind::StaticKeyword as u16 {
                seen_static = true;
            } else if kind == SyntaxKind::AbstractKeyword as u16 {
                // `abstract` on a member of a non-abstract class is
                // TS1244/TS1253, and `static` before `abstract` is TS1243.
                // Either way tsc returns before the private-identifier check.
                return Some(if !is_abstract_class || seen_static {
                    PrivateNameModifierError::PreemptedElsewhere
                } else {
                    PrivateNameModifierError::Incompatible(mod_idx, "abstract")
                });
            } else if kind == SyntaxKind::DeclareKeyword as u16 {
                return Some(if declare_is_valid_here {
                    PrivateNameModifierError::Incompatible(mod_idx, "declare")
                } else {
                    PrivateNameModifierError::PreemptedElsewhere
                });
            }
        }
        None
    }

    /// JS files carry accessibility through JSDoc tags (`@public`/`@private`/
    /// `@protected`) rather than AST modifiers. tsc anchors TS18010 at the tag
    /// itself; fall back to the member when the span cannot be recovered.
    fn report_jsdoc_accessibility_on_private_name(&mut self, member_idx: NodeIndex) {
        if !self.is_js_file() || !self.has_jsdoc_accessibility_modifier(member_idx) {
            return;
        }
        if let Some((start, len)) = self.jsdoc_accessibility_tag_span(member_idx) {
            self.error_at_position(
                start,
                len,
                diagnostic_messages::AN_ACCESSIBILITY_MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                diagnostic_codes::AN_ACCESSIBILITY_MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
            );
        } else {
            self.error_at_node(
                member_idx,
                diagnostic_messages::AN_ACCESSIBILITY_MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                diagnostic_codes::AN_ACCESSIBILITY_MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
            );
        }
    }
}

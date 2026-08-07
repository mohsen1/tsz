//! Grammar recovery for class-member modifiers written on a *type* member
//! (an interface member or a type-literal member).
//!
//! `tsc`'s `checkGrammarModifiers` rejects every class-member modifier on a
//! type member with a single diagnostic, anchored on and naming the FIRST
//! modifier: `TS1070` (`'{0}' modifier cannot appear on a type member.`) for a
//! property/method member, `TS1071` (`... on an index signature.`) for an index
//! signature. `readonly` is the one modifier `tsc` accepts on a type member and
//! is preserved.

use super::state::ParserState;
use crate::parser::NodeIndex;
use tsz_scanner::SyntaxKind;

impl ParserState {
    /// A class-member modifier keyword that is illegal on a *type* member.
    /// `readonly` is deliberately excluded: it is the one member modifier `tsc`
    /// accepts on a property signature or index signature.
    pub(crate) const fn is_illegal_type_member_modifier(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::StaticKeyword
                | SyntaxKind::AccessorKeyword
                | SyntaxKind::OverrideKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::ExportKeyword
                | SyntaxKind::InKeyword
                | SyntaxKind::OutKeyword
        )
    }

    /// Returns `(diagnostic_emitted, node)`. `tsc`'s `checkGrammarModifiers`
    /// reports at most one diagnostic for a member's leading-modifier run and
    /// returns immediately, so callers use `diagnostic_emitted` to suppress
    /// later modifier-specific checks (e.g. `readonly`-on-method TS1024,
    /// `async`-on-type-member TS1070) once this pass has already reported.
    pub(crate) fn parse_type_member_visibility_modifier_error(
        &mut self,
        start_pos: u32,
    ) -> (bool, Option<NodeIndex>) {
        if Self::is_illegal_type_member_modifier(self.token())
            && !self.look_ahead_is_property_name_after_keyword()
            && !self.look_ahead_has_line_break_after_keyword()
        {
            use tsz_common::diagnostics::diagnostic_codes;

            // `tsc` reports a single diagnostic anchored on and naming the FIRST
            // modifier, regardless of how many illegal modifiers lead the member
            // (`public static x` and `static public x` each report once).
            let modifier_text = self.scanner.get_token_text();

            // Look past the whole run of leading modifiers — the illegal ones
            // plus a legal trailing `readonly` — to classify what they decorate:
            // an index signature (TS1071) or a property/method member (TS1070).
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            while Self::is_illegal_type_member_modifier(self.token())
                || self.is_token(SyntaxKind::ReadonlyKeyword)
            {
                self.next_token();
            }
            let is_index_signature =
                self.is_token(SyntaxKind::OpenBracketToken) && self.look_ahead_is_index_signature();
            self.scanner.restore_state(snapshot);
            self.current_token = current;

            if is_index_signature {
                self.parse_error_at_current_token(
                    &format!("'{modifier_text}' modifier cannot appear on an index signature."),
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_AN_INDEX_SIGNATURE,
                );
            } else {
                self.parse_error_at_current_token(
                    &format!("'{modifier_text}' modifier cannot appear on a type member."),
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER,
                );
            }

            // Consume the whole run of illegal modifiers so the underlying
            // member parses cleanly (a single TS1070/TS1071 rather than a
            // cascade on the remaining modifiers). A trailing `readonly` is left
            // in place: the member/index-signature parser handles it.
            while Self::is_illegal_type_member_modifier(self.token()) {
                self.next_token();
            }
            if is_index_signature {
                // Skip `readonly` if present (e.g. `static readonly [s: string]: number`)
                if self.is_token(SyntaxKind::ReadonlyKeyword) {
                    self.next_token();
                }
                return (
                    true,
                    Some(self.parse_index_signature_with_modifiers(None, start_pos)),
                );
            }
            return (true, None);
        }

        (false, None)
    }

    /// If the current token is a leading `async` modifier, consumes it and,
    /// when `report` is set, emits the TS1070 diagnostic. `report` is `false`
    /// when an earlier modifier in the same member already reported — `tsc`'s
    /// single-diagnostic-per-member rule means `async` must still be consumed
    /// (it is never legal here) but must not report its own TS1070. Returns
    /// whether `async` was found (and consumed).
    pub(crate) fn parse_async_type_member_restriction(&mut self, report: bool) -> bool {
        if self.is_token(SyntaxKind::AsyncKeyword)
            && !self.look_ahead_is_property_name_after_keyword()
        {
            if report {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "'async' modifier cannot appear on a type member.",
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER,
                );
            }
            self.next_token();
            true
        } else {
            false
        }
    }
}

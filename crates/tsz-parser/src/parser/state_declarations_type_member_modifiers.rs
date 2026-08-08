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

    /// Report a run of `count` type-member modifiers directly before a
    /// `get`/`set` accessor where at least one is "hard"
    /// (`async`/`declare`/`abstract`/`override`): one TS1131 per modifier, each
    /// anchored at its own token, then defer the container's close brace so the
    /// accessor's own tail re-parses as top-level statements
    /// (TS1434/TS1005/TS1128). This reproduces tsc's abandon-body recovery,
    /// which differs from the clean-only run's bare-accessor recovery. Shared by
    /// the interface (`parse_type_members`) and type-literal
    /// (`parse_type_literal_rest`) member loops; the caller breaks its member
    /// loop immediately after, leaving the deferred close brace for
    /// `finish_type_member_container_close_brace`.
    pub(crate) fn report_hard_modifier_run_before_accessor(&mut self, count: usize) {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
        for _ in 0..count {
            let mod_start = self.token_pos();
            let mod_end = self.token_end();
            self.next_token();
            self.parse_error_at(
                mod_start,
                mod_end.saturating_sub(mod_start),
                diagnostic_messages::PROPERTY_OR_SIGNATURE_EXPECTED,
                diagnostic_codes::PROPERTY_OR_SIGNATURE_EXPECTED,
            );
        }
        self.deferred_type_member_close_braces = self
            .deferred_type_member_close_braces
            .max(self.type_member_container_depth);
        // The abandoned tail re-parses as top-level statements; an enclosing
        // `type X = <literal>` alias must not then require a trailing semicolon
        // for those tokens. See `pending_type_member_body_reparse`.
        self.pending_type_member_body_reparse = true;
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

            // `tsc` reports a single diagnostic for the member's whole
            // leading-modifier run and returns immediately, so any illegal
            // modifier trailing `async` (`async static m()`, `async static
            // public m()`, ...) must still be consumed silently so the
            // member parses cleanly — mirroring
            // `parse_type_member_visibility_modifier_error`'s and
            // `parse_type_member_property_or_method`'s equivalent "consume
            // the whole run" steps. `readonly` is deliberately excluded (as
            // elsewhere): it is legal on a property/index signature and is
            // left for `parse_type_member_property_or_method` to handle.
            while Self::is_illegal_type_member_modifier(self.token())
                && !self.look_ahead_is_property_name_after_keyword()
            {
                self.next_token();
            }

            true
        } else {
            false
        }
    }

    /// A "clean" type-member modifier: one `tsc` parses as a modifier and then
    /// rejects with a single TS1131 before an accessor, recovering the accessor
    /// as a bare (modifier-less) member. `readonly` is included because it is a
    /// legal member modifier that still cannot precede an accessor.
    pub(crate) const fn is_clean_type_member_modifier(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::StaticKeyword
                | SyntaxKind::AccessorKeyword
                | SyntaxKind::ExportKeyword
                | SyntaxKind::ReadonlyKeyword
        )
    }

    /// A "hard" type-member modifier before an accessor: one `tsc` does NOT
    /// recover into a bare accessor. Instead it abandons the type-member body
    /// after one TS1131 per modifier and re-parses the accessor's own tail as
    /// top-level statements (TS1434/TS1005/TS1128). `in`/`out` are deliberately
    /// excluded — `in` is a reserved operator whose statement re-parse differs,
    /// and both carry variance-position idiosyncrasies; they keep the
    /// pre-existing semantic TS1070 path.
    pub(crate) const fn is_hard_accessor_cascade_modifier(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::AsyncKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::OverrideKeyword
        )
    }

    /// Check whether the current token starts a run of one or more *clean*
    /// type-member modifiers directly followed, each on the same line, by a
    /// `get`/`set` accessor signature (`static get x()`, `public static get
    /// x()`, ...), as opposed to a modifier or `get`/`set` used as an ordinary
    /// property/method name (`static get(): void`, `static get: number`). tsc
    /// reports one TS1131 per modifier in the run (each anchored at its own
    /// token) then recovers by retrying at the accessor keyword, which parses
    /// as a bare (modifier-less) accessor. Returns the run length, or `0`.
    ///
    /// A run containing a "hard" modifier is handled separately by
    /// [`Self::look_ahead_hard_modifier_run_before_accessor`] (a different,
    /// abandon-body recovery); `in`/`out` stay on the semantic TS1070 path.
    pub(crate) fn look_ahead_modifier_run_before_accessor(&mut self) -> usize {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        let mut count = 0usize;
        loop {
            if !Self::is_clean_type_member_modifier(self.token())
                || self.look_ahead_is_property_name_after_keyword()
            {
                break;
            }
            self.next_token();
            count += 1;
            if self.scanner.has_preceding_line_break() {
                count = 0;
                break;
            }
        }

        let ends_in_accessor = count > 0
            && (self.is_token(SyntaxKind::GetKeyword) || self.is_token(SyntaxKind::SetKeyword))
            && !self.look_ahead_is_property_name_after_keyword();

        self.scanner.restore_state(snapshot);
        self.current_token = current;

        if ends_in_accessor { count } else { 0 }
    }

    /// Look ahead for a run of type-member modifiers (clean and/or hard)
    /// directly before a `get`/`set` accessor where AT LEAST ONE modifier is
    /// "hard" (`async`/`declare`/`abstract`/`override`). Returns the run length,
    /// or 0 when no such run ends in an accessor.
    ///
    /// Distinct from [`Self::look_ahead_modifier_run_before_accessor`], which
    /// covers clean-only runs (one TS1131 per modifier, then a recovered bare
    /// accessor). A run containing a hard modifier does not parse as any member
    /// in `tsc`: after one TS1131 per modifier the type-member body is abandoned
    /// and the accessor's own tail re-parses as top-level statements. The caller
    /// reproduces that by deferring the container's close brace (the same
    /// mechanism [`Self::recover_invalid_type_member`] uses).
    pub(crate) fn look_ahead_hard_modifier_run_before_accessor(&mut self) -> usize {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        let mut count = 0usize;
        let mut saw_hard = false;
        loop {
            let kind = self.token();
            let is_hard = Self::is_hard_accessor_cascade_modifier(kind);
            if (!Self::is_clean_type_member_modifier(kind) && !is_hard)
                || self.look_ahead_is_property_name_after_keyword()
            {
                break;
            }
            if is_hard {
                saw_hard = true;
            }
            self.next_token();
            count += 1;
            if self.scanner.has_preceding_line_break() {
                count = 0;
                break;
            }
        }

        let ends_in_accessor = count > 0
            && saw_hard
            && (self.is_token(SyntaxKind::GetKeyword) || self.is_token(SyntaxKind::SetKeyword))
            && !self.look_ahead_is_property_name_after_keyword();

        self.scanner.restore_state(snapshot);
        self.current_token = current;

        if ends_in_accessor { count } else { 0 }
    }
}

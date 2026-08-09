//! Parser state - interface, type alias, module, import/export, and control flow parsing methods
//!
//! Enum declaration parsing lives in `state_declarations_enums.rs`.

use super::state::ParserState;
use crate::parser::{
    NodeIndex, NodeList,
    node::{IdentifierData, ParameterData},
    syntax_kind_ext,
};
use tsz_common::interner::{AstAtom, IdentText};
use tsz_scanner::SyntaxKind;

fn is_reserved_interface_type_name(name: &str) -> bool {
    matches!(
        name,
        "any"
            | "unknown"
            | "never"
            | "string"
            | "number"
            | "boolean"
            | "symbol"
            | "bigint"
            | "void"
            | "undefined"
            | "null"
            | "object"
    )
}

enum TypeMemberPropertyOrMethodName {
    Property(NodeIndex),
    IndexSignature(NodeIndex),
}

/// The offending token immediately following a type member's leading
/// `readonly`, when it is not the property/index-signature name. `tsc`
/// reports a different message and code depending on which of these it is:
/// a second `readonly` is TS1030 (`'readonly' modifier already seen.`), any
/// other modifier is TS1070 (`'{0}' modifier cannot appear on a type
/// member.`).
enum SecondTypeMemberModifier {
    DuplicateReadonly,
    Illegal(String),
}

impl ParserState {
    /// Parse interface declaration
    pub(crate) fn parse_interface_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_interface_declaration_with_modifiers(start_pos, None)
    }

    /// Parse interface declaration with explicit modifiers
    pub(crate) fn parse_interface_declaration_with_modifiers(
        &mut self,
        start_pos: u32,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::InterfaceKeyword);
        let mut has_invalid_numeric_name = false;
        let mut has_invalid_hard_keyword_name = false;

        // Parse interface name - keywords like 'string', 'abstract' can be used as interface names
        // Type keywords like 'void' are parsed as names and rejected by the checker (TS2427)
        // tsc allows `yield` as an interface name even inside generators
        let name = if self.is_token(SyntaxKind::YieldKeyword) {
            self.parse_identifier_name()
        } else if self.is_identifier_or_keyword() {
            // Type keywords (void, null) are accepted as names by the parser.
            // The checker emits TS2427 for predefined type names used as interface names.
            // Other reserved words (class, function, return, etc.) still get TS1005.
            if self.is_reserved_word()
                && !matches!(
                    self.current_token,
                    SyntaxKind::VoidKeyword | SyntaxKind::NullKeyword
                )
            {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
                // Consume the invalid token to avoid cascading errors
                self.next_token();
                NodeIndex::NONE
            } else {
                has_invalid_hard_keyword_name = matches!(
                    self.current_token,
                    SyntaxKind::VoidKeyword | SyntaxKind::NullKeyword
                );
                let name_text = self.scanner.get_token_value();
                if is_reserved_interface_type_name(name_text.as_str()) {
                    use tsz_common::diagnostics::diagnostic_codes;
                    let name_start = self.token_pos();
                    let name_end = self.token_end();
                    self.parse_error_at(
                        name_start,
                        name_end - name_start,
                        &format!("Interface name cannot be '{name_text}'."),
                        diagnostic_codes::INTERFACE_NAME_CANNOT_BE,
                    );
                }
                self.parse_identifier_name()
            }
        } else if self.is_token(SyntaxKind::OpenBraceToken) {
            // TS1438: Interface must be given a name (e.g., `interface { }`)
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.parse_error_at_current_token(
                diagnostic_messages::INTERFACE_MUST_BE_GIVEN_A_NAME,
                diagnostic_codes::INTERFACE_MUST_BE_GIVEN_A_NAME,
            );
            NodeIndex::NONE
        } else if self.is_token(SyntaxKind::NumericLiteral) {
            use tsz_common::diagnostics::diagnostic_codes;
            let name_start = self.token_pos();
            let name_end = self.token_end();
            let name_text = self.scanner.get_token_value();
            self.parse_error_at(
                name_start,
                name_end - name_start,
                &format!("Interface name cannot be '{name_text}'."),
                diagnostic_codes::INTERFACE_NAME_CANNOT_BE,
            );
            self.next_token();
            has_invalid_numeric_name = true;
            self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                name_start,
                name_end,
                IdentifierData {
                    atom: AstAtom::NONE,
                    escaped_text: IdentText::empty(),
                    original_text: None,
                },
            )
        } else {
            self.parse_identifier()
        };

        // Dotted names like `Foo.I1` are not valid interface names. tsc parses
        // `interface Foo`, then fails `parseExpected('{')` at the `.` (TS1005),
        // terminates the interface with an empty body, and resumes statement
        // parsing at the segment after the dot. The remaining `I1 { }` is then
        // recovered as an expression statement (`I1;` with TS1434 "Unexpected
        // keyword or identifier") followed by a block (`{ }`). Mirror that:
        // emit TS1005 at the dot, consume only the dot, and return an
        // empty-body interface so the trailing tokens re-enter statement
        // recovery (matching tsc's emit and diagnostics).
        if self.is_token(SyntaxKind::DotToken) {
            use tsz_common::diagnostics::diagnostic_codes;
            // Emit '{' expected at the dot position (tsc expects { after the name)
            self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
            // Consume only the dot; leave the following segment(s) for the
            // statement loop so the recovered statement is preserved in emit.
            self.next_token();
            let end_pos = self.arena.get(name).map_or(start_pos, |node| node.end);
            return self.arena.add_interface(
                syntax_kind_ext::INTERFACE_DECLARATION,
                start_pos,
                end_pos,
                crate::parser::node::InterfaceData {
                    modifiers,
                    name,
                    type_parameters: None,
                    heritage_clauses: None,
                    members: Self::make_node_list(vec![]),
                },
            );
        }

        // Parse type parameters: interface IList<T> {}
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse heritage clauses (extends only for interfaces)
        // Interfaces can extend multiple types: interface A extends B, C, D { }
        let heritage_clauses = self.is_token(SyntaxKind::ExtendsKeyword).then(|| {
            let clause_start = self.token_pos();
            self.next_token();

            // TS1097: 'extends' list cannot be empty.
            if self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::ImplementsKeyword)
            {
                use tsz_common::diagnostics::diagnostic_codes;
                // Use full start position (including leading trivia) to match TSC's
                // grammarErrorAtPos(node, types.pos, 0, ...) which uses getTokenFullStart().
                self.parse_error_at(
                    self.token_full_start(),
                    0,
                    "'extends' list cannot be empty.",
                    diagnostic_codes::LIST_CANNOT_BE_EMPTY,
                );
                // Return an empty heritage clause so we can still parse the body
                let clause_end = self.token_end();
                let clause = self.arena.add_heritage(
                    syntax_kind_ext::HERITAGE_CLAUSE,
                    clause_start,
                    clause_end,
                    crate::parser::node::HeritageData {
                        token: SyntaxKind::ExtendsKeyword as u16,
                        types: Self::make_node_list(vec![]),
                    },
                );
                return Self::make_node_list(vec![clause]);
            }

            let mut types = Vec::new();
            loop {
                let type_ref = self.parse_interface_heritage_type_reference();
                types.push(type_ref);
                if !self.parse_optional(SyntaxKind::CommaToken) {
                    break;
                }
            }

            let clause_end = self.token_end();
            let clause = self.arena.add_heritage(
                syntax_kind_ext::HERITAGE_CLAUSE,
                clause_start,
                clause_end,
                crate::parser::node::HeritageData {
                    token: SyntaxKind::ExtendsKeyword as u16,
                    types: Self::make_node_list(types),
                },
            );
            Self::make_node_list(vec![clause])
        });

        // TS1176: Interface declaration cannot have 'implements' clause.
        // Parse the clause for recovery, treating it like extends.
        if self.is_token(SyntaxKind::ImplementsKeyword) {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Interface declaration cannot have 'implements' clause.",
                diagnostic_codes::INTERFACE_DECLARATION_CANNOT_HAVE_IMPLEMENTS_CLAUSE,
            );
            // Parse the implements types for error recovery (reuse extends parsing)
            self.next_token();
            while self.is_identifier_or_keyword() || self.is_token(SyntaxKind::CommaToken) {
                self.next_token();
                if self.is_token(SyntaxKind::LessThanToken) {
                    let _ = self.parse_type_arguments();
                }
            }
        }

        // Check for duplicate extends clause: interface I extends A extends B { }
        if self.is_token(SyntaxKind::ExtendsKeyword) {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "'extends' clause already seen.",
                diagnostic_codes::EXTENDS_CLAUSE_ALREADY_SEEN,
            );
            // Skip the duplicate extends and its types for recovery
            self.next_token();
            while self.is_identifier_or_keyword() || self.is_token(SyntaxKind::CommaToken) {
                self.next_token();
                if self.is_token(SyntaxKind::LessThanToken) {
                    // Skip type arguments
                    let _ = self.parse_type_arguments();
                }
            }
        }

        if has_invalid_numeric_name && self.is_token(SyntaxKind::OpenBraceToken) {
            use tsz_common::diagnostics::diagnostic_codes;
            let brace_pos = self.token_pos();
            self.parse_error_at(brace_pos, 1, "';' expected.", diagnostic_codes::EXPECTED);
        }
        if has_invalid_hard_keyword_name && self.is_token(SyntaxKind::OpenBraceToken) {
            use tsz_common::diagnostics::diagnostic_codes;
            let is_null_name = self
                .arena
                .get(name)
                .and_then(|name_node| self.arena.get_identifier(name_node))
                .is_some_and(|ident| ident.escaped_text == "null");
            if is_null_name {
                self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
            }
        }

        // Parse interface body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let saved_type_member_depth = self.type_member_container_depth;
        self.type_member_container_depth += 1;
        let members = self.parse_type_members();
        let end_pos = self.finish_type_member_container_close_brace();
        self.type_member_container_depth = saved_type_member_depth;
        // An interface parses no trailing separator, so clear any abandon-body
        // flag here rather than leak it to a later `type X = ...` alias (which
        // would then wrongly skip its own semicolon). See
        // `pending_type_member_body_reparse`.
        self.pending_type_member_body_reparse = false;
        self.arena.add_interface(
            syntax_kind_ext::INTERFACE_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::node::InterfaceData {
                modifiers,
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse type members (for interfaces and type literals)
    pub(crate) fn parse_type_members(&mut self) -> NodeList {
        let mut members = Vec::new();

        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let start_pos = self.token_pos();

            // Check for mapped type member: [identifier in ...] (TS 4.1+)
            if self.is_token(SyntaxKind::OpenBracketToken) && self.look_ahead_is_mapped_type_start()
            {
                let member = self.parse_mapped_type_member();
                if member.is_some() {
                    members.push(member);
                }
                self.parse_type_member_separator_with_asi();
                continue;
            }

            // A run of one or more type-member modifiers directly followed by a
            // `get`/`set` accessor is grammar-invalid in tsc: no modifier may precede
            // an accessor signature in an interface or type literal (unlike a plain
            // property/method, which accepts these modifiers and is validated
            // semantically as TS1024/TS1070/TS1071). tsc's parser cannot build any
            // member starting at the first modifier, reports one TS1131 per modifier
            // in the run (each anchored at its own token), and recovers by retrying
            // at the accessor keyword — which then parses as a bare (modifier-less)
            // accessor. See `look_ahead_modifier_run_before_accessor` for the exact
            // qualifying modifier set. Handled before the normal property/method path
            // so `get`/`set` is never mistaken for the property name and the
            // modifiers are never silently dropped (which previously produced a
            // misleading TS1005 or TS1070).
            // A run containing a "hard" modifier (`async`/`declare`/`abstract`/
            // `override`) before an accessor does not recover as a bare accessor
            // in tsc: after one TS1131 per modifier, tsc abandons the
            // type-member body and re-parses the accessor's own tail as
            // top-level statements (TS1434/TS1005/TS1128). Reproduced by
            // deferring the container close brace — the same mechanism
            // `recover_invalid_type_member` uses — so the tail is left for the
            // enclosing statement parser. Checked before the clean-only run
            // below because that helper stops at (and never counts) a hard
            // modifier, so the two are mutually exclusive.
            let hard_run_len = self.look_ahead_hard_modifier_run_before_accessor();
            if hard_run_len > 0 {
                self.report_hard_modifier_run_before_accessor(hard_run_len);
                break;
            }

            // The `out` variance modifier in the confined `[clean]* out
            // (get|set)` shape shares the hard-modifier abandon-body cascade
            // (`out` is a contextual keyword, so its statement re-parse is
            // byte-identical). See `look_ahead_clean_prefixed_out_before_accessor`
            // for the excluded stacked shapes and why `in` is not covered.
            let out_run_len = self.look_ahead_clean_prefixed_out_before_accessor();
            if out_run_len > 0 {
                self.report_hard_modifier_run_before_accessor(out_run_len);
                break;
            }

            // A hard modifier immediately followed by `out` then an accessor
            // (`async out get x()`) is a distinct shape from both checks
            // above: `out` itself is excluded from the reportable run and
            // falls into the abandoned-tail re-parse alongside the accessor
            // keyword. See `look_ahead_hard_modifier_then_out_before_accessor`.
            let hard_then_out_run_len = self.look_ahead_hard_modifier_then_out_before_accessor();
            if hard_then_out_run_len > 0 {
                self.report_hard_modifier_run_before_accessor(hard_then_out_run_len);
                break;
            }

            let modifier_run_len = self.look_ahead_modifier_run_before_accessor();
            if modifier_run_len > 0 {
                for _ in 0..modifier_run_len {
                    let mod_start = self.token_pos();
                    let mod_end = self.token_end();
                    self.next_token();
                    self.parse_error_at(
                        mod_start,
                        mod_end.saturating_sub(mod_start),
                        tsz_common::diagnostics::diagnostic_messages::PROPERTY_OR_SIGNATURE_EXPECTED,
                        tsz_common::diagnostics::diagnostic_codes::PROPERTY_OR_SIGNATURE_EXPECTED,
                    );
                }
                continue;
            }

            let member = self.parse_type_member(true);
            if member.is_some() {
                members.push(member);
            }

            if self.deferred_type_member_close_braces >= self.type_member_container_depth {
                break;
            }

            self.parse_type_member_separator_with_asi();

            // If we didn't make progress, emit TS1131 and skip tokens to avoid infinite loops.
            if self.token_pos() == start_pos && !self.is_token(SyntaxKind::CloseBraceToken) {
                self.parse_error_at_current_token(
                    tsz_common::diagnostics::diagnostic_messages::PROPERTY_OR_SIGNATURE_EXPECTED,
                    tsz_common::diagnostics::diagnostic_codes::PROPERTY_OR_SIGNATURE_EXPECTED,
                );

                // `var` declarations are not valid type members. tsc recovers by
                // abandoning the malformed member tail (`var x: T<>;`) and then
                // surfacing a declaration-level TS1128 at the following `}`.
                if self.is_token(SyntaxKind::VarKeyword) {
                    self.next_token(); // consume `var`
                    while !matches!(
                        self.token(),
                        SyntaxKind::SemicolonToken
                            | SyntaxKind::CloseBraceToken
                            | SyntaxKind::EndOfFileToken
                    ) {
                        self.next_token();
                    }
                    if self.is_token(SyntaxKind::SemicolonToken) {
                        self.next_token();
                    }
                    if self.is_token(SyntaxKind::CloseBraceToken) {
                        self.parse_error_at_current_token(
                            tsz_common::diagnostics::diagnostic_messages::DECLARATION_OR_STATEMENT_EXPECTED,
                            tsz_common::diagnostics::diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                        );
                    }
                } else {
                    self.next_token();
                }
            }
        }

        Self::make_node_list(members)
    }

    /// Parse a single type member (property signature, method signature, call signature, construct signature)
    pub(crate) fn parse_type_member(&mut self, in_interface_declaration: bool) -> NodeIndex {
        let start_pos = self.token_pos();
        let (modifier_diagnostic_reported, explicit) =
            self.parse_type_member_explicit_signature(start_pos);
        if let Some(member) = explicit {
            member
        } else {
            self.parse_type_member_property_or_method(
                start_pos,
                in_interface_declaration,
                modifier_diagnostic_reported,
            )
        }
    }

    /// Returns `(modifier_diagnostic_reported, node)`. `modifier_diagnostic_reported`
    /// tracks whether an illegal-modifier (TS1070/TS1071) or `async` (TS1070)
    /// diagnostic already fired for this member's leading-modifier run — `tsc`
    /// reports at most one such diagnostic per member and returns immediately, so
    /// `parse_type_member_property_or_method`'s own `readonly`-on-method (TS1024)
    /// check must be suppressed once this is `true`.
    fn parse_type_member_explicit_signature(
        &mut self,
        start_pos: u32,
    ) -> (bool, Option<NodeIndex>) {
        let (illegal_modifier_reported, node) =
            self.parse_type_member_visibility_modifier_error(start_pos);
        if node.is_some() {
            return (illegal_modifier_reported, node);
        }

        // A trailing `async` is never legal here regardless of an earlier
        // illegal modifier, so it must still be consumed — but once an illegal
        // modifier (e.g. `static`) already reported, `tsc` never reports a
        // second diagnostic for the same member (`static async x(): number` is
        // TS1070 once, at `static`, not twice).
        let async_found = self.parse_async_type_member_restriction(!illegal_modifier_reported);
        let modifier_diagnostic_reported = illegal_modifier_reported || async_found;

        if self.is_token(SyntaxKind::LessThanToken) {
            return (
                modifier_diagnostic_reported,
                Some(self.parse_call_signature(start_pos)),
            );
        }
        if self.is_token(SyntaxKind::OpenParenToken) {
            return (
                modifier_diagnostic_reported,
                Some(self.parse_call_signature(start_pos)),
            );
        }

        if self.is_token(SyntaxKind::NewKeyword) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            let is_property_name = self.is_token(SyntaxKind::ColonToken)
                || self.is_token(SyntaxKind::QuestionToken)
                || self.is_token(SyntaxKind::SemicolonToken)
                || self.is_token(SyntaxKind::CommaToken)
                || self.is_token(SyntaxKind::CloseBraceToken);
            self.scanner.restore_state(snapshot);
            self.current_token = current;
            if !is_property_name {
                return (
                    modifier_diagnostic_reported,
                    Some(self.parse_construct_signature(start_pos)),
                );
            }
        }

        if self.is_token(SyntaxKind::GetKeyword)
            && !self.look_ahead_is_property_name_after_keyword()
        {
            return (
                modifier_diagnostic_reported,
                Some(self.parse_get_accessor_signature(start_pos)),
            );
        }

        if self.is_token(SyntaxKind::SetKeyword)
            && !self.look_ahead_is_property_name_after_keyword()
        {
            return (
                modifier_diagnostic_reported,
                Some(self.parse_set_accessor_signature(start_pos)),
            );
        }

        (modifier_diagnostic_reported, None)
    }

    fn parse_type_member_property_or_method(
        &mut self,
        start_pos: u32,
        in_interface_declaration: bool,
        modifier_diagnostic_reported: bool,
    ) -> NodeIndex {
        // Capture the `readonly` keyword span so a TS1024 (readonly on a
        // method/construct signature) can be anchored at the modifier itself,
        // matching tsc's `checkGrammarModifiers`.
        let readonly_span = if self.is_token(SyntaxKind::ReadonlyKeyword)
            && !self.look_ahead_is_property_name_after_keyword()
        {
            let span = (self.token_pos(), self.token_end());
            self.next_token();
            Some(span)
        } else {
            None
        };
        let readonly = readonly_span.is_some();

        // `readonly async x(): number` / `readonly static m(): number` /
        // `readonly readonly x: number` / etc: any second leading modifier
        // written directly after `readonly` — including a repeated
        // `readonly` itself — is not the member name. `tsc` checks each
        // modifier in source order and reports (and immediately stops at)
        // the first violation: `readonly` on a method/construct signature
        // (TS1024, checked below once the member kind is known), a second
        // `readonly` once the first was already legal (TS1030), or any other
        // modifier, which is always illegal on a type member (TS1070).
        // Detected here, before name parsing, so the offending token is
        // never misread as the property name (oracle-verified: it is not a
        // valid property-name continuation in this position, e.g. `readonly
        // static: number` — `static` used AS the name — stays clean, guarded
        // by the same `look_ahead_is_property_name_after_keyword` check used
        // for the leading-modifier case). It must still be consumed even
        // when `modifier_diagnostic_reported` is already true (an earlier
        // illegal modifier fired) — only the diagnostic, not the consumption,
        // is suppressed in that case.
        let second_modifier_span = if readonly
            && (self.is_token(SyntaxKind::AsyncKeyword)
                || self.is_token(SyntaxKind::ReadonlyKeyword)
                || Self::is_illegal_type_member_modifier(self.token()))
            && !self.look_ahead_is_property_name_after_keyword()
        {
            let kind = if self.is_token(SyntaxKind::ReadonlyKeyword) {
                SecondTypeMemberModifier::DuplicateReadonly
            } else {
                SecondTypeMemberModifier::Illegal(self.scanner.get_token_text())
            };
            let span = (self.token_pos(), self.token_end());
            self.next_token();

            // A longer chain (`readonly async static x`, `readonly readonly
            // static x`, `readonly static readonly x`, ...) still reports a
            // single diagnostic naming only the first offender — every
            // modifier past it, including further repeated `readonly`s, must
            // still be consumed silently so the member parses cleanly,
            // matching `parse_type_member_visibility_modifier_error`'s
            // equivalent "consume the whole run" step for the
            // illegal-modifier-first case.
            while (self.is_token(SyntaxKind::AsyncKeyword)
                || self.is_token(SyntaxKind::ReadonlyKeyword)
                || Self::is_illegal_type_member_modifier(self.token()))
                && !self.look_ahead_is_property_name_after_keyword()
            {
                self.next_token();
            }

            if modifier_diagnostic_reported {
                None
            } else {
                Some((span, kind))
            }
        } else {
            None
        };

        let Some(name) = self.parse_type_member_property_or_method_name(start_pos, readonly) else {
            return NodeIndex::NONE;
        };

        let name = match name {
            TypeMemberPropertyOrMethodName::IndexSignature(index_signature) => {
                // An index signature can never be a method, so `readonly`'s
                // own TS1024 (readonly-on-method) never applies here — the
                // second-modifier violation (if any) always gets to report.
                self.report_type_member_second_modifier(second_modifier_span);
                return index_signature;
            }
            TypeMemberPropertyOrMethodName::Property(name) => name,
        };

        let question_token = self.parse_optional(SyntaxKind::QuestionToken);
        let modifiers = self.readonly_modifier_node_list(start_pos, readonly);

        if self.is_token(SyntaxKind::OpenParenToken) || self.is_token(SyntaxKind::LessThanToken) {
            // `readonly` is legal only on a property declaration or index
            // signature; on a method or construct signature tsc reports TS1024,
            // anchored at the `readonly` keyword, and (per single-diagnostic-per-
            // member) never separately reports a trailing second modifier.
            if !modifier_diagnostic_reported && let Some((ro_start, ro_end)) = readonly_span {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    ro_start,
                    ro_end.saturating_sub(ro_start),
                    "'readonly' modifier can only appear on a property declaration or index signature.",
                    diagnostic_codes::READONLY_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION_OR_INDEX_SIGNATURE,
                );
            }
            return self.parse_type_member_method_signature(
                start_pos,
                name,
                modifiers,
                question_token,
            );
        }

        // Property: `readonly` (if present) is legal here, so the second
        // modifier (if present) is the first — and only — offending one.
        self.report_type_member_second_modifier(second_modifier_span);

        self.parse_type_member_property_signature(
            start_pos,
            name,
            modifiers,
            question_token,
            in_interface_declaration,
        )
    }

    /// Reports the diagnostic for a type member's second leading modifier
    /// (the token right after `readonly`, if it wasn't the member name) —
    /// TS1030 for a repeated `readonly`, TS1070 for anything else. A `None`
    /// span means no such modifier was present, or `readonly`'s own TS1024
    /// (readonly-on-method) already claimed the member's one diagnostic.
    fn report_type_member_second_modifier(
        &mut self,
        second_modifier_span: Option<((u32, u32), SecondTypeMemberModifier)>,
    ) {
        let Some(((mod_start, mod_end), kind)) = second_modifier_span else {
            return;
        };
        use tsz_common::diagnostics::diagnostic_codes;
        let len = mod_end.saturating_sub(mod_start);
        match kind {
            SecondTypeMemberModifier::DuplicateReadonly => {
                self.parse_error_at(
                    mod_start,
                    len,
                    "'readonly' modifier already seen.",
                    diagnostic_codes::MODIFIER_ALREADY_SEEN,
                );
            }
            SecondTypeMemberModifier::Illegal(modifier_text) => {
                self.parse_error_at(
                    mod_start,
                    len,
                    &format!("'{modifier_text}' modifier cannot appear on a type member."),
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER,
                );
            }
        }
    }

    fn parse_type_member_property_or_method_name(
        &mut self,
        start_pos: u32,
        readonly: bool,
    ) -> Option<TypeMemberPropertyOrMethodName> {
        if self.is_token(SyntaxKind::PrivateIdentifier) {
            // TS18016: Private identifiers are not allowed outside class bodies.
            // Parse the private identifier so the member is well-formed, but emit a diagnostic.
            let name = self.parse_property_name();
            if let Some(name_node) = self.arena.get(name) {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    name_node.pos,
                    name_node.end - name_node.pos,
                    "Private identifiers are not allowed outside class bodies.",
                    diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
                );
            }
            Some(TypeMemberPropertyOrMethodName::Property(name))
        } else if self.is_token(SyntaxKind::Identifier)
            || self.is_token(SyntaxKind::StringLiteral)
            || self.is_token(SyntaxKind::NumericLiteral)
            || self.is_token(SyntaxKind::BigIntLiteral)
            || self.is_identifier_or_keyword()
        {
            // Lookahead: match tsc's isTypeMemberStart() — after consuming the property name,
            // the next token must be a valid type member continuation token (`:`, `?`, `(`, `<`,
            // `,`, or ASI-eligible). Without this check, keywords like `return` in
            // `{ return true; }` would be greedily parsed as property names.
            let snapshot = self.scanner.save_state();
            let saved_token = self.current_token;
            self.next_token(); // skip past the property name
            let is_valid_continuation = matches!(
                self.current_token,
                SyntaxKind::OpenParenToken
                    | SyntaxKind::LessThanToken
                    | SyntaxKind::QuestionToken
                    | SyntaxKind::ColonToken
                    | SyntaxKind::CommaToken
            ) || self.can_parse_semicolon();
            self.scanner.restore_state(snapshot);
            self.current_token = saved_token;

            if is_valid_continuation {
                Some(TypeMemberPropertyOrMethodName::Property(
                    self.parse_property_name(),
                ))
            } else {
                None
            }
        } else if self.is_token(SyntaxKind::OpenBracketToken) {
            if self.look_ahead_is_index_signature() || self.look_ahead_is_empty_index_signature() {
                let modifiers = self.readonly_modifier_node_list(start_pos, readonly);
                Some(TypeMemberPropertyOrMethodName::IndexSignature(
                    self.parse_index_signature_with_modifiers(modifiers, start_pos),
                ))
            } else {
                Some(TypeMemberPropertyOrMethodName::Property(
                    self.parse_property_name(),
                ))
            }
        } else {
            None
        }
    }

    fn readonly_modifier_node_list(
        &mut self,
        start_pos: u32,
        is_readonly: bool,
    ) -> Option<NodeList> {
        is_readonly.then(|| {
            let mod_idx = self
                .arena
                .create_modifier(SyntaxKind::ReadonlyKeyword, start_pos);
            Self::make_node_list(vec![mod_idx])
        })
    }

    fn parse_type_member_method_signature(
        &mut self,
        start_pos: u32,
        name: NodeIndex,
        modifiers: Option<NodeList>,
        question_token: bool,
    ) -> NodeIndex {
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // TS1005: method signatures cannot place `?` after the parameter list.
        // tsc reports that at `?`, then reports TS1131 at a following `:`.
        if self.is_token(SyntaxKind::QuestionToken) {
            self.parse_error_at_current_token(
                "';' expected.",
                tsz_common::diagnostics::diagnostic_codes::EXPECTED,
            );
            self.next_token();
            if self.is_token(SyntaxKind::ColonToken) {
                self.parse_error_at_current_token(
                    tsz_common::diagnostics::diagnostic_messages::PROPERTY_OR_SIGNATURE_EXPECTED,
                    tsz_common::diagnostics::diagnostic_codes::PROPERTY_OR_SIGNATURE_EXPECTED,
                );
            }
        }

        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();
        self.arena.add_signature(
            syntax_kind_ext::METHOD_SIGNATURE,
            start_pos,
            end_pos,
            crate::parser::node::SignatureData {
                modifiers,
                name,
                question_token,
                type_parameters,
                parameters: Some(parameters),
                type_annotation,
            },
        )
    }

    fn parse_type_member_property_signature(
        &mut self,
        start_pos: u32,
        name: NodeIndex,
        modifiers: Option<NodeList>,
        question_token: bool,
        in_interface_declaration: bool,
    ) -> NodeIndex {
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        if self.parse_optional(SyntaxKind::EqualsToken) {
            use tsz_common::diagnostics::diagnostic_codes;
            let (message, code) = if in_interface_declaration {
                (
                    "An interface property cannot have an initializer.",
                    diagnostic_codes::AN_INTERFACE_PROPERTY_CANNOT_HAVE_AN_INITIALIZER,
                )
            } else {
                (
                    "A type literal property cannot have an initializer.",
                    diagnostic_codes::A_TYPE_LITERAL_PROPERTY_CANNOT_HAVE_AN_INITIALIZER,
                )
            };
            self.parse_error_at_current_token(message, code);
            self.parse_assignment_expression();
        }

        let end_pos = self.token_end();
        self.arena.add_signature(
            syntax_kind_ext::PROPERTY_SIGNATURE,
            start_pos,
            end_pos,
            crate::parser::node::SignatureData {
                modifiers,
                name,
                question_token,
                type_parameters: None,
                parameters: None,
                type_annotation,
            },
        )
    }

    /// Parse call signature: (): returnType or <T>(): returnType
    pub(crate) fn parse_call_signature(&mut self, start_pos: u32) -> NodeIndex {
        // Parse optional type parameters: <T, U>
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        let has_open_paren = self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if has_open_paren {
            let parameters = self.parse_parameter_list();
            self.parse_expected(SyntaxKind::CloseParenToken);
            parameters
        } else {
            Self::make_node_list(vec![])
        };

        // TS1005: call signatures cannot be optional — emit "';' expected." at '?'
        // Do NOT skip '?' — let the member parsing loop handle recovery so it emits TS1131
        if self.is_token(SyntaxKind::QuestionToken) {
            self.parse_error_at_current_token(
                "';' expected.",
                tsz_common::diagnostics::diagnostic_codes::EXPECTED,
            );
        }

        // Return type (supports type predicates: param is T)
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
            // tsc reports `':' expected` for `(args) => T` in type members.
            self.parse_error_at_current_token(
                "':' expected.",
                tsz_common::diagnostics::diagnostic_codes::EXPECTED,
            );
            self.next_token(); // consume `=>` and recover by parsing the return type
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();
        self.arena.add_signature(
            syntax_kind_ext::CALL_SIGNATURE,
            start_pos,
            end_pos,
            crate::parser::node::SignatureData {
                modifiers: None,
                name: NodeIndex::NONE,
                question_token: false,
                type_parameters,
                parameters: Some(parameters),
                type_annotation,
            },
        )
    }

    /// Parse construct signature: new (): returnType or new <T>(): returnType
    pub(crate) fn parse_construct_signature(&mut self, start_pos: u32) -> NodeIndex {
        self.parse_expected(SyntaxKind::NewKeyword);

        // Parse optional type parameters: new <T>()
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        let has_open_paren = self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if has_open_paren {
            let parameters = self.parse_parameter_list();
            self.parse_expected(SyntaxKind::CloseParenToken);
            parameters
        } else {
            Self::make_node_list(vec![])
        };

        // TS1005: construct signatures cannot be optional — emit "';' expected." at '?'
        // Do NOT skip '?' — let the member parsing loop handle recovery so it emits TS1131
        if self.is_token(SyntaxKind::QuestionToken) {
            self.parse_error_at_current_token(
                "';' expected.",
                tsz_common::diagnostics::diagnostic_codes::EXPECTED,
            );
        }

        // Return type (supports type predicates)
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
            // tsc reports `':' expected` for `new (...) => T` in type members.
            self.parse_error_at_current_token(
                "':' expected.",
                tsz_common::diagnostics::diagnostic_codes::EXPECTED,
            );
            self.next_token(); // consume `=>` and recover by parsing the return type
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();
        self.arena.add_signature(
            syntax_kind_ext::CONSTRUCT_SIGNATURE,
            start_pos,
            end_pos,
            crate::parser::node::SignatureData {
                modifiers: None,
                name: NodeIndex::NONE,
                question_token: false,
                type_parameters,
                parameters: Some(parameters),
                type_annotation,
            },
        )
    }

    /// Parse index signature with modifiers (static, readonly, etc.): static [key: string]: value
    ///
    /// Handles malformed index signatures with rest params (`...`), optional params (`?`),
    /// initializers (`= expr`), and multiple params — emitting the same error codes as tsc.
    pub(crate) fn parse_index_signature_with_modifiers(
        &mut self,
        modifiers: Option<NodeList>,
        start_pos: u32,
    ) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        let bracket_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBracketToken);

        // TS1096: empty index signature `[]` — no parameters at all
        if self.is_token(SyntaxKind::CloseBracketToken) {
            // TSC emits this as grammarErrorOnNode(node, ...) in checkGrammarIndexSignatureParameters,
            // which uses the full index signature node span starting at `[`.
            // Use bracket_pos to match TSC's position.
            let bracket_end = self.token_pos(); // position of `]`
            self.parse_error_at(
                bracket_pos,
                bracket_end - bracket_pos,
                "An index signature must have exactly one parameter.",
                diagnostic_codes::AN_INDEX_SIGNATURE_MUST_HAVE_EXACTLY_ONE_PARAMETER,
            );
            self.next_token(); // consume `]`

            // Still need the type annotation
            let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
                self.parse_type()
            } else {
                NodeIndex::NONE
            };

            let end_pos = self.token_end();
            return self.arena.add_index_signature(
                syntax_kind_ext::INDEX_SIGNATURE,
                start_pos,
                end_pos,
                crate::parser::node::IndexSignatureData {
                    modifiers,
                    parameters: Self::make_node_list(vec![]),
                    type_annotation,
                    had_parameter_arity_error: true,
                },
            );
        }

        // Parse first parameter, handling malformed forms
        let param_start = self.token_pos();

        // TS1018: accessibility modifier on parameter
        // Collect modifiers without emitting error - we'll emit at param name position
        let mut has_accessibility_modifier = false;
        let mut param_modifiers = Vec::new();
        while self.is_valid_parameter_modifier() {
            param_modifiers.push(
                self.arena
                    .create_modifier(self.current_token, self.token_pos()),
            );
            has_accessibility_modifier = true;
            self.next_token();
        }

        // TS1017: rest parameter in index signature
        let dot_dot_dot_token = self.parse_optional(SyntaxKind::DotDotDotToken);
        if dot_dot_dot_token {
            self.parse_error_at(
                param_start,
                3,
                "An index signature cannot have a rest parameter.",
                diagnostic_codes::AN_INDEX_SIGNATURE_CANNOT_HAVE_A_REST_PARAMETER,
            );
        }

        let param_name = self.parse_identifier();

        // TS1018: accessibility modifier on parameter - emit at param name position
        if has_accessibility_modifier {
            if let Some(name_node) = self.arena.get(param_name) {
                self.parse_error_at(
                    name_node.pos,
                    name_node.end - name_node.pos,
                    "An index signature parameter cannot have an accessibility modifier.",
                    diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_AN_ACCESSIBILITY_MODIFIER,
                );
            } else {
                // Fallback if we can't get the node
                self.parse_error_at_current_token(
                    "An index signature parameter cannot have an accessibility modifier.",
                    diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_AN_ACCESSIBILITY_MODIFIER,
                );
            }
        }

        // TS1019: optional parameter in index signature
        let question_token = self.parse_optional(SyntaxKind::QuestionToken);
        if question_token {
            let q_end = self.token_pos();
            self.parse_error_at(
                q_end - 1,
                1,
                "An index signature parameter cannot have a question mark.",
                diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_A_QUESTION_MARK,
            );
        }

        // Parse colon and parameter type.
        // If the next token is `]` or `,`, skip — the signature is malformed
        // (e.g., `[...a]`, `[a?]`, or `[a, b]`) and other errors will be reported.
        let (_param_type_token, param_type) = if self.is_token(SyntaxKind::CloseBracketToken)
            || self.is_token(SyntaxKind::CommaToken)
        {
            (self.token(), NodeIndex::NONE)
        } else {
            self.parse_expected(SyntaxKind::ColonToken);
            let tok = self.token();
            let ty = self.parse_type();
            (tok, ty)
        };

        // TS1020: initializer in index signature - emit at param name position
        let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
            let init = self.parse_assignment_expression();
            // TSC emits error at parameter name position, not initializer position
            if let Some(name_node) = self.arena.get(param_name) {
                self.parse_error_at(
                    name_node.pos,
                    name_node.end - name_node.pos,
                    "An index signature parameter cannot have an initializer.",
                    diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_AN_INITIALIZER,
                );
            } else {
                self.parse_error_at_current_token(
                    "An index signature parameter cannot have an initializer.",
                    diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_AN_INITIALIZER,
                );
            }
            init
        } else {
            NodeIndex::NONE
        };

        let param_end = self.token_end();

        // Handle comma after first parameter - could be trailing comma (TS1025) or multiple params (TS1096)
        let mut has_multiple_params = false;
        let mut has_trailing_comma = false;
        let mut trailing_comma_pos = 0;
        let comma_pos = self.token_pos(); // Position of comma before consuming it
        if self.parse_optional(SyntaxKind::CommaToken) {
            // Save the comma position for TS1025 error
            trailing_comma_pos = comma_pos;
            // Check if this is a trailing comma (comma followed by `]`)
            if self.is_token(SyntaxKind::CloseBracketToken) {
                has_trailing_comma = true;
            } else {
                has_multiple_params = true;
                // Consume remaining parameters for recovery
                while !self.is_token(SyntaxKind::CloseBracketToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    // Skip rest token
                    self.parse_optional(SyntaxKind::DotDotDotToken);
                    if self.is_identifier_or_keyword() {
                        self.next_token();
                    }
                    // Skip optional marker
                    self.parse_optional(SyntaxKind::QuestionToken);
                    // Skip type annotation
                    if self.parse_optional(SyntaxKind::ColonToken) {
                        let _ = self.parse_type();
                    }
                    // Skip initializer
                    if self.parse_optional(SyntaxKind::EqualsToken) {
                        let _ = self.parse_assignment_expression();
                    }
                    if !self.parse_optional(SyntaxKind::CommaToken) {
                        break;
                    }
                }
            }
        }

        if has_multiple_params {
            // TSC emits grammarErrorOnNode(parameter.name, ...) — pointing at the
            // first parameter's name, not at the end of the parameter list.
            if let Some(name_node) = self.arena.get(param_name) {
                self.parse_error_at(
                    name_node.pos,
                    name_node.end - name_node.pos,
                    "An index signature must have exactly one parameter.",
                    diagnostic_codes::AN_INDEX_SIGNATURE_MUST_HAVE_EXACTLY_ONE_PARAMETER,
                );
            } else {
                self.parse_error_at_current_token(
                    "An index signature must have exactly one parameter.",
                    diagnostic_codes::AN_INDEX_SIGNATURE_MUST_HAVE_EXACTLY_ONE_PARAMETER,
                );
            }
        }

        // TS1025: trailing comma in index signature
        if has_trailing_comma {
            self.parse_error_at(
                trailing_comma_pos,
                1, // Length of the comma
                "An index signature cannot have a trailing comma.",
                diagnostic_codes::AN_INDEX_SIGNATURE_CANNOT_HAVE_A_TRAILING_COMMA,
            );
        }

        self.parse_expected(SyntaxKind::CloseBracketToken);

        // TS1005: index signatures cannot be optional — emit "';' expected." at '?'
        // Skip '?' but abort type annotation parsing — leave `: any;` for the member loop
        // to handle, so TS1131 is emitted at the right position (at `:`, not at `?`).
        let saw_question_after_bracket = if self.is_token(SyntaxKind::QuestionToken) {
            self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
            self.next_token(); // skip '?'
            true
        } else {
            false
        };

        // Parse the type annotation after `]`.
        // TS1021 (missing type annotation) is checked by the checker, not the parser,
        // matching TSC's checkGrammarIndexSignatureParameters which uses early returns
        // to suppress TS1021 when other grammar errors are present.
        let type_annotation = if saw_question_after_bracket {
            // When `?` was after `]`, don't parse the type annotation.
            // The remaining `: any;` will be handled by the member loop which emits TS1131.
            NodeIndex::NONE
        } else if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        if self.is_token(SyntaxKind::IsKeyword) {
            // `[index: number]: p1 is C;` is not a valid index-signature type.
            // TSC reports the missing separator at `is` and then recovers the
            // invalid tail as ordinary statements after the interface body is
            // abandoned. Leave the `is C` tokens in the stream so emit can
            // preserve the recovered expression statements.
            self.error_token_expected(";");
            self.deferred_type_member_close_braces = self
                .deferred_type_member_close_braces
                .max(self.type_member_container_depth);
        }

        let param_node = self.arena.add_parameter(
            syntax_kind_ext::PARAMETER,
            param_start,
            param_end,
            ParameterData {
                modifiers: if param_modifiers.is_empty() {
                    None
                } else {
                    Some(Self::make_node_list(param_modifiers))
                },
                dot_dot_dot_token,
                name: param_name,
                question_token,
                type_annotation: param_type,
                initializer,
            },
        );

        let end_pos = self.token_end();
        self.arena.add_index_signature(
            syntax_kind_ext::INDEX_SIGNATURE,
            start_pos,
            end_pos,
            crate::parser::node::IndexSignatureData {
                modifiers,
                parameters: Self::make_node_list(vec![param_node]),
                type_annotation,
                had_parameter_arity_error: has_multiple_params,
            },
        )
    }

    /// Parse get accessor signature in type context: get `foo()`: type
    /// Note: TypeScript allows bodies here (which is an error), so we parse them for error recovery
    ///
    /// The parameter list is parsed, not asserted empty. tsc's grammar for an
    /// accessor is keyed on the accessor node, not on its container, so an
    /// accessor in a type-member list accepts exactly the same parameter list as
    /// one in a class body and reports the same grammar errors on it. Asserting
    /// `()` here made the first parameter token terminate the member, which
    /// turned every such signature into a `TS1005`/`TS1131` parse cascade.
    pub(crate) fn parse_get_accessor_signature(&mut self, start_pos: u32) -> NodeIndex {
        self.parse_expected(SyntaxKind::GetKeyword);

        let name = self.parse_property_name();

        let type_parameters = self.is_token(SyntaxKind::LessThanToken).then(|| {
            self.report_accessor_type_parameters_error(name);
            self.parse_type_parameters()
        });

        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if self.is_token(SyntaxKind::CloseParenToken) {
            Self::make_node_list(vec![])
        } else {
            self.parse_parameter_list()
        };
        self.parse_expected(SyntaxKind::CloseParenToken);

        // TS1054: reported after the list is parsed, so a `,` in an otherwise
        // empty slot (`get x(,)`) yields the `TS1138` its own slot already
        // emitted and no arity error, matching tsc.
        self.report_get_accessor_parameter_count(name, &parameters);

        // Return type (supports type predicates)
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        // Parse body if present (this is an error in type context, but we handle it)
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();
        self.arena.add_accessor(
            syntax_kind_ext::GET_ACCESSOR,
            start_pos,
            end_pos,
            crate::parser::node::AccessorData {
                modifiers: None,
                name,
                type_parameters,
                parameters,
                type_annotation,
                body,
            },
        )
    }

    /// Parse set accessor signature in type context: set foo(v: type)
    /// Note: TypeScript allows bodies here (which is an error), so we parse them for error recovery
    ///
    /// Carries the same accessor grammar as a `set` accessor in a class body —
    /// `TS1094`, `TS1049`, `TS1051` and `TS1095` — because tsc keys that grammar
    /// on the accessor node rather than on its container.
    pub(crate) fn parse_set_accessor_signature(&mut self, start_pos: u32) -> NodeIndex {
        self.parse_expected(SyntaxKind::SetKeyword);

        let name = self.parse_property_name();

        let type_parameters = self.is_token(SyntaxKind::LessThanToken).then(|| {
            self.report_accessor_type_parameters_error(name);
            self.parse_type_parameters()
        });

        let had_open_paren = self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // TS1049, and the early return that suppresses the later `set`-specific
        // grammar once the count is already wrong.
        let count_error =
            had_open_paren && self.report_set_accessor_parameter_count(name, &parameters);

        self.report_set_accessor_optional_parameter(&parameters, count_error);

        // Parse the return type annotation for error recovery even though a
        // setter cannot legally carry one; the emitter preserves it.
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.report_set_accessor_return_type_annotation(name, count_error);
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        // Parse body if present (this is an error in type context, but we handle it)
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();
        self.arena.add_accessor(
            syntax_kind_ext::SET_ACCESSOR,
            start_pos,
            end_pos,
            crate::parser::node::AccessorData {
                modifiers: None,
                name,
                type_parameters,
                parameters,
                type_annotation,
                body,
            },
        )
    }

    /// Parse type alias declaration: type Foo = ... or type Foo<T> = ...
    pub(crate) fn parse_type_alias_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_type_alias_declaration_with_modifiers(start_pos, None)
    }

    pub(crate) fn parse_type_alias_declaration_with_modifiers(
        &mut self,
        start_pos: u32,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::TypeKeyword);
        let mut has_invalid_numeric_name = false;

        // TS1142: Line break not permitted between `type` and the alias name.
        // When `declare type\nT1 = ...` has a newline, tsc still parses it as a
        // type alias but emits TS1142. Without modifiers, the lookahead in
        // look_ahead_is_type_alias_declaration prevents reaching here, but the
        // `declare` path bypasses that lookahead.
        if self.scanner.has_preceding_line_break() {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Line break not permitted here.",
                diagnostic_codes::LINE_BREAK_NOT_PERMITTED_HERE,
            );
        }

        // For `type void = ...`, TSC accepts `void` as the identifier name
        // and emits TS1109 "Expression expected" from the parser (the checker
        // separately emits TS2457 "Type alias name cannot be 'void'").
        // We must not fall through to parse_identifier() which would emit TS1359.
        let name = if self.is_token(SyntaxKind::VoidKeyword) {
            let id_start = self.token_pos();
            let id_end = self.token_end();
            let atom = self.scanner.get_token_atom();
            let text = self.scanner.token_ident_text();
            self.next_token(); // consume `void`
            // Emit TS1109 at the `=` position (matching TSC behavior)
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.parse_error_at_current_token(
                diagnostic_messages::EXPRESSION_EXPECTED,
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
            self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                id_start,
                id_end,
                crate::parser::node::IdentifierData {
                    atom,
                    escaped_text: text,
                    original_text: None,
                },
            )
        } else if self.is_token(SyntaxKind::NumericLiteral) {
            use tsz_common::diagnostics::diagnostic_codes;
            let id_start = self.token_pos();
            let id_end = self.token_end();
            let text = self.scanner.get_token_value();
            self.parse_error_at(
                id_start,
                id_end - id_start,
                &format!("Type alias name cannot be '{text}'."),
                diagnostic_codes::TYPE_ALIAS_NAME_CANNOT_BE,
            );
            self.next_token();
            has_invalid_numeric_name = true;
            self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                id_start,
                id_end,
                crate::parser::node::IdentifierData {
                    atom: AstAtom::NONE,
                    escaped_text: IdentText::empty(),
                    original_text: None,
                },
            )
        } else {
            self.parse_identifier()
        };

        // Parse optional type parameters: <T, U extends Foo>
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        if has_invalid_numeric_name {
            use tsz_common::diagnostics::diagnostic_codes;
            if self.is_token(SyntaxKind::OpenBraceToken) {
                let brace_pos = self.token_pos();
                self.parse_error_at(brace_pos, 1, "';' expected.", diagnostic_codes::EXPECTED);
                let _ = self.parse_block();
            }
            self.parse_semicolon();
            let end_pos = self.token_full_start();
            return self.arena.add_type_alias(
                syntax_kind_ext::TYPE_ALIAS_DECLARATION,
                start_pos,
                end_pos,
                crate::parser::node::TypeAliasData {
                    modifiers,
                    name,
                    type_parameters,
                    type_node: NodeIndex::NONE,
                },
            );
        }

        // Parse expected equals token, but recover gracefully if missing
        // If the next token can start a type (e.g., {, (, [), emit error and continue parsing
        if self.is_token(SyntaxKind::EqualsToken) {
            self.next_token(); // Consume the equals token
        } else {
            // Emit TS1005 for missing equals token
            self.error_token_expected("=");
            // If the next token looks like a type, continue parsing anyway
            if !self.can_token_start_type() {
                // Can't recover, return early with a dummy type
                let end_pos = self.token_end();
                return self.arena.add_type_alias(
                    syntax_kind_ext::TYPE_ALIAS_DECLARATION,
                    start_pos,
                    end_pos,
                    crate::parser::node::TypeAliasData {
                        modifiers,
                        name,
                        type_parameters,
                        type_node: NodeIndex::NONE,
                    },
                );
            }
        }

        let type_node = self.parse_type();

        // When the alias's type is a `{ ... }` literal that abandoned its body
        // mid-parse (a hard modifier before an accessor), the leftover tokens
        // belong to the enclosing statement list, not the alias — re-parsed as
        // statements (TS1434/TS1005/TS1128). Requiring a separator here would
        // emit a spurious TS1005 at those tokens, so take the flag and skip.
        if !std::mem::take(&mut self.pending_type_member_body_reparse) {
            self.parse_semicolon();
        }

        let end_pos = self.token_full_start();
        self.arena.add_type_alias(
            syntax_kind_ext::TYPE_ALIAS_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::node::TypeAliasData {
                modifiers,
                name,
                type_parameters,
                type_node,
            },
        )
    }

    // =========================================================================
    // Module/Namespace Declarations
    // =========================================================================

    /// Parse ambient declaration: declare function/class/namespace/var/etc.
    pub(crate) fn parse_ambient_declaration(&mut self) -> NodeIndex {
        self.parse_ambient_declaration_with_modifiers(Vec::new())
    }

    pub(crate) fn parse_ambient_declaration_with_modifiers(
        &mut self,
        prefix_modifiers: Vec<NodeIndex>,
    ) -> NodeIndex {
        let start_pos = self.token_pos();

        // Create declare modifier node
        let declare_start = self.token_pos();
        self.parse_expected(SyntaxKind::DeclareKeyword);
        let declare_end = self.token_end();
        let declare_modifier = self.arena.add_token(
            SyntaxKind::DeclareKeyword as u16,
            declare_start,
            declare_end,
        );

        // Combine prefix modifiers (like export) with declare modifier
        let mut all_modifiers = prefix_modifiers;
        all_modifiers.push(declare_modifier);

        // Consume any redundant `declare` modifiers (`declare declare const x`).
        // tsc reports TS1030 ("'declare' modifier already seen") at each extra
        // `declare` keyword and parses the trailing ambient declaration as usual.
        while self.is_token(SyntaxKind::DeclareKeyword) {
            let dup_modifier = self.consume_modifier_with_error(
                SyntaxKind::DeclareKeyword,
                "'declare' modifier already seen.",
                tsz_common::diagnostics::diagnostic_codes::MODIFIER_ALREADY_SEEN,
            );
            all_modifiers.push(dup_modifier);
        }

        // Parse the inner declaration based on what follows 'declare'
        let saved_flags = self.context_flags;
        self.context_flags |= crate::parser::state::CONTEXT_FLAG_AMBIENT;

        let node = match self.token() {
            SyntaxKind::FunctionKeyword => {
                let modifiers = Some(Self::make_node_list(vec![declare_modifier]));
                self.parse_function_declaration_with_async(false, modifiers)
            }
            SyntaxKind::ClassKeyword => self.parse_declare_class(start_pos, declare_modifier),
            SyntaxKind::AbstractKeyword => {
                // declare abstract class
                self.parse_declare_abstract_class(start_pos, declare_modifier)
            }
            SyntaxKind::InterfaceKeyword => {
                let modifiers = Some(Self::make_node_list(vec![declare_modifier]));
                self.parse_interface_declaration_with_modifiers(start_pos, modifiers)
            }
            SyntaxKind::TypeKeyword => {
                let modifiers = Some(Self::make_node_list(vec![declare_modifier]));
                self.parse_type_alias_declaration_with_modifiers(start_pos, modifiers)
            }
            SyntaxKind::EnumKeyword => {
                let modifiers = Some(Self::make_node_list(vec![declare_modifier]));
                self.parse_enum_declaration_with_modifiers(start_pos, modifiers)
            }
            SyntaxKind::NamespaceKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::GlobalKeyword => {
                self.parse_declare_module_with_modifiers(start_pos, all_modifiers)
            }
            SyntaxKind::VarKeyword | SyntaxKind::LetKeyword => {
                let modifiers = Self::make_node_list(vec![declare_modifier]);
                self.parse_variable_statement_with_modifiers(Some(start_pos), Some(modifiers))
            }
            SyntaxKind::ConstKeyword => {
                // declare const enum or declare const variable
                if self.look_ahead_is_const_enum() {
                    self.parse_const_enum_declaration(start_pos, vec![declare_modifier])
                } else {
                    let modifiers = Self::make_node_list(vec![declare_modifier]);
                    self.parse_variable_statement_with_modifiers(Some(start_pos), Some(modifiers))
                }
            }
            SyntaxKind::UsingKeyword => {
                // declare using
                let modifiers = Self::make_node_list(vec![declare_modifier]);
                self.parse_variable_statement_with_modifiers(Some(start_pos), Some(modifiers))
            }
            SyntaxKind::ImportKeyword => {
                use tsz_common::diagnostics::diagnostic_codes;

                self.parse_error_at(
                    declare_start,
                    declare_end - declare_start,
                    "A 'declare' modifier cannot be used with an import declaration.",
                    diagnostic_codes::A_MODIFIER_CANNOT_BE_USED_WITH_AN_IMPORT_DECLARATION,
                );

                let modifiers = Some(Self::make_node_list(all_modifiers));
                if self.look_ahead_is_import_equals() {
                    self.parse_import_equals_declaration_with_modifiers(start_pos, modifiers)
                } else {
                    self.parse_import_declaration_with_modifiers(start_pos, modifiers)
                }
            }
            SyntaxKind::AwaitKeyword => {
                // declare await using
                let modifiers = Self::make_node_list(vec![declare_modifier]);
                self.parse_variable_statement_with_modifiers(Some(start_pos), Some(modifiers))
            }
            SyntaxKind::ExportKeyword => {
                // declare export ... — consume 'export' and parse the inner declaration
                // with declare modifier, so the entire statement is treated as ambient.
                let export_start = self.token_pos();
                self.parse_expected(SyntaxKind::ExportKeyword);
                let export_end = self.token_end();
                let export_modifier = self.arena.add_token(
                    SyntaxKind::ExportKeyword as u16,
                    export_start,
                    export_end,
                );
                let modifiers = Self::make_node_list(vec![declare_modifier, export_modifier]);
                // `declare export type { x }` / `declare export type * from "m"` is a
                // type-only export declaration, not the type-alias form `declare export
                // type X = Y` — same lookahead as the non-ambient path in
                // `parse_export_declaration`.
                let is_type_only_export = self.is_token(SyntaxKind::TypeKeyword) && {
                    let snapshot = self.scanner.save_state();
                    let current = self.current_token;
                    self.next_token();
                    let is_type_only = self.is_token(SyntaxKind::OpenBraceToken)
                        || self.is_token(SyntaxKind::AsteriskToken);
                    self.scanner.restore_state(snapshot);
                    self.current_token = current;
                    is_type_only
                };
                // `declare export default <expr>` where `default` is followed
                // by neither a class nor a function declaration is an
                // `ExportAssignment` node (a value expression carrying
                // `default`), not a `ClassDeclaration`/`FunctionDeclaration`
                // carrying a `default` modifier — the same class-vs-expression
                // split the sibling `async` family already draws for `default`
                // (`look_ahead_async_before_export_target`, #16403). tsc's
                // modifier-order check (TS1029) never applies to that
                // assignment form; only `AN_EXPORT_ASSIGNMENT_CANNOT_HAVE_MODIFIERS`
                // (TS1120) does, handled below alongside the declaration match
                // (residual #16432 disclosed and left unfixed).
                let is_export_default_assignment_form = self.is_token(SyntaxKind::DefaultKeyword)
                    && {
                        let snapshot = self.scanner.save_state();
                        let current = self.current_token;
                        self.next_token(); // skip `default`
                        let starts_declaration = matches!(
                            self.token(),
                            SyntaxKind::ClassKeyword
                                | SyntaxKind::AbstractKeyword
                                | SyntaxKind::FunctionKeyword
                        ) || (self.token() == SyntaxKind::AsyncKeyword
                            && self.look_ahead_is_async_function());
                        self.scanner.restore_state(snapshot);
                        self.current_token = current;
                        !starts_declaration
                    };
                // TS1029: 'export' modifier must precede 'declare' modifier.
                // Skip for `declare export as namespace` (valid UMD pattern) and
                // `declare export = expr` (export assignment — TS1120 handles it).
                // Also skip when already in an ambient context (e.g. inside `declare module`),
                // because the checker will emit TS1038 instead and tsc does not emit both.
                // Also skip in block context: tsc emits TS1029 via grammarErrorOnNode
                // in the checker, which is suppressed by hasParseDiagnostics when
                // TS1184 (Modifiers cannot appear here) is already emitted.
                // `declare export module/namespace` DOES still get TS1029 on current
                // pinned tsc (7.0.2, oracle-confirmed both at the source-file top level
                // and inside a namespace body) — an earlier version of this comment
                // claimed tsc 6.0 silenced it, which is no longer true and was never
                // re-verified against the current pin (#16403 residual).
                // Also skip for a plain export declaration (`{ }` / `*` / type-only
                // `type { }` / `type *`) — tsc emits TS1193 alone there, never TS1029
                // alongside it (oracle-confirmed). Also skip for the `default <expr>`
                // assignment form above — TS1120 handles it below, the same way
                // `EqualsToken` is skipped here.
                if !self.in_block_context()
                    && !self.is_token(SyntaxKind::AsKeyword)
                    && !self.is_token(SyntaxKind::EqualsToken)
                    && !self.is_token(SyntaxKind::OpenBraceToken)
                    && !self.is_token(SyntaxKind::AsteriskToken)
                    && !is_type_only_export
                    && !is_export_default_assignment_form
                    && (saved_flags & crate::parser::state::CONTEXT_FLAG_AMBIENT) == 0
                {
                    self.parse_error_at(
                        export_start,
                        export_end - export_start,
                        &tsz_common::diagnostics::diagnostic_messages::MODIFIER_MUST_PRECEDE_MODIFIER
                            .replace("{0}", "export")
                            .replace("{1}", "declare"),
                        tsz_common::diagnostics::diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                    );
                }
                match self.token() {
                    SyntaxKind::AsKeyword => {
                        // `declare export as namespace Foo;` — the resulting
                        // `NamespaceExportDeclaration` admits no modifiers in any
                        // container (unlike the container split the other arms of
                        // this match apply), so tsc reports TS1184 across the whole
                        // statement unconditionally and still parses the namespace
                        // export (#16389).
                        let node = self.parse_namespace_export_declaration(start_pos);
                        if let Some(n) = self.arena.get(node) {
                            let (span_start, span_end) = (n.pos, n.end);
                            self.parse_error_at(
                                span_start,
                                span_end - span_start,
                                "Modifiers cannot appear here.",
                                tsz_common::diagnostics::diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
                            );
                        }
                        node
                    }
                    SyntaxKind::FunctionKeyword => {
                        self.parse_function_declaration_with_async(false, Some(modifiers))
                    }
                    SyntaxKind::ClassKeyword => {
                        self.parse_declare_class(start_pos, declare_modifier)
                    }
                    SyntaxKind::VarKeyword
                    | SyntaxKind::LetKeyword
                    | SyntaxKind::ConstKeyword
                    | SyntaxKind::UsingKeyword
                    | SyntaxKind::AwaitKeyword => self
                        .parse_variable_statement_with_modifiers(Some(start_pos), Some(modifiers)),
                    SyntaxKind::EqualsToken => {
                        // `declare export = expr` or `export declare export = expr`
                        // tsc reports TS1120: An export assignment cannot have modifiers —
                        // but only at the top level. In a namespace body or a Block, the
                        // `ExportAssignment` node's own placement diagnostic (TS1063 in a
                        // namespace, TS1231 in a Block) wins instead and TS1120 does not
                        // fire alongside it — the same "own placement diagnostic silences
                        // the modifier diagnostic outside top level" shape the sibling
                        // `default <expr>` assignment form already implements below
                        // (#16403/#16440).
                        // Error span starts from the first modifier (export if present, else declare).
                        let error_start = all_modifiers
                            .first()
                            .and_then(|idx| self.arena.get(*idx))
                            .map_or(start_pos, |node| node.pos);
                        if !self.in_block_context() && !self.in_module_body_context() {
                            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
                            self.parse_error_at(
                                error_start,
                                self.token_pos() - error_start,
                                diagnostic_messages::AN_EXPORT_ASSIGNMENT_CANNOT_HAVE_MODIFIERS,
                                diagnostic_codes::AN_EXPORT_ASSIGNMENT_CANNOT_HAVE_MODIFIERS,
                            );
                        }
                        self.parse_export_assignment(error_start, Some(modifiers))
                    }
                    SyntaxKind::ImportKeyword => {
                        // `declare export import a = x.c;`
                        if self.look_ahead_is_import_equals() {
                            self.parse_import_equals_declaration_with_modifiers(
                                start_pos,
                                Some(modifiers),
                            )
                        } else {
                            self.parse_import_declaration_with_modifiers(start_pos, Some(modifiers))
                        }
                    }
                    SyntaxKind::ModuleKeyword | SyntaxKind::NamespaceKeyword => {
                        // `declare export module "..."` or `declare export namespace Foo`
                        self.parse_module_declaration_with_modifiers(start_pos, Some(modifiers))
                    }
                    SyntaxKind::InterfaceKeyword => {
                        // `declare export interface X { ... }`
                        self.parse_interface_declaration_with_modifiers(start_pos, Some(modifiers))
                    }
                    SyntaxKind::AsteriskToken | SyntaxKind::OpenBraceToken => {
                        // `declare export * from "m"` / `declare export { x }` (from "m")? —
                        // a plain export declaration cannot carry a `declare` modifier.
                        // tsc reports TS1193 at the first modifier and still parses the
                        // export declaration itself (the checker then resolves `x`/`"m"`
                        // as usual, e.g. TS2304/TS2307 alongside TS1193).
                        //
                        // Skipped when already in an ambient context (`declare namespace N
                        // { declare export { x }; }`): tsc reports TS1038 there instead of
                        // TS1193 (the same precedence the TS1029 check above already
                        // follows). The `declare`+`export` modifiers are still attached to
                        // the resulting `ExportDeclData` below so the checker's
                        // `check_declare_modifiers_in_ambient_body` pass can see them and
                        // report TS1038 in the nested case.
                        if (saved_flags & crate::parser::state::CONTEXT_FLAG_AMBIENT) == 0 {
                            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
                            let error_start = all_modifiers
                                .first()
                                .and_then(|idx| self.arena.get(*idx))
                                .map_or(start_pos, |node| node.pos);
                            self.parse_error_at(
                                error_start,
                                self.token_pos() - error_start,
                                diagnostic_messages::AN_EXPORT_DECLARATION_CANNOT_HAVE_MODIFIERS,
                                diagnostic_codes::AN_EXPORT_DECLARATION_CANNOT_HAVE_MODIFIERS,
                            );
                        }
                        if self.is_token(SyntaxKind::AsteriskToken) {
                            self.parse_export_star(start_pos, false, Some(modifiers))
                        } else {
                            self.parse_export_named(start_pos, false, Some(modifiers))
                        }
                    }
                    SyntaxKind::TypeKeyword if is_type_only_export => {
                        // `declare export type { x }` / `declare export type * from "m"`
                        // See the `AsteriskToken | OpenBraceToken` arm above for the
                        // already-ambient exception.
                        if (saved_flags & crate::parser::state::CONTEXT_FLAG_AMBIENT) == 0 {
                            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
                            let error_start = all_modifiers
                                .first()
                                .and_then(|idx| self.arena.get(*idx))
                                .map_or(start_pos, |node| node.pos);
                            self.parse_error_at(
                                error_start,
                                self.token_pos() - error_start,
                                diagnostic_messages::AN_EXPORT_DECLARATION_CANNOT_HAVE_MODIFIERS,
                                diagnostic_codes::AN_EXPORT_DECLARATION_CANNOT_HAVE_MODIFIERS,
                            );
                        }
                        self.parse_expected(SyntaxKind::TypeKeyword);
                        if self.is_token(SyntaxKind::AsteriskToken) {
                            self.parse_export_star(start_pos, true, Some(modifiers))
                        } else {
                            self.parse_export_named(start_pos, true, Some(modifiers))
                        }
                    }
                    SyntaxKind::TypeKeyword => {
                        // `declare export type X = ...`
                        self.parse_type_alias_declaration_with_modifiers(start_pos, Some(modifiers))
                    }
                    SyntaxKind::EnumKeyword => {
                        // `declare export enum X { ... }`
                        self.parse_enum_declaration_with_modifiers(start_pos, Some(modifiers))
                    }
                    SyntaxKind::DefaultKeyword => {
                        // `declare export default class {}` / `declare export
                        // default function f() {}` — tsc reads the whole
                        // thing as a `ClassDeclaration`/`FunctionDeclaration`
                        // carrying `declare`+`export`+`default` modifiers
                        // (the same node kind the ordinary `export default`
                        // path in `parse_export_default` builds), not the
                        // `export =`-style assignment the `EqualsToken` arm
                        // above handles. `self.context_flags` already carries
                        // `CONTEXT_FLAG_AMBIENT` here (set at the top of this
                        // function), so the reused declaration/expression
                        // classification `parse_export_default` performs
                        // still parses class/function bodies as ambient.
                        //
                        // `declare export default <expr>` (no class/function
                        // following `default`) is instead the `ExportAssignment`
                        // node the `EqualsToken` arm handles, and tsc reports
                        // its modifier violation the same way: TS1120 at the
                        // source file's own top level, silenced everywhere
                        // else by the node's own placement diagnostic (TS1258
                        // in a Block, TS1319 in a namespace body) — oracle-
                        // pinned residual #16432 disclosed and left for this.
                        if is_export_default_assignment_form
                            && !self.in_block_context()
                            && !self.in_module_body_context()
                        {
                            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
                            let error_start = all_modifiers
                                .first()
                                .and_then(|idx| self.arena.get(*idx))
                                .map_or(start_pos, |node| node.pos);
                            self.parse_error_at(
                                error_start,
                                self.token_pos() - error_start,
                                diagnostic_messages::AN_EXPORT_ASSIGNMENT_CANNOT_HAVE_MODIFIERS,
                                diagnostic_codes::AN_EXPORT_ASSIGNMENT_CANNOT_HAVE_MODIFIERS,
                            );
                        }
                        self.parse_export_default(start_pos, Some(modifiers))
                    }
                    _ => {
                        self.error_declaration_expected();
                        self.parse_expression_statement()
                    }
                }
            }
            SyntaxKind::AsyncKeyword if self.look_ahead_is_async_function() => {
                // declare async function
                // TS1040: 'async' modifier cannot be used in an ambient context
                // Emit at the 'async' keyword before consuming it, matching tsc.
                {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'async' modifier cannot be used in an ambient context.",
                        diagnostic_codes::MODIFIER_CANNOT_BE_USED_IN_AN_AMBIENT_CONTEXT,
                    );
                }
                // Pass the declare modifier to the function
                self.parse_expected(SyntaxKind::AsyncKeyword);
                let modifiers = Some(Self::make_node_list(vec![declare_modifier]));
                self.parse_function_declaration_with_async(true, modifiers)
            }
            _ => {
                self.error_declaration_expected();
                self.parse_expression_statement()
            }
        };

        self.context_flags = saved_flags;
        node
    }

    // Module/import declarations -> state_declarations_modules.rs
}

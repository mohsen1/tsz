//! Parser state - class member parsing.

use super::state::{
    CONTEXT_FLAG_ASYNC, CONTEXT_FLAG_CLASS_MEMBER_NAME, CONTEXT_FLAG_CONSTRUCTOR_PARAMETERS,
    CONTEXT_FLAG_FUNCTION_BODY, CONTEXT_FLAG_GENERATOR, CONTEXT_FLAG_GENERATOR_MEMBER_NAME,
    CONTEXT_FLAG_STATIC_BLOCK, ParserState,
};
use crate::parser::{
    NodeIndex, NodeList,
    node::{self},
    syntax_kind_ext,
};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::interner::{AstAtom, IdentText};
use tsz_scanner::SyntaxKind;

/// Pre-classified modifier flags for a single class member, computed in one
/// pass through the combined decorator + keyword-modifier list.
///
/// Constructed once by `scan_class_member_modifier_phase` so that all
/// downstream dispatch in `parse_class_member` reads named boolean fields
/// instead of performing repeated linear scans over the modifier node list.
pub(crate) struct ClassMemberModifierSet {
    /// Combined decorators + keyword modifiers in source order.
    /// Retained for AST construction and diagnostic-position lookups.
    pub(crate) modifiers: Option<NodeList>,
    /// `true` when at least one decorator was present.
    pub(crate) has_decorators: bool,
    /// `var` or `let` appeared as a modifier (invalid; triggers specific recovery).
    pub(crate) has_var_let: bool,
    pub(crate) has_static: bool,
    pub(crate) has_export: bool,
    pub(crate) has_declare: bool,
    pub(crate) has_accessor: bool,
    pub(crate) has_async: bool,
    /// `declare` appears before `override` in source order (e.g. `declare
    /// override p`). Distinct from the reverse order (`override declare p`),
    /// which is already diagnosed eagerly while scanning modifiers regardless
    /// of member kind (TS1040) — this flag exists because the `declare`-first
    /// conflict's diagnostic depends on the member kind, which isn't known
    /// until after modifier scanning finishes. See `construct_class_member`.
    pub(crate) declare_before_override: bool,
    /// `declare` appears before `async` in source order (e.g. `declare async
    /// p`). Distinct from the reverse order (`async declare p`), which is
    /// already diagnosed eagerly while scanning modifiers regardless of member
    /// kind — this flag exists because the `declare`-first ambient conflict is
    /// legal to report only on a property, and the member kind isn't known
    /// until after modifier scanning finishes. See `construct_class_member`.
    pub(crate) declare_before_async: bool,
    /// `true` when a `static`/`async` ordering conflict (TS1029) or an
    /// `async`+`declare` ambient conflict (TS1040) was already reported while
    /// scanning modifiers. tsc's grammar-modifier walk reports only the first
    /// problem found in source order, then stops — so when this is set, a
    /// method/accessor/constructor construction path must not additionally
    /// report `declare`-invalid-member-kind (TS1031): tsc's walk would have
    /// already stopped at the earlier conflict before ever reaching that check.
    pub(crate) async_declare_order_conflict_reported: bool,
    /// Start offset of a duplicate `declare` keyword whose TS1030
    /// (`'declare' modifier already seen.`) must be reported at property
    /// construction time. tsc's `checkGrammarModifiers` records `declare` as
    /// `ModifierFlags.Ambient` and reports `_0_modifier_already_seen` on the
    /// second occurrence, then `return`s — so this is `Some` only when the
    /// duplicate is the FIRST grammar error on the member's modifier list (no
    /// earlier diagnostic in the scan, and no `override`/`accessor`/`async`
    /// preceding it, which tsc would report first). The member must be a
    /// property for tsc to reach the second `declare` at all — a
    /// method/accessor/index-signature errors on the FIRST `declare`
    /// (TS1031/TS1071/TS1801) — so the emission is deferred to the property
    /// construction path and the recorded duplicate suppresses the
    /// declare/override, declare/async, and ambient-initializer (TS1039)
    /// checks, exactly as tsc's post-`return` walk does.
    pub(crate) declare_duplicate_pos: Option<u32>,
    /// Diagnostic-list length captured just before modifier parsing, used to
    /// selectively roll back modifier-ordering diagnostics when a static block
    /// is discovered after modifiers were already parsed.
    pub(crate) diag_len_before_modifiers: usize,
}

impl ParserState {
    /// Parse class member modifiers (static, public, private, protected, readonly, abstract, override).
    ///
    /// Returns the modifier list plus whether a `static`/`async` ordering
    /// conflict or an `async`+`declare` ambient conflict was already reported
    /// during the scan — callers use this to suppress a redundant
    /// declare-invalid-member-kind diagnostic once the member kind is known.
    pub(crate) fn parse_class_member_modifiers(&mut self) -> (Option<NodeList>, bool, Option<u32>) {
        let mut modifiers = Vec::new();

        // Diagnostic-list length at the start of this member's modifier scan.
        // A duplicate `declare` reports TS1030 only when it is the FIRST grammar
        // error on the member (tsc's `checkGrammarModifiers` `return`s at the
        // first problem), so the recording below fires only while this count is
        // unchanged. Position of the duplicate `declare` keyword, deferred to
        // the property construction path (see `declare_duplicate_pos`).
        let diag_len_at_scan_start = self.parse_diagnostics.len();
        let mut declare_duplicate_pos: Option<u32> = None;

        // State tracking for TS1028 (duplicates) and TS1029 (ordering)
        let mut seen_accessibility = false;
        // Matches tsc's `checkGrammarModifiers`, which reports TS1028 only once
        // per modifier list (it `return`s after the first duplicate). A third
        // accessibility keyword must not produce a second TS1028.
        let mut reported_accessibility_duplicate = false;
        let mut seen_static = false;
        let mut seen_abstract = false;
        let mut seen_readonly = false;
        let mut seen_override = false;
        let mut seen_accessor = false;
        let mut seen_async = false;
        let mut seen_declare = false;
        // Set once a `static`/`async` ordering conflict (TS1029) or an
        // `async`+`declare` ambient conflict (TS1040) has already been
        // reported for this member. tsc's grammar-modifier walk reports only
        // the FIRST problem found while scanning source order, then stops —
        // so a third modifier (e.g. `static`) joining an already-conflicting
        // `declare`/`async` pair must not add a second diagnostic, and a
        // `declare` that trails an already-reported `static`/`async`
        // ordering conflict must not add its own ambient-conflict diagnostic
        // (or, later, the declare-invalid-member-kind TS1031) on top of it.
        let mut async_declare_order_conflict_reported = false;

        loop {
            if self.should_stop_class_member_modifier() {
                break;
            }
            let start_pos = self.token_pos();

            // Before consuming token, check for TS1028 (duplicate accessibility) and TS1029 (wrong order)
            let current_kind = self.token();

            if matches!(
                current_kind,
                SyntaxKind::PublicKeyword
                    | SyntaxKind::PrivateKeyword
                    | SyntaxKind::ProtectedKeyword
            ) {
                if seen_accessibility && !reported_accessibility_duplicate {
                    // tsc emits TS1028 for a second accessibility modifier on ANY
                    // class member — property, method, accessor, or constructor —
                    // via `checkGrammarModifiers`, which records each modifier and
                    // reports the duplicate without inspecting the member kind.
                    // Emit it unconditionally at the duplicate keyword; the
                    // member-kind lookahead the old path used was based on a false
                    // premise (that tsc silently accepts duplicates on properties).
                    self.parse_error_at_current_token(
                        "Accessibility modifier already seen.",
                        diagnostic_codes::ACCESSIBILITY_MODIFIER_ALREADY_SEEN,
                    );
                    reported_accessibility_duplicate = true;
                }
                // TS1029: accessibility must come after certain modifiers.
                // `private` is excluded from the `abstract` arm: `private` and
                // `abstract` are never in a valid order (tsc reports the
                // pairwise TS1243 "cannot be used with" instead of an
                // ordering error, regardless of which one is written first —
                // see `check_modifier_combinations` in
                // `overload_compatibility.rs`), but `public`/`protected` DO
                // have a valid order with `abstract` (before it), so writing
                // either after `abstract` is exactly the same ordering
                // mistake as writing it after static/readonly/override/
                // accessor/async. `abstract` is the lowest-priority conflict
                // in the chain below, matching tsc's modifier walk: any of
                // static/readonly/override/accessor/async outranks it when
                // more than one precedes the accessibility modifier.
                let abstract_conflict = seen_abstract && current_kind != SyntaxKind::PrivateKeyword;
                if seen_static
                    || seen_readonly
                    || seen_override
                    || seen_accessor
                    || seen_async
                    || abstract_conflict
                {
                    use tsz_common::diagnostics::diagnostic_codes;
                    let current_mod = match current_kind {
                        SyntaxKind::PublicKeyword => "public",
                        SyntaxKind::PrivateKeyword => "private",
                        SyntaxKind::ProtectedKeyword => "protected",
                        _ => "accessibility",
                    };
                    let conflicting_mod = if seen_static {
                        "static"
                    } else if seen_readonly {
                        "readonly"
                    } else if seen_override {
                        "override"
                    } else if seen_accessor {
                        "accessor"
                    } else if seen_async {
                        "async"
                    } else {
                        "abstract"
                    };
                    self.parse_error_at_current_token(
                        &format!(
                            "'{current_mod}' modifier must precede '{conflicting_mod}' modifier."
                        ),
                        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                    );
                }
                seen_accessibility = true;
            } else if current_kind == SyntaxKind::StaticKeyword {
                // Check for duplicate static modifier
                // In tsc 6.0+, duplicate `static` in class members emits TS1434
                // (Unexpected keyword or identifier) because the second `static`
                // is treated as a potential property name rather than a duplicate modifier.
                if seen_static {
                    use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.parse_error_at_current_token(
                        diagnostic_messages::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                        diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                    );
                }
                // TS1029: static must come after accessibility, before certain others.
                // `static` with `abstract` is illegal in either order; the checker
                // emits TS1243 for that pair, so do not also emit an ordering error.
                // The `async` reason is additionally suppressed once `declare` has
                // already been seen: `declare async static m()` reports only TS1031
                // (declare invalid on a method) in tsc, never this ordering error,
                // because tsc's walk already stopped at `declare`.
                let async_conflict = seen_async && !seen_declare;
                if seen_readonly || seen_override || seen_accessor || async_conflict {
                    use tsz_common::diagnostics::diagnostic_codes;
                    let other = if seen_override {
                        "override"
                    } else if seen_readonly {
                        "readonly"
                    } else if seen_accessor {
                        "accessor"
                    } else {
                        "async"
                    };
                    self.parse_error_at_current_token(
                        &format!("'static' modifier must precede '{other}' modifier."),
                        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                    );
                    if async_conflict && !seen_readonly && !seen_override && !seen_accessor {
                        async_declare_order_conflict_reported = true;
                    }
                }
                seen_static = true;
            } else if current_kind == SyntaxKind::AbstractKeyword {
                // Check for duplicate abstract modifier
                if seen_abstract {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'abstract' modifier already seen.",
                        diagnostic_codes::MODIFIER_ALREADY_SEEN,
                    );
                }
                // `readonly` and `async` are excluded from this ordering
                // check: `abstract` and `readonly` legally coexist in either
                // order (a `readonly abstract` property is clean in tsc), and
                // `abstract`/`async` never coexist at all (tsc rejects the
                // combination outright with TS1243 regardless of order, not
                // this ordering diagnostic).
                if seen_override || seen_accessor {
                    use tsz_common::diagnostics::diagnostic_codes;
                    let other = if seen_override {
                        "override"
                    } else {
                        "accessor"
                    };
                    self.parse_error_at_current_token(
                        &format!("'abstract' modifier must precede '{other}' modifier."),
                        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                    );
                }
                seen_abstract = true;
            } else if current_kind == SyntaxKind::ReadonlyKeyword {
                // Check for duplicate readonly modifier
                if seen_readonly {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'readonly' modifier already seen.",
                        diagnostic_codes::MODIFIER_ALREADY_SEEN,
                    );
                }
                if seen_accessor {
                    // Auto-accessor properties cannot be readonly. tsc emits
                    // TS1243 (cannot-be-used-with) here, not TS1029
                    // (must-precede), because no ordering of these two
                    // modifiers is legal.
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'readonly' modifier cannot be used with 'accessor' modifier.",
                        diagnostic_codes::MODIFIER_CANNOT_BE_USED_WITH_MODIFIER,
                    );
                }
                // No `seen_async` ordering check here: `readonly` (data-member
                // only) and `async` (method-only) never share a legal member
                // kind, so tsc never reaches this ordering diagnostic for the
                // pair — the member's own TS1024/TS1042 already covers it.
                seen_readonly = true;
            } else if current_kind == SyntaxKind::OverrideKeyword {
                // Check for duplicate override modifier
                if seen_override {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'override' modifier already seen.",
                        diagnostic_codes::MODIFIER_ALREADY_SEEN,
                    );
                }
                // `declare override` ordering (declare before override) is NOT
                // reported here: unlike the reverse order, tsc's actual
                // diagnostic depends on the member kind, which is not known
                // until later (TS1031 on a method/accessor/constructor — the
                // existing `has_declare` check in `construct_class_member`
                // already covers that; TS1243 on a property, handled once the
                // member kind is confirmed — see `declare_before_override`).
                if seen_accessor || seen_async || seen_readonly {
                    use tsz_common::diagnostics::diagnostic_codes;
                    let other = if seen_accessor {
                        "accessor"
                    } else if seen_async {
                        "async"
                    } else {
                        "readonly"
                    };
                    self.parse_error_at_current_token(
                        &format!("'override' modifier must precede '{other}' modifier."),
                        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                    );
                }
                seen_override = true;
            } else if current_kind == SyntaxKind::AccessorKeyword {
                // Check for duplicate accessor modifier
                if seen_accessor {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'accessor' modifier already seen.",
                        diagnostic_codes::MODIFIER_ALREADY_SEEN,
                    );
                }
                // Auto-accessor properties cannot be combined with `readonly`
                // or `declare` in either order — tsc emits TS1243 on the
                // accessor keyword when readonly/declare was seen first.
                if seen_readonly {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'accessor' modifier cannot be used with 'readonly' modifier.",
                        diagnostic_codes::MODIFIER_CANNOT_BE_USED_WITH_MODIFIER,
                    );
                }
                // A private-named member's `declare`/`accessor` pairing is
                // reported as TS18019 ("modifier cannot be used with a
                // private identifier") by the checker's source-ordered
                // private-identifier modifier walk instead — tsc's single
                // ordered walk reaches that check before it would reach this
                // pairwise incompatibility. See `upcoming_member_name_is_private`.
                // Suppressed once a duplicate `declare` has been recorded
                // (`declare declare accessor`): tsc `return`ed at the duplicate
                // TS1030 before reaching the accessor pairing.
                if seen_declare
                    && declare_duplicate_pos.is_none()
                    && !self.upcoming_member_name_is_private()
                {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'accessor' modifier cannot be used with 'declare' modifier.",
                        diagnostic_codes::MODIFIER_CANNOT_BE_USED_WITH_MODIFIER,
                    );
                }
                // No `seen_async` ordering check here: `accessor` (data-member
                // only) and `async` (method-only) never share a legal member
                // kind, so tsc never reaches this ordering diagnostic for the
                // pair — the member's own TS1042 already covers it.
                seen_accessor = true;
            } else if current_kind == SyntaxKind::AsyncKeyword {
                // Check for duplicate async modifier
                if seen_async {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'async' modifier already seen.",
                        diagnostic_codes::MODIFIER_ALREADY_SEEN,
                    );
                }
                seen_async = true;
            }

            let modifier = match current_kind {
                SyntaxKind::StaticKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::StaticKeyword, start_pos)
                }
                SyntaxKind::PublicKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::PublicKeyword, start_pos)
                }
                SyntaxKind::PrivateKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::PrivateKeyword, start_pos)
                }
                SyntaxKind::ProtectedKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::ProtectedKeyword, start_pos)
                }
                SyntaxKind::ReadonlyKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::ReadonlyKeyword, start_pos)
                }
                SyntaxKind::AbstractKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::AbstractKeyword, start_pos)
                }
                SyntaxKind::OverrideKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::OverrideKeyword, start_pos)
                }
                SyntaxKind::AsyncKeyword => {
                    // TS1040: 'async' modifier cannot be used in an ambient context.
                    // Only the enclosing-ambient-context case (`declare
                    // class`/`declare namespace`) is decidable here. A
                    // member-local `declare` seen earlier in this same list
                    // (`declare async p`) is legal only on a property — whose
                    // kind isn't known until the member name/parameter list is
                    // parsed — so that combination is handled later in
                    // `scan_class_member_modifier_phase` /
                    // `construct_class_member`, once the member kind is final.
                    if self.in_ambient_context() {
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            "'async' modifier cannot be used in an ambient context.",
                            diagnostic_codes::MODIFIER_CANNOT_BE_USED_IN_AN_AMBIENT_CONTEXT,
                        );
                    }
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::AsyncKeyword, start_pos)
                }
                SyntaxKind::DeclareKeyword => {
                    // TS1030: a repeated `declare` modifier. tsc records `declare`
                    // as `ModifierFlags.Ambient` and reports
                    // `_0_modifier_already_seen` on the second occurrence, then
                    // `return`s. Record it only when it is the first grammar error
                    // on this member (no earlier diagnostic in this scan) and no
                    // modifier that conflicts with `declare` (`override`/`accessor`/
                    // `async`) precedes it — tsc would report that conflict first.
                    // A private-named member is excluded because tsc's ordered walk
                    // reaches the private-identifier check (TS1801) on the FIRST
                    // `declare` before the duplicate. The actual emission is deferred
                    // to the property construction path because a non-property member
                    // errors on the FIRST `declare` (TS1031/TS1071) and never reaches
                    // the duplicate.
                    if seen_declare
                        && declare_duplicate_pos.is_none()
                        && self.parse_diagnostics.len() == diag_len_at_scan_start
                        && !seen_override
                        && !seen_accessor
                        && !seen_async
                        && !self.upcoming_member_name_is_private()
                    {
                        declare_duplicate_pos = Some(start_pos);
                    }
                    // TS1040: 'override' modifier cannot be used in an ambient context
                    // When `override` precedes `declare`, report at `declare` position.
                    // Only the first `declare` reports it: tsc's `checkGrammarModifiers`
                    // `return`s at that error, so a trailing duplicate `declare`
                    // (`override declare declare`) must not report it a second time.
                    if seen_override && !seen_declare {
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            "'override' modifier cannot be used in an ambient context.",
                            diagnostic_codes::MODIFIER_CANNOT_BE_USED_IN_AN_AMBIENT_CONTEXT,
                        );
                    }
                    // TS1040: 'async' modifier cannot be used in an ambient context.
                    // When `async` precedes `declare` (`async declare p`), the
                    // ambient conflict is only known once `declare` is reached, so
                    // — mirroring the `override` case above — report here rather
                    // than at `async`'s own position. Suppressed when a `static`
                    // ordering conflict already fired earlier in this same scan
                    // (e.g. `async static declare m()`): tsc's walk already
                    // stopped at `static`, so `declare` is never reached at all.
                    if seen_async && !async_declare_order_conflict_reported {
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            "'async' modifier cannot be used in an ambient context.",
                            diagnostic_codes::MODIFIER_CANNOT_BE_USED_IN_AN_AMBIENT_CONTEXT,
                        );
                        async_declare_order_conflict_reported = true;
                    }
                    // Auto-accessor properties cannot be `declare`d. When
                    // `accessor` precedes `declare`, tsc emits TS1243 on the
                    // declare keyword — unless the member name is private, in
                    // which case the checker's private-identifier modifier
                    // walk reports TS18019 instead (see the accessor arm's
                    // mirror check above). Only the first `declare` reports it:
                    // tsc `return`s at that error, so a trailing duplicate
                    // `declare` (`accessor declare declare`) must not repeat it.
                    if seen_accessor && !seen_declare && !self.upcoming_member_name_is_private() {
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            "'declare' modifier cannot be used with 'accessor' modifier.",
                            diagnostic_codes::MODIFIER_CANNOT_BE_USED_WITH_MODIFIER,
                        );
                    }
                    seen_declare = true;
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::DeclareKeyword, start_pos)
                }
                SyntaxKind::AccessorKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::AccessorKeyword, start_pos)
                }
                // Handle const as a modifier - error is reported by checker (1248)
                // But only if not followed by line break (ASI would make it a property name)
                SyntaxKind::ConstKeyword => {
                    // Look ahead: if there's a line break after const, treat as property name not modifier
                    let snapshot = self.scanner.save_state();
                    let saved_token = self.current_token;
                    self.next_token();

                    // Check if followed by var/let (invalid pattern: const var foo)
                    // In this case, consume const without adding to modifiers, let var/let handler emit error
                    if matches!(
                        self.current_token,
                        SyntaxKind::VarKeyword | SyntaxKind::LetKeyword
                    ) {
                        // Restore state, consume const, and continue - var/let will emit TS1440
                        self.scanner.restore_state(snapshot);
                        self.current_token = saved_token;
                        self.next_token(); // Consume const
                        continue;
                    }

                    if self.scanner.has_preceding_line_break() {
                        // Restore and break - const is a property name
                        self.scanner.restore_state(snapshot);
                        self.current_token = saved_token;
                        break;
                    }
                    self.arena
                        .create_modifier(SyntaxKind::ConstKeyword, start_pos)
                }
                // Handle 'export' - not valid as class member modifier
                SyntaxKind::ExportKeyword => {
                    // Skip emitting generic unexpected modifier for export when it
                    // introduces a constructor declaration. Constructor-specific
                    // validation emits TS1031.
                    let snapshot = self.scanner.save_state();
                    let saved_token = self.current_token;
                    self.next_token();
                    let next_is_constructor = self.current_token == SyntaxKind::ConstructorKeyword
                        && !self.scanner.has_preceding_line_break();
                    // Skip TS1031 for index signatures (e.g., `export [x: string]: string`).
                    // The checker emits the more specific TS1071 instead.
                    let next_is_index_sig = self.current_token == SyntaxKind::OpenBracketToken
                        && !self.scanner.has_preceding_line_break();
                    self.scanner.restore_state(snapshot);
                    self.current_token = saved_token;

                    if !next_is_constructor && !next_is_index_sig {
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            "'export' modifier cannot appear on class elements of this kind.",
                            diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_CLASS_ELEMENTS_OF_THIS_KIND,
                        );
                    }
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::ExportKeyword, start_pos)
                }
                // Handle 'let' and 'var' - could be property names or invalid modifiers
                SyntaxKind::LetKeyword | SyntaxKind::VarKeyword => {
                    // Look ahead to distinguish between property name and modifier
                    // var() { } or var followed by line break -> property name (valid)
                    // public var foo -> modifier (invalid)
                    let snapshot = self.scanner.save_state();
                    let saved_token = self.current_token;
                    self.next_token();

                    // If followed by open paren, it's a method name (valid)
                    if self.current_token == SyntaxKind::OpenParenToken {
                        // Restore and break - var/let is a property name
                        self.scanner.restore_state(snapshot);
                        self.current_token = saved_token;
                        break;
                    }

                    // If followed by line break, ASI makes it a property name (valid)
                    if self.scanner.has_preceding_line_break() {
                        // Restore and break - var/let is a property name
                        self.scanner.restore_state(snapshot);
                        self.current_token = saved_token;
                        break;
                    }

                    // If followed by semicolon, comma, equals, or closing brace, it's a property name (valid)
                    // Examples: var; | var, | var = | var }
                    if matches!(
                        self.current_token,
                        SyntaxKind::SemicolonToken
                            | SyntaxKind::CommaToken
                            | SyntaxKind::EqualsToken
                            | SyntaxKind::CloseBraceToken
                    ) {
                        // Restore and break - var/let is a property name
                        self.scanner.restore_state(snapshot);
                        self.current_token = saved_token;
                        break;
                    }

                    // Otherwise it's being used as a modifier (invalid)
                    // Restore state to emit error at var/let position, then consume it
                    self.scanner.restore_state(snapshot);
                    self.current_token = saved_token;

                    // Check if followed by 'constructor' - emit TS1068 instead of TS1440
                    let is_followed_by_constructor = if self.current_token == SyntaxKind::VarKeyword
                        || self.current_token == SyntaxKind::LetKeyword
                    {
                        let snapshot2 = self.scanner.save_state();
                        let saved_token2 = self.current_token;
                        self.next_token();
                        let result = self.current_token == SyntaxKind::ConstructorKeyword;
                        self.scanner.restore_state(snapshot2);
                        self.current_token = saved_token2;
                        result
                    } else {
                        false
                    };

                    if is_followed_by_constructor {
                        self.parse_error_at_current_token(
                            "Unexpected token. A constructor, method, accessor, or property was expected.",
                            diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
                        );
                    } else {
                        self.parse_error_at_current_token(
                            "Variable declaration not allowed at this location.",
                            diagnostic_codes::VARIABLE_DECLARATION_NOT_ALLOWED_AT_THIS_LOCATION,
                        );
                    }
                    // Consume var/let and add to modifiers list
                    // This prevents parse_constructor_with_modifiers from being called
                    let var_token = self.token();
                    self.next_token();

                    // Add var/let to modifiers and return early
                    // Don't continue parsing modifiers (e.g., don't process 'export' in 'var export foo')
                    let var_modifier = self.arena.create_modifier(var_token, start_pos);
                    modifiers.push(var_modifier);
                    return (
                        Some(Self::make_node_list(modifiers)),
                        async_declare_order_conflict_reported,
                        declare_duplicate_pos,
                    );
                }
                // `in` / `out` are variance modifiers that only apply to type
                // parameters (of class/interface/type alias). When they appear on a
                // class member, `should_stop_class_member_modifier` already verified
                // the next token looks like a property name, so consume them as
                // modifiers and let the checker emit TS1274 — much better than the
                // generic TS1434 we used to fall through to.
                SyntaxKind::InKeyword => {
                    self.next_token();
                    self.arena.create_modifier(SyntaxKind::InKeyword, start_pos)
                }
                SyntaxKind::OutKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::OutKeyword, start_pos)
                }
                _ => break,
            };
            modifiers.push(modifier);
        }

        let modifier_list = if modifiers.is_empty() {
            None
        } else {
            Some(Self::make_node_list(modifiers))
        };
        (
            modifier_list,
            async_declare_order_conflict_reported,
            declare_duplicate_pos,
        )
    }

    /// Peeks past any remaining modifier keywords, without consuming them,
    /// to see whether the class member's name (parsed after all modifiers)
    /// will be a private identifier (`#name`).
    ///
    /// The modifier list is parsed before the name, so a check that needs to
    /// know whether the member is private-named — because tsc's own ordered
    /// modifier walk would reach the private-identifier check
    /// (TS18010/TS18019) before a later pairwise modifier check — has to look
    /// ahead for it.
    fn upcoming_member_name_is_private(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let saved_token = self.current_token;
        let mut is_private = false;
        loop {
            match self.current_token {
                SyntaxKind::StaticKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::ReadonlyKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::OverrideKeyword
                | SyntaxKind::AsyncKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::AccessorKeyword => {
                    self.next_token();
                }
                SyntaxKind::PrivateIdentifier => {
                    is_private = true;
                    break;
                }
                _ => break,
            }
        }
        self.scanner.restore_state(snapshot);
        self.current_token = saved_token;
        is_private
    }

    pub(crate) fn should_stop_class_member_modifier(&mut self) -> bool {
        if !matches!(
            self.token(),
            SyntaxKind::StaticKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::ReadonlyKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::OverrideKeyword
                | SyntaxKind::AsyncKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::AccessorKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::ExportKeyword
                | SyntaxKind::InKeyword
                | SyntaxKind::OutKeyword
        ) {
            return false;
        }

        if self.is_token(SyntaxKind::StaticKeyword) && self.look_ahead_is_static_block() {
            return true;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let next = self.current_token;
        let has_line_break = self.scanner.has_preceding_line_break();
        self.scanner.restore_state(snapshot);
        self.current_token = current;

        // ASI: if the next token is on a new line, treat the keyword as a property name.
        // `static` is still a modifier before `accessor` even across a line break;
        // the accessor token itself can then decide whether it is a modifier or name.
        if has_line_break {
            if current == SyntaxKind::StaticKeyword && next == SyntaxKind::AccessorKeyword {
                return false;
            }
            return true;
        }

        matches!(
            next,
            SyntaxKind::OpenParenToken
                | SyntaxKind::LessThanToken
                | SyntaxKind::QuestionToken
                | SyntaxKind::ExclamationToken
                | SyntaxKind::ColonToken
                | SyntaxKind::EqualsToken
                | SyntaxKind::SemicolonToken
                // When followed by } or EOF, treat the keyword as a property name, not a modifier
                // This allows patterns like: class C { public }
                | SyntaxKind::CloseBraceToken
                | SyntaxKind::EndOfFileToken
        )
    }

    /// Parse constructor with modifiers
    pub(crate) fn parse_constructor_with_modifiers(
        &mut self,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ConstructorKeyword);

        // Check for type parameters on constructor (invalid but parse for better error reporting)
        // tsc emits TS1092 in the checker at the typeParameters NodeArray position,
        // which starts after '<' (i.e., at the first type parameter or '>' if empty).
        // We emit it here in the parser but must match tsc's position: after '<'.
        let type_parameters = self.is_token(SyntaxKind::LessThanToken).then(|| {
            let less_than_end = self.token_end();
            let type_params = self.parse_type_parameters();
            self.parse_error_at(
                less_than_end,
                0,
                "Type parameters cannot appear on a constructor declaration.",
                diagnostic_codes::TYPE_PARAMETERS_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
            );
            type_params
        });

        let has_open_paren = self.parse_expected(SyntaxKind::OpenParenToken);
        let saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_CONSTRUCTOR_PARAMETERS;
        let parameters = if has_open_paren {
            let params = self.parse_parameter_list();
            self.context_flags = saved_flags;
            self.parse_expected(SyntaxKind::CloseParenToken);
            params
        } else {
            // When `(` is missing (e.g., `constructor\n}`), skip parameter parsing
            // and `)` expectation to avoid cascading `')' expected` errors.
            self.context_flags = saved_flags;
            NodeList::new()
        };

        // Recovery: Handle return type annotation on constructor (invalid but users write it)
        if self.parse_optional(SyntaxKind::ColonToken) {
            if self.should_recover_constructor_return_type_at_class_member_boundary() {
                self.error_type_expected();
            } else {
                let missing_type = !self.can_token_start_type() && self.is_type_terminator_token();
                if !missing_type {
                    self.parse_error_at_current_token(
                        "Type annotation cannot appear on a constructor declaration.",
                        diagnostic_codes::TYPE_ANNOTATION_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
                    );
                }
                // Consume the type annotation for recovery (use parse_return_type to match tsc,
                // which parses type predicates even in invalid constructor return types)
                let _ = self.parse_return_type();
            }
        }

        // Push a new label scope for the constructor body
        // Clear static block flag - constructor creates a new function boundary
        let body_saved_flags = self.context_flags;
        self.context_flags &= !CONTEXT_FLAG_STATIC_BLOCK;
        self.context_flags |= CONTEXT_FLAG_FUNCTION_BODY;
        self.push_label_scope();
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            NodeIndex::NONE
        };
        self.pop_label_scope();
        self.context_flags = body_saved_flags;

        let end_pos = self.token_end();
        self.arena.add_constructor(
            syntax_kind_ext::CONSTRUCTOR,
            start_pos,
            end_pos,
            crate::parser::node::ConstructorData {
                modifiers,
                type_parameters,
                parameters,
                body,
            },
        )
    }

    fn should_recover_constructor_return_type_at_class_member_boundary(&mut self) -> bool {
        if !self.scanner.has_preceding_line_break() {
            return false;
        }

        if matches!(
            self.current_token,
            SyntaxKind::CloseBraceToken
                | SyntaxKind::CloseParenToken
                | SyntaxKind::CommaToken
                | SyntaxKind::SemicolonToken
                | SyntaxKind::EndOfFileToken
        ) {
            return false;
        }

        if self.is_constructor_return_type_recovery_class_member_start() {
            return true;
        }

        if !self.is_property_name() {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let result = !self.scanner.has_preceding_line_break()
            && matches!(
                self.current_token,
                SyntaxKind::OpenParenToken
                    | SyntaxKind::LessThanToken
                    | SyntaxKind::QuestionToken
                    | SyntaxKind::ExclamationToken
                    | SyntaxKind::ColonToken
                    | SyntaxKind::EqualsToken
                    | SyntaxKind::SemicolonToken
            );
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    const fn is_constructor_return_type_recovery_class_member_start(&mut self) -> bool {
        matches!(
            self.current_token,
            SyntaxKind::PublicKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::StaticKeyword
                | SyntaxKind::ReadonlyKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::OverrideKeyword
                | SyntaxKind::AccessorKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::AtToken
                | SyntaxKind::AsteriskToken
        )
    }

    /// Parse get accessor with modifiers: static get `foo()` { }
    pub(crate) fn parse_get_accessor_with_modifiers(
        &mut self,
        modifiers: Option<NodeList>,
        start_pos: u32,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::GetKeyword);

        let name = self.parse_property_name();

        let type_parameters = self.is_token(SyntaxKind::LessThanToken).then(|| {
            self.report_accessor_type_parameters_error(name);
            self.parse_type_parameters()
        });

        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if self.is_token(SyntaxKind::CloseParenToken) {
            Self::make_node_list(vec![])
        } else if self.is_token(SyntaxKind::CommaToken) {
            // `get x(,)` — comma can't start a parameter declaration.
            // tsc emits TS1138 "Parameter declaration expected" here,
            // NOT TS1054 (which is for getters that have actual parameters).
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.parse_error_at_current_token(
                diagnostic_messages::PARAMETER_DECLARATION_EXPECTED,
                diagnostic_codes::PARAMETER_DECLARATION_EXPECTED,
            );
            // Skip the comma and continue parsing to recover
            self.next_token();
            Self::make_node_list(vec![])
        } else {
            use tsz_common::diagnostics::diagnostic_codes;
            let parsed = self.parse_parameter_list();
            // A `this` parameter is not a value parameter, so a getter whose
            // only parameter is `this` has zero parameters as far as this
            // arity grammar is concerned — tsc rejects it in the checker with
            // TS2784, not here with TS1054. `report_set_accessor_parameter_count`
            // already makes the same exclusion for the setter arm.
            let only_this_parameter = parsed.nodes.len() == 1
                && parsed.nodes.first().is_some_and(|&param_idx| {
                    let name_idx = match self.arena.get_parameter_at(param_idx) {
                        Some(param) => param.name,
                        None => return false,
                    };
                    self.arena
                        .get(name_idx)
                        .is_some_and(|name_node| name_node.kind == SyntaxKind::ThisKeyword as u16)
                });
            if !only_this_parameter {
                // Report error at the accessor name, matching tsc behavior
                if let Some(name_node) = self.arena.get(name) {
                    self.parse_error_at(
                        name_node.pos,
                        name_node.end - name_node.pos,
                        "A 'get' accessor cannot have parameters.",
                        diagnostic_codes::A_GET_ACCESSOR_CANNOT_HAVE_PARAMETERS,
                    );
                } else {
                    self.parse_error_at_current_token(
                        "A 'get' accessor cannot have parameters.",
                        diagnostic_codes::A_GET_ACCESSOR_CANNOT_HAVE_PARAMETERS,
                    );
                }
            }
            parsed
        };
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Optional return type (supports type predicates)
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        let body = self.parse_accessor_body(&modifiers);

        let end_pos = self.token_full_start();
        self.arena.add_accessor(
            syntax_kind_ext::GET_ACCESSOR,
            start_pos,
            end_pos,
            crate::parser::node::AccessorData {
                modifiers,
                name,
                type_parameters,
                parameters,
                type_annotation,
                body,
            },
        )
    }

    /// Parse the body of an accessor (get or set).
    ///
    /// A class accessor requires a `{` brace body. tsc reaches this through
    /// `parseFunctionBlockOrSemicolon`, which (mirroring methods) parses an
    /// optional semicolon when ASI applies and otherwise calls
    /// `parseFunctionBlock` -> `parseExpected(OpenBraceToken)`. Two distinct
    /// `tsc` mechanisms therefore report a missing brace body, and we replicate
    /// both:
    ///
    /// 1. Signature followed by a non-`{`, non-semicolon token (`get x() return
    ///    1`): the parser itself reports TS1005 `'{' expected` at that token via
    ///    `parseExpected`, recovering with an empty block so the trailing tokens
    ///    re-parse (surfacing TS1128). We delegate to [`parse_block`], which
    ///    emits the same diagnostic and returns an empty block without consuming.
    /// 2. Body-less signature where ASI applies (`get x();`, `get x()` before
    ///    `}`/EOF/line break): the parser accepts it, then `checkGrammarAccessor`
    ///    requires a brace body for any non-ambient, non-abstract accessor and
    ///    reports TS1005 `'{' expected` at the last character of the signature.
    ///
    /// Ambient (`declare class`) and `abstract` accessors are legitimately
    /// body-less and produce no brace diagnostic.
    ///
    /// Returns `NodeIndex::NONE` for a body-less accessor; otherwise the parsed
    /// (possibly empty, recovered) block.
    fn parse_accessor_body(&mut self, _modifiers: &Option<NodeList>) -> NodeIndex {
        // Clear static block flag - accessor creates a new function boundary
        let saved_flags = self.context_flags;
        self.context_flags &= !CONTEXT_FLAG_STATIC_BLOCK;
        self.context_flags |= CONTEXT_FLAG_FUNCTION_BODY;
        self.push_label_scope();
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else if self.can_parse_semicolon() {
            // Mechanism 2: a body-less accessor (`get x();`, or before `}` / EOF /
            // a line break where ASI applies). tsc accepts the parse here and lets
            // `checkGrammarAccessor` report TS1005 `'{' expected` for any
            // non-ambient, non-abstract accessor. tsz's checker already mirrors
            // `checkGrammarAccessor`, so emitting TS1005 in the parser as well
            // double-counts the diagnostic (the ambient/abstract gating likewise
            // lives in the checker) — see the #14958 regression. Accept the
            // body-less signature and leave the brace diagnostic to the checker.
            //
            // NOTE (wave-3 checkpoint): the checker does NOT actually emit this
            // grammar error, so tsz drops it for a non-ambient, non-abstract class
            // accessor (abstractPropertyNegative.ts, giant.ts). Re-emitting it here
            // is blocked: adding the parse error makes the checker's
            // `has_parse_errors` guards suppress the file's semantic diagnostics
            // (TS2416/2540/2654/2676), a net loss. Fixing this needs the checker to
            // own `checkGrammarAccessor` (or to stop swallowing semantic checks on
            // grammar errors), not a parser emission.
            self.parse_semicolon();
            NodeIndex::NONE
        } else {
            // Mechanism 1: delegate to `parse_block`, which reports TS1005
            // `'{' expected` at the current (non-`{`) token and recovers with an
            // empty block without consuming the following tokens.
            self.parse_block()
        };
        self.pop_label_scope();
        self.context_flags = saved_flags;
        body
    }

    /// Emit TS1031 at the position of a specific modifier keyword in the modifier list.
    /// Used for constructor declarations where tsc's grammarErrorOnNode anchors at the modifier.
    fn emit_modifier_error_on_constructor(
        &mut self,
        modifiers: &Option<NodeList>,
        kind: SyntaxKind,
        message: &str,
        code: u32,
    ) {
        if let Some(mods) = modifiers {
            for &idx in &mods.nodes {
                if let Some(node) = self.arena.get(idx)
                    && node.kind == kind as u16
                {
                    self.parse_error_at(node.pos, node.end - node.pos, message, code);
                    return;
                }
            }
        }
        // Fallback if modifier not found in list
        self.parse_error_at_current_token(message, code);
    }

    /// Emit TS1031 "'declare' modifier cannot appear on class elements of this kind."
    /// at the position of the `declare` modifier in the given modifier list.
    pub(super) fn emit_declare_on_non_property_error(&mut self, modifiers: &Option<NodeList>) {
        if let Some(mods) = modifiers {
            for &idx in &mods.nodes {
                if let Some(node) = self.arena.get(idx)
                    && node.kind == SyntaxKind::DeclareKeyword as u16
                {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at(
                        node.pos,
                        node.end - node.pos,
                        "'declare' modifier cannot appear on class elements of this kind.",
                        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_CLASS_ELEMENTS_OF_THIS_KIND,
                    );
                    break;
                }
            }
        }
    }

    /// Emit TS1275 "'accessor' modifier can only appear on a property declaration."
    /// at the position of the `accessor` modifier in the given modifier list.
    /// Used when a class member with an `accessor` modifier turns out to be a
    /// constructor, method, getter, or setter rather than a property declaration.
    pub(crate) fn emit_accessor_modifier_only_on_property_error(
        &mut self,
        modifiers: &Option<NodeList>,
    ) {
        if let Some(mods) = modifiers {
            for &idx in &mods.nodes {
                if let Some(node) = self.arena.get(idx)
                    && node.kind == SyntaxKind::AccessorKeyword as u16
                {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at(
                        node.pos,
                        node.end - node.pos,
                        "'accessor' modifier can only appear on a property declaration.",
                        diagnostic_codes::ACCESSOR_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION,
                    );
                    break;
                }
            }
        }
    }

    /// Modifier scan + classification phase.
    ///
    /// Parses keyword modifiers, combines them with any already-parsed
    /// decorators, handles a misplaced `@` after keywords, and returns a
    /// [`ClassMemberModifierSet`] whose boolean fields let `construct_class_member`
    /// branch on named flags instead of repeated linear scans over the node list.
    fn scan_class_member_modifier_phase(
        &mut self,
        decorators: Option<NodeList>,
    ) -> ClassMemberModifierSet {
        let has_decorators = decorators.is_some();
        let diag_len_before_modifiers = self.parse_diagnostics.len();
        let (parsed_modifiers, async_declare_order_conflict_reported, declare_duplicate_pos) =
            self.parse_class_member_modifiers();
        let had_keyword_modifiers = parsed_modifiers.is_some();

        let mut combined = match (decorators, parsed_modifiers) {
            (Some(dec), Some(kw)) => {
                let mut nodes = dec.nodes;
                nodes.extend(kw.nodes);
                Some(NodeList {
                    nodes,
                    pos: dec.pos,
                    end: kw.end,
                    has_trailing_comma: false,
                })
            }
            (Some(dec), None) => Some(dec),
            (None, Some(kw)) => Some(kw),
            (None, None) => None,
        };

        // TS1436: `@` appearing after keyword modifiers (e.g., `public @dec prop`).
        if had_keyword_modifiers && self.is_token(SyntaxKind::AtToken) {
            self.parse_error_at_current_token(
                "Decorators must precede the name and all keywords of property declarations.",
                diagnostic_codes::DECORATORS_MUST_PRECEDE_THE_NAME_AND_ALL_KEYWORDS_OF_PROPERTY_DECLARATIONS,
            );
            if let Some(late_decs) = self.parse_decorators() {
                match combined {
                    Some(ref mut mods) => {
                        mods.nodes.extend(late_decs.nodes);
                        mods.end = late_decs.end;
                    }
                    None => combined = Some(late_decs),
                }
            }
        }

        let mut has_var_let = false;
        let mut has_static = false;
        let mut has_export = false;
        let mut has_declare = false;
        let mut has_accessor = false;
        let mut has_async = false;
        // Source-order index of the first `declare`/`override`/`async`
        // modifier, used below to detect `declare` appearing before
        // `override`/`async`.
        let mut declare_index: Option<usize> = None;
        let mut override_index: Option<usize> = None;
        let mut async_index: Option<usize> = None;
        if let Some(ref mods) = combined {
            for (i, &idx) in mods.nodes.iter().enumerate() {
                if let Some(node) = self.arena.get(idx)
                    && let Some(kind) = SyntaxKind::try_from_u16(node.kind)
                {
                    match kind {
                        SyntaxKind::VarKeyword | SyntaxKind::LetKeyword => has_var_let = true,
                        SyntaxKind::StaticKeyword => has_static = true,
                        SyntaxKind::ExportKeyword => has_export = true,
                        SyntaxKind::DeclareKeyword => {
                            has_declare = true;
                            declare_index.get_or_insert(i);
                        }
                        SyntaxKind::AccessorKeyword => has_accessor = true,
                        SyntaxKind::AsyncKeyword => {
                            has_async = true;
                            async_index.get_or_insert(i);
                        }
                        SyntaxKind::OverrideKeyword => {
                            override_index.get_or_insert(i);
                        }
                        _ => {}
                    }
                }
            }
        }
        let declare_before_override =
            matches!((declare_index, override_index), (Some(d), Some(o)) if d < o);
        let declare_before_async =
            matches!((declare_index, async_index), (Some(d), Some(a)) if d < a);

        ClassMemberModifierSet {
            modifiers: combined,
            has_decorators,
            has_var_let,
            has_static,
            has_export,
            has_declare,
            has_accessor,
            has_async,
            declare_before_override,
            declare_before_async,
            async_declare_order_conflict_reported,
            declare_duplicate_pos,
            diag_len_before_modifiers,
        }
    }

    /// Parse set accessor with modifiers: static set foo(value) { }
    pub(crate) fn parse_set_accessor_with_modifiers(
        &mut self,
        modifiers: Option<NodeList>,
        start_pos: u32,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::SetKeyword);

        let name = self.parse_property_name();

        let type_parameters = self.is_token(SyntaxKind::LessThanToken).then(|| {
            self.report_accessor_type_parameters_error(name);
            self.parse_type_parameters()
        });

        let had_open_paren = self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if self.is_token(SyntaxKind::CloseParenToken) {
            Self::make_node_list(vec![])
        } else {
            self.parse_parameter_list()
        };
        self.parse_expected(SyntaxKind::CloseParenToken);

        // TS1049: A 'set' accessor must have exactly one parameter. tsc's
        // `checkGrammarAccessor` reports the count error before the other
        // `set`-specific grammar checks, so a wrong count suppresses them.
        let count_error =
            had_open_paren && self.report_set_accessor_parameter_count(name, &parameters);

        // TS1051: A 'set' accessor cannot have an optional parameter.
        self.report_set_accessor_optional_parameter(&parameters, count_error);

        // Parse return type annotation for error recovery (tsc preserves it in JS output).
        // Setters cannot legally have return type annotations, but we store it so the
        // emitter can preserve it.
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            // TS1095, suppressed when TS1049 already fired.
            self.report_set_accessor_return_type_annotation(name, count_error);
            // Use parse_return_type to match tsc, which parses type predicates
            // even in invalid setter return types
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        let body = self.parse_accessor_body(&modifiers);

        let end_pos = self.token_full_start();
        self.arena.add_accessor(
            syntax_kind_ext::SET_ACCESSOR,
            start_pos,
            end_pos,
            crate::parser::node::AccessorData {
                modifiers,
                name,
                type_parameters,
                parameters,
                type_annotation,
                body,
            },
        )
    }

    /// TS1049: a `set` accessor must have exactly one parameter. Mirrors tsc
    /// `checkGrammarAccessor` via `doesAccessorHaveCorrectParameterCount`: the
    /// value-parameter count is correct only when it is exactly one, or two
    /// when the first parameter is a `this` parameter (a `this` parameter does
    /// not count toward the value parameter). Reported on the accessor name,
    /// like tsc's `grammarErrorOnNode(accessor.name, …)`.
    ///
    /// Returns whether the diagnostic fired so callers can suppress the later
    /// `set`-specific grammar checks (return type, optional parameter), matching
    /// tsc's single-error early return once the count is already wrong.
    pub(crate) fn report_set_accessor_parameter_count(
        &mut self,
        name: NodeIndex,
        parameters: &NodeList,
    ) -> bool {
        let count = parameters.nodes.len();
        if count == 1 || (count == 2 && self.first_parameter_is_this(parameters)) {
            return false;
        }
        use tsz_common::diagnostics::diagnostic_codes;
        if let Some(name_node) = self.arena.get(name) {
            self.parse_error_at(
                name_node.pos,
                name_node.end - name_node.pos,
                "A 'set' accessor must have exactly one parameter.",
                diagnostic_codes::A_SET_ACCESSOR_MUST_HAVE_EXACTLY_ONE_PARAMETER,
            );
        } else {
            self.parse_error_at_current_token(
                "A 'set' accessor must have exactly one parameter.",
                diagnostic_codes::A_SET_ACCESSOR_MUST_HAVE_EXACTLY_ONE_PARAMETER,
            );
        }
        true
    }

    /// Whether a signature's first parameter is a `this` parameter.
    ///
    /// A `this` parameter is not a value parameter, so every accessor arity rule
    /// discounts it. Shared by the `get` and `set` arity checks so the two
    /// cannot drift apart.
    pub(crate) fn first_parameter_is_this(&self, parameters: &NodeList) -> bool {
        parameters.nodes.first().is_some_and(|&param_idx| {
            let name_idx = match self.arena.get_parameter_at(param_idx) {
                Some(param) => param.name,
                None => return false,
            };
            self.arena
                .get(name_idx)
                .is_some_and(|name_node| name_node.kind == SyntaxKind::ThisKeyword as u16)
        })
    }

    /// TS1054: a `get` accessor cannot have parameters. The counterpart to
    /// `report_set_accessor_parameter_count`, and discounting a leading `this`
    /// parameter for the same reason: tsc's `checkGrammarAccessor` reads the
    /// accessor's *value* parameters, so a getter whose only parameter is `this`
    /// has correct arity and draws `TS2784` alone, with no `TS1054`.
    ///
    /// Reported on the accessor name, like tsc's `grammarErrorOnNode(accessor.name, …)`.
    ///
    /// Returns whether the diagnostic fired.
    pub(crate) fn report_get_accessor_parameter_count(
        &mut self,
        name: NodeIndex,
        parameters: &NodeList,
    ) -> bool {
        let count = parameters.nodes.len();
        if count == 0 || (count == 1 && self.first_parameter_is_this(parameters)) {
            return false;
        }
        use tsz_common::diagnostics::diagnostic_codes;
        if let Some(name_node) = self.arena.get(name) {
            self.parse_error_at(
                name_node.pos,
                name_node.end - name_node.pos,
                "A 'get' accessor cannot have parameters.",
                diagnostic_codes::A_GET_ACCESSOR_CANNOT_HAVE_PARAMETERS,
            );
        } else {
            self.parse_error_at_current_token(
                "A 'get' accessor cannot have parameters.",
                diagnostic_codes::A_GET_ACCESSOR_CANNOT_HAVE_PARAMETERS,
            );
        }
        true
    }

    /// TS1051: a `set` accessor cannot have an optional parameter. tsc anchors
    /// the error at the `?` token, which begins at the parameter name's end.
    ///
    /// Suppressed when the parameter count was already wrong, matching tsc's
    /// single-error early return out of `checkGrammarAccessor`.
    pub(crate) fn report_set_accessor_optional_parameter(
        &mut self,
        parameters: &NodeList,
        count_error: bool,
    ) {
        if count_error {
            return;
        }
        let Some(&first_param) = parameters.nodes.first() else {
            return;
        };
        let Some(param_node) = self.arena.get(first_param) else {
            return;
        };
        let data_idx = param_node.data_index as usize;
        let Some(param_data) = self.arena.parameters.get(data_idx) else {
            return;
        };
        if !param_data.question_token {
            return;
        }
        use tsz_common::diagnostics::diagnostic_codes;
        let question_pos = self
            .arena
            .get(param_data.name)
            .map_or(param_node.pos, |name_node| name_node.end);
        self.parse_error_at(
            question_pos,
            1, // `?` is a single character
            "A 'set' accessor cannot have an optional parameter.",
            diagnostic_codes::A_SET_ACCESSOR_CANNOT_HAVE_AN_OPTIONAL_PARAMETER,
        );
    }

    /// TS1095: a `set` accessor cannot have a return type annotation. Reported
    /// on the accessor name, and suppressed when the parameter count was already
    /// wrong, matching tsc's single-error early return.
    pub(crate) fn report_set_accessor_return_type_annotation(
        &mut self,
        name: NodeIndex,
        count_error: bool,
    ) {
        if count_error {
            return;
        }
        use tsz_common::diagnostics::diagnostic_codes;
        if let Some(name_node) = self.arena.get(name) {
            self.parse_error_at(
                name_node.pos,
                name_node.end - name_node.pos,
                "A 'set' accessor cannot have a return type annotation.",
                diagnostic_codes::A_SET_ACCESSOR_CANNOT_HAVE_A_RETURN_TYPE_ANNOTATION,
            );
        } else {
            self.parse_error_at_current_token(
                "A 'set' accessor cannot have a return type annotation.",
                diagnostic_codes::A_SET_ACCESSOR_CANNOT_HAVE_A_RETURN_TYPE_ANNOTATION,
            );
        }
    }

    /// Parse class members
    pub(crate) fn parse_class_members(&mut self) -> NodeList {
        use tsz_common::diagnostics::diagnostic_codes;

        let mut members = Vec::new();

        while !matches!(
            self.token(),
            SyntaxKind::CloseBraceToken | SyntaxKind::EndOfFileToken
        ) {
            if let Some(close_pos) = self.class_member_list_outer_declaration_recovery_close_pos() {
                self.parse_error_at(
                    close_pos,
                    1,
                    "Declaration or statement expected.",
                    diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                );
                self.suppress_next_missing_class_close_brace_error_once = true;
                break;
            }

            if self.is_token(SyntaxKind::TryKeyword) && self.look_ahead_is_try_block_same_line() {
                self.parse_error_at_current_token(
                    "Unexpected token. A constructor, method, accessor, or property was expected.",
                    diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
                );
                break;
            }

            if self.is_token(SyntaxKind::OpenBraceToken) {
                self.parse_error_at_current_token(
                    "Unexpected token. A constructor, method, accessor, or property was expected.",
                    diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
                );
                self.suppress_next_missing_class_close_brace_error_once = true;
                break;
            }

            if self.recover_module_like_class_member_as_outer_statement() {
                break;
            }

            let member = self.parse_class_member();
            if member.is_some() {
                let recovered_invalid_if_member =
                    self.class_member_is_recovered_invalid_if_method(member);
                // Don't consume trailing semicolon if the member itself is a
                // SemicolonClassElement — that would eat the next standalone `;`.
                let is_semi_element = self
                    .arena
                    .get(member)
                    .is_some_and(|n| n.kind == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT);
                if !is_semi_element {
                    self.parse_optional(SyntaxKind::SemicolonToken);
                }
                members.push(member);

                if recovered_invalid_if_member
                    && matches!(
                        self.token(),
                        SyntaxKind::CatchKeyword | SyntaxKind::FinallyKeyword
                    )
                {
                    break;
                }

                if recovered_invalid_if_member
                    && matches!(
                        self.token(),
                        SyntaxKind::ExclamationEqualsToken
                            | SyntaxKind::ExclamationEqualsEqualsToken
                            | SyntaxKind::EqualsEqualsToken
                            | SyntaxKind::EqualsEqualsEqualsToken
                            | SyntaxKind::LessThanToken
                            | SyntaxKind::LessThanEqualsToken
                            | SyntaxKind::GreaterThanToken
                            | SyntaxKind::GreaterThanEqualsToken
                    )
                {
                    self.suppress_next_missing_class_close_brace_error_once = true;
                    break;
                }

                if self.is_token(SyntaxKind::OpenBraceToken)
                    && !self.scanner.has_preceding_line_break()
                    && self
                        .arena
                        .get(member)
                        .and_then(|node| self.arena.get_property_decl(node))
                        .is_some_and(|prop| prop.initializer.is_some())
                {
                    self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
                    self.suppress_next_missing_class_close_brace_error_once = true;
                    break;
                }

                if self.is_token(SyntaxKind::ColonToken) {
                    self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
                    self.next_token();
                    continue;
                }

                // After a successfully parsed member without a trailing semicolon,
                // if the next token cannot start a new class member, emit TS1068
                // and skip. This matches tsc's parseList/abortParsingListOrMoveToNextToken
                // behavior for ClassMembers context. If a prior TS1005 was already emitted
                // at this exact position (from parseSemicolon within the member), the
                // parse_error_at dedup will suppress this TS1068, preserving the TS1005.
                if !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                    && !self.is_token(SyntaxKind::SemicolonToken)
                    && !self.is_token(SyntaxKind::AtToken) // decorator
                    && !self.is_token(SyntaxKind::AsteriskToken) // generator method
                    && !self.is_property_name()
                {
                    if self.is_token(SyntaxKind::Unknown) {
                        self.parse_error_at_current_token(
                            tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                            diagnostic_codes::INVALID_CHARACTER,
                        );
                    } else {
                        self.parse_error_at_current_token(
                            "Unexpected token. A constructor, method, accessor, or property was expected.",
                            diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
                        );
                    }
                    self.next_token();
                }
            }
        }

        Self::make_node_list(members)
    }

    fn class_member_list_outer_declaration_recovery_close_pos(&mut self) -> Option<u32> {
        if !self.is_token(SyntaxKind::ClassKeyword)
            || !self.scanner.has_preceding_line_break()
            || !self.look_ahead_next_is_identifier_or_keyword_on_same_line()
        {
            return None;
        }

        self.previous_significant_close_brace_pos_ending_at(self.scanner.get_token_full_start())
    }

    fn previous_significant_close_brace_pos_ending_at(&self, token_end: usize) -> Option<u32> {
        let close_pos = token_end.checked_sub(1)?;
        (self.get_source_text().as_bytes().get(close_pos) == Some(&b'}'))
            .then(|| self.u32_from_usize(close_pos))
    }

    fn look_ahead_is_try_block_same_line(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let is_try_block =
            self.is_token(SyntaxKind::OpenBraceToken) && !self.scanner.has_preceding_line_break();
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_try_block
    }

    fn recover_invalid_character_class_member(&mut self) {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        if self.current_unknown_starts_braced_unicode_escape_debris() {
            self.parse_error_at_current_token(
                diagnostic_messages::INVALID_CHARACTER,
                diagnostic_codes::INVALID_CHARACTER,
            );
            self.next_token();
            if self.is_identifier_or_keyword() && self.scanner.get_token_text_ref() == "u" {
                self.parse_error_at_current_token(
                    "Unexpected keyword or identifier.",
                    diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                );
                self.next_token();
            }
            return;
        }

        while self.is_token(SyntaxKind::Unknown) {
            self.parse_error_at_current_token(
                diagnostic_messages::INVALID_CHARACTER,
                diagnostic_codes::INVALID_CHARACTER,
            );
            self.next_token();
        }

        if matches!(
            self.token(),
            SyntaxKind::ColonToken
                | SyntaxKind::QuestionToken
                | SyntaxKind::ExclamationToken
                | SyntaxKind::EqualsToken
        ) {
            while !matches!(
                self.token(),
                SyntaxKind::SemicolonToken
                    | SyntaxKind::CloseBraceToken
                    | SyntaxKind::EndOfFileToken
            ) {
                self.next_token();
            }
            self.parse_optional(SyntaxKind::SemicolonToken);
        }
    }

    /// Parse a single class member
    pub(crate) fn parse_class_member(&mut self) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;
        let start_pos = self.token_pos();

        // Clear any leftover recovered-clause yield suppression from the
        // previous member; it is (re)armed below only for a misplaced
        // `case`/`default` clause and must not leak into sibling members.
        self.suppress_recovered_clause_member_yield_grammar = false;

        if self.is_token(SyntaxKind::SemicolonToken) {
            let end_pos = self.token_end();
            self.next_token();
            return self.arena.add_token(
                syntax_kind_ext::SEMICOLON_CLASS_ELEMENT,
                start_pos,
                end_pos,
            );
        }

        // Note: Reserved keywords like `if`, `for`, `delete`, `function`, etc. are valid
        // property names in class bodies (e.g., `class C { delete; for; if() {} }`).
        // We do NOT reject them here — they flow through to normal class member parsing
        // where is_property_name() correctly accepts them.

        if self.is_token(SyntaxKind::Unknown) {
            self.recover_invalid_character_class_member();
            return NodeIndex::NONE;
        }

        // `case` and `default` are valid property names by themselves, but when
        // followed by another property name on the same line they are usually a
        // misplaced switch clause in a class body. Match tsc's class-member list
        // recovery by reporting TS1068 at the clause keyword and retrying from
        // the following token.
        if matches!(
            self.token(),
            SyntaxKind::CaseKeyword | SyntaxKind::DefaultKeyword
        ) && self.look_ahead_is_property_name_same_line()
        {
            self.parse_error_at_current_token(
                "Unexpected token. A constructor, method, accessor, or property was expected.",
                diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
            );
            self.next_token();
            // tsc keeps the recovered clause body as a class member (it still
            // emits, e.g. `case d = () => {...}` -> `this.d = () => {...}`) but
            // does not run the post-parse grammar checks on it, so the
            // yield-outside-generator check (TS1163) does not fire. tsz emits
            // TS1163 eagerly in the parser, so suppress it while this recovered
            // member is parsed. The flag is reset at the top of the next
            // `parse_class_member`, scoping it to exactly this member.
            self.suppress_recovered_clause_member_yield_grammar = true;
        }

        // Handle bare `#` that can't become a PrivateIdentifier.
        // Preserve standalone `#` as a recovered private name at boundaries.
        if self.is_token(SyntaxKind::HashToken) {
            let rescanned = self.scanner.re_scan_hash_token();
            if rescanned != SyntaxKind::PrivateIdentifier {
                self.report_bare_hash_invalid_character();
                if self.bare_hash_is_followed_by_statement_boundary() {
                    self.current_token = SyntaxKind::PrivateIdentifier;
                } else {
                    self.next_token();
                    return NodeIndex::NONE;
                }
            } else {
                self.current_token = rescanned;
            }
        }

        let decorators = self.parse_decorators();

        // If decorators were found before a static block, emit TS1206
        // TSC anchors this error at the decorator position, not the `static` keyword.
        if decorators.is_some()
            && self.is_token(SyntaxKind::StaticKeyword)
            && self.look_ahead_is_static_block()
        {
            if let Some(ref dec_list) = decorators
                && let Some(&first_dec_idx) = dec_list.nodes.first()
                && let Some(dec_node) = self.arena.get(first_dec_idx)
            {
                let start = dec_node.pos;
                let length = dec_node.end.saturating_sub(dec_node.pos);
                self.parse_error_at(
                    start,
                    length,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
            }
            return self.parse_static_block();
        }

        // Handle static block: static { ... }
        if self.is_token(SyntaxKind::StaticKeyword) && self.look_ahead_is_static_block() {
            return self.parse_static_block();
        }

        if matches!(
            self.token(),
            SyntaxKind::GlobalKeyword | SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword
        ) && self.look_ahead_is_module_declaration()
        {
            self.recover_invalid_module_like_class_member();
            return NodeIndex::NONE;
        }

        if self.look_ahead_is_class_body_function_statement() {
            self.recover_invalid_class_body_function_statement();
            return NodeIndex::NONE;
        }

        if self.look_ahead_is_class_body_variable_statement() {
            self.recover_invalid_class_body_variable_statement();
            return NodeIndex::NONE;
        }

        let mods = self.scan_class_member_modifier_phase(decorators);

        // Handle static block after modifiers: { ... }
        // Case 1: `static` not yet consumed (no preceding modifiers or only decorators)
        if self.is_token(SyntaxKind::StaticKeyword) && self.look_ahead_is_static_block() {
            if let Some(ref ml) = mods.modifiers {
                // Truncate modifier-ordering diagnostics (TS1028/TS1029) emitted
                // during parse_class_member_modifiers — tsc only emits TS1184 here.
                self.parse_diagnostics
                    .truncate(mods.diag_len_before_modifiers);
                if let Some(first_node) = self.arena.get(ml.nodes[0]) {
                    self.parse_error_at(
                        first_node.pos,
                        first_node.end - first_node.pos,
                        "Modifiers cannot appear here.",
                        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
                    );
                }
            }
            return self.parse_static_block();
        }
        // Case 2: `static` was consumed as a modifier and `{` follows (e.g. `async static {`)
        // The last modifier is `static` and current token is `{` — this is a static block
        // with invalid preceding modifiers.
        if self.is_token(SyntaxKind::OpenBraceToken)
            && let Some(ref ml) = mods.modifiers
        {
            let last_is_static = ml
                .nodes
                .last()
                .and_then(|&idx| self.arena.get(idx))
                .is_some_and(|n| n.kind == SyntaxKind::StaticKeyword as u16);
            if last_is_static {
                // Truncate modifier-ordering diagnostics — tsc only emits TS1184.
                self.parse_diagnostics
                    .truncate(mods.diag_len_before_modifiers);
                // Emit TS1184 at the first modifier's position (matches tsc).
                if let Some(first_node) = self.arena.get(ml.nodes[0]) {
                    self.parse_error_at(
                        first_node.pos,
                        first_node.end - first_node.pos,
                        "Modifiers cannot appear here.",
                        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
                    );
                }
                return self.parse_static_block();
            }
        }

        // ── Member construction ───────────────────────────────────────────────
        self.construct_class_member(start_pos, mods)
    }

    /// Construct a class member after modifiers have been scanned and classified.
    ///
    /// Dispatches to constructor, get/set accessor, index signature, and
    /// ordinary method/property declaration paths.
    fn construct_class_member(
        &mut self,
        start_pos: u32,
        mods: ClassMemberModifierSet,
    ) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        // Handle constructor — but not when var/let is in modifiers (invalid pattern).
        if self.is_token(SyntaxKind::ConstructorKeyword) && !mods.has_var_let {
            // TS1206: Decorators are not valid on constructors.
            if mods.has_decorators {
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
            }

            if mods.has_static {
                self.emit_modifier_error_on_constructor(
                    &mods.modifiers,
                    SyntaxKind::StaticKeyword,
                    "'static' modifier cannot appear on a constructor declaration.",
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
                );
            }

            // TS1031: tsc anchors at the modifier keyword via grammarErrorOnNode(modifier)
            if mods.has_export {
                self.emit_modifier_error_on_constructor(
                    &mods.modifiers,
                    SyntaxKind::ExportKeyword,
                    "'export' modifier cannot appear on class elements of this kind.",
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_CLASS_ELEMENTS_OF_THIS_KIND,
                );
            } else if mods.has_declare {
                self.emit_modifier_error_on_constructor(
                    &mods.modifiers,
                    SyntaxKind::DeclareKeyword,
                    "'declare' modifier cannot appear on class elements of this kind.",
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_CLASS_ELEMENTS_OF_THIS_KIND,
                );
            }

            // TS1275: 'accessor' modifier can only appear on a property declaration.
            if mods.has_accessor {
                self.emit_accessor_modifier_only_on_property_error(&mods.modifiers);
            }

            return self.parse_constructor_with_modifiers(mods.modifiers);
        }

        // Handle generator methods: *foo() or async *#bar()
        let asterisk_token = self.parse_optional(SyntaxKind::AsteriskToken);

        // Handle get accessor: get foo() { }
        if !asterisk_token && self.is_token(SyntaxKind::GetKeyword) && self.look_ahead_is_accessor()
        {
            // TS1031: 'declare' modifier cannot appear on class elements of this kind
            if mods.has_declare {
                self.emit_declare_on_non_property_error(&mods.modifiers);
            }
            // TS1275: 'accessor' modifier can only appear on a property declaration.
            if mods.has_accessor {
                self.emit_accessor_modifier_only_on_property_error(&mods.modifiers);
            }
            let saved_member_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_CLASS_MEMBER_NAME;
            let accessor = self.parse_get_accessor_with_modifiers(mods.modifiers, start_pos);
            self.context_flags = saved_member_flags;
            return accessor;
        }

        // Handle set accessor: set foo(value) { }
        if !asterisk_token && self.is_token(SyntaxKind::SetKeyword) && self.look_ahead_is_accessor()
        {
            // TS1031: 'declare' modifier cannot appear on class elements of this kind
            if mods.has_declare {
                self.emit_declare_on_non_property_error(&mods.modifiers);
            }
            // TS1275: 'accessor' modifier can only appear on a property declaration.
            if mods.has_accessor {
                self.emit_accessor_modifier_only_on_property_error(&mods.modifiers);
            }
            let saved_member_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_CLASS_MEMBER_NAME;
            let accessor = self.parse_set_accessor_with_modifiers(mods.modifiers, start_pos);
            self.context_flags = saved_member_flags;
            return accessor;
        }

        // Handle index signatures: [key: Type]: ValueType
        if self.is_token(SyntaxKind::OpenBracketToken) && self.look_ahead_is_index_signature() {
            let sig = self.parse_index_signature_with_modifiers(mods.modifiers, start_pos);
            self.parse_semicolon();
            return sig;
        }

        // `function foo() {}` inside a class body is handled by
        // `look_ahead_is_class_body_function_statement` above and recovers
        // via `recover_invalid_module_like_class_member` to match tsc.
        // Keep `function` as a potential member name here for valid forms like
        // `function() {}` or `function;`.

        // `const x = 1` is invalid; `const() {}` is a valid method name. Trigger only when
        // an identifier/bracket follows on the same line (no ASI), indicating a modifier misuse.
        if matches!(
            self.token(),
            SyntaxKind::ConstKeyword | SyntaxKind::LetKeyword | SyntaxKind::VarKeyword
        ) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            let next_token = self.token();
            let has_line_break = self.scanner.has_preceding_line_break();
            self.scanner.restore_state(snapshot);
            self.current_token = current;

            if !has_line_break
                && matches!(
                    next_token,
                    SyntaxKind::Identifier
                        | SyntaxKind::PrivateIdentifier
                        | SyntaxKind::OpenBracketToken
                )
            {
                self.parse_error_at_current_token(
                    "A class member cannot have the 'const', 'let', or 'var' keyword.",
                    diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
                );
                self.next_token();
            }
        }

        // `try { ... }` is not a valid class member even after modifiers like `public`. Emit
        // TS1068 rather than cascading into TS1434/TS1435. Unlike const/let/var, `try` is a
        // valid property name so we only emit the diagnostic and let it continue as a name.
        if self.is_token(SyntaxKind::TryKeyword) && self.look_ahead_is_try_block_same_line() {
            self.parse_error_at_current_token(
                "Unexpected token. A constructor, method, accessor, or property was expected.",
                diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
            );
        }

        // `class D {}` / `enum E {}` are invalid class members even after modifiers like `public`.
        // `class;` and `class(){}` are valid property/method names — only trigger when an
        // identifier follows on the same line (distinguishes declaration from member name).
        if matches!(
            self.token(),
            SyntaxKind::ClassKeyword | SyntaxKind::EnumKeyword
        ) && self.look_ahead_next_is_identifier_or_keyword_on_same_line()
        {
            self.parse_error_at_current_token(
                "Unexpected token. A constructor, method, accessor, or property was expected.",
                diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
            );
            self.next_token(); // skip class/enum keyword
            self.next_token(); // skip the identifier name
            // Skip extends/implements clauses (e.g., `class D extends E {`).
            while self.is_identifier_or_keyword()
                && !self.is_token(SyntaxKind::OpenBraceToken)
                && !self.is_token(SyntaxKind::CloseBraceToken)
                && !self.is_token(SyntaxKind::SemicolonToken)
                && !self.is_token(SyntaxKind::EndOfFileToken)
            {
                self.next_token();
                if self.is_token(SyntaxKind::CommaToken) {
                    self.next_token();
                }
            }
            if self.is_token(SyntaxKind::OpenBraceToken) {
                // DON'T consume the final `}` — leave it for the outer class body to use
                // as its closing brace (tsc error recovery behavior).
                let mut depth = 1u32;
                self.next_token(); // consume `{`
                while depth > 0 && !self.is_token(SyntaxKind::EndOfFileToken) {
                    if self.is_token(SyntaxKind::OpenBraceToken) {
                        depth += 1;
                    } else if self.is_token(SyntaxKind::CloseBraceToken) {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    self.next_token();
                }
            }
            return NodeIndex::NONE;
        }

        // Parse member name
        let name_saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_CLASS_MEMBER_NAME;
        // Note: Do NOT set CONTEXT_FLAG_GENERATOR or CONTEXT_FLAG_ASYNC here.
        // The yield/await context must only be active during the method body
        // (parameters + block), not during property name parsing.  Otherwise
        // `yield` inside a computed property name like `async * [yield]()`
        // would be parsed as a YieldExpression instead of an Identifier.
        // The generator/async flags are correctly set later (inside construct_class_member_method).
        // However, track the generator asterisk so we can suppress TS1213
        // for `yield` in computed property names of generator methods — tsc
        // does not emit TS1213 in this position.
        if asterisk_token {
            self.context_flags |= CONTEXT_FLAG_GENERATOR_MEMBER_NAME;
        }
        let has_modifiers = mods.modifiers.is_some();
        let name = if self.is_property_name() {
            self.parse_property_name()
        } else if has_modifiers
            && self.is_token(SyntaxKind::OpenBraceToken)
            && self.next_token_is_open_bracket()
        {
            let token_start = self.token_pos();
            let decl_pos = if token_start > 0 { token_start - 1 } else { 0 };
            self.parse_error_at(
                decl_pos,
                1,
                "Declaration expected.",
                diagnostic_codes::DECLARATION_EXPECTED,
            );
            self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
            self.next_token();
            while !self.is_token(SyntaxKind::CloseBraceToken)
                && !self.is_token(SyntaxKind::EndOfFileToken)
            {
                let before = self.token_pos();
                let _ = self.parse_statement();
                if self.token_pos() == before {
                    self.next_token();
                }
            }
            self.context_flags = name_saved_flags;
            return NodeIndex::NONE;
        } else if asterisk_token {
            // After asterisk (*), we expect an identifier (method name).
            // Create a missing identifier and continue parsing the method
            // body so we don't produce cascading TS1068/TS1128 errors.
            self.error_identifier_expected();
            let pos = self.token_pos();
            self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                pos,
                pos,
                node::IdentifierData {
                    atom: AstAtom::NONE,
                    escaped_text: IdentText::empty(),
                    original_text: None,
                },
            )
        } else {
            if has_modifiers {
                // TSC emits TS1146 at the position where the name was expected
                // (just before the current token) and TS1005 at the current token.
                // We must emit them at different positions so the dedup logic
                // in parse_error_at doesn't suppress the second one.
                let token_start = self.token_pos();
                let decl_pos = if token_start > 0 { token_start - 1 } else { 0 };
                self.parse_error_at(
                    decl_pos,
                    1,
                    "Declaration expected.",
                    diagnostic_codes::DECLARATION_EXPECTED,
                );
                self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
            } else {
                self.parse_error_at_current_token(
                    "Unexpected token. A constructor, method, accessor, or property was expected.",
                    diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
                );
            }
            self.context_flags = name_saved_flags;
            self.next_token();
            return NodeIndex::NONE;
        };
        self.context_flags = name_saved_flags;

        // TS18012: '#constructor' is a reserved word
        if let Some(name_node) = self.arena.get(name)
            && name_node.kind == SyntaxKind::PrivateIdentifier as u16
            && let Some(ident) = self.arena.get_identifier(name_node)
            && (ident.escaped_text == "constructor" || ident.escaped_text == "#constructor")
        {
            self.parse_error_at(
                name_node.pos,
                name_node.end - name_node.pos,
                "'#constructor' is a reserved word.",
                diagnostic_codes::CONSTRUCTOR_IS_A_RESERVED_WORD,
            );
        }

        // Parse optional ? or ! after property name
        let question_token_pos = self.token_pos();
        let question_token_end = self.token_end();
        let question_token = self.parse_optional(SyntaxKind::QuestionToken);
        let exclamation_token = if question_token {
            false
        } else {
            self.parse_optional(SyntaxKind::ExclamationToken)
        };

        // TS1436: Decorator after property name (e.g., `private prop @decorator`).
        // Detect `@` after the member name where `:`, `=`, `;`, `(`, or `<` is expected.
        // Only when `@` is on the SAME line — if on a new line, ASI applies and the
        // property ends normally; the `@` starts a new decorated member.
        let late_property_name_decorator =
            self.is_token(SyntaxKind::AtToken) && !self.scanner.has_preceding_line_break();
        if late_property_name_decorator {
            self.parse_error_at_current_token(
                "Decorators must precede the name and all keywords of property declarations.",
                diagnostic_codes::DECORATORS_MUST_PRECEDE_THE_NAME_AND_ALL_KEYWORDS_OF_PROPERTY_DECLARATIONS,
            );
            // Keep the decorator token in the stream. TSC reports TS1436 for
            // the property, then recovers by applying the decorator to the
            // following class member.
        }

        let method_saved_flags = self.context_flags;
        self.context_flags &=
            !(CONTEXT_FLAG_ASYNC | CONTEXT_FLAG_GENERATOR | CONTEXT_FLAG_STATIC_BLOCK);
        if mods.has_async {
            self.context_flags |= CONTEXT_FLAG_ASYNC;
        }
        if asterisk_token {
            self.context_flags |= CONTEXT_FLAG_GENERATOR;
        }
        self.context_flags |= CONTEXT_FLAG_FUNCTION_BODY;

        // Check if it's a method or property.
        // Method: foo() or foo<T>().
        // `async *` members always require a member body/parameter list form, so treat
        // asterisk forms as methods even when '(' is missing (for recovery).
        let is_method_like = !mods.has_var_let
            && (asterisk_token
                || self.is_token(SyntaxKind::OpenParenToken)
                || self.is_token(SyntaxKind::LessThanToken));

        // TS1276: An 'accessor' property cannot be declared optional.
        // tsc anchors at the `?` token for properties only; accessor methods
        // report TS1275 instead.
        if !is_method_like && question_token && mods.has_accessor {
            self.parse_error_at(
                question_token_pos,
                question_token_end - question_token_pos,
                "An 'accessor' property cannot be declared optional.",
                diagnostic_codes::AN_ACCESSOR_PROPERTY_CANNOT_BE_DECLARED_OPTIONAL,
            );
        }

        if is_method_like {
            self.construct_class_member_method(
                start_pos,
                mods,
                asterisk_token,
                name,
                question_token,
                method_saved_flags,
            )
        } else if mods.has_var_let
            && (self.is_token(SyntaxKind::OpenParenToken)
                || self.is_token(SyntaxKind::LessThanToken))
        {
            // var/let modifier followed by () - emit errors and attempt recovery
            use tsz_common::diagnostics::diagnostic_codes;

            // tsc parses this dropped member as trailing statements
            // (`var <name>;` plus an arrow recovered from `() { }`) and emits
            // them after the class. Track whether the member matches that
            // exact shape — `var` (not `let`), plain identifier name, empty
            // `()`, no type parameters or return type, empty `{ }` — and
            // record it on the arena for the class emitters to consume.
            let recovered_var_fn_name = self.class_member_var_fn_recovery_name(&mods, name);
            let mut matches_var_fn_recovery_shape = recovered_var_fn_name.is_some();

            // Emit error for '('
            if self.is_token(SyntaxKind::OpenParenToken) {
                self.parse_error_at_current_token("',' expected.", diagnostic_codes::EXPECTED);
                // Consume '(' for recovery
                self.next_token();

                // Parse parameters (may be empty)
                let params = self.parse_parameter_list();
                matches_var_fn_recovery_shape &= params.nodes.is_empty();

                // Consume ')' without emitting an error
                self.parse_expected(SyntaxKind::CloseParenToken);
            } else {
                matches_var_fn_recovery_shape = false;
            }

            // Skip optional type parameters and return type for recovery
            if self.is_token(SyntaxKind::LessThanToken) {
                matches_var_fn_recovery_shape = false;
                let _ = self.parse_type_parameters();
            }
            if self.parse_optional(SyntaxKind::ColonToken) {
                matches_var_fn_recovery_shape = false;
                let _ = self.parse_return_type();
            }

            // Emit error for '{' - "'=>' expected"
            if self.is_token(SyntaxKind::OpenBraceToken) {
                self.parse_error_at_current_token("'=>' expected.", diagnostic_codes::EXPECTED);
                self.next_token(); // Consume '{'
                // Empty `{ }` body: the close brace immediately follows.
                matches_var_fn_recovery_shape &= self.is_token(SyntaxKind::CloseBraceToken);
            } else {
                matches_var_fn_recovery_shape = false;
            }

            // Parse a statement to balance braces
            // This consumes '{ }' so the class members loop doesn't see them
            self.context_flags = method_saved_flags;
            let _ = self.parse_statement();

            if matches_var_fn_recovery_shape && let Some(recovery_name) = recovered_var_fn_name {
                self.arena.class_body_var_fn_recoveries.push(
                    crate::parser::node::ClassBodyVarFnRecovery {
                        pos: start_pos,
                        name: recovery_name,
                    },
                );
            }

            // Return NONE to indicate this is not a valid member
            NodeIndex::NONE
        } else {
            if let Some(dup_pos) = mods.declare_duplicate_pos {
                // TS1030: duplicate `declare` on a property. tsc's
                // `checkGrammarModifiers` reports `_0_modifier_already_seen` at
                // the second `declare` and `return`s, so the declare/override
                // (TS1243) and declare/async (TS1040) conflicts below — and the
                // ambient-initializer check (TS1039) in the checker — are all
                // suppressed. Anchored at the duplicate keyword, width of
                // `declare`, matching tsc's `grammarErrorOnNode(modifier)`.
                self.parse_error_at(
                    dup_pos,
                    "declare".len() as u32,
                    "'declare' modifier already seen.",
                    diagnostic_codes::MODIFIER_ALREADY_SEEN,
                );
            } else {
                // TS1243: 'override' modifier cannot be used with 'declare' modifier.
                // `declare override p` — the member kind is only known now (a plain
                // property), which is the one member-local-`declare` host tsc
                // allows for `override` to coexist with at all. The reverse order
                // (`override declare p`) is already reported eagerly while scanning
                // modifiers (TS1040, ambient conflict), regardless of member kind.
                if mods.declare_before_override {
                    self.emit_modifier_error_on_constructor(
                        &mods.modifiers,
                        SyntaxKind::OverrideKeyword,
                        "'override' modifier cannot be used with 'declare' modifier.",
                        diagnostic_codes::MODIFIER_CANNOT_BE_USED_WITH_MODIFIER,
                    );
                }
                // TS1040: 'async' modifier cannot be used in an ambient context.
                // `declare async p` — the member kind is only known now (a plain
                // property), which is the one member-local-`declare` host tsc
                // allows, so this is where the ambient conflict is decidable. The
                // reverse order (`async declare p`) is already reported eagerly
                // while scanning modifiers, regardless of member kind, since tsc
                // resolves that conflict before member kind is even relevant.
                if mods.declare_before_async {
                    self.emit_modifier_error_on_constructor(
                        &mods.modifiers,
                        SyntaxKind::AsyncKeyword,
                        "'async' modifier cannot be used in an ambient context.",
                        diagnostic_codes::MODIFIER_CANNOT_BE_USED_IN_AN_AMBIENT_CONTEXT,
                    );
                }
            }
            self.construct_class_member_property(
                start_pos,
                mods,
                name,
                question_token,
                exclamation_token,
                late_property_name_decorator,
                method_saved_flags,
            )
        }
    }

    /// Recovered declaration name for a class-body member dropped by the
    /// var/let-modifier recovery, when the member can still match tsc's
    /// `var <name>() { }` recovery emit: the invalid modifier must be `var`
    /// (not `let`) and the member name a plain identifier. Prefers the
    /// identifier's original escape spelling for emit parity.
    fn class_member_var_fn_recovery_name(
        &self,
        mods: &ClassMemberModifierSet,
        name: NodeIndex,
    ) -> Option<String> {
        let has_var_modifier = mods.modifiers.as_ref().is_some_and(|list| {
            list.nodes.iter().any(|&idx| {
                self.arena
                    .get(idx)
                    .is_some_and(|node| node.kind == SyntaxKind::VarKeyword as u16)
            })
        });
        if !has_var_modifier {
            return None;
        }
        let name_node = self.arena.get(name)?;
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        if let Some(original) = self
            .arena
            .get_identifier(name_node)
            .and_then(|ident| ident.original_text.clone())
        {
            return Some(String::from(original));
        }
        let text = self.arena.identifier_text(name)?;
        if text.is_empty() {
            return None;
        }
        Some(text.to_string())
    }
}

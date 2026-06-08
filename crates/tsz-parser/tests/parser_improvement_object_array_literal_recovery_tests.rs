//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — object array literal recovery.

use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

/// Return the element count of the first array-literal expression in the parse tree.
fn first_array_literal_element_count(parser: &crate::parser::ParserState) -> Option<usize> {
    let arena = parser.get_arena();
    arena
        .nodes
        .iter()
        .find(|node| node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
        .and_then(|node| arena.get_literal_expr(node))
        .map(|data| data.elements.nodes.len())
}

/// Count expression statements (used to assert that array tails which terminate
/// the literal re-parse as separate statements, matching tsc's recovery shape).
fn expression_statement_count(parser: &crate::parser::ParserState) -> usize {
    let arena = parser.get_arena();
    arena
        .nodes
        .iter()
        .filter(|node| node.kind == syntax_kind_ext::EXPRESSION_STATEMENT)
        .count()
}

#[test]
fn test_object_literal_statement_recovery_after_shorthand_property() {
    let source = "var v = { a\nreturn;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let return_pos = source.find("return").expect("return position") as u32;
    let semicolon_pos = source.rfind(';').expect("semicolon position") as u32;
    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == return_pos
            && diag.message == "',' expected."),
        "Expected missing comma at the statement keyword, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == semicolon_pos
            && diag.message == "':' expected."),
        "Expected missing ':' at the trailing semicolon, got {diagnostics:?}"
    );
    // tsc suppresses '}}' expected at EOF when a recent error (within 1 char)
    // already reported the issue. Matching that behavior here.
}

#[test]
fn test_object_literal_statement_recovery_after_missing_initializer() {
    let source = "var v = { a:\nreturn;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let return_pos = source.find("return").expect("return position") as u32;
    let semicolon_pos = source.rfind(';').expect("semicolon position") as u32;

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == 1109 && diag.start == return_pos),
        "Expected TS1109 at the statement keyword after a missing initializer, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|diag| !(diag.code == 1005
            && diag.start == return_pos
            && diag.message == "',' expected.")),
        "Missing initializer recovery should not inject a comma error at the next statement keyword: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == semicolon_pos
            && diag.message == "':' expected."),
        "Expected missing ':' at the trailing semicolon, got {diagnostics:?}"
    );
    // tsc suppresses '}}' expected at EOF when a recent error (within 1 char)
    // already reported the issue. Matching that behavior here.
}

#[test]
fn test_object_literal_statement_recovery_after_trailing_comma() {
    let source = "var v = { a: 1,\nreturn;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let return_pos = source.find("return").expect("return position") as u32;
    let semicolon_pos = source.rfind(';').expect("semicolon position") as u32;

    assert!(
        diagnostics.iter().all(|diag| !(diag.code == 1005
            && diag.start == return_pos
            && diag.message == "',' expected.")),
        "Trailing-comma recovery should not add an extra comma error at the next statement keyword: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == semicolon_pos
            && diag.message == "':' expected."),
        "Expected missing ':' at the trailing semicolon, got {diagnostics:?}"
    );
    // tsc suppresses '}}' expected at EOF when a recent error (within 1 char)
    // already reported the issue. Matching that behavior here.
}

#[test]
fn test_array_literal_semicolon_recovers_as_missing_comma() {
    let source = "var texCoords = [2, 2, 0.5000001192092895, 0.8749999 ; 403953552, 0.5000001192092895, 0.8749999403953552];";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let semicolon_pos = source.find(';').expect("semicolon position") as u32;
    let close_bracket_pos = source.rfind(']').expect("close bracket position") as u32;

    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == semicolon_pos
            && diag.message == "',' expected."),
        "Expected missing comma at the array literal semicolon, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == close_bracket_pos
            && diag.message == "';' expected."),
        "Expected trailing ';' recovery at the array close bracket, got {diagnostics:?}"
    );
}

/// Structural rule: an array-literal member list terminates at a `;` that
/// cannot begin an array element when the token after the `;` could begin a
/// fresh statement-level element list. The literal closes at the prior
/// boundary and the tail re-parses as a separate statement, matching tsc's
/// recovery node shape. Verified at the AST level (not just diagnostics) and
/// across element shapes so the rule is not keyed on a specific spelling.
#[test]
fn array_literal_terminates_at_semicolon_before_numeric_element() {
    let source = "var v = [1, 2 ; 3, 4];";
    let (parser, _root) = parse_source(source);

    assert_eq!(
        first_array_literal_element_count(&parser),
        Some(2),
        "array should terminate at the `;`, keeping only the elements before it; diagnostics: {:?}",
        parser.get_diagnostics()
    );
    assert!(
        expression_statement_count(&parser) >= 1,
        "the `3, 4` tail after the `;` should re-parse as a statement, not stay inside the array; diagnostics: {:?}",
        parser.get_diagnostics()
    );
    let semicolon_pos = source.find(';').expect("semicolon position") as u32;
    assert!(
        parser
            .get_diagnostics()
            .iter()
            .any(|diag| diag.code == diagnostic_codes::EXPECTED
                && diag.start == semicolon_pos
                && diag.message == "',' expected."),
        "Expected ',' expected at the terminating `;`, got {:?}",
        parser.get_diagnostics()
    );
}

/// Same rule with identifier elements and a renamed binding to prove the
/// behavior is structural (keyed on token kind) and not on numeric literals
/// or specific identifier spellings.
#[test]
fn array_literal_terminates_at_semicolon_before_identifier_element() {
    let source = "var arr = [alpha, beta ; gamma, delta];";
    let (parser, _root) = parse_source(source);

    assert_eq!(
        first_array_literal_element_count(&parser),
        Some(2),
        "array with identifier elements should terminate at the `;`; diagnostics: {:?}",
        parser.get_diagnostics()
    );
    assert!(
        expression_statement_count(&parser) >= 1,
        "the `gamma, delta` tail should re-parse as a statement; diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

/// Negative/boundary case: a `;` immediately before the closing `]` is a
/// mistyped comma, not a list terminator. The single element is preserved and
/// the literal still closes on the `]` (no spurious extra statement, and no
/// dropped element).
#[test]
fn array_literal_semicolon_directly_before_close_bracket_keeps_element() {
    let source = "var v = [first ; ];";
    let (parser, _root) = parse_source(source);

    assert_eq!(
        first_array_literal_element_count(&parser),
        Some(1),
        "trailing `;` before `]` should keep the element, not terminate early; diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

/// Well-formed negative case: a correct array literal must not be terminated
/// early and must not gain any recovery diagnostics.
#[test]
fn well_formed_array_literal_is_not_terminated_early() {
    let source = "var v = [1, 2, 3, 4];";
    let (parser, _root) = parse_source(source);

    assert_eq!(
        first_array_literal_element_count(&parser),
        Some(4),
        "well-formed array literal should keep all elements; diagnostics: {:?}",
        parser.get_diagnostics()
    );
    assert!(
        parser.get_diagnostics().is_empty(),
        "well-formed array literal should not produce recovery diagnostics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_trailing_comma_in_object_literal() {
    // Trailing commas should be allowed in object literals
    let source = r"
const obj = {
    a: 1,
    b: 2,
};
";
    let (parser, _root) = parse_source(source);

    // Should not emit any errors for trailing comma
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no errors for trailing comma in object literal, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_trailing_comma_in_array_literal() {
    // Trailing commas should be allowed in array literals
    let source = r"
const arr = [
    1,
    2,
    3,
];
";
    let (parser, _root) = parse_source(source);

    // Should not emit any errors for trailing comma
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no errors for trailing comma in array literal, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_array_terminated_by_close_paren_emits_comma_expected() {
    // Regression for conformance test
    // `destructuringParameterDeclaration2.ts` line 8:
    //   `a0([1, "string", [["world"]]);`
    // The outer `[` is never closed before the `)`. tsc reports a single TS1005
    // `',' expected.` at the `)`. Before this fix, we reported `']' expected.`
    // because the array-literal loop broke without first emitting the missing-
    // separator diagnostic that tsc's parseDelimitedList unconditionally emits.
    let source = "a0([1, \"string\", [[\"world\"]]);\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();

    let close_paren_pos = source.find(')').expect("`)` is in the source") as u32;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED
                && d.start == close_paren_pos
                && d.message == "',' expected."),
        "expected TS1005 `',' expected.` at the `)`, got {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED
                && d.start == close_paren_pos
                && d.message == "']' expected."),
        "TS1005 `']' expected.` at the `)` should be dedup'd by the comma error, got {diagnostics:?}"
    );
}

#[test]
fn test_array_terminated_by_close_brace_emits_comma_expected() {
    // Sibling case: array literal terminated by an enclosing `}` (e.g. block
    // boundary). Same expectation — tsc reports `,' expected` rather than
    // `]' expected`.
    let source = "{ const x = [1, 2 }\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();

    let close_brace_pos = source.find('}').expect("`}` is in the source") as u32;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED
                && d.start == close_brace_pos
                && d.message == "',' expected."),
        "expected TS1005 `',' expected.` at the `}}`, got {diagnostics:?}"
    );
}

#[test]
fn test_array_terminated_by_close_bracket_keeps_clean_close() {
    // Sanity guard: a normal `[1, 2]` must not gain a spurious comma diagnostic.
    let source = "var a = [1, 2];\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.is_empty(),
        "well-formed array literal must not emit diagnostics, got {diagnostics:?}"
    );
}

#[test]
fn test_object_literal_comma_recovery_after_short_distance_colon_error() {
    // Regression for conformance test
    // `conformance/classes/nestedClassDeclaration.ts`:
    //   `var x = {\n    class C4 {\n    }\n}`
    // tsc emits TWO TS1005 errors here:
    //   - `':' expected.` at column 11 (the `C` of `C4`)
    //   - `',' expected.` at column 14 (the `{`)
    // We previously emitted only the first because our `error_comma_expected`
    // applies a 3-byte distance suppression that swallows the legitimate comma
    // diagnostic when the gap is exactly 3 columns. tsc's `parseErrorAtPosition`
    // dedups only on exact same position; the unexpected-token recovery path in
    // `parse_object_literal` now bypasses the distance gate so it emits.
    let source = "var x = {\n    class C4 {\n    }\n}\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let line2_offset = source.find("    class C4").expect("C4 line is in source") as u32;
    let c4_pos = line2_offset + "    class ".len() as u32; // position of `C` in `C4`
    let open_brace_pos = source.find("C4 {").expect("C4 { is in source") as u32 + 3; // position of `{`

    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED
                && d.start == c4_pos
                && d.message == "':' expected."),
        "expected TS1005 `':' expected.` at `C4`, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED
                && d.start == open_brace_pos
                && d.message == "',' expected."),
        "expected TS1005 `',' expected.` at `{{` after `C4`, got {diagnostics:?}"
    );
}

/// Member nodes of the first object-literal expression in the parse tree.
fn first_object_literal_members(parser: &crate::parser::ParserState) -> Vec<NodeIndex> {
    let arena = parser.get_arena();
    arena
        .nodes
        .iter()
        .find(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
        .and_then(|node| arena.get_literal_expr(node))
        .map(|data| data.elements.nodes.clone())
        .unwrap_or_default()
}

/// Text of an identifier node, or `None` if the node is not an identifier.
fn identifier_text(parser: &crate::parser::ParserState, node: NodeIndex) -> Option<String> {
    let arena = parser.get_arena();
    let n = arena.get(node)?;
    arena.get_identifier(n).map(|id| id.escaped_text.clone())
}

/// Structural rule: when a reserved word is used as an object-literal property
/// name and is followed by a value expression instead of `:`, tsc reports
/// `':' expected.` but still parses the member as a property assignment whose
/// initializer is that value (`{ class C4 {} }` → `class: C4`). It is never a
/// value-less shorthand. Keyed on the token category (reserved word), not the
/// spelling — exercised with `class`, `for`, and `if`.
#[test]
fn reserved_word_property_name_without_colon_recovers_as_property_assignment() {
    for keyword in ["class", "for", "if"] {
        let source = format!("var x = {{\n    {keyword} C4 {{\n    }}\n}}\n");
        let (parser, _root) = parse_source(&source);

        let members = first_object_literal_members(&parser);
        assert!(
            !members.is_empty(),
            "expected at least one recovered member for `{keyword}`, got none"
        );
        let arena = parser.get_arena();
        let first = arena.get(members[0]).expect("first member node");
        assert_eq!(
            first.kind,
            syntax_kind_ext::PROPERTY_ASSIGNMENT,
            "`{keyword}` as a property name with a following value must be a property assignment, not a shorthand"
        );
        let prop = arena
            .get_property_assignment(first)
            .expect("property assignment data");
        assert_eq!(
            identifier_text(&parser, prop.name).as_deref(),
            Some(keyword),
            "property name should be the reserved word `{keyword}`"
        );
        assert_eq!(
            identifier_text(&parser, prop.initializer).as_deref(),
            Some("C4"),
            "`{keyword}` member's value should be the following identifier `C4`"
        );
    }
}

/// Structural rule: a stray operator/punctuation token that cannot start an
/// object-literal element (here the `:` left after recovering `x()?: 1`) is
/// reported via TS1136 and skipped — it must not be folded into a bogus
/// empty-named member. The following numeric literal `1` becomes a property
/// assignment with a missing value (rendered `1:` by the emitter), not a
/// shorthand. Two iteration-variable spellings prove the rule is structural.
#[test]
fn optional_method_then_colon_value_skips_stray_colon_and_keeps_numeric_member() {
    for method in ["x", "method"] {
        let source = format!("var b = {{\n    {method}()?: 1\n}}\n");
        let (parser, _root) = parse_source(&source);

        let members = first_object_literal_members(&parser);
        assert_eq!(
            members.len(),
            2,
            "expected exactly two members (the method and the numeric `1`) for `{method}`, got {members:?}"
        );
        let arena = parser.get_arena();
        let m0 = arena.get(members[0]).expect("method member");
        assert_eq!(
            m0.kind,
            syntax_kind_ext::METHOD_DECLARATION,
            "first member should be the recovered method `{method}()`"
        );
        let m1 = arena.get(members[1]).expect("numeric member");
        assert_eq!(
            m1.kind,
            syntax_kind_ext::PROPERTY_ASSIGNMENT,
            "numeric `1` member must be a property assignment with a missing value"
        );
        let prop = arena
            .get_property_assignment(m1)
            .expect("numeric member property assignment data");
        assert_ne!(
            prop.name, prop.initializer,
            "missing value must be a distinct empty node so the emitter renders `1:`"
        );
    }
}

/// Structural rule: when an identifier shorthand member is immediately followed
/// by a non-comma token (`{ a[1], }`), tsc recovers `a` as shorthand (`',' expected.`
/// at the trailing token) and then parses the next computed/literal member; the
/// missing `:` on that member is reported via `parseErrorAtCurrentToken`, whose
/// dedup is *exact-position only*. tsz must not drop that `:' expected.` even
/// when the prior `',' expected.` lands within its distance-suppression window
/// (which only happens for *short* computed names like `[1]`). Exercised with a
/// numeric and a string computed key, and a renamed leading binding, to prove the
/// rule is keyed on the recovery shape, not the spelling/length of the key.
#[test]
fn short_computed_member_after_shorthand_still_reports_missing_colon() {
    for (lead, key) in [("a", "1"), ("a", "\"s\""), ("zzz", "1"), ("zzz", "\"ss\"")] {
        let source = format!("var x = {{ {lead}[{key}], }};\n");
        let (parser, _root) = parse_source(&source);
        let diagnostics = parser.get_diagnostics();

        // `',' expected.` at the `[` immediately after the shorthand identifier.
        let bracket_pos = source.find('[').expect("`[` in source") as u32;
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == diagnostic_codes::EXPECTED
                    && d.start == bracket_pos
                    && d.message == "',' expected."),
            "expected `',' expected.` at the `[` for `{lead}[{key}]`, got {diagnostics:?}"
        );

        // `':' expected.` at the trailing `,` that closes the computed member.
        let comma_pos = source.find("],").expect("`],` in source") as u32 + 1;
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == diagnostic_codes::EXPECTED
                    && d.start == comma_pos
                    && d.message == "':' expected."),
            "expected `':' expected.` at the trailing `,` for `{lead}[{key}]`, got {diagnostics:?}"
        );
    }
}

/// Structural rule: tsc's `isIdentifier()` returns false for a contextually
/// reserved keyword — `await` inside a `static { }` block (or async context) and
/// `yield` inside a generator. Such a token can never start a shorthand member,
/// so `({ await })` inside a static block is a property assignment missing its
/// `:`, and tsc reports `':' expected.` at the `}`. Before the fix tsz treated
/// `await` as an identifier and silently produced a shorthand, dropping the
/// diagnostic.
#[test]
fn contextually_reserved_keyword_member_reports_missing_colon() {
    let source = "class C {\n    static {\n        ({ await });\n    }\n}\n";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    // The `:' expected.` lands at the `}` that closes the object literal.
    let close_brace_pos = source.find(" });").expect("` });` in source") as u32 + 1;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED
                && d.start == close_brace_pos
                && d.message == "':' expected."),
        "expected `':' expected.` at the `}}` after `await` in a static block, got {diagnostics:?}"
    );

    let members = first_object_literal_members(&parser);
    assert_eq!(members.len(), 1, "expected one member, got {members:?}");
    let arena = parser.get_arena();
    let member = arena.get(members[0]).expect("member node");
    assert_eq!(
        member.kind,
        syntax_kind_ext::PROPERTY_ASSIGNMENT,
        "`await` in a static block must be a property assignment, not a shorthand"
    );
}

/// Negative case proving the contextual-reserved rule is context-sensitive:
/// outside an async/generator/static-block context, `await` and `yield` are
/// ordinary identifiers, so `({ await })` is a valid shorthand member with no
/// `':' expected.` diagnostic.
#[test]
fn await_outside_reserved_context_stays_shorthand() {
    let source = "function f() { ({ await }); }\n";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED && d.message == "':' expected."),
        "`await` outside a reserved context is a shorthand, not a missing-colon member, got {diagnostics:?}"
    );
    let members = first_object_literal_members(&parser);
    assert_eq!(members.len(), 1, "expected one member, got {members:?}");
    let arena = parser.get_arena();
    let member = arena.get(members[0]).expect("member node");
    assert_eq!(
        member.kind,
        syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT,
        "`await` outside a reserved context must stay a shorthand member"
    );
}

/// A value-less object property with an explicit colon (`{ prop: }`) is a
/// property assignment with a distinct missing-value node (so the emitter keeps
/// the colon, matching tsc's `prop:`), not a shorthand that drops it.
#[test]
fn object_property_with_explicit_colon_and_missing_value_keeps_distinct_initializer() {
    let source = "var b = {\n    prop:\n}\n";
    let (parser, _root) = parse_source(source);

    let members = first_object_literal_members(&parser);
    assert_eq!(members.len(), 1, "expected one member, got {members:?}");
    let arena = parser.get_arena();
    let member = arena.get(members[0]).expect("member node");
    assert_eq!(member.kind, syntax_kind_ext::PROPERTY_ASSIGNMENT);
    let prop = arena
        .get_property_assignment(member)
        .expect("property assignment data");
    assert_ne!(
        prop.name, prop.initializer,
        "missing value must be a distinct node so the emitter renders `prop:`"
    );
}

/// Assert that a body-less object method which is the final member reports a
/// single TS1005 `'{' expected.` at the trailing `;`, with no spurious
/// `',' expected.` from the outer member-list loop — mirroring tsc.
fn assert_open_brace_at_final_semicolon(source: &str) {
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let semicolon_pos = source.find(';').expect("semicolon position") as u32;

    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == semicolon_pos
            && diag.message == "'{' expected."),
        "expected `'{{' expected.` at the trailing `;` for {source:?}, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|diag| diag.message != "',' expected."),
        "no spurious `',' expected.` for {source:?}, got {diagnostics:?}"
    );
}

/// An object-literal method whose `{` body is missing and which is the final
/// member (the trailing `;` is followed by `}` / EOF) reports the single TS1005
/// `'{' expected.` tsc would, not the `',' expected.` tsz used to emit.
#[test]
fn object_method_missing_body_as_last_member_reports_open_brace() {
    assert_open_brace_at_final_semicolon("var v = { foo(); }");
}

/// Same rule with an explicit return-type annotation before the missing body.
#[test]
fn object_method_missing_body_with_return_type_reports_open_brace() {
    assert_open_brace_at_final_semicolon("var v = { foo(): number; }");
}

/// The rule also applies when the body-less method follows other members.
#[test]
fn object_method_missing_body_after_prior_member_reports_open_brace() {
    assert_open_brace_at_final_semicolon("var v = { a: 1, foo(); }");
}

/// When a *further member* follows the `;`, tsc recovers the `;` as a delimiter
/// and reports a missing comma at the next member instead of `'{' expected.`.
/// This locks in the surgical scope: the fix must NOT emit `'{' expected.` here.
#[test]
fn object_method_missing_body_followed_by_member_keeps_comma_recovery() {
    let source = "var v = { foo(); b: 2 }";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    assert!(
        diagnostics
            .iter()
            .all(|diag| diag.message != "'{' expected."),
        "a `;` followed by a further member must not report `'{{' expected.`, got {diagnostics:?}"
    );
}

/// A well-formed object method (`foo() {}`) is unaffected: no TS1005.
#[test]
fn object_method_with_body_reports_no_missing_brace() {
    let source = "var v = { foo() {} }";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    assert!(
        diagnostics.iter().all(|diag| diag.code != 1005),
        "a well-formed object method must not produce TS1005, got {diagnostics:?}"
    );
}

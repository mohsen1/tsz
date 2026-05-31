//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — nullable type recovery.

use crate::parser::test_fixture::{assert_no_errors, parse_source, parse_source_named};

#[test]
fn test_postfix_question_emits_ts17019() {
    // `string?` should emit TS17019, not TS1005 or TS1110
    let source = "let x: string?;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts17019_count = diagnostics.iter().filter(|d| d.code == 17019).count();
    assert!(
        ts17019_count >= 1,
        "Expected TS17019 for postfix '?' on type, got diagnostics: {diagnostics:?}"
    );
    // Should NOT emit TS1005 or TS1110 cascade
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
    let ts1110_count = diagnostics.iter().filter(|d| d.code == 1110).count();
    assert_eq!(
        ts1005_count, 0,
        "Should not emit TS1005 for nullable type, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1110_count, 0,
        "Should not emit TS1110 for nullable type, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_prefix_question_emits_ts17020() {
    // `?string` should emit TS17020, not TS1110
    let source = "let x: ?string;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts17020_count = diagnostics.iter().filter(|d| d.code == 17020).count();
    assert!(
        ts17020_count >= 1,
        "Expected TS17020 for prefix '?' on type, got diagnostics: {diagnostics:?}"
    );
    // Should NOT emit TS1110 cascade
    let ts1110_count = diagnostics.iter().filter(|d| d.code == 1110).count();
    assert_eq!(
        ts1110_count, 0,
        "Should not emit TS1110 for nullable type, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_prefix_question_simplifies_ts17020_suggestions() {
    for (input, expected) in [
        ("unknown", "unknown"),
        ("never", "never"),
        ("void", "void"),
        ("undefined", "null | undefined"),
        ("null", "null | undefined"),
        ("number", "number | null | undefined"),
    ] {
        let source = format!("let x: ?{input};");
        let (parser, _root) = parse_source_named(&format!("{input}.ts"), &source);

        let diagnostic = parser
            .get_diagnostics()
            .iter()
            .find(|d| d.code == 17020)
            .unwrap_or_else(|| {
                panic!(
                    "Expected TS17020 for ?{input}, got {:?}",
                    parser.get_diagnostics()
                )
            });
        assert_eq!(
            diagnostic.message,
            format!(
                "'?' at the start of a type is not valid TypeScript syntax. Did you mean to write '{expected}'?"
            ),
            "wrong TS17020 suggestion for ?{input}"
        );
    }
}

#[test]
fn test_multiple_nullable_types() {
    // Multiple nullable types in different positions
    let source = r"
function f(x: string?): ?number {
    return null;
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts17019_count = diagnostics.iter().filter(|d| d.code == 17019).count();
    let ts17020_count = diagnostics.iter().filter(|d| d.code == 17020).count();
    assert!(
        ts17019_count >= 1,
        "Expected at least 1 TS17019 for postfix '?', got diagnostics: {diagnostics:?}"
    );
    assert!(
        ts17020_count >= 1,
        "Expected at least 1 TS17020 for prefix '?', got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_nullable_type_in_type_predicate() {
    // `x is ?string` should emit TS17020
    let source = "function f(x: any): x is ?string { return true; }";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts17020_count = diagnostics.iter().filter(|d| d.code == 17020).count();
    assert!(
        ts17020_count >= 1,
        "Expected TS17020 for '?string' in type predicate, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_nullable_type_no_cascade() {
    // Nullable type should not cause cascading errors
    let source = r#"
let a: string? = "hello";
let b: ?number = 42;
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    // Should only have TS17019 and TS17020, no cascade
    let cascade_codes: Vec<u32> = diagnostics
        .iter()
        .filter(|d| d.code == 1005 || d.code == 1109 || d.code == 1110 || d.code == 1128)
        .map(|d| d.code)
        .collect();
    assert!(
        cascade_codes.is_empty(),
        "Nullable types should not cause cascading errors, got: {cascade_codes:?}. All: {diagnostics:?}"
    );
}

#[test]
fn test_invalid_nonnullable_type_recovery_reports_ts17019_and_ts17020() {
    let source = r#"
function f1(a: string): a is string! { return true; }
function f2(a: string): a is !string { return true; }
const a = 1 as any!;
const b = 1 as !any;
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![17019, 17020, 17019, 17020],
        "Expected TS17019/TS17020 recovery for invalid non-nullable type syntax, got diagnostics: {diagnostics:?}"
    );
}

/// Postfix `?` after an array suffix (e.g., `string[]?`) must emit TS17019
/// like a postfix on a bare type, not cascade into TS1005/TS1110. tsc treats
/// `parsePostfixTypeOrHigher` as a loop over `[]` and `?`/`!`, so the JSDoc
/// nullable applies to the array.
#[test]
fn postfix_question_after_array_emits_ts17019() {
    for source in [
        "type A = string[]?;",
        "type B = number[]?;",
        "type C = string[][]?;",
        "type D = readonly string[]?;",
    ] {
        let (parser, _root) = parse_source(source);
        let diagnostics = parser.get_diagnostics();
        assert!(
            diagnostics.iter().any(|d| d.code == 17019),
            "Expected TS17019 for `{source}`, got {diagnostics:?}"
        );
        assert!(
            diagnostics.iter().all(|d| d.code != 1005 && d.code != 1110),
            "Postfix `?` after array should not cascade into TS1005/TS1110 for `{source}`, got {diagnostics:?}"
        );
    }
}

/// Same rule applies to postfix `!`: `string[]!` should report TS17019 once,
/// not cascade.
#[test]
fn postfix_bang_after_array_emits_ts17019() {
    let (parser, _root) = parse_source("type A = string[]!;");
    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.iter().any(|d| d.code == 17019),
        "Expected TS17019 for `string[]!`, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 1005 && d.code != 1110),
        "Postfix `!` after array should not cascade for `string[]!`, got {diagnostics:?}"
    );
}

/// Postfix `?` after indexed-access (`T['k']?`) follows the same rule as the
/// array suffix path. Both share `parse_array_type` and must fall through to
/// the JSDoc-nullable handler.
#[test]
fn postfix_question_after_indexed_access_emits_ts17019() {
    let source = "type X = { abc: string }; type A = X['abc']?;";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.iter().any(|d| d.code == 17019),
        "Expected TS17019 for `X['abc']?`, got {diagnostics:?}"
    );
}

/// `(string | number)?` should drop the outer parens in the suggestion text
/// — tsc displays `string | number | undefined`, not `(string | number) | undefined`.
#[test]
fn postfix_question_strips_outer_parens_in_suggestion() {
    let (parser, _root) = parse_source("type A = (string | number)?;");
    let diagnostics = parser.get_diagnostics();
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 17019)
        .expect("expected TS17019 for `(string | number)?`");
    assert_eq!(
        diag.message,
        "'?' at the end of a type is not valid TypeScript syntax. Did you mean to write 'string | number | undefined'?",
        "expected suggestion without outer parens, got {diagnostics:?}"
    );
}

/// Nested parens are peeled iteratively: `((string))?` suggests `string |
/// undefined`.
#[test]
fn postfix_question_peels_nested_parens_in_suggestion() {
    let (parser, _root) = parse_source("type A = ((string))?;");
    let diagnostics = parser.get_diagnostics();
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 17019)
        .expect("expected TS17019 for `((string))?`");
    assert_eq!(
        diag.message,
        "'?' at the end of a type is not valid TypeScript syntax. Did you mean to write 'string | undefined'?",
        "expected nested parens to be peeled, got {diagnostics:?}"
    );
}

/// `[string[]?]` is a valid tuple-optional element and must not emit TS17019:
/// the `?` belongs to the tuple, not to a JSDoc-nullable on `string[]`. This
/// guards the `IN_TUPLE_ELEMENT` suppression when the array-suffix path now
/// falls through to the postfix handler.
#[test]
fn array_optional_inside_tuple_is_tuple_optional_not_jsdoc_nullable() {
    for source in [
        "type A = [string[]?];",
        "type B = [(string | number)?];",
        "type C = [number[][]?];",
    ] {
        let (parser, _root) = parse_source(source);
        let diagnostics = parser.get_diagnostics();
        assert!(
            diagnostics.iter().all(|d| d.code != 17019),
            "Tuple-optional `[T[]?]` must not emit TS17019 for `{source}`, got {diagnostics:?}"
        );
        assert!(
            diagnostics.iter().all(|d| d.code != 1005 && d.code != 1110),
            "Tuple-optional `[T[]?]` must not cascade for `{source}`, got {diagnostics:?}"
        );
    }
}

/// `infer X extends T[]?` (postfix `?` on the constraint inside an `infer`
/// extends clause) must produce TS17019 on the constraint, not roll back as
/// an outer conditional `?`. Reported as the recovery surface for #11333
/// (variadic tuple utility parser recovery).
#[test]
fn infer_extends_array_postfix_emits_ts17019_on_constraint() {
    let source = "type A<T> = T extends [infer X extends string[]?, infer Y] ? [X, Y] : never;";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.iter().any(|d| d.code == 17019),
        "Expected TS17019 for `infer X extends string[]?`, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 1110),
        "Should not cascade into TS1110 `Type expected.` for the trailing element, got {diagnostics:?}"
    );
}

/// Bare `infer X` must not absorb a postfix `[]` array suffix. tsc's
/// `parseTypeOperatorOrHigher` returns `parseInferType()` directly without
/// running postfix parsing, so `[infer X[]]` should recover as the
/// stray-`[` pattern (TS1005 `,` expected), not silently parse as
/// `[(infer X)[]]`.
#[test]
fn bare_infer_followed_by_array_suffix_does_not_absorb_brackets() {
    let source = "type A<T> = T extends [infer X[]] ? X : never;";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.iter().any(|d| d.code == 1005),
        "Expected TS1005 `',' expected.` for bare `infer X[]`, got {diagnostics:?}"
    );
}

/// Parenthesized `infer` accepts the array suffix and should not regress —
/// `(infer X)[]` is the documented way to write an array of an inferred type.
#[test]
fn parenthesized_infer_followed_by_array_suffix_parses() {
    assert_no_errors("type A<T> = T extends (infer X)[] ? X : never;");
}

/// Bare `infer X` followed by a postfix `?` outside a tuple still leaves the
/// `?` for the surrounding context (so `T extends infer U ? U : never` parses
/// as a conditional type, not as a JSDoc nullable on the infer). This pins
/// down the "bare infer skips postfix" rule across non-tuple contexts.
#[test]
fn bare_infer_question_in_conditional_extends_position_parses_as_conditional() {
    assert_no_errors("type A<T> = T extends infer U ? U : never;");
}

/// The variadic-utility shape from issue #11333 should parse cleanly:
/// `Tail<T>`, then a mapped type over `keyof Tail<T>`, then a function
/// returning that. The parser must not cross-pollute state between the
/// declarations.
#[test]
fn variadic_tuple_infer_mapped_chain_parses_cleanly() {
    assert_no_errors(
        r"
type Tail<T extends any[]> = T extends [any, ...infer R] ? R : never;
type MapTail<T extends any[]> = { [K in keyof Tail<T>]: Tail<T>[K] };
function f<T extends any[]>(x: T, y: MapTail<T>) { return [x, y] as const; }
",
    );
}

/// Adjacent stress cases for the same rule: deeply nested infer + variadic +
/// conditional + mapped. Varies type-parameter names so the fix is structural,
/// not name-dependent (per CLAUDE.md §25 anti-hardcoding directive).
#[test]
fn variadic_tuple_infer_adjacent_shapes_parse_cleanly() {
    for source in [
        // Reverse via variadic + mapped recursion.
        "type Reverse<T extends any[]> = T extends [...infer R, infer L] ? [L, ...Reverse<R>] : T;",
        // Multi-position infer with `extends` constraint.
        "type Head<U extends any[]> = U extends [infer H extends string, ...any[]] ? H : never;",
        // Mapped + indexed access over a variadic alias.
        "type M<P extends any[]> = { [K in keyof P]: P[K] };",
        // Template-literal infer in a conditional cascade.
        "type S<X> = X extends `${infer A}-${infer B}` ? [A, B] : never;",
        // Conditional with `infer R extends string` constraint plus outer `?`.
        "type C<T> = T extends infer R extends string ? R : never;",
        // Variadic concat via recursive conditional.
        "type Cat<A extends any[], B extends any[]> = A extends [infer Hd, ...infer Rs] ? [Hd, ...Cat<Rs, B>] : B;",
    ] {
        assert_no_errors(source);
    }
}

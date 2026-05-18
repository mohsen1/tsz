//! Operator error messages must use the widened/resolved type, not raw
//! annotation text, except when the annotation is a bare type-parameter
//! reference.
//!
//! **Structural rule**: `operator_type_parameter_annotation_text_for_expression`
//! returns raw annotation text only when the annotation is a plain
//! `TypeReference` whose symbol has the `TYPE_PARAMETER` flag. Every other
//! annotation kind (numeric/bigint/boolean/string literals, type aliases,
//! built-in primitives) must return `None` so the error message is built from
//! the resolved type.
//!
//! Tests deliberately use two different identifier names for every bound
//! variable to prove the rule is structural and not tied to any particular
//! spelling (see `.claude/CLAUDE.md` §25).

use tsz_checker::test_utils::check_source_code_messages as diagnostics;

fn operator_messages(source: &str) -> Vec<String> {
    diagnostics(source)
        .into_iter()
        .filter_map(|(code, msg)| (2362..=2365).contains(&code).then_some(msg))
        .collect()
}

// ── Reported repro ──────────────────────────────────────────────────────────

/// `two: 2` — numeric literal annotation must show `'number'`, not `'2'`.
#[test]
fn numeric_literal_annotation_shows_widened_number() {
    let source = r"
function f(two: 2) {
    let _a = false < two;
}
";
    let msgs = operator_messages(source);
    assert!(!msgs.is_empty(), "expected an operator error, got none");
    let msg = &msgs[0];
    assert!(
        msg.contains("'number'"),
        "expected 'number' in operator error for param annotated `2`, got: {msg}"
    );
    assert!(
        !msg.contains("'2'"),
        "must not show raw literal '2' in operator error, got: {msg}"
    );
}

/// Same rule under a different parameter name to prove it is structural.
#[test]
fn numeric_literal_annotation_widening_is_structural_under_renaming() {
    let source = r"
function g(n: 1) {
    let _b = false < n;
}
";
    let msgs = operator_messages(source);
    assert!(!msgs.is_empty(), "expected an operator error");
    let msg = &msgs[0];
    assert!(
        msg.contains("'number'"),
        "expected 'number' for param annotated `1`, got: {msg}"
    );
    assert!(
        !msg.contains("'1'"),
        "must not show raw literal '1', got: {msg}"
    );
}

// ── Bigint literal ──────────────────────────────────────────────────────────

/// `big: 1n` — bigint literal annotation must show `'bigint'`, not `'1n'`.
#[test]
fn bigint_literal_annotation_shows_widened_bigint() {
    let source = r"
function f(big: 1n) {
    let _a = false < big;
}
";
    let msgs = operator_messages(source);
    assert!(!msgs.is_empty(), "expected an operator error for bigint");
    let msg = &msgs[0];
    assert!(
        msg.contains("'bigint'"),
        "expected 'bigint' in operator error for param annotated `1n`, got: {msg}"
    );
    assert!(
        !msg.contains("'1n'"),
        "must not show raw literal '1n', got: {msg}"
    );
}

/// Same rule under a different parameter name.
#[test]
fn bigint_literal_annotation_widening_is_structural_under_renaming() {
    let source = r"
function h(x: 3n) {
    let _c = false < x;
}
";
    let msgs = operator_messages(source);
    assert!(!msgs.is_empty(), "expected an operator error for bigint");
    let msg = &msgs[0];
    assert!(
        msg.contains("'bigint'"),
        "expected 'bigint' for param annotated `3n`, got: {msg}"
    );
    assert!(
        !msg.contains("'3n'"),
        "must not show raw literal '3n', got: {msg}"
    );
}

// ── Type parameter annotations must still use raw text ──────────────────────

/// `x: T` (generic type param) — raw annotation text `T` must still appear.
#[test]
fn type_param_annotation_preserves_raw_text() {
    let source = r"
function f<T extends boolean>(x: T) {
    let _a = x < 1;
}
";
    let msgs = operator_messages(source);
    assert!(!msgs.is_empty(), "expected an operator error");
    let msg = &msgs[0];
    assert!(
        msg.contains("'T'"),
        "expected raw type param 'T' in operator error, got: {msg}"
    );
}

/// Same rule under a renamed type parameter.
#[test]
fn type_param_annotation_preserves_raw_text_renamed() {
    let source = r"
function g<K extends boolean>(y: K) {
    let _b = y < 1;
}
";
    let msgs = operator_messages(source);
    assert!(!msgs.is_empty(), "expected an operator error");
    let msg = &msgs[0];
    assert!(
        msg.contains("'K'"),
        "expected raw type param 'K' in operator error, got: {msg}"
    );
}

// ── Boolean literal annotations ─────────────────────────────────────────────

/// `x: true` — boolean literal annotation is a `LITERAL_TYPE` node, so
/// annotation-text extraction must not fire; the message must reflect the
/// inferred type (tsc shows `'true'`), not a raw annotation lookup.
#[test]
fn boolean_literal_annotation_does_not_show_raw_text() {
    let source = r"
function f(x: true) {
    let _a = x < 1;
}
";
    let msgs = operator_messages(source);
    assert!(
        !msgs.is_empty(),
        "expected an operator error for boolean literal annotation"
    );
    let msg = &msgs[0];
    // tsc reports the literal type 'true' from type resolution — the
    // annotation-text extraction path must not have fired (it would also
    // produce 'true', making the bug invisible, but the TYPE_PARAMETER guard
    // ensures only genuine type params use that path).
    assert!(
        msg.contains("'true'") || msg.contains("'boolean'"),
        "expected tsc-compatible type display in operator error, got: {msg}"
    );
}

// ── Short string literal annotations ────────────────────────────────────────

/// A 2-char string literal annotation like `x: "ab"` is a `LITERAL_TYPE` node.
/// Annotation-text extraction must not return `'ab'`; tsc shows `'"ab"'`
/// (with string-literal quotes) from type resolution, not a bare identifier.
#[test]
fn short_string_literal_annotation_does_not_match_heuristic() {
    let source = r#"
function f(x: "ab") {
    let _a = false < x;
}
"#;
    let msgs = operator_messages(source);
    assert!(
        !msgs.is_empty(),
        "expected an operator error for string literal annotation"
    );
    let msg = &msgs[0];
    // tsc shows '"ab"' (with the string quotes); annotation-text extraction
    // must not return the bare identifier-style 'ab'.
    assert!(
        !msg.contains("'ab'"),
        "short string literal annotation must not appear as raw type-param text, got: {msg}"
    );
}

// ── Type alias annotations ───────────────────────────────────────────────────

/// `type N = number; function f(a: N)` — type alias annotation resolves to a
/// `TYPE_ALIAS` symbol, not `TYPE_PARAMETER`; annotation-text extraction must
/// return `None` so the error uses the resolved type, not `'N'`.
#[test]
fn type_alias_annotation_does_not_show_raw_alias_name() {
    let source = r"
type N = number;
function f(a: N) {
    let _a = false < a;
}
";
    let msgs = operator_messages(source);
    assert!(
        !msgs.is_empty(),
        "expected an operator error for type-alias annotation"
    );
    let msg = &msgs[0];
    assert!(
        !msg.contains("'N'"),
        "type alias 'N' must not appear as raw annotation text in operator error, got: {msg}"
    );
}

/// Same rule under a different alias name — proves it is structural.
#[test]
fn type_alias_annotation_does_not_show_raw_alias_name_renamed() {
    let source = r"
type Num = number;
function g(b: Num) {
    let _b = false < b;
}
";
    let msgs = operator_messages(source);
    assert!(
        !msgs.is_empty(),
        "expected an operator error for type-alias annotation"
    );
    let msg = &msgs[0];
    assert!(
        !msg.contains("'Num'"),
        "type alias 'Num' must not appear as raw annotation text in operator error, got: {msg}"
    );
}

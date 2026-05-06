use crate::test_utils::check_source_diagnostics;

fn check_source_with_strict_null(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_source_diagnostics(source)
}

/// Object-literal property elaboration: when a numeric literal property is
/// passed to a parameter expecting a boolean literal, tsc reports the source
/// as the widened primitive (`number`) rather than the AST literal (`1`).
/// Mirrors `getWidenedLiteralLikeTypeForContextualType`: literal `1` is not a
/// "literal of contextual type" against `true`, so its display widens.
///
/// Direct literal-to-literal assignments (`let x: 1 = "abc"`) are unaffected
/// because the source carries its own literal TypeId rather than being
/// pre-widened by property elaboration.
#[test]
fn ts2322_property_elaboration_widens_cross_primitive_literal_source() {
    let diagnostics = check_source_with_strict_null(
        r#"
const fn1 = (s: { a: true }) => {};
fn1({ a: 1 });
fn1({ a: 1 } satisfies unknown);
"#,
    );
    let messages: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text.as_str())
        .collect();
    assert!(
        messages
            .iter()
            .all(|m| m.contains("Type 'number' is not assignable to type 'true'.")),
        "Expected widened source display 'number' for cross-primitive-kind \
         property elaboration (`a: 1` against `a: true`), got: {messages:#?}"
    );
    assert_eq!(
        messages.len(),
        2,
        "Expected exactly two TS2322 diagnostics (one per call), got: {messages:#?}"
    );
}

/// Direct literal-to-literal assignment must keep the literal display:
/// `let x: 1 = "abc"` reports `Type '"abc"'`, not `Type 'string'`.
/// `let literal2: true = 1 satisfies number` keeps `Type '1'` because
/// `satisfies` preserves the inner literal type.
#[test]
fn ts2322_direct_literal_assignment_preserves_ast_literal_display() {
    let diagnostics = check_source_with_strict_null(
        r#"
const a: 1 = "abc";
const b: 1 = true;
const c: true = 1 satisfies number;
"#,
    );
    let messages: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text.as_str())
        .collect();
    let has = |needle: &str| messages.iter().any(|m| m.contains(needle));
    assert!(
        has("Type '\"abc\"' is not assignable to type '1'."),
        "Expected literal `\"abc\"` for direct string-to-1 assignment, got: {messages:#?}"
    );
    assert!(
        has("Type 'true' is not assignable to type '1'."),
        "Expected literal `true` for direct boolean-to-1 assignment, got: {messages:#?}"
    );
    assert!(
        has("Type '1' is not assignable to type 'true'."),
        "Expected literal `1` for `let c: true = 1 satisfies number`, got: {messages:#?}"
    );
}

/// TS2345 against an optional parameter whose underlying type is
/// literal-sensitive (`null`, a literal, etc.) must render the
/// externally-visible parameter type with `| undefined` and preserve the
/// argument literal — matching tsc's diagnostic surface.  Covers both
/// parameters with explicit `?` and parameters made optional by a default
/// initializer such as `z = null`.
///
/// The bound variable name in `fd` (`z`) is intentionally different from
/// `fi` (`x`) so the fix can't be hardcoded to a particular spelling.
#[test]
fn ts2345_optional_parameter_with_null_default_includes_undefined_surface() {
    let diagnostics = check_source_with_strict_null(
        r#"
function fi(x = null) {}
function fd(z = null, o = { a: 0 }) {}

fi("hi");
fd("hi");
"#,
    );
    let messages: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == 2345)
        .map(|d| d.message_text.as_str())
        .collect();
    let null_undefined_count = messages
        .iter()
        .filter(|m| {
            m.contains(
            "Argument of type '\"hi\"' is not assignable to parameter of type 'null | undefined'.",
        )
        })
        .count();
    assert_eq!(
        null_undefined_count, 2,
        "Expected two TS2345 diagnostics with `'\"hi\"'` arg and `null | undefined` \
         param (one each for `fi` and `fd`), got: {messages:#?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("undefined | undefined")),
        "Expected no duplicated `undefined`, got: {messages:#?}"
    );
    assert!(
        !messages
            .iter()
            .any(|m| m.contains("parameter of type 'null'.")),
        "Optional null-default param must surface `null | undefined`, not bare `null`, \
         got: {messages:#?}"
    );
}

/// Parameters whose declared type already contains `undefined` should not
/// double-up the surface — `union2` deduplicates, so the rendered type stays
/// `string | undefined`, not `string | undefined | undefined`.
#[test]
fn ts2345_optional_parameter_already_includes_undefined_does_not_duplicate() {
    let diagnostics = check_source_with_strict_null(
        r#"
function f(x?: string | undefined) {}
f(42);
"#,
    );
    let messages: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == 2345)
        .map(|d| d.message_text.as_str())
        .collect();
    assert!(
        !messages.iter().any(|m| m.contains("undefined | undefined")),
        "Expected no duplicated `undefined`, got: {messages:#?}"
    );
}

/// Optional non-rest parameters whose underlying type has at least one
/// non-nullish member must NOT surface the synthetic `| undefined`. tsc
/// elides it once a regular (non-spread) argument fills the slot — so a
/// plain `number` optional param renders as `'number'`, not
/// `'number | undefined'`. This is the inverse of the null-default case
/// above and ensures the widen-then-strip pipeline still strips when the
/// underlying type carries a non-nullish member.
#[test]
fn ts2345_optional_parameter_with_concrete_default_strips_synthetic_undefined() {
    let diagnostics = check_source_with_strict_null(
        r#"
function f(x?: number) {}
f("hi");
"#,
    );
    let messages: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == 2345)
        .map(|d| d.message_text.as_str())
        .collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("parameter of type 'number'.")),
        "Expected stripped `'number'` for non-spread arg landing on \
         optional concrete param, got: {messages:#?}"
    );
    assert!(
        !messages
            .iter()
            .any(|m| m.contains("parameter of type 'number | undefined'")),
        "Synthetic `| undefined` must not surface for non-spread args on \
         optional concrete params, got: {messages:#?}"
    );
}

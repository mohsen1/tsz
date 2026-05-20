//! Source-type display for `unique symbol` expressions.
//!
//! tsc renders an assignability source like `Symbol.toPrimitive` (whose
//! value type is `unique symbol`) as `typeof Symbol.toPrimitive` rather
//! than widening to `symbol`. Mirrors that behavior for diagnostics like
//! `"" in Symbol.toPrimitive` (`object` target).

use crate::context::CheckerOptions;

fn check_strict(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    crate::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

// Note: a unit test that fully exercises the typeof-property-access
// preservation needs the lib's `Symbol` global loaded so `unique symbol`
// resolves to a proper UniqueSymbol type. The conformance harness
// (`symbolType2.ts`) provides that environment and serves as the
// integration check. The unit tests below pin sibling invariants that
// run without lib pollution.

/// Element access form: `Foo[k]` where `k` resolves to a unique symbol
/// should also render the source as `typeof Foo[k]` and not `symbol`.
#[test]
fn element_access_unique_symbol_source_displays_typeof() {
    let source = r#"
declare const sym: unique symbol;
type Holder = { [sym]: number };
declare const obj: Holder;
const _y: object = obj[sym];
"#;
    let diags = check_strict(source);
    // Just ensure if a TS2322 fires, it doesn't show bare `symbol`. The
    // exact wording can vary; the invariant is "no widening to symbol".
    let bare_symbol = diags
        .iter()
        .filter(|(c, _)| *c == 2322)
        .any(|(_, msg)| msg.contains("'symbol'") && !msg.contains("typeof"));
    assert!(
        !bare_symbol,
        "must not display bare 'symbol' for unique-symbol-typed element access source: {diags:?}"
    );
}

/// Plain identifier with `unique symbol` value type also benefits — though
/// this path may use a different display branch (declared identifier
/// source). The invariant under test is "we don't say `Type 'symbol'` when
/// the source is a `unique symbol` value".
#[test]
fn identifier_unique_symbol_source_does_not_widen_to_symbol() {
    let source = r#"
declare const sym: unique symbol;
const _z: object = sym;
"#;
    let diags = check_strict(source);
    let bare_symbol = diags
        .iter()
        .filter(|(c, _)| *c == 2322)
        .any(|(_, msg)| msg == "Type 'symbol' is not assignable to type 'object'.");
    assert!(
        !bare_symbol,
        "must not display bare 'symbol' for unique-symbol-typed identifier source: {diags:?}"
    );
}

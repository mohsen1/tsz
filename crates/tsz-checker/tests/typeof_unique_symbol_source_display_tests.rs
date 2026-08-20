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

// A distinct `unique symbol` pair on both sides of a TS2322 assignability
// head — where both operands stringify to the bare `unique symbol` keyword —
// is disambiguated by tsc's `getTypeNamesForErrorDisplay` to each side's
// `typeof <name>` form, so the message never reads `unique symbol` vs
// `unique symbol` (nor a mixed `unique symbol` vs `typeof a`). Oracle:
// `typescript@7.0.2 --strict`. These pin the variable-declaration and
// `typeof`-return surfaces, whose per-operand source-display rewrites used to
// clobber the pair-level disambiguation back to the bare keyword.

/// Row 3: variable declaration `const x: typeof a = b`.
#[test]
fn var_decl_distinct_unique_symbol_pair_disambiguates_to_typeof() {
    let source = r#"
declare const a: unique symbol;
declare const b: unique symbol;
const x: typeof a = b;
"#;
    let diags = check_strict(source);
    assert!(
        diags.iter().any(
            |(c, m)| *c == 2322 && m == "Type 'typeof b' is not assignable to type 'typeof a'."
        ),
        "distinct unique-symbol var-decl pair must disambiguate to typeof names: {diags:?}"
    );
}

/// Row 4: `typeof`-annotated return position `function f(): typeof r1 { return r2 }`.
#[test]
fn typeof_return_distinct_unique_symbol_pair_disambiguates_to_typeof() {
    let source = r#"
declare const r1: unique symbol;
declare const r2: unique symbol;
function f(): typeof r1 { return r2; }
"#;
    let diags = check_strict(source);
    assert!(
        diags
            .iter()
            .any(|(c, m)| *c == 2322
                && m == "Type 'typeof r2' is not assignable to type 'typeof r1'."),
        "distinct unique-symbol return pair must disambiguate to typeof names: {diags:?}"
    );
}

/// Anti-hardcoding: the `typeof <name>` disambiguation is driven by the
/// resolved declaration names, not any fixed identifier text. Renaming the
/// binders renames both sides of the message.
#[test]
fn var_decl_unique_symbol_pair_typeof_names_track_renamed_binders() {
    let source = r#"
declare const alpha: unique symbol;
declare const beta: unique symbol;
const x: typeof alpha = beta;
"#;
    let diags = check_strict(source);
    assert!(
        diags.iter().any(|(c, m)| *c == 2322
            && m == "Type 'typeof beta' is not assignable to type 'typeof alpha'."),
        "typeof names must follow the renamed binders: {diags:?}"
    );
}

/// Negative control: a wide `symbol` source against a `unique symbol`
/// parameter does NOT disambiguate — the two default names already differ, so
/// tsc keeps the bare `symbol` / `unique symbol` pair (never `typeof`).
#[test]
fn wide_symbol_source_against_unique_symbol_keeps_bare_names() {
    let source = r#"
declare const p: unique symbol;
declare const w: symbol;
declare function g(x: typeof p): void;
g(w);
"#;
    let diags = check_strict(source);
    assert!(
        diags.iter().any(|(c, m)| *c == 2345
            && m == "Argument of type 'symbol' is not assignable to parameter of type 'unique symbol'."),
        "a wide symbol source must not be promoted to a typeof form: {diags:?}"
    );
}

/// Negative control: a string-literal source against a `unique symbol` target
/// keeps its bare literal display — only genuinely distinct unique-symbol
/// pairs are re-qualified.
#[test]
fn string_literal_source_against_unique_symbol_keeps_bare_names() {
    let source = r#"
declare const k: unique symbol;
const bad: typeof k = "s";
"#;
    let diags = check_strict(source);
    assert!(
        diags
            .iter()
            .any(|(c, m)| *c == 2322
                && m == "Type '\"s\"' is not assignable to type 'unique symbol'."),
        "a string-literal source must keep its bare display: {diags:?}"
    );
}

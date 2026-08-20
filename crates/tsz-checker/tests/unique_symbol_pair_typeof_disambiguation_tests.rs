//! `unique symbol` assignability-pair display disambiguation (#17813).
//!
//! `unique symbol` operands stringify to the bare `unique symbol` keyword by
//! default, so two distinct unique symbols collide as `unique symbol` vs
//! `unique symbol`. tsc re-qualifies each colliding side to its `typeof <name>`
//! form (`getTypeNamesForErrorDisplay`), the same rule that keeps a two-unique-
//! symbol mismatch a TS2322 rather than a type-unassignable-to-itself message.
//! tsz mirrors that through the shared `finalize_pair_display_for_diagnostic`
//! gateway, reusing the solver's existing `unique_symbol_ref` /
//! `resolve_unique_symbol_name` queries to name each side. The resolved name is
//! the symbol's short name (a static class member is unqualified in `typeof`
//! error display — matching tsc, e.g. `recursiveFunctionTypes.ts` renders
//! `typeof C.g` as `typeof g`).
//!
//! These run without the lib: `declare const s: unique symbol` and
//! `static readonly p: unique symbol = <symbol value>` both yield proper
//! `unique symbol` identities. The `Symbol()`-call forms are covered with the
//! real lib by `conformance/types/uniqueSymbol/*`.

use tsz_checker::context::CheckerOptions;

fn check_strict(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

/// Flattened `(code, message)` list of a diagnostic's message plus every
/// nested `related_information` line — the elaboration chain a `--pretty`
/// render prints beneath the head.
fn check_strict_with_chain(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    let mut out = Vec::new();
    for d in tsz_checker::test_utils::check_source(source, "test.ts", options) {
        out.push((d.code, d.message_text));
        for rel in &d.related_information {
            out.push((rel.code, rel.message_text.clone()));
        }
    }
    out
}

fn find(diags: &[(u32, String)], code: u32) -> Option<&String> {
    diags.iter().find(|(c, _)| *c == code).map(|(_, m)| m)
}

#[test]
fn ts2345_argument_unique_symbol_pair_renders_typeof() {
    // Distinct unique-symbol argument vs parameter: tsc renders
    // `typeof q` vs `typeof p`, not `unique symbol` vs `unique symbol`.
    let src = r#"
declare const p: unique symbol;
declare const q: unique symbol;
declare function f(x: typeof p): void;
f(q);
"#;
    let diags = check_strict(src);
    let msg = find(&diags, 2345).expect("expected TS2345 for `f(q)`");
    assert!(
        msg.contains("'typeof q'") && msg.contains("'typeof p'"),
        "TS2345 must disambiguate the unique-symbol pair to `typeof q`/`typeof p`, got: {msg:?}"
    );
    assert!(
        !msg.contains("'unique symbol' is not assignable to parameter of type 'unique symbol'"),
        "must not leave the colliding bare-keyword pair, got: {msg:?}"
    );
}

#[test]
fn ts2322_static_class_member_pair_renders_short_typeof_names() {
    // Two distinct `static readonly _: unique symbol` on one class. tsc uses the
    // SHORT member name (not the class-qualified `C.x`) in `typeof` error
    // display — a static *class* member is unqualified, unlike namespace/enum
    // members. Verified against the corpus oracle: `recursiveFunctionTypes.ts`
    // renders `typeof C.g` as `typeof g`. So the disambiguated pair is
    // `typeof y` vs `typeof x`.
    let src = r#"
declare const sym: symbol;
class C {
    static readonly x: unique symbol = sym;
    static readonly y: unique symbol = sym;
}
const z: typeof C.x = C.y;
"#;
    let diags = check_strict(src);
    let msg = find(&diags, 2322).expect("expected TS2322 for `typeof C.x = C.y`");
    assert_eq!(
        msg, "Type 'typeof y' is not assignable to type 'typeof x'.",
        "static class members use short `typeof y`/`typeof x` names, got: {msg:?}"
    );
}

#[test]
fn ts2322_static_class_member_pair_renders_short_names_with_renamed_binders() {
    // §Anti-hardcoding: the disambiguation must not depend on the chosen names.
    let src = r#"
declare const aSym: symbol;
class Registry {
    static readonly FIRST: unique symbol = aSym;
    static readonly SECOND: unique symbol = aSym;
}
const slot: typeof Registry.FIRST = Registry.SECOND;
"#;
    let diags = check_strict(src);
    let msg = find(&diags, 2322).expect("expected TS2322 for renamed static members");
    assert_eq!(
        msg, "Type 'typeof SECOND' is not assignable to type 'typeof FIRST'.",
        "renamed static members use short `typeof SECOND`/`typeof FIRST` names, got: {msg:?}"
    );
}

#[test]
fn ts2322_nested_property_leaf_unique_symbol_pair_renders_typeof() {
    // The property-mismatch drill leaf is the two unique symbols directly: tsc
    // renders `typeof h` vs `typeof g` at that leaf.
    let src = r#"
declare const g: unique symbol;
declare const h: unique symbol;
declare const src: { m: typeof h };
const dst: { m: typeof g } = src;
"#;
    // The head is the object shape (`{ m: unique symbol; }` — its own display
    // residual, Layer 5); the disambiguated leaf is the nested pair, carried in
    // the elaboration chain (`related_information`).
    let chain = check_strict_with_chain(src);
    let leaf = chain
        .iter()
        .filter(|(c, _)| *c == 2322)
        .find(|(_, m)| m.contains("typeof"))
        .map(|(_, m)| m)
        .expect("expected a disambiguated leaf line in the elaboration chain");
    assert_eq!(
        leaf, "Type 'typeof h' is not assignable to type 'typeof g'.",
        "the property leaf must disambiguate to `typeof h`/`typeof g`, got: {leaf:?}"
    );
}

#[test]
fn wide_symbol_source_keeps_bare_unique_symbol_target() {
    // Negative control: a wide `symbol` source vs a `unique symbol` parameter
    // does NOT collide (different default strings), so tsc keeps the bare
    // `unique symbol` on the target — no spurious `typeof` qualification.
    let src = r#"
declare const g: unique symbol;
declare const w: symbol;
declare function need(x: typeof g): void;
need(w);
"#;
    let diags = check_strict(src);
    let msg = find(&diags, 2345).expect("expected TS2345 for `need(w)`");
    assert_eq!(
        msg, "Argument of type 'symbol' is not assignable to parameter of type 'unique symbol'.",
        "wide symbol source must keep the bare `unique symbol` target, got: {msg:?}"
    );
}

#[test]
fn string_literal_source_keeps_bare_unique_symbol_target() {
    // Negative control: a string-literal source vs a `unique symbol` target
    // do not collide, so the target stays the bare `unique symbol` keyword.
    let src = r#"
declare const k: unique symbol;
const bad: typeof k = "s";
"#;
    let diags = check_strict(src);
    let msg = find(&diags, 2322).expect("expected TS2322 for `typeof k = \"s\"`");
    assert_eq!(
        msg, "Type '\"s\"' is not assignable to type 'unique symbol'.",
        "string-literal source must keep the bare `unique symbol` target, got: {msg:?}"
    );
}

#[test]
fn same_identity_static_member_self_assignment_is_accepted() {
    // Positive control: assigning a static member's `typeof` to itself is the
    // same identity — no diagnostic, so the disambiguator is never reached.
    let src = r#"
declare const sym: symbol;
class C {
    static readonly X: unique symbol = sym;
}
const self_: typeof C.X = C.X;
"#;
    let diags = check_strict(src);
    assert!(
        find(&diags, 2322).is_none(),
        "same-identity self-assignment must not emit TS2322: {diags:?}"
    );
}

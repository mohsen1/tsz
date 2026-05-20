//! Declaration-site assignability for `static readonly p: unique symbol`.
//!
//! tsc treats `static readonly p: unique symbol [= Symbol()]` on a class as a
//! "declared owner" for a `unique symbol` annotation, analogous to a
//! `const x: unique symbol` variable declaration. The annotation lowers to
//! `symbol` at the assignability check so a fresh-symbol initializer
//! (`Symbol()` / `Symbol("desc")`) is accepted, and the property's stored
//! type is recovered as `unique symbol` identified by the property's binder
//! symbol so `typeof Class.p` queries see a distinct unique-symbol identity.
//!
//! Before this regression test, the relation-side lowering recognized only
//! variable declarations, so `static readonly p: unique symbol = Symbol()`
//! emitted a spurious `TS2322: Type 'symbol' is not assignable to type
//! 'unique symbol'`. Conformance test
//! `conformance/types/uniqueSymbol/uniqueSymbols.ts` exercised exactly that
//! shape on line 64 (`static readonly readonlyStaticTypeAndCall: unique
//! symbol = Symbol();`).

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

fn ts2322_count(diags: &[(u32, String)]) -> usize {
    diags.iter().filter(|(c, _)| *c == 2322).count()
}

// Unit tests run without the lib, so `Symbol()` is not typed as `symbol`
// here. Use `declare const sym: symbol` to drive the `symbol`-typed
// initializer through the same declaration-site assignability gate.
// The conformance suite (`uniqueSymbols.ts` / `uniqueSymbolsDeclarations.ts`)
// covers the `Symbol()` call form with the real lib loaded.

#[test]
fn static_readonly_unique_symbol_accepts_symbol_typed_initializer() {
    let source = r#"
declare const sym: symbol;
class C {
    static readonly p: unique symbol = sym;
}
"#;
    let diags = check_strict(source);
    assert_eq!(
        ts2322_count(&diags),
        0,
        "static readonly: unique symbol = (symbol value) must not emit TS2322: {diags:?}"
    );
}

#[test]
fn static_readonly_unique_symbol_renamed_property_accepts_symbol_typed_initializer() {
    // §25: the fix must not depend on user-chosen identifier names.
    let source = r#"
declare const aDifferentName: symbol;
class Q {
    static readonly NAMED_KEY: unique symbol = aDifferentName;
}
"#;
    let diags = check_strict(source);
    assert_eq!(
        ts2322_count(&diags),
        0,
        "renamed property must also work: {diags:?}"
    );
}

#[test]
fn static_readonly_unique_symbol_ambient_decl_is_accepted() {
    let source = r#"
class C {
    static readonly p: unique symbol;
}
"#;
    let diags = check_strict(source);
    // The annotation alone — no initializer — must not trigger TS2322.
    assert_eq!(
        ts2322_count(&diags),
        0,
        "ambient static readonly: unique symbol must not emit TS2322: {diags:?}"
    );
}

#[test]
fn static_readonly_unique_symbol_works_inside_class_expression() {
    let source = r#"
declare const sym: symbol;
const CE = class {
    static readonly q: unique symbol = sym;
};
"#;
    let diags = check_strict(source);
    assert_eq!(
        ts2322_count(&diags),
        0,
        "class-expression form must also work: {diags:?}"
    );
}

#[test]
fn static_readonly_unique_symbol_works_inside_namespace() {
    let source = r#"
declare const sym: symbol;
namespace NS {
    export class Inner {
        static readonly r: unique symbol = sym;
    }
}
"#;
    let diags = check_strict(source);
    assert_eq!(
        ts2322_count(&diags),
        0,
        "namespace-nested class must also work: {diags:?}"
    );
}

#[test]
fn const_variable_unique_symbol_still_works() {
    // Negative-coverage anchor: the variable-declaration path must remain
    // accepting after the class-property generalization.
    let source = r#"
declare const sym: symbol;
const k: unique symbol = sym;
"#;
    let diags = check_strict(source);
    assert_eq!(
        ts2322_count(&diags),
        0,
        "const x: unique symbol = (symbol value) must still work: {diags:?}"
    );
}

#[test]
fn non_static_readonly_unique_symbol_initializer_is_still_rejected() {
    // tsc rejects `readonly x: unique symbol = ...` on an *instance*
    // property (no `static`). The relaxation is scoped to the
    // `static readonly` declaration form, not to any readonly property.
    let source = r#"
declare const sym: symbol;
class C {
    readonly p: unique symbol = sym;
}
"#;
    let diags = check_strict(source);
    assert!(
        ts2322_count(&diags) >= 1,
        "instance readonly: unique symbol = (symbol) should still be rejected (tsc parity): {diags:?}"
    );
}

#[test]
fn static_non_readonly_unique_symbol_initializer_is_still_rejected() {
    // tsc rejects `static p: unique symbol = ...` without `readonly`.
    let source = r#"
declare const sym: symbol;
class C {
    static p: unique symbol = sym;
}
"#;
    let diags = check_strict(source);
    assert!(
        ts2322_count(&diags) >= 1,
        "static non-readonly: unique symbol = (symbol) should still be rejected (tsc parity): {diags:?}"
    );
}

#[test]
fn static_readonly_unique_symbol_preserves_distinct_identity_per_property() {
    // Two `static readonly p: unique symbol` properties on the same class
    // must have distinct unique-symbol identities. The conditional checks
    // for assignability from one's typeof to the other's typeof; in tsc
    // those are not mutually assignable, so the conditional resolves to
    // `false`. Assigning `true` triggers TS2322.
    let source = r#"
declare const sym: symbol;
class C {
    static readonly X: unique symbol = sym;
    static readonly Y: unique symbol = sym;
}
type Same = typeof C.X extends typeof C.Y ? true : false;
const sameWrong: Same = true;
"#;
    let diags = check_strict(source);
    assert!(
        ts2322_count(&diags) >= 1,
        "typeof C.X and typeof C.Y must be distinct unique-symbol identities: {diags:?}"
    );
}

#[test]
fn static_readonly_unique_symbol_self_assigns_through_typeof() {
    // Symmetric positive coverage for the distinct-identity test:
    // `typeof C.X = C.X` must remain valid (same unique symbol identity).
    let source = r#"
declare const sym: symbol;
class C {
    static readonly X: unique symbol = sym;
}
const self: typeof C.X = C.X;
"#;
    let diags = check_strict(source);
    assert_eq!(
        ts2322_count(&diags),
        0,
        "self-assignment through typeof must remain valid: {diags:?}"
    );
}

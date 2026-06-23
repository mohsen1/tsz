//! Grammar checks for misplaced `unique symbol` type operators — the
//! `UniqueKeyword` arm of tsc's `checkGrammarTypeOperatorNode`.
//!
//! `unique symbol` is only legal as the type of a `const` variable in a
//! variable statement, a `static readonly` class property, or a `readonly`
//! property signature. Every other position is rejected with a dedicated
//! diagnostic. tsz previously implemented none of these checks (a silent
//! false-negative on the whole placement grammar).
//!
//! Owner: `crates/tsz-checker/src/types/unique_symbol_arena.rs`
//! (`unique_symbol_grammar_violation`) + the per-file sweep in
//! `crates/tsz-checker/src/state/state_checking/source_file.rs`.

use tsz_checker::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn let_and_var_require_const() {
    // TS1332: a variable whose type is `unique symbol` must be `const`.
    let codes = codes("let a: unique symbol;\nvar b: unique symbol;\n");
    assert_eq!(
        codes.iter().filter(|&&c| c == 1332).count(),
        2,
        "both `let` and `var` declarations are TS1332; got {codes:?}"
    );
}

#[test]
fn const_in_variable_statement_is_valid() {
    // Negative control: a `const` variable statement is the canonical valid site.
    let codes = codes("declare const ok: unique symbol;\n");
    assert!(
        !codes.contains(&1332) && !codes.contains(&1335) && !codes.contains(&1333),
        "a `const` unique symbol declaration is valid; got {codes:?}"
    );
}

#[test]
fn binding_pattern_name_is_ts1333() {
    // TS1333: a binding-pattern (non-identifier) name cannot carry the type.
    let codes = codes("declare const {}: unique symbol;\n");
    assert!(
        codes.contains(&1333),
        "a binding-pattern declaration is TS1333; got {codes:?}"
    );
}

#[test]
fn function_parameter_and_return_are_ts1335() {
    // TS1335: function parameter and return-type positions are not allowed.
    let param = codes("declare function takesIt(value: unique symbol): void;\n");
    assert!(
        param.contains(&1335),
        "a `unique symbol` parameter is TS1335; got {param:?}"
    );
    let ret = codes("declare function makesIt(): unique symbol;\n");
    assert!(
        ret.contains(&1335),
        "a `unique symbol` return type is TS1335; got {ret:?}"
    );
}

#[test]
fn type_alias_body_is_ts1335() {
    // TS1335: a bare type-alias body is rejected even though it is never
    // otherwise materialized when the alias is unused — the position-independent
    // sweep still visits it.
    let codes = codes("type Unused = unique symbol;\n");
    assert!(
        codes.contains(&1335),
        "a `unique symbol` type alias body is TS1335; got {codes:?}"
    );
}

#[test]
fn class_property_must_be_static_and_readonly() {
    // TS1331: a class property that is not both `static` and `readonly`.
    let plain = codes("declare class Holder { field: unique symbol; }\n");
    assert!(
        plain.contains(&1331),
        "a plain class property is TS1331; got {plain:?}"
    );
    let readonly_only = codes("declare class Holder { readonly field: unique symbol; }\n");
    assert!(
        readonly_only.contains(&1331),
        "a `readonly` (non-static) class property is TS1331; got {readonly_only:?}"
    );
}

#[test]
fn static_readonly_class_property_is_valid() {
    // Negative control: `static readonly` is the canonical valid class site.
    let codes = codes("declare class Holder { static readonly field: unique symbol; }\n");
    assert!(
        !codes.contains(&1331) && !codes.contains(&1335),
        "a `static readonly` class property is valid; got {codes:?}"
    );
}

#[test]
fn interface_property_must_be_readonly() {
    // TS1330: an interface / type-literal property signature must be `readonly`.
    let codes = codes("interface Shape { writable: unique symbol; }\n");
    assert!(
        codes.contains(&1330),
        "a non-readonly interface property is TS1330; got {codes:?}"
    );
}

#[test]
fn readonly_interface_property_is_valid() {
    // Negative control: a `readonly` property signature is valid.
    let codes = codes("interface Shape { readonly tag: unique symbol; }\n");
    assert!(
        !codes.contains(&1330) && !codes.contains(&1335),
        "a `readonly` interface property is valid; got {codes:?}"
    );
}

#[test]
fn unique_over_non_symbol_is_ts1005() {
    // TS1005: `unique` is only permitted over the `symbol` keyword.
    let codes = codes("declare const bad: unique number;\n");
    assert!(
        codes.contains(&1005),
        "`unique number` reports TS1005 ('symbol' expected); got {codes:?}"
    );
}

#[test]
fn parenthesized_unique_symbol_resolves_through_to_owner() {
    // tsc walks up parenthesized types: `(unique symbol)` on a `const` is valid,
    // and on a `let` still reports TS1332 (the parenthesis is transparent).
    let valid = codes("declare const ok: (unique symbol);\n");
    assert!(
        !valid.contains(&1332) && !valid.contains(&1335),
        "a parenthesized unique symbol on a const is valid; got {valid:?}"
    );
    let invalid = codes("let bad: (unique symbol);\n");
    assert!(
        invalid.contains(&1332),
        "a parenthesized unique symbol on a let is still TS1332; got {invalid:?}"
    );
}

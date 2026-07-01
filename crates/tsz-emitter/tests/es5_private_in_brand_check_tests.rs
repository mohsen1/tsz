//! Regression tests for the ES5 downlevel of the private-in ("ergonomic brand
//! check") operator `#name in obj`.
//!
//! At sub-ES2022 targets `tsc` lowers `#name in obj` to
//! `__classPrivateFieldIn(<state>, obj)`, where `<state>` is the same brand the
//! member's field/method/accessor accessors use: the field's `WeakMap`
//! (`_C_x`) for a private field, or the instances `WeakSet` (`_C_instances`)
//! for an instance method or accessor. Once the private member is lowered to a
//! `WeakMap`/`WeakSet`, the raw `#name in obj` form is invalid JavaScript.
//!
//! Before this fix, tsz applied the transform on the ES2015–ES2021 path but
//! left the ES5 class-to-IR expression converter without an `in`-operator case,
//! so an ES5 class body emitted the raw `#name in obj` verbatim (a syntax
//! error). The converter already lowered `obj.#x` reads/writes on the same
//! path, so only the brand-check operator was missing.
//!
//! Source: `crates/tsz-emitter/src/transforms/class_es5_ast_to_ir_expressions.rs`
//! (`convert_binary_expression`, the `InKeyword` + `PrivateIdentifier` branch).

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print as parse_lower_emit;

fn es5() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::ESNext,
        ..Default::default()
    }
}

/// A private *field* brand check lowers to `__classPrivateFieldIn(<weakmap>, obj)`
/// and never emits the raw `#name in obj` form.
#[test]
fn private_field_brand_check_lowers_to_helper() {
    let output = parse_lower_emit(
        "class C { #x = 1; check(o: any) { return #x in o; } }",
        es5(),
    );
    assert!(
        output.contains("return __classPrivateFieldIn(_C_x, o);"),
        "Private field brand check must lower to __classPrivateFieldIn against the field WeakMap.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("#x in o"),
        "The raw `#x in o` form is invalid ES5 and must not be emitted.\nOutput:\n{output}"
    );
}

/// An instance private *method* brands against the instances `WeakSet`
/// (`_C_instances`), not against a per-field `WeakMap`.
#[test]
fn private_method_brand_check_uses_instances_weakset() {
    let output = parse_lower_emit(
        "class C { #m() { return 1; } check(o: any) { return #m in o; } }",
        es5(),
    );
    assert!(
        output.contains("return __classPrivateFieldIn(_C_instances, o);"),
        "Private method brand check must brand against the instances WeakSet.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("#m in o"),
        "Raw brand check leaked.\nOutput:\n{output}"
    );
}

/// An instance private *getter* brands against the instances `WeakSet`.
#[test]
fn private_getter_brand_check_uses_instances_weakset() {
    let output = parse_lower_emit(
        "class C { get #g() { return 1; } check(o: any) { return #g in o; } }",
        es5(),
    );
    assert!(
        output.contains("return __classPrivateFieldIn(_C_instances, o);"),
        "Private getter brand check must brand against the instances WeakSet.\nOutput:\n{output}"
    );
}

/// A setter-only accessor has no read slot; the brand must still resolve
/// through the write slot to the instances `WeakSet`.
#[test]
fn setter_only_private_accessor_brand_check_resolves() {
    let output = parse_lower_emit(
        "class C { set #s(v: number) {} check(o: any) { return #s in o; } }",
        es5(),
    );
    assert!(
        output.contains("return __classPrivateFieldIn(_C_instances, o);"),
        "Setter-only accessor brand check must resolve via the write slot.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("#s in o"),
        "Raw brand check leaked.\nOutput:\n{output}"
    );
}

/// Two field brand checks in one expression each resolve to their own `WeakMap`.
#[test]
fn multiple_field_brand_checks_each_resolve() {
    let output = parse_lower_emit(
        "class C { #x = 1; #y = 2; check(o: any) { return #x in o && #y in o; } }",
        es5(),
    );
    assert!(
        output.contains("__classPrivateFieldIn(_C_x, o) && __classPrivateFieldIn(_C_y, o)"),
        "Each field brand check must resolve to its own WeakMap.\nOutput:\n{output}"
    );
}

/// The transform composes with a surrounding negation; only the operand is
/// rewritten.
#[test]
fn negated_brand_check_lowers_operand_only() {
    let output = parse_lower_emit(
        "class C { #x = 1; check(o: any) { return !(#x in o); } }",
        es5(),
    );
    assert!(
        output.contains("return !(__classPrivateFieldIn(_C_x, o));"),
        "Negated brand check must lower the operand while preserving the negation.\nOutput:\n{output}"
    );
}

/// The rule keys on the operand being a private identifier with a storage slot,
/// never on its spelling: a renamed class/field produces a correspondingly
/// renamed brand var.
#[test]
fn brand_check_is_not_hardcoded_to_a_name() {
    let output = parse_lower_emit(
        "class Zebra { #alpha = 1; probe(thing: any) { return #alpha in thing; } }",
        es5(),
    );
    assert!(
        output.contains("return __classPrivateFieldIn(_Zebra_alpha, thing);"),
        "Brand var must derive from the actual class/field names, not a hardcoded string.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("#alpha in thing"),
        "Raw brand check leaked for renamed binders.\nOutput:\n{output}"
    );
}

/// The brand check composes with a private field read on the same receiver in
/// the same expression — the pre-existing `obj.#x` read path is unaffected and
/// the brand check now lowers alongside it.
#[test]
fn brand_check_composes_with_private_field_read() {
    let output = parse_lower_emit(
        "class C { #x = 1; f(o: any) { return (#x in o) ? o.#x : 0; } }",
        es5(),
    );
    assert!(
        output.contains(
            "(__classPrivateFieldIn(_C_x, o)) ? __classPrivateFieldGet(o, _C_x, \"f\") : 0"
        ),
        "Brand check and field read must both lower on the ES5 path.\nOutput:\n{output}"
    );
}

/// The transform fires inside a nested arrow that closes over the class scope,
/// not only in the immediate method body.
#[test]
fn brand_check_lowers_inside_nested_arrow() {
    let output = parse_lower_emit(
        "class C { #x = 1; make() { return (o: any) => #x in o; } }",
        es5(),
    );
    assert!(
        output.contains("return __classPrivateFieldIn(_C_x, o);"),
        "Brand check inside a nested arrow must lower.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("#x in o"),
        "Raw brand check leaked in arrow.\nOutput:\n{output}"
    );
}

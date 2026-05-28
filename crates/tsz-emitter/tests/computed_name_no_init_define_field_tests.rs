//! Computed-name class fields WITHOUT an initializer, under
//! `useDefineForClassFields` and a pre-ES2022 target, are still
//! runtime-materialized as defined fields.
//!
//! Structural rule: when a class property declaration has a non-literal
//! computed name and is runtime-materialized as a defined field under define
//! semantics (whether or not it has an initializer), its computed name is
//! hoisted to a temp and referenced at the materialization site, exactly as for
//! a computed-name field that DOES have an initializer.
//!
//! tsc (target=es2015, useDefineForClassFields):
//! ```js
//! var _a;
//! class C {
//!     constructor() {
//!         Object.defineProperty(this, _a, { ... value: void 0 });
//!     }
//! }
//! _a = x;
//! ```
//!
//! These tests vary the computed-name expression and the bound name so they
//! pin the structural behavior rather than a single rendered fingerprint.

use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_with_opts;

fn emit_define_es2015(source: &str) -> String {
    let opts = PrintOptions {
        target: ScriptTarget::ES2015,
        use_define_for_class_fields: true,
        ..Default::default()
    };
    parse_and_print_with_opts(source, opts)
}

/// A no-initializer field with a computed identifier name must hoist the name to
/// a temp, reference the temp at the `Object.defineProperty` site, and assign the
/// temp after the class body.
#[test]
fn no_init_computed_identifier_field_hoists_temp() {
    let source = r#"const x = 1;
class C {
    [x]: string;
}
"#;
    let output = emit_define_es2015(source);

    assert!(
        output.contains("var _a;"),
        "Computed name temp should be declared before the class.\nOutput:\n{output}"
    );
    assert!(
        output.contains("Object.defineProperty(this, _a, {"),
        "No-init computed field must materialize via the hoisted temp, not the inline name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("value: void 0"),
        "No-init defined field uses `value: void 0`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = x;"),
        "Computed name expression must be hoisted to the temp after the class body.\nOutput:\n{output}"
    );
    // The raw key expression must NOT appear inline as a property-name string.
    assert!(
        !output.contains("Object.defineProperty(this, \"x\","),
        "Computed name must not be emitted inline as a literal property name.\nOutput:\n{output}"
    );
}

/// The rule keys on structure, not the spelling of the bound name or the key
/// expression. A different name (`k`) and a different non-literal expression
/// (member access) must behave identically.
#[test]
fn no_init_computed_member_access_field_hoists_temp_other_name() {
    let source = r#"const keys = { id: "id" };
class Model {
    [keys.id]: number;
}
"#;
    let output = emit_define_es2015(source);

    assert!(
        output.contains("var _a;"),
        "Computed name temp should be declared before the class.\nOutput:\n{output}"
    );
    assert!(
        output.contains("Object.defineProperty(this, _a, {"),
        "No-init computed field must materialize via the hoisted temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = keys.id;"),
        "Member-access computed name must be hoisted to the temp after the class body.\nOutput:\n{output}"
    );
}

/// A no-init computed field SIBLING to an initialized computed field must keep
/// temp ordering deterministic: the first member gets `_a`, the second `_b`,
/// matching source order and tsc's allocation.
#[test]
fn no_init_and_initialized_computed_siblings_keep_temp_order() {
    let source = r#"const a = "a";
const b = "b";
class C {
    [a]: string;
    [b] = 1;
}
"#;
    let output = emit_define_es2015(source);

    assert!(
        output.contains("var _a, _b;"),
        "Both computed names should be hoisted to temps.\nOutput:\n{output}"
    );
    // First member (no initializer) -> _a, defined with value: void 0.
    assert!(
        output.contains("Object.defineProperty(this, _a, {"),
        "First (no-init) computed field should materialize through _a.\nOutput:\n{output}"
    );
    // Second member (initialized) -> _b.
    assert!(
        output.contains("Object.defineProperty(this, _b, {"),
        "Second (initialized) computed field should materialize through _b.\nOutput:\n{output}"
    );
    // Temp assignments preserve source order: _a = a, _b = b.
    let a_pos = output
        .find("_a = a")
        .expect("_a = a hoist assignment should be present");
    let b_pos = output
        .find("_b = b")
        .expect("_b = b hoist assignment should be present");
    assert!(
        a_pos < b_pos,
        "Hoisted temp assignments should preserve source order (_a before _b).\nOutput:\n{output}"
    );
}

/// Negative/fallback: WITHOUT `useDefineForClassFields`, a no-initializer typed
/// field has no runtime effect, so it must NOT be materialized and must NOT
/// allocate a hoist temp for its computed name.
#[test]
fn no_init_computed_field_without_define_is_erased() {
    let opts = PrintOptions {
        target: ScriptTarget::ES2015,
        use_define_for_class_fields: false,
        ..Default::default()
    };
    let source = r#"const x = 1;
class C {
    [x]: string;
}
"#;
    let output = parse_and_print_with_opts(source, opts);

    assert!(
        !output.contains("Object.defineProperty(this, _a"),
        "Without define semantics, a no-init typed field must not be materialized.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var _a;"),
        "Without materialization, no computed-name temp should be allocated.\nOutput:\n{output}"
    );
}

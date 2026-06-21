//! Heritage base typed from the flow-narrowed reference, not the declared symbol.
//!
//! Structural rule: when a class `extends <expr>` and `<expr>` is a value
//! reference (parameter/variable) whose control-flow-narrowed type at the
//! heritage location is a constructor, tsc types the base via `checkExpression`
//! (flow narrowing) and accepts it. tsz previously read the binding's *declared*
//! type via `get_type_of_symbol` in the heritage constructor check, so a binding
//! narrowed from `Ctor | undefined` to `Ctor` (e.g. inside `klass ? ... : ...`)
//! was wrongly seen as possibly-undefined and emitted a false TS2507.

use crate::context::CheckerOptions;
use crate::test_utils::{check_with_options, diagnostic_count};

fn strict() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
}

const CTOR: &str = "type Ctor = new (...args: any[]) => object;\n";

#[test]
fn conditional_narrowed_parameter_base_no_ts2507() {
    let src = format!(
        "{CTOR}export const make = (klass?: Ctor) => klass ? class extends klass {{}} : null;\n"
    );
    let diags = check_with_options(&src, strict());
    assert_eq!(
        diagnostic_count(&diags, 2507),
        0,
        "a base narrowed to a constructor by `klass ? ...` must not emit TS2507: {diags:?}"
    );
}

#[test]
fn if_guarded_narrowed_parameter_base_no_ts2507() {
    let src = format!(
        "{CTOR}export function make(klass?: Ctor) {{ if (klass) {{ return class extends klass {{}}; }} return null; }}\n"
    );
    let diags = check_with_options(&src, strict());
    assert_eq!(
        diagnostic_count(&diags, 2507),
        0,
        "a base narrowed to a constructor inside `if (klass)` must not emit TS2507: {diags:?}"
    );
}

#[test]
fn and_narrowed_parameter_base_no_ts2507() {
    let src =
        format!("{CTOR}export const make = (klass?: Ctor) => klass && class extends klass {{}};\n");
    let diags = check_with_options(&src, strict());
    assert_eq!(
        diagnostic_count(&diags, 2507),
        0,
        "a base narrowed to a constructor by `klass && ...` must not emit TS2507: {diags:?}"
    );
}

#[test]
fn narrowed_local_variable_base_no_ts2507() {
    // Vary the binder kind: a local `let` narrowed by assignment, not a parameter.
    let src = format!(
        "{CTOR}declare const base: Ctor | undefined;\nexport function make() {{ let local: Ctor | undefined = base; if (local) {{ return class extends local {{}}; }} return null; }}\n"
    );
    let diags = check_with_options(&src, strict());
    assert_eq!(
        diagnostic_count(&diags, 2507),
        0,
        "a local variable narrowed to a constructor must not emit TS2507: {diags:?}"
    );
}

#[test]
fn unnarrowed_optional_constructor_base_still_ts2507() {
    // Negative control: an un-narrowed `Ctor | undefined` base is genuinely
    // possibly-undefined and must still report TS2507.
    let src =
        format!("{CTOR}declare const base: Ctor | undefined;\nexport class C extends base {{}}\n");
    let diags = check_with_options(&src, strict());
    assert_eq!(
        diagnostic_count(&diags, 2507),
        1,
        "an un-narrowed `Ctor | undefined` base must still report TS2507: {diags:?}"
    );
}

#[test]
fn non_constructor_narrowed_base_still_ts2507() {
    // Negative control: narrowing away `undefined` from a non-constructor value
    // still leaves a non-constructable base, which must report TS2507.
    let src = "declare const base: number | undefined;\nexport function make() { if (base) { return class extends base {}; } return null; }\n";
    let diags = check_with_options(src, strict());
    assert_eq!(
        diagnostic_count(&diags, 2507),
        1,
        "a narrowed non-constructor base must still report TS2507: {diags:?}"
    );
}

#[test]
fn hoisted_var_shadowing_outer_class_base_no_ts2507() {
    // Monotonicity guard (TS conformance `classWithStaticFieldInParameterInitializer.3`,
    // microsoft/TypeScript#36295): a class `extends C` inside a parameter
    // initializer where the arrow body hoists `var C`. The flow-narrowed value
    // type of that binding is not a constructor, but the declared type already
    // is — accepting the base must dominate. The flow-narrowed path is only a
    // *fallback*, so it must never turn an already-accepted base into TS2507.
    let src = "class C {}\n((b = class extends C { static x = 1 }) => { var C; })();\n";
    let diags = check_with_options(src, strict());
    assert_eq!(
        diagnostic_count(&diags, 2507),
        0,
        "a hoisted-var heritage base whose declared type is a constructor must not emit TS2507: {diags:?}"
    );
}

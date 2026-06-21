//! `class extends <value>` types the base via flow-narrowing, not the declared
//! symbol type (TS2507). Mined from effect. (#14260)

use super::super::core::*;

/// A `class extends <param>` whose base parameter is flow-narrowed to a
/// constructor (here inside a truthy `klass ? … : …`) must be accepted — tsc
/// types the base via `checkExpression`. tsz previously read the declared
/// `Ctor | undefined` symbol type and reported a false TS2507. Binder name
/// varied to confirm the fix is structural.
#[test]
fn class_extends_flow_narrowed_value_base_no_ts2507() {
    for klass in ["klass", "Base", "ctor"] {
        let source = format!(
            r#"
type Ctor<T = {{}}> = new (...args: Array<any>) => T;
export const make = ({klass}?: Ctor) => ({klass} ? class extends {klass} {{}} : null);
"#
        );
        let diagnostics = compile_and_get_diagnostics(&source);
        assert!(
            !has_error(&diagnostics, 2507),
            "[{klass}] a flow-narrowed `Ctor` base must be accepted; got TS2507. \
             Diagnostics: {diagnostics:#?}"
        );
    }
}

/// Negative control: an un-narrowed `Ctor | undefined` base must STILL report
/// TS2507 — the fix uses the flow-narrowed expression type, which here is still
/// `Ctor | undefined` (not a constructor), so the error is preserved.
#[test]
fn class_extends_unnarrowed_optional_base_still_ts2507() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type Ctor<T = {}> = new (...args: Array<any>) => T;
declare const maybe: Ctor | undefined;
class C extends maybe {}
        ",
    );
    assert!(
        has_error(&diagnostics, 2507),
        "an un-narrowed `Ctor | undefined` base must still report TS2507. \
         Diagnostics: {diagnostics:#?}"
    );
}

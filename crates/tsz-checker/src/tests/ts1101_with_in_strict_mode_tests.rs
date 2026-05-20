//! Regression tests for TS1101 — `with` statements are not allowed in
//! strict mode.
//!
//! Class bodies and modules are auto-strict per the ECMA spec, so a `with`
//! syntactically nested inside either is a parser-level error. Source:
//! `compiler/conformance/salsa/plainJSBinderErrors.ts` line 31 expects
//! TS1101 at the `with` keyword span.

use crate::test_utils::check_source_diagnostics;

fn diag_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// `with` inside a class method body — class body is auto-strict.
#[test]
fn ts1101_with_inside_class_method_in_strict_mode() {
    let codes = diag_codes(
        r#"
class C {
    m(o: object) {
        with (o) {
            return 1;
        }
    }
}
"#,
    );
    assert!(
        codes.contains(&1101),
        "Expected TS1101 for `with` in class body. Got: {codes:?}"
    );
}

/// Anti-hardcoding cover: same shape with different identifier names.
#[test]
fn ts1101_with_inside_class_method_renamed() {
    let codes = diag_codes(
        r#"
class WrappedThing {
    invoke(target: object) {
        with (target) {
            return 2;
        }
    }
}
"#,
    );
    assert!(
        codes.contains(&1101),
        "Renamed variant: TS1101 should fire for `with` in class body. Got: {codes:?}"
    );
}

/// Module-top-level cover: a file containing `import`/`export` is
/// auto-strict per the ECMA spec. TS1101 must fire.
#[test]
fn ts1101_with_at_module_top_level() {
    let codes = diag_codes(
        r#"
export const _marker = 1;
declare const o: any;
with (o) {
    let x = 3;
}
"#,
    );
    assert!(
        codes.contains(&1101),
        "Module top-level (file has `export`) is strict; TS1101 should fire. Got: {codes:?}"
    );
}

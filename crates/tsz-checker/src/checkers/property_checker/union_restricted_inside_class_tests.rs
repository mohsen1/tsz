//! Regression tests for union restricted-property access inside class bodies.

fn check_diagnostics(source: &str) -> Vec<u32> {
    crate::test_utils::check_source_codes(source)
}

fn has_code(diagnostics: &[u32], code: u32) -> bool {
    diagnostics.contains(&code)
}

/// tsc emits TS2339 (not TS2445) even inside a class body: the union surface
/// doesn't expose a property restricted across different declaring classes.
#[test]
fn union_different_protected_inside_class_method_emits_ts2339() {
    let diagnostics = check_diagnostics(
        r#"
        class Foo { protected prop: number = 0; }
        class Bar { protected prop: string = ""; }
        class Accessor {
            method(x: Foo | Bar) {
                let v = x.prop;
            }
        }
    "#,
    );

    assert!(
        has_code(&diagnostics, 2339),
        "expected TS2339 for union with different-class protected, inside class body"
    );
    assert!(
        !has_code(&diagnostics, 2445),
        "should NOT emit TS2445 for union type - got: {diagnostics:?}"
    );
}

/// Same rule with different name choices: proves the fix is not keyed to
/// specific identifiers (`Foo`/`Bar`/`prop`).
#[test]
fn union_different_protected_inside_class_method_different_names_emits_ts2339() {
    let diagnostics = check_diagnostics(
        r#"
        class Alpha { protected value: number = 0; }
        class Beta { protected value: string = ""; }
        class Visitor {
            run(x: Alpha | Beta) {
                let v = x.value;
            }
        }
    "#,
    );

    assert!(
        has_code(&diagnostics, 2339),
        "expected TS2339 for union with different-class protected (Alpha/Beta/value)"
    );
    assert!(
        !has_code(&diagnostics, 2445),
        "should NOT emit TS2445 for union type (Alpha/Beta/value)"
    );
}

#[test]
fn union_same_protected_declaring_class_inside_method_does_not_emit_ts2339() {
    let diagnostics = check_diagnostics(
        r#"
        class Base { protected prop: number = 0; }
        class Derived extends Base { }
        class Accessor {
            method(x: Base | Derived) {
                let v = x.prop;
            }
        }
    "#,
    );

    assert!(!has_code(&diagnostics, 2339));
}

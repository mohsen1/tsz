//! Parameter decorators on class members do NOT put their arguments in the
//! class-definition TDZ: `@dec(C)` on a constructor/method parameter of class
//! `C` must not report TS2449 "Class 'C' used before its declaration", even
//! though the same expression on a method/property decorator does.
//!
//! Regression: tsz's `is_in_decorator_of_declaration` treated parameter
//! decorators the same as member decorators, causing false TS2449s on the
//! constructor/method-parameter cases in
//! `useBeforeDeclaration_classDecorators.2.ts`.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn get_diagnostic_codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            experimental_decorators: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

#[test]
fn parameter_decorator_referencing_enclosing_class_no_ts2449() {
    let source = r#"
declare const dec: any;
class C {
    constructor(@dec(C) a: any) {}
    static m1(@dec(C) a: any) {}
    m2(@dec(C) a: any) {}
}
"#;
    let codes = get_diagnostic_codes(source);
    assert!(
        !codes.contains(&2449),
        "parameter decorators must not trigger TS2449 on the enclosing class; got: {codes:?}"
    );
}

#[test]
fn method_decorator_referencing_enclosing_class_still_emits_ts2449() {
    // The narrowing is parameter-decorator-specific: method/property
    // decorators continue to put the class in TDZ at class-body evaluation
    // time. This locks in that the fix doesn't broaden to all decorators.
    let source = r#"
declare const dec: any;
class C {
    @dec(C) m() {}
}
"#;
    let codes = get_diagnostic_codes(source);
    assert!(
        codes.contains(&2449),
        "method decorator referencing the enclosing class still needs TS2449; got: {codes:?}"
    );
}

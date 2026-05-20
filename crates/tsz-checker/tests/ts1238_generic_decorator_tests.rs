//! Regression test for TS1238 with generic class decorators.
//!
//! Mirrors the conformance failure
//! `TypeScript/tests/cases/conformance/decorators/decoratorCallGeneric.ts`.
//! tsz currently emits 0 diagnostics for this scenario; tsc emits TS1238.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source, check_source_codes_experimental_decorators};

const SOURCE: &str = r#"
interface I<T> {
    prototype: T,
    m: () => T
}
function dec<T>(c: I<T>) { }

@dec
class C {
    _brand: any;
    static m() {}
}
"#;

#[test]
fn ts1238_generic_decorator_constraint_mismatch_emits() {
    // The decorator `dec<T>(c: I<T>)` requires `c.m` to return `T`, but
    // class `C` has `static m()` returning `void`. tsc emits TS1238 because
    // generic inference cannot satisfy the constraint.
    let codes = check_source_codes_experimental_decorators(SOURCE);
    assert!(codes.contains(&1238), "expected TS1238, got {codes:?}");
}

#[test]
fn ts1238_generic_decorator_constraint_mismatch_emits_with_target_es2015() {
    // Same as above but with `@target: es2015`, mirroring the conformance
    // test directive `// @target: es2015`. Some library type loading is
    // target-dependent; if this test fails while the other passes, the
    // bypass is target/lib-driven.
    let opts = CheckerOptions {
        experimental_decorators: true,
        target: tsz_common::common::ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let codes: Vec<u32> = check_source(SOURCE, "test.ts", opts)
        .iter()
        .map(|d| d.code)
        .collect();
    assert!(
        codes.contains(&1238),
        "expected TS1238 with target=es2015, got {codes:?}"
    );
}

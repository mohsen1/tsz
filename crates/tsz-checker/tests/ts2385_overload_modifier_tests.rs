//! Tests for TS2385: Overload signatures must all be public, private or protected.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_lib_files};

const LIB_NAMES: &[&str] = &["es5.d.ts", "es2015.d.ts"];

fn get_error_codes(source: &str) -> Vec<u32> {
    let libs = load_lib_files(LIB_NAMES);
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
        .iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn ts2385_public_overload_private_impl() {
    let codes = get_error_codes("class C { public foo(): void; private foo(x?: any) { } }");
    assert!(codes.contains(&2385), "Expected TS2385, got: {codes:?}");
}

#[test]
fn ts2385_protected_overloads_private_impl() {
    let codes = get_error_codes(
        "class C {
            protected foo(x: string): void;
            protected foo(x: number): void;
            private foo(x: any) { }
        }",
    );
    let count = codes.iter().filter(|&&c| c == 2385).count();
    assert_eq!(count, 2, "Expected 2 TS2385 errors, got {count}: {codes:?}");
}

#[test]
fn ts2385_no_error_when_modifiers_match() {
    let codes =
        get_error_codes("class C { private foo(x: string): void; private foo(x: any) { } }");
    assert!(
        !codes.contains(&2385),
        "Should NOT emit TS2385, got: {codes:?}"
    );
}

#[test]
fn ts2385_no_error_all_public() {
    let codes = get_error_codes("class C { public foo(x: string): void; public foo(x: any) { } }");
    assert!(
        !codes.contains(&2385),
        "Should NOT emit TS2385, got: {codes:?}"
    );
}

#[test]
fn ts2385_static_methods_checked_separately() {
    let codes = get_error_codes(
        "class C {
            private foo(x: string): void;
            private foo(x: any) { }
            private static foo(x: string): void;
            public static foo(x: any) { }
        }",
    );
    let count = codes.iter().filter(|&&c| c == 2385).count();
    // tsc emits TS2385 twice for the static overload signature: once from
    // the duplicate-identifier check and once from overload-modifier agreement.
    assert_eq!(
        count, 2,
        "Expected 2 TS2385 for static mismatch (matches tsc), got {count}: {codes:?}"
    );
}

#[test]
fn ts2385_implicit_public_matches_explicit_public() {
    let codes = get_error_codes("class C { foo(x: string): void; public foo(x: any) { } }");
    assert!(
        !codes.contains(&2385),
        "Implicit public should match explicit, got: {codes:?}"
    );
}

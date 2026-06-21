//! Regression: an untyped (`any`) callee invoked with an explicit type argument
//! that is a keyword type (e.g. `void`) must emit only `TS2347` ("Untyped
//! function calls may not accept type arguments"), not a spurious `TS2693`
//! ("'void' only refers to a type, but is being used as a value here"). The
//! untyped-call path validated type arguments through the value-context
//! `get_type_of_node`; it now uses the type-node checker.
//!
//! Owner: `crates/tsz-checker/src/types/computation/call/inner.rs`.

use tsz_checker::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn untyped_call_with_void_type_arg_is_only_ts2347() {
    let codes = codes(
        r#"
declare const f: any;
export const x = f<void>(undefined);
"#,
    );
    assert!(
        codes.contains(&2347),
        "untyped call with a type argument is TS2347; got {codes:?}"
    );
    assert!(
        !codes.contains(&2693),
        "a `void` keyword type argument must not be reported as a value (TS2693); got {codes:?}"
    );
}

#[test]
fn untyped_call_with_other_keyword_type_args_no_ts2693() {
    // `unknown`, `never`, `string` keyword type args likewise must not be
    // reported as values.
    let codes = codes(
        r#"
declare const g: any;
export const a = g<unknown>();
export const b = g<never>();
export const c = g<string>();
"#,
    );
    assert!(
        !codes.contains(&2693),
        "keyword type arguments on an untyped call must not emit TS2693; got {codes:?}"
    );
}

#[test]
fn untyped_call_with_unresolved_type_arg_still_ts2304() {
    // Negative control: the loop's purpose — an unresolved type *name* in a type
    // argument still emits TS2304 even though the call is untyped.
    let codes = codes(
        r#"
declare const h: any;
export const y = h<InvalidReference>();
"#,
    );
    assert!(
        codes.contains(&2304),
        "an unresolved type name in a type argument still emits TS2304; got {codes:?}"
    );
}

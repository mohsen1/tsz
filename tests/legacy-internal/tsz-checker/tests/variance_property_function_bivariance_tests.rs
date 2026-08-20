//! Tests for `strictFunctionTypes: false` bivariance on function-*typed
//! properties (not method syntax), matching TypeScript's
//! `varianceCantBeStrictWhileStructureIsnt.ts` conformance case.
//!
//! `tsc` compares a property whose type is a function (`member: (cb: T) =>
//! void`) contravariantly on its parameters only when `strictFunctionTypes`
//! is on; when the flag is off, the comparison is bivariant, exactly like a
//! method (`member(cb: T): void`). The structural function-subtype checker
//! (`are_parameters_compatible_impl`) already honored the flag correctly, but
//! two separate O(1) "same generic base" variance fast paths — one in the
//! solver's `check_application_variance` relation-query boundary and one in
//! `SubtypeChecker::resolve_application_variances` — asked for a
//! *declared-mode* variance mask that is session-stable and ignores
//! `strictFunctionTypes` entirely. That always measured `T` as strictly
//! contravariant and hard-rejected the pair before the structural (correct)
//! comparison ever ran.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn diagnostics(source: &str, strict_function_types: bool) -> Vec<u32> {
    let opts = CheckerOptions {
        strict: false,
        strict_null_checks: false,
        no_implicit_any: false,
        strict_function_types,
        ..CheckerOptions::default()
    };
    check_source(source, "test.ts", opts)
        .iter()
        .map(|d| d.code)
        .collect()
}

/// The conformance witness itself: a property (not method) function member,
/// compared in every direction between a wide and a narrow instantiation.
#[test]
fn property_function_member_is_bivariant_under_relaxed_strict_function_types() {
    let source = r#"
interface Foo<T> {
    member: (cb: T) => void;
}

declare var a: Foo<string>;
declare var b: Foo<"">;

a = b;
b = a;
"#;
    assert_eq!(
        diagnostics(source, false),
        Vec::<u32>::new(),
        "strictFunctionTypes:false must make a property-typed function member bivariant"
    );
}

/// Same shape, renamed binders — the fix must not be keyed on any identifier.
#[test]
fn property_function_member_bivariance_holds_under_renamed_binders() {
    let source = r#"
interface Wrapper<Value> {
    handler: (input: Value) => void;
}

declare var wide: Wrapper<number>;
declare var narrow: Wrapper<1>;

wide = narrow;
narrow = wide;
"#;
    assert_eq!(diagnostics(source, false), Vec::<u32>::new());
}

/// Two distinct generic interfaces in one file (the original conformance
/// fixture has both `Foo` and `Bar`), each independently bivariant.
#[test]
fn multiple_generic_interfaces_are_independently_bivariant() {
    let source = r#"
interface Foo<T> {
    member: (cb: T) => void;
}

interface Bar<T> {
    member: (cb: T) => void;
}

declare var a: Foo<string>;
declare var b: Foo<"">;
declare var a2: Bar<string>;
declare var b2: Bar<"">;

a = b;
b = a;
a2 = b2;
b2 = a2;
"#;
    assert_eq!(diagnostics(source, false), Vec::<u32>::new());
}

/// Negative control: the same property-function shape under the DEFAULT
/// (strict) `strictFunctionTypes` setting must still reject the unsound
/// narrowing direction. This is the case the fast path must keep rejecting;
/// the fix must not have turned bivariance on unconditionally.
#[test]
fn property_function_member_stays_contravariant_under_strict_function_types() {
    let source = r#"
interface Foo<T> {
    member: (cb: T) => void;
}

declare var a: Foo<string>;
declare var b: Foo<"">;

a = b;
"#;
    let codes = diagnostics(source, true);
    assert!(
        codes.contains(&2322),
        "strictFunctionTypes:true must still reject the contravariant-unsound \
         direction; got {codes:?}"
    );
}

/// Negative control: a genuinely incompatible pair of type arguments (no
/// literal/widened relationship in either direction) must stay rejected even
/// under `strictFunctionTypes: false` — bivariance tries both directions, it
/// does not accept unrelated types.
#[test]
fn property_function_member_rejects_unrelated_type_arguments_even_when_relaxed() {
    let source = r#"
interface Foo<T> {
    member: (cb: T) => void;
}

declare var a: Foo<string>;
declare var c: Foo<number>;

a = c;
"#;
    let codes = diagnostics(source, false);
    assert!(
        codes.contains(&2322),
        "unrelated type arguments must still fail under bivariance; got {codes:?}"
    );
}

/// Regression control: method-syntax members (already bivariant regardless
/// of `strictFunctionTypes`) must be unaffected by this fix.
#[test]
fn method_syntax_member_bivariance_is_unaffected() {
    let source = r#"
interface Foo<T> {
    member(cb: T): void;
}

declare var a: Foo<string>;
declare var b: Foo<"">;

a = b;
b = a;
"#;
    assert_eq!(diagnostics(source, true), Vec::<u32>::new());
    assert_eq!(diagnostics(source, false), Vec::<u32>::new());
}

//! Tests for type-literal-nested generic-without-args TS2314 behavior.
//!
//! When a generic class or interface is referenced without required type
//! arguments inside a type literal (e.g. `var x: { a: C } = ...`), tsc:
//!   1. Emits TS2314 ("Generic type 'C<T>' requires 1 type argument(s).")
//!   2. Treats the reference as any-like errorType so cascading TS2322 from a
//!      structural mismatch against the naked-type-parameter form is
//!      suppressed.
//!
//! Without (2), the in-type-literal resolution path leaves `C` as a bare
//! `Lazy(DefId)` whose evaluation is `C<T>` (with naked `T`).  Comparing a
//! concrete instance like `new C<number>()` against that produces a spurious
//! TS2322 "Type 'C<number>' is not assignable to type 'C<T>'." that tsc
//! never emits.

use tsz_checker::test_utils::check_source_codes;

/// Class type used without type arguments in a type literal property.
/// Reproduces `genericsWithoutTypeParameters1.ts`: assigning `new C<number>()`
/// to `{ a: C }` must emit TS2314 once, with no extra TS2322 from the
/// `C<number>` ⇄ `C<T>` mismatch.
#[test]
fn test_class_in_type_literal_without_args_no_spurious_ts2322() {
    let source = r#"
class C<T> {
    foo(): T { return null as any; }
}
var x: { a: C } = { a: new C<number>() };
"#;
    let codes = check_source_codes(source);

    assert!(
        codes.contains(&2314),
        "Expected TS2314 for missing type arguments on C, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Should not emit TS2322 when the target type is any-like errorType from missing type args, got: {codes:?}"
    );
}

/// Interface type used without type arguments in a type literal property.
#[test]
fn test_interface_in_type_literal_without_args_no_spurious_ts2322() {
    let source = r#"
interface I<T> {
    bar(): T;
}
var x: { a: I } = { a: { bar() { return 1; } } };
"#;
    let codes = check_source_codes(source);

    assert!(
        codes.contains(&2314),
        "Expected TS2314 for missing type arguments on I, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Should not emit TS2322 when the target type is any-like errorType from missing type args, got: {codes:?}"
    );
}

/// Bare generic self references inside class/interface construction must still
/// become any-like after TS2314. Otherwise return inference can collapse an
/// erroneous return expression plus fallthrough into `undefined`, producing a
/// cascading TS2532 on callers of the inferred method.
#[test]
fn test_recursive_class_bare_generic_annotation_is_any_like_after_ts2314() {
    let source = r#"
class MemberName<A, B, C> {
    static create<A, B, C>(): MemberName {
    }
}

class PullTypeSymbol<A, B, C> {
    private _elementType: PullTypeSymbol = null as any;

    toString() {
        this.getScopedNameEx().toString();
    }

    getScopedNameEx() {
        if (this._elementType) {
            return MemberName.create();
        }
    }
}
"#;
    let codes = check_source_codes(source);

    assert!(
        codes.contains(&2314),
        "Expected TS2314 for missing type arguments, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2532),
        "Should not emit TS2532 from a method whose erroneous return annotation is any-like, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2339),
        "Should not emit TS2339 through a property whose erroneous bare generic annotation is any-like, got: {codes:?}"
    );
}

/// Sanity check: when type arguments ARE supplied correctly, no TS2314 is
/// emitted and a real type mismatch still produces TS2322.
#[test]
fn test_class_in_type_literal_with_args_still_emits_ts2322_on_real_mismatch() {
    let source = r#"
class C<T> {
    foo(): T { return null as any; }
}
var x: { a: C<string> } = { a: new C<number>() };
"#;
    let codes = check_source_codes(source);

    assert!(
        !codes.contains(&2314),
        "TS2314 should not be emitted when type arguments are supplied, got: {codes:?}"
    );
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for genuine string vs number mismatch, got: {codes:?}"
    );
}

/// Default-typed generics used bare in a type literal must continue to
/// resolve to `Application(base, [defaults])` so they remain assignable from
/// matching concrete instantiations.
#[test]
fn test_all_defaults_in_type_literal_no_ts2314_no_ts2322() {
    let source = r#"
interface Box<T = string> {
    value: T;
}
declare const b: Box<string>;
var x: { a: Box } = { a: b };
"#;
    let codes = check_source_codes(source);

    assert!(
        !codes.contains(&2314),
        "TS2314 should not fire for a generic with all-default type params, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Default-typed generic should be assignable from explicit form, got: {codes:?}"
    );
}

//! `Foo<unknown, ...>` should NOT bypass variance when assigned to `Foo<X, ...>`
//! for arbitrary X. The previous fast path returned `true` for any same-base
//! Application pair where source args were all `unknown`, which incorrectly
//! allowed `A<unknown>` (with covariant/invariant T) to be assignable to
//! `A<string>` even though `unknown` is NOT a subtype of `string`.
//!
//! The fix narrows the fast path to require at least one `never` arg in the
//! target — the typical signature of inference fallback for Thenable-like
//! constructors (e.g., `EPromise<never, A>`). User-written `A<unknown>` →
//! `A<string>` no longer slips through.

use tsz_common::options::checker::CheckerOptions;

fn diags_strict(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
}

#[test]
fn ts2322_emitted_for_class_unknown_arg_to_string_arg() {
    let diags = diags_strict(
        r#"
declare class A<T> { foo: T }
declare const a: A<unknown>;
const b: A<string> = a;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322
            && d.message_text.contains("'A<unknown>'")
            && d.message_text.contains("'A<string>'")),
        "Expected TS2322 'A<unknown>' not assignable to 'A<string>'; got: {diags:?}"
    );
}

#[test]
fn ts2322_emitted_for_interface_unknown_arg_to_string_arg() {
    let diags = diags_strict(
        r#"
interface A<T> { foo: T }
declare const a: A<unknown>;
const b: A<string> = a;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322
            && d.message_text.contains("'A<unknown>'")
            && d.message_text.contains("'A<string>'")),
        "Expected TS2322 'A<unknown>' not assignable to 'A<string>'; got: {diags:?}"
    );
}

#[test]
fn class_accessor_with_divergent_set_type_emits_ts2322_on_setter_write() {
    // From conformance test getAndSetNotIdenticalType2.ts.
    // The setter takes `A<string>`; the getter returns `A<T>` (instantiated to
    // `A<unknown>`). Reading `r = x.x` gives `r: A<unknown>`; writing `x.x = r`
    // must fail because `A<unknown>` is not assignable to the setter's `A<string>`.
    let diags = diags_strict(
        r#"
class A<T> { foo: T = null as any }
class C<T> {
    data: A<T> = null as any;
    get x(): A<T> { return this.data; }
    set x(v: A<string>) { this.data = v; }
}
const x = new C<number>();
const r = x.x;
x.x = r;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322
            && d.message_text.contains("'A<number>'")
            && d.message_text.contains("'A<string>'")),
        "Expected TS2322 'A<number>' not assignable to 'A<string>' on setter write; got: {diags:?}"
    );
}

#[test]
fn class_with_no_explicit_type_args_unknown_arg_still_errors_on_concrete_target() {
    // `new C()` defaults to `C<unknown>`; `r = x.x` is `A<unknown>`; the setter
    // takes `A<string>`. Even with the inference-fallback shortcut allowing
    // `unknown` → some patterns, this assignment must still fail because the
    // target arg is `string` (concrete), not `never`.
    let diags = diags_strict(
        r#"
class A<T> { foo: T = null as any }
class C<T> {
    data: A<T> = null as any;
    get x(): A<T> { return this.data; }
    set x(v: A<string>) { this.data = v; }
}
const x = new C();
const r = x.x;
x.x = r;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322
            && d.message_text.contains("'A<unknown>'")
            && d.message_text.contains("'A<string>'")),
        "Expected TS2322 'A<unknown>' not assignable to 'A<string>'; got: {diags:?}"
    );
}

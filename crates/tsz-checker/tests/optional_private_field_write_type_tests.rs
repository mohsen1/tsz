//! Tests for TS2322 on optional private field writes (#x?: T).
//!
//! Structural rule: When a private class field is optional (`#x?: T`), under
//! `strictNullChecks` without `exactOptionalPropertyTypes`, its write-context
//! type is `T | undefined`, matching the public optional field rule.
//!
//! Repro: kysely false TS2322 assigning `undefined` to `#promise?: Promise<void>`
//! (issue #10749).

use crate::context::CheckerOptions;

fn diags_with_options(source: &str, options: CheckerOptions) -> Vec<u32> {
    crate::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn strict_opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
}

fn eopt_opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        exact_optional_property_types: true,
        ..CheckerOptions::default()
    }
}

// ---------------------------------------------------------------------------
// Minimal repro from issue #10749
// ---------------------------------------------------------------------------

#[test]
fn optional_private_promise_field_assign_undefined_no_error() {
    let source = r#"
class Mutex {
    #promise?: Promise<void>
    unlock(): void {
        this.#promise = undefined
    }
}
"#;
    let diags = diags_with_options(source, strict_opts());
    let ts2322: Vec<_> = diags.iter().filter(|&&c| c == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "assigning undefined to optional #promise should not produce TS2322; got codes: {diags:?}"
    );
}

#[test]
fn optional_private_fn_field_assign_undefined_no_error() {
    // Different name from the repro — ensures the fix is not name-keyed.
    let source = r#"
class Mutex {
    #resolve?: () => void
    unlock(): void {
        this.#resolve = undefined
    }
}
"#;
    let diags = diags_with_options(source, strict_opts());
    let ts2322: Vec<_> = diags.iter().filter(|&&c| c == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "assigning undefined to optional #resolve should not produce TS2322; got codes: {diags:?}"
    );
}

#[test]
fn optional_private_field_different_names_no_error() {
    // Another name variation: #counter?, #state? — verifies structural, not spelling match.
    let source = r#"
class Counter {
    #counter?: number
    #state?: string
    reset(): void {
        this.#counter = undefined
        this.#state = undefined
    }
}
"#;
    let diags = diags_with_options(source, strict_opts());
    let ts2322: Vec<_> = diags.iter().filter(|&&c| c == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "assigning undefined to optional #counter/#state should not produce TS2322; got codes: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative: wrong-type assignment still errors
// ---------------------------------------------------------------------------

#[test]
fn optional_private_field_wrong_type_still_errors() {
    let source = r#"
class Foo {
    #bar?: number
    bad(): void {
        this.#bar = "string"
    }
}
"#;
    let diags = diags_with_options(source, strict_opts());
    let ts2322_count = diags.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "assigning a string to optional #bar: number should produce TS2322; got codes: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative: non-optional private field rejects undefined
// ---------------------------------------------------------------------------

#[test]
fn non_optional_private_field_rejects_undefined() {
    let source = r#"
class Foo {
    #bar: number = 0
    bad(): void {
        this.#bar = undefined
    }
}
"#;
    let diags = diags_with_options(source, strict_opts());
    let ts2322_count = diags.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "assigning undefined to non-optional #bar should produce TS2322; got codes: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// exactOptionalPropertyTypes: assigning undefined IS an error
// ---------------------------------------------------------------------------

#[test]
fn exact_optional_private_field_rejects_undefined() {
    let source = r#"
class Foo {
    #val?: number
    bad(): void {
        this.#val = undefined
    }
}
"#;
    let diags = diags_with_options(source, eopt_opts());
    let ts2322_count = diags.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "with exactOptionalPropertyTypes, assigning undefined to optional #val should produce TS2322; got codes: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Public optional field behaves identically (regression guard)
// ---------------------------------------------------------------------------

#[test]
fn optional_public_field_assign_undefined_no_error() {
    let source = r#"
class Foo {
    bar?: number
    reset(): void {
        this.bar = undefined
    }
}
"#;
    let diags = diags_with_options(source, strict_opts());
    let ts2322: Vec<_> = diags.iter().filter(|&&c| c == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "assigning undefined to optional public field should not produce TS2322; got codes: {diags:?}"
    );
}

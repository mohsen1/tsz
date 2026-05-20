//! Locks in the JSX-spread missing-required-property suppression: when the
//! solver reports the spread expression is structurally assignable to the
//! whole props type, the property-by-property TS2741 missing-prop check must
//! NOT fire.
//!
//! Without this gate, a JSX spread like `{...{}}` into an intrinsic element
//! whose props type only requires members inherited from `Object.prototype`
//! (e.g. `toString`) emits a false-positive TS2741 — the missing-prop check
//! walks the spread's declared property shape (empty for `{}`) and never
//! consults the apparent shape that includes inherited Object members.
//!
//! Regression target: TypeScript's `tsxAttributeResolution5.tsx`, the
//! `<test2 {...{}} />` line.
//!
//! These tests load the standard lib so `{}` carries Object's inherited
//! members, matching the conformance harness and the `tsz` CLI.

use tsz_checker::test_utils::{check_source_with_libs, load_lib_files};
use tsz_common::checker_options::{CheckerOptions, JsxMode};
use tsz_common::diagnostics::Diagnostic;

const LIB_NAMES: &[&str] = &[
    "es5.d.ts",
    "es2015.d.ts",
    "es2015.core.d.ts",
    "es2015.collection.d.ts",
    "es2015.iterable.d.ts",
    "es2015.generator.d.ts",
    "es2015.promise.d.ts",
    "es2015.proxy.d.ts",
    "es2015.reflect.d.ts",
    "es2015.symbol.d.ts",
    "es2015.symbol.wellknown.d.ts",
];

fn check_jsx_with_libs_diagnostics(source: &str) -> Vec<Diagnostic> {
    let options = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        ..CheckerOptions::default()
    };
    let libs = load_lib_files(LIB_NAMES);
    check_source_with_libs(source, "test.tsx", options, &libs)
}

fn check_jsx_with_libs(source: &str) -> Vec<u32> {
    check_jsx_with_libs_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect()
}

/// Empty spread into a target whose only required member is inherited from
/// `Object.prototype` (here `toString`) must NOT emit TS2741 — `{}` is
/// structurally assignable to `{ toString(): string }` because every object
/// has `toString` via Object inheritance. The fix in
/// `crates/tsz-checker/src/checkers/jsx/props/resolution.rs` consults the
/// solver's whole-spread assignability via `is_assignable_to(spread, props)`
/// before invoking the property-by-property missing check; if assignability
/// holds, `spread_covers_all` is set and the missing check is skipped.
///
/// Mirrors the `<test2 {...{}} />` line of TypeScript's
/// `tsxAttributeResolution5.tsx`.
#[test]
fn empty_spread_into_object_prototype_only_target_does_not_emit_ts2741() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test2: Attribs2;
    }
}
interface Attribs2 {
    toString(): string;
}
<test2 {...{}} />;
"#;
    let codes = check_jsx_with_libs(source);
    assert!(
        !codes.contains(&2741),
        "Empty spread into target requiring only inherited `toString` must not emit TS2741; got: {codes:?}"
    );
}

/// Sibling lock: when the spread is genuinely missing a non-inherited required
/// property (`x: string`), TS2741 MUST still fire. The assignability gate only
/// suppresses missing-prop errors when the solver agrees the spread fully
/// satisfies the props type.
///
/// Mirrors the `<test1 {...{}} />` line of `tsxAttributeResolution5.tsx`.
#[test]
fn empty_spread_missing_non_inherited_required_property_still_emits_ts2741() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test1: Attribs1;
    }
}
interface Attribs1 {
    x: string;
}
<test1 {...{}} />;
"#;
    let codes = check_jsx_with_libs(source);
    assert!(
        codes.contains(&2741),
        "Empty spread into target requiring `x: string` must still emit TS2741; got: {codes:?}"
    );
}

/// Generic spread whose constraint mismatches the target keeps TS2322 on the
/// element tag. This guards the assignability gate from masking real type
/// mismatches: when `T extends { x: number }` is spread into a target wanting
/// `{ x: string }`, the solver's `is_assignable_to(T, target)` returns false,
/// so `spread_covers_all` stays unset and the deferred per-spread check
/// emits TS2322.
///
/// Mirrors the `make2`/`make3` lines of `tsxAttributeResolution5.tsx`.
#[test]
fn generic_spread_with_incompatible_constraint_still_emits_ts2322() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test1: Attribs1;
    }
}
interface Attribs1 {
    x: string;
}
function make2<T extends { x: number }>(obj: T) {
    return <test1 {...obj} />;
}
function make3<T extends { y: string }>(obj: T) {
    return <test1 {...obj} />;
}
"#;
    let codes = check_jsx_with_libs(source);
    let ts2322_count = codes.iter().filter(|&&c| c == 2322).count();
    assert!(
        ts2322_count >= 2,
        "Both make2 and make3 must emit TS2322 (generic spread incompatible with target); got codes: {codes:?}"
    );
}

/// Generic JSX spreads must render the source as the type parameter itself,
/// not the normalized intersection with its constraint. tsc reports
/// `Type 'T' is not assignable to type 'Attribs1'.` for both functions.
#[test]
fn generic_spread_ts2322_message_preserves_type_parameter_source() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test1: Attribs1;
    }
}
interface Attribs1 {
    x: string;
}
function make2<T extends { x: number }>(obj: T) {
    return <test1 {...obj} />;
}
function make3<U extends { y: string }>(obj: U) {
    return <test1 {...obj} />;
}
"#;
    let diagnostics = check_jsx_with_libs_diagnostics(source);
    let ts2322_messages: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2322)
        .map(|diag| diag.message_text.as_str())
        .collect();
    assert_eq!(
        ts2322_messages.len(),
        2,
        "Expected one TS2322 for each incompatible generic JSX spread, got: {diagnostics:#?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains("Type 'T' is not assignable to type 'Attribs1'")),
        "Expected first generic spread to render source as T, got: {ts2322_messages:#?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains("Type 'U' is not assignable to type 'Attribs1'")),
        "Expected renamed generic spread to render source as U, got: {ts2322_messages:#?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .all(|message| !message.contains("& { x: number; }")
                && !message.contains("& { y: string; }")),
        "Generic spread source display must not include the evaluated constraint intersection: {ts2322_messages:#?}"
    );
}

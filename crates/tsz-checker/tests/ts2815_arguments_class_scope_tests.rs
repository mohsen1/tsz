//! `arguments` referenced in a class field initializer, static block, or
//! computed property name.
//!
//! tsc reports TS2815 (`'arguments' cannot be referenced in property
//! initializers or class static initialization blocks`) **only** when the
//! reference would otherwise capture an *enclosing* function's `arguments`
//! object — i.e. the class holding the initializer/static-block is itself
//! nested inside a (non-arrow) function. When the class sits at module or
//! global scope there is no such function, so `arguments` is simply an
//! undefined name: tsc reports the ordinary "cannot find name" diagnostic
//! (TS2662 with an inherited-`Function`-member suggestion, or TS2304 for a
//! computed property name evaluated in the class's enclosing scope), never
//! TS2815.
//!
//! Regression witness: tsz previously emitted TS2815 unconditionally for any
//! `arguments` in these positions, ignoring the enclosing-function scope.
//!
//! The suite has two tiers:
//!   * the TS2815 *gating* decision does not depend on lib resolution, so it is
//!     asserted with the fast no-lib helper;
//!   * the exact fall-through code (TS2304 / TS2662) is a lib-dependent
//!     name-resolution result, so it is asserted with `es5.d.ts` wired in
//!     (that lib carries the `Function` interface whose inherited members drive
//!     the TS2662 suggestion).
//!
//! Names are varied across cases (`C`/`Widget`, `f`/`run`) so the rule is
//! anchored on scope structure, not on any particular identifier.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::test_utils::{
    check_source_strict_codes, check_source_with_libs_code_messages, load_lib_files,
    strict_checker_options,
};

const TS2815: u32 = 2815;
const TS2304: u32 = 2304;
const TS2662: u32 = 2662;

fn es5_libs() -> Vec<Arc<LibFile>> {
    load_lib_files(&["es5.d.ts"])
}

/// Diagnostic codes for `source`, strict, with `es5.d.ts` wired in.
fn lib_codes(source: &str) -> Vec<u32> {
    check_source_with_libs_code_messages(source, "test.ts", strict_checker_options(), &es5_libs())
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

// ===========================================================================
// Tier 1 — the TS2815 gating decision (lib-independent).
// ===========================================================================

/// The core regression guard: TS2815 must not fire for `arguments` at module
/// scope, where there is no enclosing function to capture from.
fn assert_no_ts2815(source: &str) {
    let codes = check_source_strict_codes(source);
    assert!(
        !codes.contains(&TS2815),
        "TS2815 must not fire for `arguments` at module scope (no enclosing \
         function to capture): {codes:?}\nsource:\n{source}"
    );
}

fn assert_ts2815(source: &str) {
    let codes = check_source_strict_codes(source);
    assert!(
        codes.contains(&TS2815),
        "expected TS2815 for `arguments` inside an enclosing function, got: \
         {codes:?}\nsource:\n{source}"
    );
}

#[test]
fn module_scope_static_block_is_not_ts2815() {
    assert_no_ts2815("class C { static { arguments; } }");
}

#[test]
fn module_scope_instance_field_initializer_is_not_ts2815() {
    assert_no_ts2815("class Widget { p = arguments; }");
}

#[test]
fn module_scope_static_field_initializer_is_not_ts2815() {
    assert_no_ts2815("class C { static p = arguments; }");
}

#[test]
fn module_scope_computed_property_name_is_not_ts2815() {
    assert_no_ts2815("class Widget { [arguments] = 1; }");
}

#[test]
fn module_scope_transparent_arrow_is_not_ts2815() {
    // Arrow functions are transparent for `arguments`; the arrow does not
    // introduce an enclosing-function boundary, so the module-scope rule holds.
    assert_no_ts2815("class C { p = (() => arguments)(); }");
}

#[test]
fn module_scope_own_static_arguments_member_is_not_ts2815() {
    assert_no_ts2815("class C { static arguments = 1; static { arguments; } }");
}

#[test]
fn function_scope_static_block_is_ts2815() {
    assert_ts2815("function f() { class C { static { arguments; } } }");
}

#[test]
fn function_scope_field_initializer_is_ts2815() {
    assert_ts2815("function run() { class Widget { p = arguments; } }");
}

#[test]
fn function_scope_computed_property_name_is_ts2815() {
    assert_ts2815("function f() { class C { [arguments] = 1; } }");
}

#[test]
fn function_scope_transparent_arrow_is_ts2815() {
    assert_ts2815("function run() { class C { p = (() => arguments)(); } }");
}

#[test]
fn class_nested_in_method_static_block_is_ts2815() {
    // The method is a (non-arrow) function boundary, so the inner class's
    // static block sits inside a function: TS2815.
    assert_ts2815("class Outer { m() { class Inner { static { arguments; } } } }");
}

// Regression controls: `arguments` that resolves normally must be untouched.

#[test]
fn method_body_arguments_has_no_grammar_error() {
    let codes = check_source_strict_codes("class C { m() { return arguments; } }");
    assert!(
        !codes.contains(&TS2815) && !codes.contains(&TS2304) && !codes.contains(&TS2662),
        "`arguments` in a method body must resolve cleanly: {codes:?}"
    );
}

#[test]
fn function_body_arguments_has_no_grammar_error() {
    let codes = check_source_strict_codes("function f() { return arguments; }");
    assert!(
        !codes.contains(&TS2815) && !codes.contains(&TS2304) && !codes.contains(&TS2662),
        "`arguments` in a function body must resolve cleanly: {codes:?}"
    );
}

// ===========================================================================
// Tier 2 — exact fall-through code parity (lib-backed).
// ===========================================================================

// NOTE on the module-scope *fall-through* code (TS2304 / TS2662): the real
// `tsz` CLI, run with the full default lib set, reports
//   `class C { static { arguments; } }`  -> TS2662 (suggests `C.arguments`)
//   `class Widget { [arguments] = 1; }`  -> TS2304
// matching `typescript@7.0.2`. The unit-test harness cannot reproduce that
// fall-through — with only `es5.d.ts` wired in it emits no cannot-find-name
// diagnostic for an unresolved module-scope `arguments` (a known
// harness/CLI divergence, cf. #16125) — so those exact codes are pinned at
// the CLI/oracle level in the PR rather than asserted here. What this suite
// pins is the part the harness *does* see faithfully and the part the fix
// actually changes: that TS2815 no longer fires at module scope.

#[test]
fn module_scope_own_static_member_suggests_ts2662() {
    // With an own static `arguments`, the suggestion resolves and tsc/tsz agree
    // on TS2662.
    let codes = lib_codes("class C { static arguments = 1; static { arguments; } }");
    assert!(
        !codes.contains(&TS2815),
        "TS2815 must not fire with an own static `arguments` member: {codes:?}"
    );
    assert!(
        codes.contains(&TS2662),
        "expected TS2662 static-member suggestion: {codes:?}"
    );
}

#[test]
fn function_scope_static_block_reports_ts2815_lib_backed() {
    let codes = lib_codes("function f() { class C { static { arguments; } } }");
    assert!(
        codes.contains(&TS2815),
        "expected TS2815 inside an enclosing function (lib-backed): {codes:?}"
    );
}

// ===========================================================================
// Tier 3 — TS2662/TS2663 reach inherited members, not just own declared ones
// (#16815). `arguments`/`caller` are not declared on the class at all; they
// are inherited from `lib.es5.d.ts`'s `Function` interface, which every
// class constructor type structurally carries.
// ===========================================================================

#[test]
fn instance_field_initializer_arguments_suggests_inherited_static_member() {
    let codes = lib_codes("class Widget { p = arguments; }");
    assert!(
        !codes.contains(&TS2815),
        "TS2815 must not fire at module scope: {codes:?}"
    );
    assert!(
        !codes.contains(&TS2304),
        "expected the inherited-member suggestion (TS2662), not bare TS2304: {codes:?}"
    );
    assert!(
        codes.contains(&TS2662),
        "expected TS2662 suggesting the inherited `Function.arguments` static member: {codes:?}"
    );
}

#[test]
fn static_block_arguments_suggests_inherited_static_member() {
    let codes = lib_codes("class C { static { arguments; } }");
    assert!(
        codes.contains(&TS2662) && !codes.contains(&TS2304) && !codes.contains(&TS2815),
        "expected TS2662 suggesting the inherited `Function.arguments` static member: {codes:?}"
    );
}

#[test]
fn instance_field_initializer_caller_suggests_inherited_static_member() {
    // `caller` is a different `Function`-interface member than `arguments`,
    // confirming the fix is a general property lookup, not special-cased.
    let codes = lib_codes("class Widget { p = caller; }");
    assert!(
        codes.contains(&TS2662) && !codes.contains(&TS2304),
        "expected TS2662 suggesting the inherited `Function.caller` static member: {codes:?}"
    );
}

#[test]
fn instance_field_initializer_inherited_base_static_suggests_ts2662() {
    // Not a `Function` member at all — an ordinary static inherited from a
    // real `extends` base, exercising the same property-lookup fallback.
    let codes = lib_codes("class Base { static s = 1; } class D extends Base { p = s; }");
    assert!(
        codes.contains(&TS2662) && !codes.contains(&TS2304),
        "expected TS2662 suggesting the inherited static member 'D.s': {codes:?}"
    );
}

#[test]
fn instance_field_initializer_inherited_base_instance_member_suggests_ts2663() {
    let codes = lib_codes("class Base { m() {} } class E extends Base { p = m; }");
    let ts2663 = 2663;
    assert!(
        codes.contains(&ts2663) && !codes.contains(&TS2304),
        "expected TS2663 suggesting the inherited instance member 'this.m': {codes:?}"
    );
}

#[test]
fn instance_field_initializer_unreachable_name_does_not_get_a_spurious_suggestion() {
    // The fallback row: a name that resolves to nothing anywhere must not
    // trigger a false TS2662/TS2663 suggestion. Per the Tier 2 note above,
    // this harness (only `es5.d.ts` wired in) emits no cannot-find-name
    // diagnostic at all for an unresolved module-scope name — the real
    // TS2304 fallback is pinned at the CLI/oracle level in the PR (see
    // `class G { p = nonexistent; }` in the issue's adjacent-case matrix).
    let codes = lib_codes("class Widget { p = totallyUnreachableName; }");
    assert!(
        !codes.contains(&TS2662),
        "must not spuriously suggest a static member for an unreachable name: {codes:?}"
    );
}

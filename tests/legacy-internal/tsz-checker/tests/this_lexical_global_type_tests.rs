//! `this` at the top level of a script (and inside arrow chains, which have
//! no `this` binding of their own and inherit lexically) resolves to its real
//! TYPE — `typeof globalThis` in a script, `undefined` in a module —
//! regardless of `noImplicitThis`. Previously `dispatch_this_keyword`
//! (`dispatch/this.rs`) gated the whole type computation behind
//! `no_implicit_this()`, not just the implicit-any *warning*: with
//! `noImplicitThis` off (the common case — `strict: false` or unset), every
//! such `this` silently fell through to `any`, and even with `noImplicitThis`
//! on, an arrow-captured `this` still hard-coded `any` alongside its TS7041
//! warning instead of resolving `typeof globalThis`.
//!
//! Structural rule: `tsc` computes the type of `this` from lexical position
//! alone; `noImplicitThis` only controls whether it also emits a warning
//! (TS2683 for a plain function's owner-less `this`, TS7041 for an arrow
//! capturing the global `this`) — never what the type actually is. tsz's
//! `dispatch/this.rs` must make the same two decisions independently instead
//! of gating the type on the warning's flag.
//!
//! Oracle-verified against pinned `typescript@7.0.2`.
//!
//! Owner: `dispatch/this.rs::dispatch_this_keyword`; the concrete `typeof
//! globalThis` object comes from `CheckerContext::global_this_surface_type`
//! (see `global_this_typeof_surface_tests.rs`).

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_codes, check_with_options_code_messages};

const TS2403: u32 = 2403; // Subsequent variable declarations must have the same type.
const TS2532: u32 = 2532; // Object is possibly 'undefined'.
const TS7041: u32 = 7041; // The containing arrow function captures the global value of 'this'.

fn no_implicit_this_options() -> CheckerOptions {
    CheckerOptions {
        no_implicit_this: true,
        ..CheckerOptions::default()
    }
}

// ---------------------------------------------------------------------------
// Positive: script top level, `noImplicitThis` off (the fixture's own
// `strict: false` shape) — `this` must type-identify with `typeof globalThis`,
// not `any`.
// ---------------------------------------------------------------------------

#[test]
fn top_level_this_is_typeof_global_this_under_default_options() {
    let codes = check_source_codes(
        r#"
var t!: typeof globalThis;
var t = this;
"#,
    );
    assert!(
        !codes.contains(&TS2403),
        "top-level `this` must type-identify with `typeof globalThis`, not fall back to `any`: {codes:?}"
    );
}

#[test]
fn arrow_body_this_is_typeof_global_this_under_default_options() {
    let codes = check_source_codes(
        r#"
var q = () => {
    var t!: typeof globalThis;
    var t = this;
};
"#,
    );
    assert!(
        !codes.contains(&TS2403),
        "an arrow function has no `this` of its own and inherits the lexical (script) `this`, `typeof globalThis`: {codes:?}"
    );
}

#[test]
fn nested_arrow_chain_this_is_typeof_global_this_under_default_options() {
    let codes = check_source_codes(
        r#"
var q = () => () => {
    var t!: typeof globalThis;
    var t = this;
};
"#,
    );
    assert!(
        !codes.contains(&TS2403),
        "a chain of arrows still inherits the script-level `this`: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Positive: `noImplicitThis` on — the warning fires, but the resolved TYPE is
// unaffected (still `typeof globalThis`, never downgraded to `any`).
// ---------------------------------------------------------------------------

#[test]
fn arrow_capturing_this_still_types_as_global_this_alongside_ts7041() {
    let pairs = check_with_options_code_messages(
        r#"
var q = () => {
    var t!: typeof globalThis;
    var t = this;
};
"#,
        no_implicit_this_options(),
    );
    let codes: Vec<u32> = pairs.iter().map(|(c, _)| *c).collect();
    assert!(
        codes.contains(&TS7041),
        "noImplicitThis must still warn that the arrow captures the global `this`: {codes:?}"
    );
    assert!(
        !codes.contains(&TS2403),
        "the TS7041 warning must not change the resolved type away from `typeof globalThis`: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative controls: cases this fix must not touch.
// ---------------------------------------------------------------------------

#[test]
fn plain_function_this_is_still_any_under_default_options() {
    // A plain (non-arrow) function's owner-less `this` is `any` regardless of
    // `noImplicitThis` — unaffected by this fix, which only concerns the
    // lexically-inherited (arrow/top-level) case.
    let codes = check_source_codes(
        r#"
function f() {
    var t!: any;
    var t = this;
}
"#,
    );
    assert!(
        !codes.contains(&TS2403),
        "a plain function's `this` is still `any`: {codes:?}"
    );
}

#[test]
fn plain_function_this_still_reports_ts2683_under_no_implicit_this() {
    let pairs = check_with_options_code_messages(
        "function f() { return this; }",
        no_implicit_this_options(),
    );
    assert!(
        pairs.iter().any(|(code, _)| *code == 2683),
        "a plain function's implicit-any `this` warning is unaffected by this fix: {pairs:?}"
    );
}

#[test]
fn module_top_level_this_is_still_undefined() {
    // An external module's top-level `this` is `undefined`, not
    // `typeof globalThis` — unaffected by this fix (`is_external_module()`
    // branch unchanged). Accessing a property on it under strict null checks
    // still reports TS2532.
    let codes = check_with_options_code_messages(
        r#"
export {};
this.foo;
"#,
        CheckerOptions {
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    let codes: Vec<u32> = codes.iter().map(|(c, _)| *c).collect();
    assert!(
        codes.contains(&TS2532),
        "module top-level `this` is `undefined`, so a property read is possibly-undefined: {codes:?}"
    );
}

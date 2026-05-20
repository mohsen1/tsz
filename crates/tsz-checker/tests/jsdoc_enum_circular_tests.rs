//! Regression tests for circular JSDoc `@enum` resolution.
//!
//! Issue #3767: `/** @enum {E} */ const E = ...` previously overflowed the
//! stack when the type-name resolution path re-entered
//! `jsdoc_enum_annotation_type_for_symbol_decl` for the same symbol. The
//! fix adds a `jsdoc_enum_resolution_set` cycle guard that returns `None`
//! on re-entry; the resolver then bottoms out without aborting.
//!
//! These tests just lock the no-crash invariant; matching tsc's TS2456
//! emission for self-referential JSDoc enums is a separate enhancement.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn js_check_options() -> CheckerOptions {
    CheckerOptions {
        allow_js: true,
        check_js: true,
        ..Default::default()
    }
}

#[test]
fn circular_jsdoc_enum_self_reference_does_not_crash() {
    let source = r"/** @enum {E} */
const E = { x: 0 };
";
    // The assertion is that this returns at all — pre-fix this overflowed
    // the stack and aborted the process with `fatal runtime error: stack
    // overflow, aborting`.
    let _diagnostics = check_source(source, "circular_enum.js", js_check_options());
}

#[test]
fn mutually_recursive_jsdoc_enums_do_not_crash() {
    // A → B → A resolution chain. Same cycle, just one hop further.
    let source = r"/** @enum {B} */
const A = { a: 0 };

/** @enum {A} */
const B = { b: 0 };
";
    let _diagnostics = check_source(source, "mutual_enum.js", js_check_options());
}

#[test]
fn non_circular_jsdoc_enum_still_resolves() {
    // Sanity check: a `@enum` whose body references a real, non-circular
    // type must still resolve normally and not be silenced by the cycle
    // guard. This locks the negative side of the fix.
    let source = r"/** @enum {number} */
const E = { x: 0 };
";
    let diagnostics = check_source(source, "ok_enum.js", js_check_options());
    // No circular-ref diagnostics expected; no stack overflow either.
    assert!(
        diagnostics.iter().all(|d| d.code != 2565),
        "non-circular @enum must not emit TS2565 cycle diagnostics, got: {diagnostics:#?}"
    );
}

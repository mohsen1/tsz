//! Regression tests: a builtin-lib `type X = SomeInterface` alias must resolve to
//! the referenced interface, not collapse to the empty object `{}`.
//!
//! `resolve_lib_type_with_params` previously fed type-alias declaration nodes
//! through `lower_merged_interface_declarations`, which treats a type-alias node
//! as an interface with no members and returns a *non-ERROR* empty object. That
//! empty object then shadowed the real alias body, so `keyof`/indexed-access/
//! property lookups over lib aliases such as `SVGMatrix = DOMMatrix`,
//! `WindowProxy = Window`, or `ElementTagNameMap = HTMLElementTagNameMap & …`
//! all saw `{}`. The sibling resolver `resolve_lib_type_by_name` already guarded
//! this with an `is_type_alias` check; the fix brings both paths in line.
//!
//! The rule is structural ("a lib type-alias symbol lowers via the type-alias
//! path, never interface lowering") so it must hold regardless of the alias body
//! shape — a plain reference, an intersection, or a `Pick`/`Exclude` composite.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{check_source_with_libs, load_lib_files};

fn check_with_dom(source: &str) -> Vec<Diagnostic> {
    let libs = load_lib_files(&["es5.d.ts", "es2015.d.ts", "dom.d.ts"]);
    // `load_lib_files` silently skips missing files. A partial load (e.g. only
    // `es5.d.ts`) would leave DOM globals like `SVGMatrix`/`Window` undefined,
    // and assertions of the form `!any(d.code == 2339)` would pass on
    // `TS2304: Cannot find name` instead — green-lighting a broken test. Require
    // every requested lib file is present so the test actually exercises DOM.
    assert_eq!(
        libs.len(),
        3,
        "all three lib files must be loaded for DOM tests"
    );
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
}

/// `type SVGMatrix = DOMMatrix` — a plain alias to an interface. Accessing a
/// member that lives directly on `DOMMatrix` must succeed (no `TS2339` on `{}`).
#[test]
fn lib_alias_to_plain_interface_exposes_members() {
    // Snippet is type-correct under tsc: every diagnostic is a regression.
    let diags = check_with_dom("declare const m: SVGMatrix; const x = m.a;");
    assert!(
        diags.is_empty(),
        "SVGMatrix (= DOMMatrix) must expose DOMMatrix members, got: {diags:#?}"
    );
}

/// `type WindowProxy = Window` — a different alias/interface pair, proving the
/// fix is not keyed to one spelling.
#[test]
fn lib_alias_to_window_interface_exposes_members() {
    let diags = check_with_dom("declare const w: WindowProxy; const d = w.document;");
    assert!(
        diags.is_empty(),
        "WindowProxy (= Window) must expose Window members, got: {diags:#?}"
    );
}

/// `type ElementTagNameMap = HTMLElementTagNameMap & Pick<…>` — an intersection
/// alias. `keyof` over it must be a non-empty key space (so a known tag key is
/// accepted), proving the alias did not collapse to `{}` (whose `keyof` is
/// `never`). A wrong key must still be rejected so the test fails on a regression
/// that flips the alias to `any` rather than reporting `{}`.
#[test]
fn lib_intersection_alias_keyof_is_not_never() {
    // If `keyof ElementTagNameMap` were `never` (the `{}` bug), `K` would be
    // `never` and the `"div"` literal would be unassignable. With the alias
    // resolved correctly, `"div"` is a valid key and the assignment type-checks;
    // the bogus key `"definitely-not-a-real-tag"` must still be rejected, which
    // is the lock against a regression that masks the alias as `any`.
    let source = r#"
type K = keyof ElementTagNameMap;
const k: K = "div";
const bad: K = "definitely-not-a-real-tag";
"#;
    let diags = check_with_dom(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        vec![2322],
        "exactly one TS2322 for the bogus tag key (alias must expose a real key union, not `never` or `any`), got: {diags:#?}"
    );
}

/// Indexed access over the intersection alias resolves to the element type, so
/// a member of that element (`HTMLDivElement.align`) is reachable, and a member
/// that is NOT on the element must still be rejected — proving the alias did
/// not collapse to `any` (which would silently admit `.notReal`).
#[test]
fn lib_intersection_alias_indexed_access_resolves_element() {
    let source = r#"
declare const el: ElementTagNameMap["div"];
const a = el.align;
const b = el.notReal;
"#;
    let diags = check_with_dom(source);
    // `.align` (a real HTMLDivElement member) must succeed; `.notReal` must be
    // rejected with a missing-property diagnostic. tsc emits TS2339; tsz can
    // emit either TS2339 or TS2812 (which adds a 'try including the dom lib'
    // hint) depending on whether the lib option lists "dom" explicitly. Accept
    // either, but require exactly one diagnostic on the bad access and none on
    // the good one.
    assert_eq!(
        diags.len(),
        1,
        "exactly one diagnostic expected, got: {diags:#?}"
    );
    let d = &diags[0];
    assert!(
        d.code == 2339 || d.code == 2812,
        "`.notReal` must be rejected as a missing property (TS2339 or TS2812), got code {} message {:?}",
        d.code,
        d.message_text
    );
    assert!(
        d.message_text.contains("notReal"),
        "the rejection must be about the `.notReal` access, not `.align`; message = {:?}",
        d.message_text
    );
}

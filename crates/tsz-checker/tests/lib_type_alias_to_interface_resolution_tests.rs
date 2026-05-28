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
    assert!(!libs.is_empty(), "DOM lib files should be available");
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
}

/// `type SVGMatrix = DOMMatrix` — a plain alias to an interface. Accessing a
/// member that lives directly on `DOMMatrix` must succeed (no TS2339 on `{}`).
#[test]
fn lib_alias_to_plain_interface_exposes_members() {
    let diags = check_with_dom("declare const m: SVGMatrix; const x = m.a;");
    assert!(
        !diags.iter().any(|d| d.code == 2339),
        "SVGMatrix (= DOMMatrix) must expose DOMMatrix members, got: {diags:#?}"
    );
}

/// `type WindowProxy = Window` — a different alias/interface pair, proving the
/// fix is not keyed to one spelling.
#[test]
fn lib_alias_to_window_interface_exposes_members() {
    let diags = check_with_dom("declare const w: WindowProxy; const d = w.document;");
    assert!(
        !diags.iter().any(|d| d.code == 2339),
        "WindowProxy (= Window) must expose Window members, got: {diags:#?}"
    );
}

/// `type ElementTagNameMap = HTMLElementTagNameMap & Pick<…>` — an intersection
/// alias. `keyof` over it must be a non-empty key space (so a known tag key is
/// accepted), proving the alias did not collapse to `{}` (whose `keyof` is
/// `never`).
#[test]
fn lib_intersection_alias_keyof_is_not_never() {
    // If `keyof ElementTagNameMap` were `never` (the `{}` bug), `K` would be
    // `never` and the `"div"` literal would be unassignable. With the alias
    // resolved correctly, `"div"` is a valid key and the assignment type-checks.
    let source = r#"
type K = keyof ElementTagNameMap;
const k: K = "div";
"#;
    let diags = check_with_dom(source);
    assert!(
        !diags.iter().any(|d| d.code == 2322 || d.code == 2339),
        "keyof ElementTagNameMap must be a real key union, got: {diags:#?}"
    );
}

/// Indexed access over the intersection alias resolves to the element type, so
/// a member of that element (`HTMLDivElement.align`) is reachable.
#[test]
fn lib_intersection_alias_indexed_access_resolves_element() {
    let source = r#"
declare const el: ElementTagNameMap["div"];
const a = el.align;
"#;
    let diags = check_with_dom(source);
    assert!(
        !diags.iter().any(|d| d.code == 2339),
        "ElementTagNameMap[\"div\"] must resolve to HTMLDivElement, got: {diags:#?}"
    );
}

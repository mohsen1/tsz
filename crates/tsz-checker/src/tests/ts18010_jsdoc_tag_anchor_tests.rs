//! Regression tests for TS18010 ("An accessibility modifier cannot be used
//! with a private identifier.") anchor in JS files.
//!
//! When a `.js`/`.cjs`/`.mjs` file declares a class with a private-named
//! member (`#name`) and a JSDoc accessibility tag (`@public`/`@private`/
//! `@protected`), tsc anchors the diagnostic at the JSDoc tag itself —
//! e.g. underlining `@public` inside `/** @public */` — rather than at the
//! whole member declaration.
//!
//! Source: `conformance/classes/members/privateNames/privateNamesIncompatibleModifiersJs.ts`.
//! See also `crates/tsz-checker/src/state/state_checking/class.rs:373` —
//! the JS-mode branch now consults `jsdoc_accessibility_tag_span` to recover
//! the tag's source position.

use crate::test_utils::check_js_source_diagnostics;

fn diags_for_js(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_js_source_diagnostics(source)
}

/// `@public` JSDoc on a private field: tsc anchors at the `@public` tag,
/// which sits 4 chars (`/** ` prefix) past the start of the comment.
#[test]
fn ts18010_anchors_at_jsdoc_public_tag_on_private_field() {
    let source = "\
class A {
    /** @public */
    #a = 1;
}
";
    let diags = diags_for_js(source);
    let ts18010: Vec<_> = diags.iter().filter(|d| d.code == 18010).collect();
    assert_eq!(
        ts18010.len(),
        1,
        "Expected exactly one TS18010, got: {diags:#?}"
    );
    let d = ts18010[0];
    let start = d.start as usize;
    let end = (d.start + d.length) as usize;
    let underlined = &source[start..end];
    assert_eq!(
        underlined, "@public",
        "TS18010 must underline the JSDoc tag itself, got: {underlined:?} at start={start}"
    );
}

/// Anti-hardcoding cover: same shape with `@private` and a different field
/// name proves the helper isn't matching one specific tag spelling or name.
#[test]
fn ts18010_anchors_at_jsdoc_private_tag_renamed_field() {
    let source = "\
class WrappedThing {
    /** @private */
    #secret = 0;
}
";
    let diags = diags_for_js(source);
    let ts18010: Vec<_> = diags.iter().filter(|d| d.code == 18010).collect();
    assert_eq!(ts18010.len(), 1, "Expected one TS18010, got: {diags:#?}");
    let d = ts18010[0];
    let underlined = &source[d.start as usize..(d.start + d.length) as usize];
    assert_eq!(underlined, "@private");
}

/// Multi-line JSDoc: the tag is on a non-first line. Anchor must still
/// point at the `@protected` tag, not at the `/**` opener.
#[test]
fn ts18010_anchors_at_protected_tag_in_multiline_jsdoc() {
    let source = "\
class C {
    /**
     * @protected
     */
    #m() { return 1; }
}
";
    let diags = diags_for_js(source);
    let ts18010: Vec<_> = diags.iter().filter(|d| d.code == 18010).collect();
    assert_eq!(ts18010.len(), 1, "Expected one TS18010, got: {diags:#?}");
    let d = ts18010[0];
    let underlined = &source[d.start as usize..(d.start + d.length) as usize];
    assert_eq!(underlined, "@protected");
}

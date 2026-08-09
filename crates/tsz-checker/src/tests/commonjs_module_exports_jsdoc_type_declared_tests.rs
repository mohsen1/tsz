//! A leading JSDoc `@type` tag on `module.exports = X` / `exports = X` is a
//! declared type, like a `: T` annotation on a variable declaration — later
//! reads of `module.exports` in the same file must see exactly that declared
//! type, not a re-widened structural inference of the initializer.
//!
//! Before this fix, `infer_commonjs_export_rhs_type` ignored the `@type` tag
//! entirely for the export *surface* (though the assignment statement's own
//! excess-property check already honored it via `assignment_ops.rs`), so a
//! later read fell back to the object literal's own inferred shape. Worse,
//! the one caller that does thread the declared type through then widened it
//! for "fresh literal" display anyway, silently turning e.g. `"red" | "blue"`
//! members back into `string` while keeping the alias's display name — a
//! same-name-different-shape TS2322 false positive.
//!
//! Verified against the pinned tsc 7.0.2 (`--allowJs --checkJs`).

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn js_diags(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

const TYPE_MISMATCH: u32 = 2322;

/// `TypeScript/tests/cases/compiler/expandoFunctionContextualTypesJs.ts`
/// (reduced): a `@type`-annotated `module.exports = { color: "red" }` must be
/// readable back as the declared `MyComponentProps` alias — not a
/// structurally widened `{ color: string }` that happens to share its
/// display name.
#[test]
fn jsdoc_type_declared_module_exports_reads_back_as_the_declared_type() {
    let source = concat!(
        "/** @typedef {{ color: \"red\" | \"blue\" }} MyComponentProps */\n",
        "/**\n * @param {{ props: MyComponentProps }} p\n */\n",
        "function expectLiteral(p) {}\n",
        "/**\n * @type {MyComponentProps}\n */\n",
        "module.exports = {\n    color: \"red\"\n}\n",
        "expectLiteral({ props: module.exports });\n",
    );
    let diags = js_diags(source);
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
}

/// The bare `exports = X` form (not `module.exports`) gets the same
/// treatment.
#[test]
fn jsdoc_type_declared_bare_exports_reads_back_as_the_declared_type() {
    let source = concat!(
        "/** @typedef {{ color: \"red\" | \"blue\" }} MyComponentProps */\n",
        "/**\n * @param {{ props: MyComponentProps }} p\n */\n",
        "function expectLiteral(p) {}\n",
        "/**\n * @type {MyComponentProps}\n */\n",
        "exports = {\n    color: \"red\"\n}\n",
        "expectLiteral({ props: exports });\n",
    );
    let diags = js_diags(source);
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
}

/// Renamed typedef/binder: the fix must not key off the literal name
/// `MyComponentProps` from the reduced fixture.
#[test]
fn jsdoc_type_declared_module_exports_renamed_typedef_still_reads_back_correctly() {
    let source = concat!(
        "/** @typedef {{ tone: \"dark\" | \"light\" }} Palette */\n",
        "/**\n * @param {{ theme: Palette }} p\n */\n",
        "function useTheme(p) {}\n",
        "/**\n * @type {Palette}\n */\n",
        "module.exports = {\n    tone: \"dark\"\n}\n",
        "useTheme({ theme: module.exports });\n",
    );
    let diags = js_diags(source);
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
}

/// Negative: a genuinely mismatched literal against the declared `@type`
/// must still report at the read site, not be silently accepted.
#[test]
fn jsdoc_type_declared_module_exports_mismatch_still_reports() {
    let source = concat!(
        "/** @typedef {{ color: \"red\" | \"blue\" }} MyComponentProps */\n",
        "/**\n * @param {{ props: MyComponentProps }} p\n */\n",
        "function expectLiteral(p) {}\n",
        "/**\n * @type {MyComponentProps}\n */\n",
        "module.exports = {\n    color: \"red\"\n}\n",
        "expectLiteral({ props: { color: \"green\" } });\n",
    );
    let diags = js_diags(source);
    assert!(
        diags.iter().any(|(code, _)| *code == TYPE_MISMATCH),
        "expected TS2322 for the mismatched literal, got: {diags:?}"
    );
}

/// Regression guard: without a `@type` tag, `module.exports` keeps its prior
/// structurally-inferred (and display-widened) shape.
#[test]
fn module_exports_without_jsdoc_type_keeps_structural_inference() {
    let source = concat!(
        "module.exports = {\n    color: \"red\"\n}\n",
        "/**\n * @type {{ color: \"blue\" }}\n */\n",
        "var x = module.exports;\n",
    );
    let diags = js_diags(source);
    assert!(
        diags.iter().any(|(code, _)| *code == TYPE_MISMATCH),
        "expected TS2322: without an explicit @type the export keeps its own \
         literal shape ({{ color: \"red\" }}), which does not match \"blue\", \
         got: {diags:?}"
    );
}

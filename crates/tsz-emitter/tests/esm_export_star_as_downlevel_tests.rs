//! Regression tests for downleveling `export * as ns from "mod"` under
//! `--module es2015`.
//!
//! `export * as ns from "mod"` (a namespace re-export) is an ES2020 feature.
//! When the module output target predates it (`module: es2015`), tsc rewrites
//! the statement into a namespace import plus a re-export:
//!
//! ```js
//! import * as ns_1 from "mod";
//! export { ns_1 as ns };
//! ```
//!
//! The generated import binding is named after the export name (an identifier
//! clause), matching tsc's `getGeneratedNameForNode` (`export * as ns` ->
//! `ns_1`). tsz previously only downleveled the rare string-literal export-name
//! form (`export * as "str" from "mod"`) and emitted the identifier form
//! verbatim, producing output that is invalid for an ES2015 module target.
//!
//! For `module: es2020` and later (which support the syntax natively) the
//! statement is preserved unchanged.
//!
//! Source: `crates/tsz-emitter/src/emitter/module_emission/core/mod.rs`
//! (`emit_export_declaration_es6` — the `export * as` arm).

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print as parse_lower_emit;

fn es2015_opts() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::ES2015,
        ..Default::default()
    }
}

fn esnext_opts() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES2020,
        module: ModuleKind::ESNext,
        ..Default::default()
    }
}

#[test]
fn es2015_downlevels_identifier_namespace_reexport() {
    // Vary the binder names to keep the assertion structural (anti-hardcoding).
    let output = parse_lower_emit("export * as widgets from \"./widgets\";\n", es2015_opts());

    assert!(
        output.contains("import * as widgets_1 from \"./widgets\";"),
        "identifier namespace re-export should downlevel to a namespace import \
         named after the export (`widgets_1`).\nOutput:\n{output}"
    );
    assert!(
        output.contains("export { widgets_1 as widgets };"),
        "downleveled re-export should bind the generated import back to the \
         export name.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("export * as"),
        "the ES2020 `export * as` form must not survive for an ES2015 module \
         target.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_downlevel_uses_a_distinct_export_name_base() {
    // A different export name derives a different generated binding base.
    let output = parse_lower_emit("export * as gadgets from \"gadgets-lib\";\n", es2015_opts());

    assert!(
        output.contains("import * as gadgets_1 from \"gadgets-lib\";"),
        "the generated binding base tracks the export name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("export { gadgets_1 as gadgets };"),
        "re-export binds the generated import to the export name.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_downlevel_avoids_colliding_with_a_local_binding() {
    // A local `helpers_1` forces the generated name to skip to `helpers_2`,
    // exactly as tsc's unique-name generator does.
    let source =
        "const helpers_1 = 0;\nexport { helpers_1 };\nexport * as helpers from \"./helpers\";\n";
    let output = parse_lower_emit(source, es2015_opts());

    assert!(
        output.contains("import * as helpers_2 from \"./helpers\";"),
        "the generated binding must not collide with an existing `helpers_1`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("export { helpers_2 as helpers };"),
        "the re-export uses the collision-free generated name.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_still_downlevels_string_literal_namespace_reexport() {
    // The pre-existing string-literal export-name form keeps downleveling.
    let output = parse_lower_emit("export * as \"str name\" from \"./m\";\n", es2015_opts());

    assert!(
        output.contains("import * as ") && output.contains("from \"./m\";"),
        "string-literal namespace re-export still downlevels to a namespace \
         import.\nOutput:\n{output}"
    );
    assert!(
        output.contains("as \"str name\" }"),
        "the string export name is preserved in the re-export.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("export * as"),
        "no `export * as` form survives for an ES2015 module target.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_preserves_plain_star_reexport() {
    // `export * from "mod"` (no `as`) is valid ES2015 and must be preserved.
    let output = parse_lower_emit("export * from \"./barrel\";\n", es2015_opts());

    assert!(
        output.contains("export * from \"./barrel\";"),
        "a plain star re-export is unchanged for an ES2015 module target.\nOutput:\n{output}"
    );
}

#[test]
fn esnext_preserves_identifier_namespace_reexport() {
    // ES2020+ module targets support `export * as ns` natively.
    let output = parse_lower_emit("export * as tools from \"./tools\";\n", esnext_opts());

    assert!(
        output.contains("export * as tools from \"./tools\";"),
        "an ES2020+ module target preserves the native `export * as` form.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("import * as"),
        "no downlevel import should be synthesized for a native target.\nOutput:\n{output}"
    );
}

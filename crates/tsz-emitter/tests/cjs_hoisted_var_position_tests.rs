//! Regression tests for CJS module hoisted-var insertion position.
//!
//! In CommonJS module mode, tsc emits hoisted temp `var` declarations
//! (e.g. `var _Foo_field;` for class private field `WeakMaps`, `var _a;` for
//! assignment destructuring) BEFORE the
//! `Object.defineProperty(exports, "__esModule", { value: true })` preamble
//! and any `exports.X = void 0;` initializations. tsz now routes these
//! `hoisted_assignment_temps` (and friends) through the pre-preamble
//! insertion offset (`cjs_destr_hoist_byte_offset`) when CJS mode is
//! active.
//!
//! Source: `crates/tsz-emitter/src/emitter/source_file/emit.rs`
//! (the hoisted-vars insertion block at the bottom of the source-file
//! emitter).

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn parse_lower_print(source: &str, opts: PrintOptions) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    lower_and_print(&parser.arena, root, opts).code
}

/// `var _Foo_field;` for a class private field in CJS module mode must come
/// BEFORE `Object.defineProperty(exports, "__esModule", ...)`. tsc places
/// all hoisted class-lowering vars at the very top of the file body, after
/// `"use strict";` and helpers, but before any CJS preamble.
#[test]
fn cjs_class_private_field_var_before_es_module_preamble() {
    let source = r#"export class Foo {
    #field = true;
    f() {
        this.#field = this.#field;
        #field in this;
    }
}
"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2020,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    let var_pos = output
        .find("var _Foo_field;")
        .expect("expected `var _Foo_field;` declaration; output:\n{output}");
    let preamble_pos = output
        .find("Object.defineProperty(exports, \"__esModule\"")
        .expect("expected CJS __esModule preamble; output:\n{output}");

    assert!(
        var_pos < preamble_pos,
        "private-field WeakMap var must be hoisted BEFORE the __esModule \
         preamble in CJS mode.\nvar pos: {var_pos}, preamble pos: \
         {preamble_pos}\noutput:\n{output}"
    );
}

/// Assignment destructuring temp `var _a;` from
/// `export const [] = [];` must be hoisted BEFORE the CJS preamble. This
/// path was already correct (it goes through `cjs_destructuring_export_temps`)
/// but the test pins the contract so it cannot regress alongside the
/// `hoisted_assignment_temps` reroute.
#[test]
fn cjs_destructuring_export_temp_before_es_module_preamble() {
    let source = "export const [] = [];\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    let var_pos = output
        .find("var _a;")
        .expect("expected `var _a;` declaration; output:\n{output}");
    let preamble_pos = output
        .find("Object.defineProperty(exports, \"__esModule\"")
        .expect("expected CJS __esModule preamble; output:\n{output}");

    assert!(
        var_pos < preamble_pos,
        "destructuring temp must be hoisted BEFORE the __esModule preamble \
         in CJS mode.\nvar pos: {var_pos}, preamble pos: {preamble_pos}\n\
         output:\n{output}"
    );
}

/// In NON-CJS module mode (ES2015 modules), there is no `__esModule`
/// preamble. The hoisted private-field var simply sits at the top of the
/// emitted body after `"use strict";` (if any) and helpers. This test
/// asserts that we did not break the non-CJS path: the var must still be
/// emitted somewhere in the output (i.e. routing did not silently drop it).
#[test]
fn esm_class_private_field_var_still_emitted() {
    let source = r#"export class Foo {
    #field = true;
    f() {
        this.#field = this.#field;
        #field in this;
    }
}
"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2020,
            module: ModuleKind::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var _Foo_field;"),
        "non-CJS lowering must still emit the private-field WeakMap var; \
         output:\n{output}"
    );
    assert!(
        !output.contains("Object.defineProperty(exports, \"__esModule\""),
        "ES2015 module mode must not emit a CJS __esModule preamble; \
        output:\n{output}"
    );
}

#[test]
fn cjs_renamed_export_of_declare_const_has_no_runtime_assignment() {
    let source = "declare const _await: any;\nexport { _await as await };\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    assert!(
        output.contains("exports.await = void 0;"),
        "renamed ambient export should keep the CJS export preamble; output:\n{output}"
    );
    assert!(
        !output.contains("exports.await = _await;"),
        "renamed ambient export must not emit a runtime assignment; output:\n{output}"
    );
}

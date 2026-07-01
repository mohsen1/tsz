//! Regression tests for the emission order of an ESM named export relative to a
//! class's deferred private/accessor `WeakMap` storage initialization at
//! `--target es5`.
//!
//! Structural rule: when a top-level `export class C { ... }` is lowered to an
//! ES5 IIFE and defers its private-field / auto-accessor `WeakMap` storage
//! instantiation to module scope, `tsc` emits the re-export statement
//! `export { C };` immediately after the class IIFE and *before* the
//! `_C_x = new WeakMap();` storage init — the same position the CommonJS path
//! uses for `exports.C = C;`. `export default C;` is the deliberate exception:
//! `tsc` emits it *after* the storage init.
//!
//! The assertions key on the relative order of the structural artifacts
//! (`export { ... };` vs `= new WeakMap()`), not on a fixed fixture string, and
//! each shape is exercised under distinct class/member names so the ordering is
//! proven to key on the lowering structure rather than a user-chosen identifier.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::context::emit::EmitContext;
use tsz_emitter::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_emitter::lowering::LoweringPass;
use tsz_parser::parser::ParserState;

fn parse_lower_emit(source: &str, opts: PrinterOptions) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

fn es5_module(module: ModuleKind) -> PrinterOptions {
    PrinterOptions {
        target: ScriptTarget::ES5,
        module,
        ..Default::default()
    }
}

/// Byte offset of the first occurrence of `needle`, or a panic with the full
/// emitted output for a readable failure.
fn index_of(haystack: &str, needle: &str) -> usize {
    haystack
        .find(needle)
        .unwrap_or_else(|| panic!("expected to find {needle:?} in emitted output:\n{haystack}"))
}

/// An ESM-exported ES5-lowered class with a private instance field emits
/// `export { C };` before the deferred `_C_x = new WeakMap();` storage init.
#[test]
fn named_export_precedes_private_field_weakmap_init_es5_esm() {
    for module in [ModuleKind::ESNext, ModuleKind::ES2020, ModuleKind::ES2015] {
        let source = r#"
export class Repository {
    #store = 1;
    read() { return this.#store; }
}
"#;
        let output = parse_lower_emit(source, es5_module(module));
        let export_pos = index_of(&output, "export { Repository };");
        let init_pos = index_of(&output, "= new WeakMap()");
        assert!(
            export_pos < init_pos,
            "export must precede WeakMap storage init (module={module:?}):\n{output}"
        );
    }
}

/// The same ordering holds for an auto-accessor (`accessor v = ...`) whose
/// storage `WeakMap` is likewise deferred to module scope. A distinct class /
/// member name proves the rule keys on structure, not identifier.
#[test]
fn named_export_precedes_accessor_storage_init_es5_esm() {
    let source = r#"
export class Cell {
    accessor payload = 0;
}
"#;
    let output = parse_lower_emit(source, es5_module(ModuleKind::ESNext));
    let export_pos = index_of(&output, "export { Cell };");
    let init_pos = index_of(&output, "= new WeakMap()");
    assert!(
        export_pos < init_pos,
        "export must precede accessor storage init:\n{output}"
    );
}

/// Two exported ES5-lowered classes each emit their own re-export before their
/// own storage init, in source order.
#[test]
fn multiple_named_exports_each_precede_their_storage_init_es5_esm() {
    let source = r#"
export class Alpha { #a = 1; }
export class Beta { accessor b = 2; }
"#;
    let output = parse_lower_emit(source, es5_module(ModuleKind::ESNext));
    let alpha_export = index_of(&output, "export { Alpha };");
    let beta_export = index_of(&output, "export { Beta };");
    let alpha_init = index_of(&output, "_Alpha_a = new WeakMap()");
    let beta_init = index_of(&output, "_Beta_b_accessor_storage = new WeakMap()");
    assert!(
        alpha_export < alpha_init,
        "Alpha export must precede its storage init:\n{output}"
    );
    assert!(
        beta_export < beta_init,
        "Beta export must precede its storage init:\n{output}"
    );
    assert!(
        alpha_export < beta_export,
        "exports must be emitted in source order:\n{output}"
    );
}

/// `export default class` is the exception: `tsc` emits `export default C;`
/// *after* the deferred storage init, so the default path must keep that order.
#[test]
fn default_export_follows_private_field_storage_init_es5_esm() {
    let source = r#"
export default class Handler {
    #state = 1;
    read() { return this.#state; }
}
"#;
    let output = parse_lower_emit(source, es5_module(ModuleKind::ESNext));
    let init_pos = index_of(&output, "= new WeakMap()");
    let export_pos = index_of(&output, "export default Handler;");
    assert!(
        init_pos < export_pos,
        "default export must follow the WeakMap storage init:\n{output}"
    );
}

/// The CommonJS analogue is unaffected: `exports.C = C;` still precedes the
/// storage init (no `export { ... }` ESM statement is emitted).
#[test]
fn commonjs_export_precedes_storage_init_and_emits_no_esm_export_es5() {
    let source = r#"
export class Service {
    #dep = 1;
    read() { return this.#dep; }
}
"#;
    let output = parse_lower_emit(source, es5_module(ModuleKind::CommonJS));
    assert!(
        !output.contains("export { Service };"),
        "CommonJS output must not emit an ESM re-export:\n{output}"
    );
    let export_pos = index_of(&output, "exports.Service = Service;");
    let init_pos = index_of(&output, "= new WeakMap()");
    assert!(
        export_pos < init_pos,
        "CommonJS export must precede the WeakMap storage init:\n{output}"
    );
}

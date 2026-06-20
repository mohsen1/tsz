//! ES5 parameter object-binding destructuring temp-ordering tests.
//!
//! Structural rule: when a function-parameter object-binding element has both a
//! default initializer and a *nested binding pattern* target, `tsc` materializes
//! a fresh temp for the default-checked value before destructuring the nested
//! pattern (mirroring `ensureIdentifier` after `createDefaultValueCheck`):
//!
//! ```js
//! function f(_a) { var _b = _a.outer, _c = _b === void 0 ? init : _b, leaf = _c.leaf; }
//! ```
//!
//! It does NOT reuse the source temp (`_b = _a.outer, _b = _b === void 0 ? init : _b,
//! leaf = _b.leaf`). This mirrors the sibling array-pattern path and the
//! variable-statement path. Binder names are varied so the assertions track the
//! structural shape, not any spelling.

use crate::context::emit::EmitContext;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_es5(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn nested_object_pattern_default_uses_fresh_temp() {
    let source = "function single({ outer: { leaf } = { leaf: 0 } }: any) {}\n";
    let output = emit_es5(source);

    assert!(
        output
            .contains("var _b = _a.outer, _c = _b === void 0 ? { leaf: 0 } : _b, leaf = _c.leaf;"),
        "Nested object-pattern default should destructure from a fresh temp.\nOutput:\n{output}"
    );
    // Guard against the old reused-temp shape.
    assert!(
        !output.contains("_b = _b === void 0"),
        "The source temp must not be reused for the default-checked value.\nOutput:\n{output}"
    );
}

#[test]
fn renamed_nested_object_pattern_default_uses_fresh_temp() {
    let source = "function renamed({ src: { v: w } = { v: 0 } }: any) {}\n";
    let output = emit_es5(source);

    assert!(
        output.contains("var _b = _a.src, _c = _b === void 0 ? { v: 0 } : _b, w = _c.v;"),
        "Renamed nested binding tracks the structural shape, not the name.\nOutput:\n{output}"
    );
}

#[test]
fn multiple_nested_pattern_defaults_advance_temps_sequentially() {
    let source = "function pair({ a: { x } = { x: 1 }, b: { y } = { y: 2 } }: any) {}\n";
    let output = emit_es5(source);

    assert!(
        output.contains(
            "var _b = _a.a, _c = _b === void 0 ? { x: 1 } : _b, x = _c.x, \
             _d = _a.b, _e = _d === void 0 ? { y: 2 } : _d, y = _e.y;"
        ),
        "Each nested default should claim its own source/default temp pair.\nOutput:\n{output}"
    );
}

#[test]
fn nested_array_pattern_default_uses_fresh_temp() {
    let source = "function arr({ p: [q] = [7] }: any) {}\n";
    let output = emit_es5(source);

    assert!(
        output.contains("var _b = _a.p, _c = _b === void 0 ? [7] : _b, q = _c[0];"),
        "Nested array-pattern default mirrors the object-pattern path.\nOutput:\n{output}"
    );
}

#[test]
fn nested_default_before_object_rest_uses_fresh_temp() {
    let source = "function withRest({ k, m: { z } = { z: 1 }, ...others }: any) {}\n";
    let output = emit_es5(source);

    assert!(
        output.contains(
            "var k = _a.k, _b = _a.m, _c = _b === void 0 ? { z: 1 } : _b, \
             z = _c.z, others = __rest(_a, [\"k\", \"m\"]);"
        ),
        "A nested default ahead of an object rest still uses a fresh temp.\nOutput:\n{output}"
    );
}

#[test]
fn identifier_target_default_reuses_source_temp() {
    // Negative control: when the binding target is a plain identifier (no nested
    // pattern to destructure), tsc reuses the source temp for the default check
    // (`_b = _a.p, p = _b === void 0 ? 5 : _b`). This behavior is unchanged.
    let source = "function identCtrl({ p = 5 }: any) {}\n";
    let output = emit_es5(source);

    assert!(
        output.contains("var _b = _a.p, p = _b === void 0 ? 5 : _b;"),
        "Identifier-target defaults reuse the source temp (unchanged).\nOutput:\n{output}"
    );
}

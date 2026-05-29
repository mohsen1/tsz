//! Regression tests: a class declared in a System module's top-level `using`
//! region must reuse its hoisted `var` binding, not get a synthetic rename.
//!
//! When a System (or other wrapper) module has a top-level `using`/`await using`
//! declaration, the execute closure emits its statements inside a `try { … }`
//! region while the class/var bindings are hoisted into the closure's
//! `var …` list. The class body is lowered to ES5 as
//! `Name = /** @class */ (function () { … }())` and must assign to the
//! hoisted `Name`. tsz previously synthesized a `Name_1` local because the
//! block-scoping pass treated the empty (wrapper-level) scope stack as if it
//! were a nested block, producing `var Name_1 = …` that diverged from tsc and
//! left the hoisted `Name` undefined.
//!
//! Structural rule: when a class declaration is registered for block-scoping
//! with no enclosing block scope at all (module/script top level inside a
//! wrapper), it reuses its own name instead of receiving a synthetic binding.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::context::emit::EmitContext;
use tsz_emitter::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_emitter::lowering::LoweringPass;

#[path = "test_support.rs"]
mod test_support;

fn parse_lower_emit(source: &str, opts: PrinterOptions) -> String {
    let (parser, root) = test_support::parse_source(source);
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

fn system_es5_opts(legacy_decorators: bool) -> PrinterOptions {
    PrinterOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::System,
        no_emit_helpers: true,
        legacy_decorators,
        ..Default::default()
    }
}

#[test]
fn plain_class_in_top_level_using_region_reuses_hoisted_name() {
    let source = "export {};\nusing before = null;\nclass C {\n}\n";
    let output = parse_lower_emit(source, system_es5_opts(false));
    assert!(
        output.contains("C = /** @class */ (function () {"),
        "class must assign to the hoisted `C`.\n{output}"
    );
    assert!(
        !output.contains("C_1"),
        "class must not be renamed to `C_1`.\n{output}"
    );
}

#[test]
fn plain_class_in_top_level_using_region_reuses_hoisted_name_renamed() {
    // Same rule, different identifier — must not be keyed on the spelling `C`.
    let source = "export {};\nusing guard = null;\nclass Widget {\n}\n";
    let output = parse_lower_emit(source, system_es5_opts(false));
    assert!(
        output.contains("Widget = /** @class */ (function () {"),
        "class must assign to the hoisted `Widget`.\n{output}"
    );
    assert!(
        !output.contains("Widget_1"),
        "class must not be renamed to `Widget_1`.\n{output}"
    );
}

#[test]
fn legacy_decorated_class_in_top_level_using_region_reuses_hoisted_name() {
    let source = "export {};\ndeclare var dec: any;\nusing before = null;\n@dec\nclass C {\n}\n";
    let output = parse_lower_emit(source, system_es5_opts(true));
    assert!(
        output.contains("C = /** @class */ (function () {"),
        "decorated class must assign to the hoisted `C`.\n{output}"
    );
    assert!(
        !output.contains("var C_1"),
        "decorated class must not be renamed to `C_1`.\n{output}"
    );
    // The legacy `__decorate` call stays inside the IIFE (before `return C`).
    assert!(
        output.contains("C = __decorate(["),
        "legacy decorate must be applied to `C`.\n{output}"
    );
}

#[test]
fn legacy_decorated_class_in_top_level_using_region_reuses_hoisted_name_renamed() {
    let source =
        "export {};\ndeclare var deco: any;\nusing res = null;\n@deco\nclass Service {\n}\n";
    let output = parse_lower_emit(source, system_es5_opts(true));
    assert!(
        output.contains("Service = /** @class */ (function () {"),
        "decorated class must assign to the hoisted `Service`.\n{output}"
    );
    assert!(
        !output.contains("Service_1"),
        "decorated class must not be renamed to `Service_1`.\n{output}"
    );
}

#[test]
fn exported_decorated_class_in_top_level_using_region_exports_hoisted_name() {
    // `export class C` inside the using region: the export call wraps the
    // hoisted-name assignment, not a synthetic local.
    let source =
        "export {};\ndeclare var dec: any;\nusing before = null;\n@dec\nexport class C {\n}\n";
    let output = parse_lower_emit(source, system_es5_opts(true));
    assert!(
        output.contains("exports_1(\"C\", C = /** @class */ (function () {"),
        "exported decorated class must export the hoisted-name assignment.\n{output}"
    );
    assert!(
        !output.contains("C_1"),
        "exported decorated class must not be renamed.\n{output}"
    );
}

#[test]
fn exported_decorated_self_ref_class_in_using_region_uses_outer_alias() {
    // A class that references itself in value position (e.g. `new C()`) requires
    // the outer-alias pattern.  tsc emits two separate exports_1 calls:
    //   exports_1("C", C = C_1 = /** @class */ (function () { … C_1 = C; … }()));
    //   exports_1("C", C = C_1 = __decorate([dec], C));
    // The inner `C_1 = C;` captures the alias so self-references inside the IIFE
    // body resolve correctly once the outer `C_1` is reassigned by `__decorate`.
    let source = "export {};\ndeclare var dec: any;\nusing before = null;\n@dec\nexport class C {\n    method() { return new C(); }\n}\n";
    let output = parse_lower_emit(source, system_es5_opts(true));
    // Must use the outer-alias variable (C_1) hoisted alongside C.
    assert!(
        output.contains("var before, C_1, C,") || output.contains("var before, C_1, C"),
        "C_1 must be hoisted as a module-level var.\n{output}"
    );
    // First export_1 call: C = C_1 = IIFE
    assert!(
        output.contains("exports_1(\"C\", C = C_1 = /** @class */ (function () {"),
        "first exports_1 must assign the IIFE to both C and C_1.\n{output}"
    );
    // Methods inside IIFE use C_1 for self-references.
    assert!(
        output.contains("return new C_1()"),
        "method self-reference must use alias C_1.\n{output}"
    );
    // Second export_1 call: re-export after __decorate.
    assert!(
        output.contains("exports_1(\"C\", C = C_1 = __decorate(["),
        "second exports_1 must reassign both C and C_1 after __decorate.\n{output}"
    );
    // Must not produce a C_2 rename.
    assert!(
        !output.contains("C_2"),
        "must not produce a synthetic C_2 rename.\n{output}"
    );
}

#[test]
fn class_before_top_level_using_reuses_hoisted_name() {
    // The class precedes the `using` declaration but is still emitted through
    // the top-level-using region; it must reuse `C` too.
    let source = "export {};\ndeclare var dec: any;\n@dec\nclass C {\n}\nexport { C };\nusing after = null;\n";
    let output = parse_lower_emit(source, system_es5_opts(true));
    assert!(
        output.contains("C = /** @class */ (function () {"),
        "class before the using must assign to the hoisted `C`.\n{output}"
    );
    assert!(
        output.contains("exports_1(\"C\", C)"),
        "the export call must reference the hoisted `C`.\n{output}"
    );
    assert!(
        !output.contains("C_1"),
        "class before the using must not be renamed.\n{output}"
    );
}

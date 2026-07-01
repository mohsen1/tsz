//! ESM re-export ordering for ES5-lowered `export class` declarations.
//!
//! At `--target es5` with an ESM module output, a top-level `export class`
//! that lowers a private field or auto-accessor to `WeakMap` storage must emit
//! the `export { C };` re-export immediately after the class IIFE and *before*
//! the deferred storage instantiation (`_C_x = new WeakMap();`) — the same slot
//! the CommonJS `exports.C = C;` assignment uses. `export default class` is the
//! deliberate exception: `tsc` emits `export default C;` *after* the storage
//! init.

use crate::emitter::ModuleKind;
use tsz_common::ScriptTarget;

use super::emit_with_module_and_target;

/// Index of the `export { <name> };` re-export in the output.
fn named_export_pos(output: &str, name: &str) -> usize {
    let needle = format!("export {{ {name} }};");
    output
        .find(&needle)
        .unwrap_or_else(|| panic!("expected `{needle}` in output.\nOutput:\n{output}"))
}

/// Index of the deferred `<storage> = new WeakMap();` init in the output.
fn weakmap_init_pos(output: &str) -> usize {
    output
        .find("= new WeakMap();")
        .unwrap_or_else(|| panic!("expected a deferred WeakMap init in output.\nOutput:\n{output}"))
}

const ESM_MODULES: [ModuleKind; 3] = [ModuleKind::ES2015, ModuleKind::ES2020, ModuleKind::ESNext];

#[test]
fn named_export_of_private_field_class_precedes_weakmap_init() {
    // Binder name varied from the issue's `P` to guard against name-hardcoding.
    let source = "export class Widget { #x = 1; getX() { return this.#x; } }\n";
    for module in ESM_MODULES {
        let output = emit_with_module_and_target(source, module, ScriptTarget::ES5);
        assert!(
            named_export_pos(&output, "Widget") < weakmap_init_pos(&output),
            "`export {{ Widget }};` must precede the WeakMap init for module {module:?}.\nOutput:\n{output}"
        );
    }
}

#[test]
fn named_export_of_auto_accessor_class_precedes_weakmap_init() {
    let source = "export class Cell { accessor value = 1; }\n";
    for module in ESM_MODULES {
        let output = emit_with_module_and_target(source, module, ScriptTarget::ES5);
        assert!(
            named_export_pos(&output, "Cell") < weakmap_init_pos(&output),
            "`export {{ Cell }};` must precede the accessor WeakMap init for module {module:?}.\nOutput:\n{output}"
        );
    }
}

#[test]
fn each_class_reexport_is_colocated_with_its_own_iife() {
    // Two exported private-field classes plus a plain named re-export. Each
    // class's `export { C };` sits with its own IIFE, before its storage init;
    // the non-class `export { local };` stays at the module trailer.
    let source = "export class First { #a = 1; }\n\
         export class Second { #b = 2; }\n\
         const local = 3;\n\
         export { local };\n";
    let output = emit_with_module_and_target(source, ModuleKind::ESNext, ScriptTarget::ES5);

    let first_export = named_export_pos(&output, "First");
    let second_export = named_export_pos(&output, "Second");
    let first_init = output
        .find("_First_a = new WeakMap();")
        .expect("First storage init");
    let second_init = output
        .find("_Second_b = new WeakMap();")
        .expect("Second storage init");

    assert!(
        first_export < first_init,
        "`export {{ First }};` must precede its WeakMap init.\nOutput:\n{output}"
    );
    assert!(
        second_export < second_init,
        "`export {{ Second }};` must precede its WeakMap init.\nOutput:\n{output}"
    );
    // The two classes stay in source order.
    assert!(
        first_export < second_export,
        "class re-exports must stay in source order.\nOutput:\n{output}"
    );
    // The non-class local re-export is still present (at the trailer).
    assert!(
        output.contains("export { local };"),
        "plain non-class re-export must remain.\nOutput:\n{output}"
    );
}

#[test]
fn default_export_class_stays_after_weakmap_init() {
    // `export default class` is the deliberate exception: the re-export follows
    // the storage init (tsz already matched tsc here; guard against regression).
    let source = "export default class Box { #x = 1; }\n";
    for module in ESM_MODULES {
        let output = emit_with_module_and_target(source, module, ScriptTarget::ES5);
        let default_export = output.find("export default Box;").unwrap_or_else(|| {
            panic!("expected `export default Box;` for module {module:?}.\nOutput:\n{output}")
        });
        assert!(
            default_export > weakmap_init_pos(&output),
            "`export default Box;` must follow the WeakMap init for module {module:?}.\nOutput:\n{output}"
        );
    }
}

#[test]
fn static_private_field_class_has_no_deferred_init_and_reexports_inline() {
    // A `static #s` private field keeps its storage inside the class IIFE, so
    // there is no deferred module-scope init and no ordering divergence — the
    // named re-export is still emitted.
    let source = "export class Registry { static #s = 1; }\n";
    let output = emit_with_module_and_target(source, ModuleKind::ESNext, ScriptTarget::ES5);
    assert!(
        output.contains("export { Registry };"),
        "static-private-field class must still be re-exported.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("= new WeakMap();\nexport { Registry };"),
        "static-private storage stays inside the IIFE; no trailing init before the export.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_named_class_export_is_unaffected() {
    // The ESM staging must not perturb the CommonJS path, which keeps its
    // `exports.C = C;` assignment before the storage init.
    let source = "export class Widget { #x = 1; }\n";
    let output = emit_with_module_and_target(source, ModuleKind::CommonJS, ScriptTarget::ES5);
    let cjs_export = output
        .find("exports.Widget = Widget;")
        .expect("CommonJS export assignment");
    assert!(
        cjs_export < weakmap_init_pos(&output),
        "CommonJS `exports.Widget = Widget;` must precede the WeakMap init.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("export { Widget };"),
        "CommonJS output must not emit an ESM `export {{ }}` form.\nOutput:\n{output}"
    );
}

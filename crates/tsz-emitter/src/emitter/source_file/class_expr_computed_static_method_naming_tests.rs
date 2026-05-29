//! Structural emit tests for the named-evaluation rule on anonymous class
//! expressions that carry a computed-named *static method* (or accessor).
//!
//! Structural rule: a computed static method/accessor name is emitted inline
//! in the class body and carries no post-construction static state, so it only
//! forces the `(_tmp = class {...}, __setFunctionName(_tmp, "X"), _tmp)`
//! wrapping when the binding *also* loses JS named evaluation -- i.e. a
//! `using`/`await using` declaration lowered to `__addDisposableResource`,
//! which moves the class out of direct-assignment position. A plain
//! `var X = class {...}` keeps named evaluation and needs no helper.

use crate::context::emit::EmitContext;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit(source: &str, target: ScriptTarget, use_define_for_class_fields: bool) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target,
        use_define_for_class_fields,
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

// --- Reported repro: plain `var X = class { static [expr]() {} }` ---
// Named evaluation holds, so no `__setFunctionName`, no temp-comma wrapping.

#[test]
fn var_bound_anonymous_class_computed_static_method_keeps_named_evaluation_es2015() {
    let source = "const KEYS = { dispose: 'd' } as const;\nvar X = class {\n    static [KEYS.dispose]() {}\n};\n";
    let output = emit(source, ScriptTarget::ES2015, false);

    assert!(
        !output.contains("__setFunctionName"),
        "Plain `var X = class {{ static [expr]() {{}} }}` keeps JS named evaluation; \
         it must NOT emit __setFunctionName.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var X = class {"),
        "The anonymous class must stay in direct-assignment position (no temp wrap).\nOutput:\n{output}"
    );
    assert!(
        !output.contains("(_a = class"),
        "No temp-comma wrapping should be introduced for an inline computed static method.\nOutput:\n{output}"
    );
}

// Renamed binding + renamed computed-name source: proves the rule is keyed on
// structure (computed static method), not on the identifier `X`/`KEYS`/`dispose`.
#[test]
fn renamed_var_bound_anonymous_class_computed_static_method_keeps_named_evaluation_es2015() {
    let source = "const NAMES = { tag: 't' } as const;\nvar Widget = class {\n    static [NAMES.tag]() {}\n};\n";
    let output = emit(source, ScriptTarget::ES2015, false);

    assert!(
        !output.contains("__setFunctionName"),
        "Renaming the binding/key must not change the rule: still no __setFunctionName.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var Widget = class {"),
        "Renamed anonymous class binding must stay in direct-assignment position.\nOutput:\n{output}"
    );
}

// Same rule under ES5 lowering and under useDefineForClassFields=true: the
// inline computed static method still must not trigger __setFunctionName.
#[test]
fn var_bound_anonymous_class_computed_static_method_keeps_named_evaluation_es5() {
    let source = "const KEYS = { dispose: 'd' } as const;\nvar X = class {\n    static [KEYS.dispose]() {}\n};\n";
    let output = emit(source, ScriptTarget::ES5, false);

    assert!(
        !output.contains("__setFunctionName"),
        "ES5 lowering of a var-bound anonymous class with an inline computed static \
         method must not emit __setFunctionName.\nOutput:\n{output}"
    );
}

#[test]
fn var_bound_anonymous_class_computed_static_method_keeps_named_evaluation_define_fields() {
    let source = "const KEYS = { dispose: 'd' } as const;\nvar X = class {\n    static [KEYS.dispose]() {}\n};\n";
    let output = emit(source, ScriptTarget::ES2015, true);

    assert!(
        !output.contains("__setFunctionName"),
        "useDefineForClassFields=true must not change the inline-method rule.\nOutput:\n{output}"
    );
}

// --- `using` binding loses named evaluation ---
// The class is lowered into `__addDisposableResource(env, <class>, ...)`, so
// tsc captures it in a temp and assigns the name with __setFunctionName.

#[test]
fn using_bound_anonymous_class_computed_static_method_emits_set_function_name() {
    let source = "using C1 = class {\n    static [Symbol.dispose]() {}\n};\n";
    let output = emit(source, ScriptTarget::ES2018, false);

    assert!(
        output.contains("__setFunctionName"),
        "A `using` binding moves the anonymous class out of direct-assignment position, \
         so the inline computed static method requires __setFunctionName.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__addDisposableResource"),
        "`using` should lower through __addDisposableResource.\nOutput:\n{output}"
    );
}

// Renamed `using` binding + different built-in well-known key: still wraps.
#[test]
fn renamed_using_bound_anonymous_class_computed_static_method_emits_set_function_name() {
    let source = "using disposable = class {\n    static [Symbol.asyncDispose]() {}\n};\n";
    let output = emit(source, ScriptTarget::ES2018, false);

    assert!(
        output.contains("__setFunctionName"),
        "Renaming the `using` binding/key must not change the rule: still __setFunctionName.\nOutput:\n{output}"
    );
}

// --- Negative/sibling: computed static *field* still wraps (unchanged) ---
// A computed static field carries post-construction state (an initializer), so
// it has always required the temp-comma wrapping regardless of named
// evaluation. This is the `has_static_field_comma_expr` path, which the
// inline-method rule change does not touch; the test guards against accidental
// regression of that path.

#[test]
fn var_bound_anonymous_class_computed_static_field_still_wraps_es2015() {
    let source =
        "const KEYS = { x: 'x' } as const;\nvar X = class {\n    static [KEYS.x] = 1;\n};\n";
    let output = emit(source, ScriptTarget::ES2015, false);

    // The class must be captured in a temp so the static field initializer can
    // be assigned to the materialized class object.
    assert!(
        output.contains("= class {") && output.contains(" = 1,"),
        "A computed static *field* with an initializer must keep the temp-comma wrapping \
         so the field can be assigned to the materialized class.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var X = class {\n    static"),
        "A runtime computed static field must not be emitted as a plain inline class body.\nOutput:\n{output}"
    );
}

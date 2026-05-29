//! Regression tests for generated private-element helper naming in ES2015+
//! class lowering.
//!
//! Structural rules covered:
//!
//! 1. File-wide uniquing of generated private-helper names. When a nested
//!    class reuses the enclosing class's name, its private-helper variable
//!    (`_<Class>_<field>`) collides with the outer class's helper and must be
//!    uniquified with an `_N` suffix. The uniquing set is accumulated across
//!    every class in the same lexical scope rather than reset per class.
//!
//! 2. Class-expression temp scope inside a private-field initializer. When a
//!    private field's initializer is a class expression that lowers to a
//!    comma expression needing a temp (`_a`), and the enclosing class has no
//!    explicit constructor (so one is synthesized), that temp must be declared
//!    in the synthesized constructor body (`var _a;`) and must not leak to the
//!    enclosing function/file hoist list.
//!
//! Both rules hold regardless of the class name, the private member name, or
//! the iteration/binding variable spelling, so each shape is exercised under
//! at least two distinct name choices. The assertions key on the structural
//! artifact (uniquified suffix, in-constructor `var` declaration), not on a
//! fixed fixture string.

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

fn es2015_esnext() -> PrinterOptions {
    PrinterOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::ESNext,
        ..Default::default()
    }
}

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

// ---------------------------------------------------------------------------
// Rule 1: file-wide uniquing of generated private-helper names.
// ---------------------------------------------------------------------------

/// An inner class that shadows the outer class name has its private-field
/// helper uniquified against the outer class's helper. With the outer `#foo`
/// taking `_A_foo`, the inner (same-named) class's `#foo` must become
/// `_A_foo_1`.
#[test]
fn nested_same_named_class_uniquifies_private_helper() {
    let source = r#"
class A {
    #foo: string;
    constructor() {
        class A {
            #foo: string;
        }
    }
}
"#;
    let output = parse_lower_emit(source, es2015_esnext());

    assert_eq!(
        count_occurrences(&output, "_A_foo_1"),
        // Inner helper appears in: var decl, `.set(this, ...)`, `= new WeakMap()`.
        3,
        "Inner same-named class must uniquify its private helper to _A_foo_1.\nOutput:\n{output}"
    );
    // The plain `_A_foo` (outer) must never be assigned twice as a fresh
    // `new WeakMap()`; the inner one is uniquified.
    assert_eq!(
        count_occurrences(&output, "_A_foo = new WeakMap()"),
        1,
        "Outer helper keeps the un-suffixed name exactly once.\nOutput:\n{output}"
    );
    assert_eq!(
        count_occurrences(&output, "_A_foo_1 = new WeakMap()"),
        1,
        "Inner helper takes the suffixed name exactly once.\nOutput:\n{output}"
    );
}

/// Same rule, different class/member spelling: the uniquing keys on the
/// structural collision, not on the identifiers `A`/`foo`.
#[test]
fn nested_same_named_class_uniquifies_private_helper_renamed() {
    let source = r#"
class Box {
    #data: string;
    constructor() {
        class Box {
            #data: string;
        }
    }
}
"#;
    let output = parse_lower_emit(source, es2015_esnext());

    assert_eq!(
        count_occurrences(&output, "_Box_data_1 = new WeakMap()"),
        1,
        "Inner same-named class must uniquify regardless of names.\nOutput:\n{output}"
    );
    assert_eq!(
        count_occurrences(&output, "_Box_data = new WeakMap()"),
        1,
        "Outer helper keeps the un-suffixed name regardless of names.\nOutput:\n{output}"
    );
}

// ---------------------------------------------------------------------------
// Rule 2: class-expression temp scope inside a private-field initializer of a
// class with a synthesized constructor.
// ---------------------------------------------------------------------------

/// A private field initialized with a class expression that lowers to a comma
/// form needing a temp: the temp is declared inside the synthesized
/// constructor body, not hoisted to module scope.
#[test]
fn class_expression_private_init_temp_stays_in_synthesized_constructor() {
    let source = r#"
class Outer {
    #inner = class {
        static tag = 1;
    };
}
"#;
    let output = parse_lower_emit(source, es2015_esnext());

    // The temp must be declared as `var _a;` inside the constructor body, not
    // appended to the module-level `var _Outer_inner, _a;` line.
    let ctor_pos = output
        .find("constructor()")
        .expect("synthesized constructor should be emitted");
    let after_ctor = &output[ctor_pos..];
    assert!(
        after_ctor.contains("var _a;"),
        "Class-expression temp must be declared inside the synthesized \
         constructor body.\nOutput:\n{output}"
    );
    // It must not leak onto the module-level WeakMap declaration line.
    assert!(
        !output.contains("_Outer_inner, _a"),
        "Class-expression temp must not leak to the module-level hoist \
         list.\nOutput:\n{output}"
    );
}

/// Same temp-scope rule with two private fields each holding a class
/// expression, and different class/member spelling. Both temps stay inside the
/// constructor body.
#[test]
fn class_expression_private_init_temps_renamed_two_fields() {
    let source = r#"
class Container {
    #first = class {
        static a = 1;
    };
    #second = class Named {
        static b = 2;
    };
}
"#;
    let output = parse_lower_emit(source, es2015_esnext());

    let ctor_pos = output
        .find("constructor()")
        .expect("synthesized constructor should be emitted");
    let after_ctor = &output[ctor_pos..];
    assert!(
        after_ctor.contains("var _a, _b;"),
        "Both class-expression temps must be declared inside the synthesized \
         constructor body regardless of names.\nOutput:\n{output}"
    );
    // The module-level declaration should only carry the WeakMap helpers, not
    // the in-constructor temps.
    assert!(
        !output.contains("_Container_first, _Container_second, _a"),
        "Class-expression temps must not leak to module scope regardless of \
         names.\nOutput:\n{output}"
    );
}

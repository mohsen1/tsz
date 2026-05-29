//! Regression tests for class-lowering hoisted temp / post-class init
//! double-emission across hoist buckets and pending-init paths.
//!
//! Structural rule: when a class-lowering hoisted temp declaration or a
//! post-class initialization line (e.g. `_X = new WeakMap()`) is reachable
//! from more than one emission path, each artifact must be declared/emitted
//! exactly once across all hoist buckets and pending-init paths.
//!
//! Two distinct shapes are covered:
//!
//! 1. A private field whose `WeakMap` initializer is reachable from both the
//!    pre-static-elements private-init path and the post-class init path.
//!    The `_X = new WeakMap()` line must appear exactly once.
//!
//! 2. A class alias temp (`var _a;`) that is reserved as a file-level class
//!    temp and is also collected with the class's other hoisted temps. The
//!    `var _a;` declaration must appear exactly once.
//!
//! Both shapes hold regardless of the class name, member name, or how many
//! members exist, so the assertions count occurrences of the structural
//! artifact rather than matching a fixed fixture string. Each shape is run
//! under at least two distinct name choices to prove the dedup keys on the
//! double-emission structure, not on a user-chosen identifier.

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

/// A private field referenced from a static block lowers the field into a
/// `WeakMap`. The `_<Class>_<field> = new WeakMap()` initializer is reachable
/// from both the pre-static private-init path and the later post-class init
/// path; it must be emitted exactly once.
#[test]
fn private_field_weakmap_init_emitted_once_with_static_block() {
    let source = r#"
class Widget {
    #value: number;
    constructor(v: number) { this.#value = v; }
    read() { return this.#value; }
    static {
        helper = { get(o: Widget) { return o.#value; } };
    }
}
let helper: { get(o: Widget): number };
"#;
    let output = parse_lower_emit(source, es2015_esnext());

    let init_count = count_occurrences(&output, "= new WeakMap()");
    assert_eq!(
        init_count, 1,
        "WeakMap initializer must be emitted exactly once across the \
         pre-static and post-class init paths.\nOutput:\n{output}"
    );
    // The storage temp must also be declared exactly once.
    let decl_count = count_occurrences(&output, "var _Widget_value;");
    assert_eq!(
        decl_count, 1,
        "Private field storage temp must be declared exactly once.\n\
         Output:\n{output}"
    );
}

/// Same `WeakMap`-init invariant under different class/member names. The dedup
/// must key on the double-emission structure, not on the spelling of the
/// class or the private field.
#[test]
fn private_field_weakmap_init_emitted_once_under_renamed_members() {
    let source = r#"
class Sprocket {
    #count: number;
    constructor(n: number) { this.#count = n; }
    total() { return this.#count; }
    static {
        peer = { fetch(o: Sprocket) { return o.#count; } };
    }
}
let peer: { fetch(o: Sprocket): number };
"#;
    let output = parse_lower_emit(source, es2015_esnext());

    assert_eq!(
        count_occurrences(&output, "= new WeakMap()"),
        1,
        "WeakMap initializer must be emitted exactly once regardless of \
         class/member names.\nOutput:\n{output}"
    );
    assert_eq!(
        count_occurrences(&output, "var _Sprocket_count;"),
        1,
        "Private field storage temp must be declared exactly once \
         regardless of names.\nOutput:\n{output}"
    );
}

/// A class with a static initializer that references `this` reserves a class
/// alias temp (`var _a;`). That temp is reachable from the file-level class
/// temp bucket; it must be declared exactly once.
#[test]
fn static_this_class_alias_temp_declared_once() {
    let source = r#"
class Gadget {
    static label = this;
}
export {};
"#;
    let output = parse_lower_emit(source, es2015_esnext());

    let alias_decls = count_occurrences(&output, "var _a;");
    assert_eq!(
        alias_decls, 1,
        "Class alias temp `var _a;` must be declared exactly once across \
         hoist buckets.\nOutput:\n{output}"
    );
}

/// Same class-alias invariant with a different class name and an additional
/// static member, proving the dedup is structural rather than fixture-keyed.
#[test]
fn static_this_class_alias_temp_declared_once_renamed() {
    let source = r#"
class Contraption {
    static origin = this;
    static count = 0;
}
export {};
"#;
    let output = parse_lower_emit(source, es2015_esnext());

    assert_eq!(
        count_occurrences(&output, "var _a;"),
        1,
        "Class alias temp must be declared exactly once regardless of the \
         class name or extra static members.\nOutput:\n{output}"
    );
}

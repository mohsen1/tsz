//! Legacy (`experimentalDecorators`) per-member `__decorate` call ordering.
//!
//! Structural rule: when emitting per-member `__decorate` calls for a class,
//! `tsc` emits the calls for every decorated instance/prototype member (in
//! declaration order) before any decorated static member (in declaration
//! order). For a getter/setter pair `tsc` emits a single `__decorate` call
//! whose member decorators come from the first accessor (in declaration order)
//! that has decorators and whose `__param(...)` entries are merged from the
//! setter's decorated parameters.
//!
//! These tests vary identifier spellings (member names, type-parameter-free
//! decorator expressions) so they pin the structural rule, not a spelling, and
//! cover both the ES2015+ direct path and the ES5 IIFE-lowering path.

use tsz_common::ScriptTarget;
use tsz_emitter::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_parser::ParserState;

fn emit_source(source: &str, options: PrinterOptions) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut printer = EmitterPrinter::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

fn legacy_options(target: ScriptTarget) -> PrinterOptions {
    PrinterOptions {
        legacy_decorators: true,
        target,
        ..Default::default()
    }
}

/// Position (byte offset) of the `__decorate` call that targets `member` on the
/// given `receiver` (e.g. `Cls.prototype` or `Cls`). Asserts the call exists.
fn decorate_pos(output: &str, receiver: &str, member: &str) -> usize {
    let needle = format!("], {receiver}, \"{member}\"");
    output.find(&needle).unwrap_or_else(|| {
        panic!(
            "expected a `__decorate` call for `{receiver}` member `{member}`.\nOutput:\n{output}"
        )
    })
}

/// Source whose static members are declared *before* its instance members, so
/// emitting them in declaration order would interleave static and instance.
/// `tsc` reorders so every instance member precedes every static member.
const INTERLEAVED_SOURCE: &str = r#"@((t) => { })
class Container {
    @((t, k, d) => { })
    static alpha() {}

    @((t, k) => { })
    static beta = 1;

    @((t, k, d) => { })
    gamma() {}

    @((t, k) => { })
    delta = 1;
}
"#;

#[test]
fn instance_member_decorators_emit_before_static_es2015() {
    let output = emit_source(INTERLEAVED_SOURCE, legacy_options(ScriptTarget::ES2015));
    let gamma = decorate_pos(&output, "Container.prototype", "gamma");
    let delta = decorate_pos(&output, "Container.prototype", "delta");
    let alpha = decorate_pos(&output, "Container", "alpha");
    let beta = decorate_pos(&output, "Container", "beta");

    // Instance group precedes static group; source order preserved within each.
    assert!(
        gamma < delta,
        "instance source order not preserved:\n{output}"
    );
    assert!(alpha < beta, "static source order not preserved:\n{output}");
    assert!(
        delta < alpha,
        "all instance `__decorate` calls must precede all static ones:\n{output}"
    );
}

#[test]
fn instance_member_decorators_emit_before_static_es5() {
    let output = emit_source(INTERLEAVED_SOURCE, legacy_options(ScriptTarget::ES5));
    let gamma = decorate_pos(&output, "Container.prototype", "gamma");
    let delta = decorate_pos(&output, "Container.prototype", "delta");
    let alpha = decorate_pos(&output, "Container", "alpha");
    let beta = decorate_pos(&output, "Container", "beta");

    assert!(
        gamma < delta,
        "instance source order not preserved:\n{output}"
    );
    assert!(alpha < beta, "static source order not preserved:\n{output}");
    assert!(
        delta < alpha,
        "all instance `__decorate` calls must precede all static ones:\n{output}"
    );
}

/// A getter carrying member decorators paired with a setter carrying parameter
/// decorators. The two must collapse into a single `__decorate` call whose
/// member decorators come first and whose `__param(...)` entries follow.
const ACCESSOR_MERGE_SOURCE: &str = r#"declare function PropDeco(target: Object, key: string): void;
declare function ParamDeco(target: Object, key: string, idx: number): void;
class Holder {
    @PropDeco
    get value() { return 0; }
    set value(@ParamDeco next: number) { }
}
"#;

#[test]
fn accessor_getter_decorator_merges_setter_param_es2015() {
    let output = emit_source(ACCESSOR_MERGE_SOURCE, legacy_options(ScriptTarget::ES2015));
    // Exactly one __decorate call for the pair.
    assert_eq!(
        output.matches("], Holder.prototype, \"value\"").count(),
        1,
        "getter/setter pair must produce a single __decorate call:\n{output}"
    );
    let call = decorate_pos(&output, "Holder.prototype", "value");
    let body = &output[..call];
    let prop = body
        .rfind("PropDeco")
        .expect("missing getter member decorator");
    let param = body
        .rfind("__param(0, ParamDeco)")
        .expect("missing merged setter parameter decorator");
    assert!(
        prop < param,
        "member decorator must precede merged __param entry:\n{output}"
    );
}

#[test]
fn accessor_getter_decorator_merges_setter_param_es5() {
    let output = emit_source(ACCESSOR_MERGE_SOURCE, legacy_options(ScriptTarget::ES5));
    assert_eq!(
        output.matches("], Holder.prototype, \"value\"").count(),
        1,
        "getter/setter pair must produce a single __decorate call:\n{output}"
    );
    let call = decorate_pos(&output, "Holder.prototype", "value");
    let body = &output[..call];
    let prop = body
        .rfind("PropDeco")
        .expect("missing getter member decorator");
    let param = body
        .rfind("__param(0, ParamDeco)")
        .expect("missing merged setter parameter decorator");
    assert!(
        prop < param,
        "member decorator must precede merged __param entry:\n{output}"
    );
}

/// When both accessors of a pair are (invalidly) decorated, `tsc` still emits a
/// single call using the first-declared accessor's decorators. Here the setter
/// is declared first, so its decorator wins regardless of the getter's.
const FIRST_DECLARED_WINS_SOURCE: &str = r#"declare function decFirst(target: any, k: string, d: any): any;
declare function decSecond(target: any, k: string, d: any): any;
class Pair {
    @decFirst set thing(v: number) { }
    @decSecond get thing() { return 0; }
}
"#;

#[test]
fn first_declared_accessor_decorator_wins_es2015() {
    let output = emit_source(
        FIRST_DECLARED_WINS_SOURCE,
        legacy_options(ScriptTarget::ES2015),
    );
    assert_eq!(
        output.matches("], Pair.prototype, \"thing\"").count(),
        1,
        "getter/setter pair must produce a single __decorate call:\n{output}"
    );
    let call = decorate_pos(&output, "Pair.prototype", "thing");
    let body = &output[..call];
    assert!(
        body.contains("decFirst"),
        "first-declared accessor's decorator must be used:\n{output}"
    );
    assert!(
        !body.contains("decSecond"),
        "second accessor's decorator must not appear in the merged call:\n{output}"
    );
}

#[test]
fn first_declared_accessor_decorator_wins_es5() {
    let output = emit_source(
        FIRST_DECLARED_WINS_SOURCE,
        legacy_options(ScriptTarget::ES5),
    );
    assert_eq!(
        output.matches("], Pair.prototype, \"thing\"").count(),
        1,
        "getter/setter pair must produce a single __decorate call:\n{output}"
    );
    let call = decorate_pos(&output, "Pair.prototype", "thing");
    let body = &output[..call];
    assert!(
        body.contains("decFirst"),
        "first-declared accessor's decorator must be used:\n{output}"
    );
    assert!(
        !body.contains("decSecond"),
        "second accessor's decorator must not appear in the merged call:\n{output}"
    );
}

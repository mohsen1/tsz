//! Decorator `design:*` metadata must guard possibly-absent global
//! constructors with a runtime-presence check, matching `tsc`:
//!
//! - `bigint` is ALWAYS serialized as
//!   `typeof BigInt === "function" ? BigInt : Object`
//!   (`getGlobalBigIntNameWithFallback`) — `BigInt` is not implied by any
//!   target.
//! - `symbol` is serialized as
//!   `typeof Symbol === "function" ? Symbol : Object`
//!   (`getGlobalSymbolNameWithFallback`) only for **pre-ES2015** targets; at
//!   `ES2015`+ `Symbol` is assumed present and emitted bare.
//!
//! An unguarded `BigInt`/`Symbol` reference throws a `ReferenceError` (or
//! records wrong metadata) on runtimes lacking the global, which is exactly the
//! scenario the guard exists for.

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

fn legacy_metadata_options(target: ScriptTarget) -> PrinterOptions {
    PrinterOptions {
        legacy_decorators: true,
        emit_decorator_metadata: true,
        target,
        ..Default::default()
    }
}

const GUARDED_BIGINT: &str = "typeof BigInt === \"function\" ? BigInt : Object";
const GUARDED_SYMBOL: &str = "typeof Symbol === \"function\" ? Symbol : Object";

fn assert_design_type(output: &str, member: &str, serialized: &str) {
    let needle = format!("__metadata(\"design:type\", {serialized})\n], C.prototype, \"{member}\"");
    assert!(
        output.contains(&needle),
        "expected `{member}` design:type to serialize to `{serialized}`.\nOutput:\n{output}"
    );
}

/// `bigint` is guarded at every target (here ES2017), via both the `bigint`
/// keyword and a `bigint` identifier type reference.
#[test]
fn bigint_keyword_is_guarded_at_es2017() {
    let source = r#"declare const dec: any;
class C {
  @dec a: bigint;
}
"#;
    let output = emit_source(source, legacy_metadata_options(ScriptTarget::ES2017));
    assert_design_type(&output, "a", GUARDED_BIGINT);
    // The bare unguarded form must NOT appear as a metadata argument.
    assert!(
        !output.contains("__metadata(\"design:type\", BigInt)"),
        "bare BigInt leaked into metadata.\nOutput:\n{output}"
    );
}

/// `bigint` stays guarded even at `ESNext` — `BigInt` presence is not implied by
/// the target, so the guard must not be target-gated.
#[test]
fn bigint_is_guarded_at_esnext() {
    let source = r#"declare const dec: any;
class C {
  @dec a: bigint;
}
"#;
    let output = emit_source(source, legacy_metadata_options(ScriptTarget::ESNext));
    assert_design_type(&output, "a", GUARDED_BIGINT);
    assert!(
        !output.contains("__metadata(\"design:type\", BigInt)"),
        "bare BigInt leaked into ESNext metadata.\nOutput:\n{output}"
    );
}

/// `bigint` is guarded at ES5 too (and the ES5 downlevel decorator path uses a
/// separate serializer, so it must agree).
#[test]
fn bigint_is_guarded_at_es5() {
    let source = r#"declare const dec: any;
class C {
  @dec a: bigint;
}
"#;
    let output = emit_source(source, legacy_metadata_options(ScriptTarget::ES5));
    assert!(
        output.contains(&format!("__metadata(\"design:type\", {GUARDED_BIGINT})")),
        "bigint should be guarded at ES5.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__metadata(\"design:type\", BigInt)"),
        "bare BigInt leaked into ES5 metadata.\nOutput:\n{output}"
    );
}

/// `symbol` is guarded for pre-ES2015 targets (ES5 here).
#[test]
fn symbol_is_guarded_at_es5() {
    let source = r#"declare const dec: any;
class C {
  @dec a: symbol;
}
"#;
    let output = emit_source(source, legacy_metadata_options(ScriptTarget::ES5));
    assert!(
        output.contains(&format!("__metadata(\"design:type\", {GUARDED_SYMBOL})")),
        "symbol should be guarded at ES5.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__metadata(\"design:type\", Symbol)"),
        "bare Symbol leaked into ES5 metadata.\nOutput:\n{output}"
    );
}

/// `symbol` is emitted bare at ES2015 (the threshold) and above (ES2017) — it
/// must NOT regress to the guarded form.
#[test]
fn symbol_is_bare_at_es2015_and_above() {
    let source = r#"declare const dec: any;
class C {
  @dec a: symbol;
}
"#;
    for target in [
        ScriptTarget::ES2015,
        ScriptTarget::ES2017,
        ScriptTarget::ESNext,
    ] {
        let output = emit_source(source, legacy_metadata_options(target));
        assert_design_type(&output, "a", "Symbol");
        assert!(
            !output.contains(GUARDED_SYMBOL),
            "symbol must be bare at {target:?}, not guarded.\nOutput:\n{output}"
        );
    }
}

/// At ES2017, tsc emits guarded `BigInt` but bare `Symbol` — the two globals
/// are gated independently.
#[test]
fn bigint_and_symbol_are_gated_independently_at_es2017() {
    let source = r#"declare const dec: any;
class C {
  @dec a: bigint;
  @dec b: symbol;
}
"#;
    let output = emit_source(source, legacy_metadata_options(ScriptTarget::ES2017));
    assert_design_type(&output, "a", GUARDED_BIGINT);
    assert_design_type(&output, "b", "Symbol");
}

/// `design:paramtypes` and `design:returntype` share the serializer, so a
/// `bigint` param/return is guarded inside the array and the return position.
#[test]
fn bigint_guarded_in_paramtypes_and_returntype() {
    let source = r#"declare const dec: any;
class C {
  @dec method(a: bigint, b: symbol): bigint { return 0n; }
}
"#;
    let output = emit_source(source, legacy_metadata_options(ScriptTarget::ES2017));
    // symbol is bare at ES2017; bigint is guarded in both positions.
    assert!(
        output.contains(&format!(
            "__metadata(\"design:paramtypes\", [{GUARDED_BIGINT}, Symbol])"
        )),
        "paramtypes should guard bigint and keep Symbol bare at ES2017.\nOutput:\n{output}"
    );
    assert!(
        output.contains(&format!(
            "__metadata(\"design:returntype\", {GUARDED_BIGINT})"
        )),
        "returntype bigint should be guarded.\nOutput:\n{output}"
    );
}

/// `bigint | undefined` unwraps to the single meaningful member and serializes
/// to the guarded bigint form (matching tsc).
#[test]
fn bigint_union_with_undefined_is_guarded() {
    let source = r#"declare const dec: any;
class C {
  @dec a: bigint | undefined;
}
"#;
    let output = emit_source(source, legacy_metadata_options(ScriptTarget::ES2017));
    assert_design_type(&output, "a", GUARDED_BIGINT);
}

/// A `bigint` literal type (`1n`) and a negative bigint literal type (`-1n`)
/// both serialize to the guarded bigint form, like the bare `bigint` keyword.
#[test]
fn bigint_literal_types_are_guarded() {
    let source = r#"declare const dec: any;
class C {
  @dec a: 1n;
  @dec b: -1n;
}
"#;
    let output = emit_source(source, legacy_metadata_options(ScriptTarget::ES2017));
    assert_design_type(&output, "a", GUARDED_BIGINT);
    assert_design_type(&output, "b", GUARDED_BIGINT);
}

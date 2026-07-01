//! Regression tests for the discarded-value context of private-field
//! `++`/`--` lowering.
//!
//! When a private field is down-leveled to a `WeakMap` helper, a postfix
//! `#x++` whose result is *used* must be lowered to the value-preserving form
//! `(__classPrivateFieldSet(...), _old)` so the pre-increment value is what the
//! surrounding expression observes. The simpler statement form
//! `__classPrivateFieldSet(...)` (which evaluates to the *new* value) is only
//! valid when the result is discarded.
//!
//! `tsc`'s `discardedValueVisitor` treats a value as discarded only for the
//! immediate expression of an expression statement (and the for-loop
//! incrementor), transparent through parentheses only. A call argument, binary
//! operand, `void` operand, comma operand, etc. are value-used positions even
//! when the enclosing statement's own result is discarded. tsz previously
//! propagated the statement's discarded-value flag into those children, so
//! `sink(this.#x++);` miscompiled to the new-value form.

use crate::context::emit::EmitContext;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit(source: &str, target: ScriptTarget) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target,
        use_define_for_class_fields: false,
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

/// The value-preserving form must recover the pre-increment value, which needs
/// a *second* temp (`(_b = get, _a = _b++, _b)` plus a trailing `, _a`), so its
/// method declares `var _a, _b`. The discarded statement form reuses a single
/// working temp (`(_a = get, _a++, _a)`), declaring only `var _a`. Each test
/// isolates one `this`-received increment, so the second temp is present iff
/// the old value is preserved. The temp names are compiler-generated, not user
/// identifiers, so keying on them does not hard-code binder names.
fn uses_value_form(method_body: &str) -> bool {
    method_body.contains("var _a, _b")
}

fn method_line<'a>(output: &'a str, needle: &str) -> &'a str {
    output
        .lines()
        .find(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("expected a line containing `{needle}`.\nOutput:\n{output}"))
}

#[test]
fn private_postfix_as_call_argument_in_discarded_statement_uses_value_form() {
    // `sink(widget.#tally++)` — the enclosing call statement's result is
    // discarded, but the argument's value is used, so the increment must
    // preserve the old value.
    let source = r#"
declare function sink(n: number): void;
class Widget {
    #tally = 0;
    bump() { sink(this.#tally++); }
}
"#;
    let output = emit(source, ScriptTarget::ES2015);
    let line = method_line(&output, "bump()");
    assert!(
        uses_value_form(line),
        "a private-field postfix used as a call argument must preserve the old value even when the call statement is discarded.\nOutput:\n{output}"
    );
}

#[test]
fn private_postfix_as_bare_statement_uses_statement_form() {
    // Binder names deliberately differ from the call-argument test so the rule
    // is structural, not keyed on identifiers.
    let source = r#"
class Gauge {
    #level = 0;
    step() { this.#level++; }
}
"#;
    let output = emit(source, ScriptTarget::ES2015);
    let line = method_line(&output, "step()");
    assert!(
        !uses_value_form(line),
        "a private-field postfix that is the whole statement expression should use the discarded (statement) form.\nOutput:\n{output}"
    );
}

#[test]
fn private_postfix_through_parentheses_stays_discarded() {
    // Parentheses are transparent to the discarded-value context in tsc, so
    // `(this.#count++);` keeps the statement form.
    let source = r#"
class Meter {
    #count = 0;
    tick() { (this.#count++); }
}
"#;
    let output = emit(source, ScriptTarget::ES2015);
    let line = method_line(&output, "tick()");
    assert!(
        !uses_value_form(line),
        "parentheses are transparent to discarded-value context; the statement form should be kept.\nOutput:\n{output}"
    );
}

#[test]
fn private_postfix_as_comma_operand_uses_value_form() {
    // tsc treats a comma operand as value-used even when the comma expression
    // is the discarded statement expression.
    let source = r#"
declare function other(): void;
class Ledger {
    #entries = 0;
    record() { other(), this.#entries++; }
}
"#;
    let output = emit(source, ScriptTarget::ES2015);
    let line = method_line(&output, "record()");
    assert!(
        uses_value_form(line),
        "a private-field postfix that is a comma operand is value-used and must preserve the old value.\nOutput:\n{output}"
    );
}

#[test]
fn private_postfix_as_void_operand_uses_value_form() {
    let source = r#"
class Counter {
    #hits = 0;
    ping() { void this.#hits++; }
}
"#;
    let output = emit(source, ScriptTarget::ES2015);
    let line = method_line(&output, "ping()");
    assert!(
        uses_value_form(line),
        "the operand of `void` is value-used in tsc; the increment must preserve the old value.\nOutput:\n{output}"
    );
}

#[test]
fn private_postfix_returned_call_argument_uses_value_form() {
    // Regression guard for the already-correct case: `return sink(this.#x++)`.
    let source = r#"
declare function sink(n: number): number;
class Tracker {
    #seen = 0;
    read() { return sink(this.#seen++); }
}
"#;
    let output = emit(source, ScriptTarget::ES2015);
    let line = method_line(&output, "read()");
    assert!(
        uses_value_form(line),
        "a call argument whose enclosing call result is used must preserve the old value.\nOutput:\n{output}"
    );
}

#[test]
fn private_postfix_nested_call_argument_uses_value_form() {
    // `sink(sink(this.#x++));` — outer call discarded, inner argument used.
    let source = r#"
declare function sink(n: number): number;
class Relay {
    #depth = 0;
    forward() { sink(sink(this.#depth++)); }
}
"#;
    let output = emit(source, ScriptTarget::ES2015);
    let line = method_line(&output, "forward()");
    assert!(
        uses_value_form(line),
        "a private-field postfix nested in a discarded outer call is still value-used.\nOutput:\n{output}"
    );
}

//! Bare, unannotated JS parameters must `.d.ts`-emit as required
//! (`tree: any`), matching tsc: the `optional` bit such parameters carry
//! exists only for call-arity leniency (#17227 / #17238). Genuine optionals
//! (`@param {number} [a]`, a default initializer) keep `?`. These pin the
//! AST-driven `declare function` parameter path; the interned-shape
//! `TypePrinter` path is covered by the `masked_*` tests in
//! `tests/type_printer.rs`.

use super::*;

#[test]
fn bare_js_function_declaration_param_emits_required() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
function walk(tree) {}
export const g = walk;
"#,
    );
    assert!(
        output.contains("declare function walk(tree: any): void;"),
        "Expected bare JS param to emit required: {output}"
    );
    assert!(
        !output.contains("tree?"),
        "Did not expect a spurious optional marker: {output}"
    );
}

#[test]
fn bare_js_method_and_class_params_emit_required() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export class Walker { visit(node) {} }
export const helpers = { log(entry) {} };
"#,
    );
    assert!(
        output.contains("visit(node: any)"),
        "Expected bare class-method param to emit required: {output}"
    );
    assert!(
        output.contains("log(entry: any)"),
        "Expected bare object-method param to emit required: {output}"
    );
    assert!(
        !output.contains("node?") && !output.contains("entry?"),
        "Did not expect spurious optional markers: {output}"
    );
}

#[test]
fn genuine_js_optionals_keep_question_mark() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @param {number} [count] */
export function jsdocOptional(count) {}
export function defaulted(step = 1) {}
"#,
    );
    assert!(
        output.contains("jsdocOptional(count?: number"),
        "Expected JSDoc-optional param to keep `?`: {output}"
    );
    assert!(
        output.contains("defaulted(step?:"),
        "Expected defaulted param to keep `?`: {output}"
    );
}

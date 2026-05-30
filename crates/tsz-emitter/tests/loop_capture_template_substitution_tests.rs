//! Regression coverage for ES5 block-scoping closure-capture analysis that must
//! descend into template-literal substitutions (`${...}`).
//!
//! A block-scoped variable declared in a loop body and referenced only inside a
//! template-literal substitution expression that is captured by a closure must
//! still trigger the per-iteration loop-function (IIFE) lowering. Previously the
//! capture analyzer's child-walk swallowed `TEMPLATE_EXPRESSION` /
//! `TEMPLATE_SPAN` / `TAGGED_TEMPLATE_EXPRESSION` nodes via a catch-all arm, so
//! captures buried in `${...}` were never detected.
//!
//! The rule is keyed on AST node KIND, not identifier spelling, so every test
//! varies the captured binding name to prove generality (§25/§26).

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print;
use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::PrintOptions;

fn emit_es5(source: &str) -> String {
    parse_and_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    )
}

/// Witness: a body-declared block-scoped variable referenced only inside a
/// template substitution captured by an arrow must produce a per-iteration
/// loop function.
#[test]
fn for_of_template_substitution_capture_emits_loop_function() {
    let output = emit_es5(
        r#"
declare const items: string[];
declare function sink(s: () => string): void;
for (const fontType of items) {
    sink(() => `color: ${fontType};`);
}
"#,
    );

    assert!(
        output.contains("var _loop_1 = function"),
        "capture inside a template substitution must trigger the loop-function lowering.\nOutput:\n{output}"
    );
}

/// Same structure, different binding name. If the fix were keyed on the
/// identifier spelling, renaming the binding would silently bypass it.
#[test]
fn for_of_template_substitution_capture_renamed_binding() {
    let output = emit_es5(
        r#"
declare const list: string[];
declare function sink(s: () => string): void;
for (const widgetKind of list) {
    sink(() => `border: ${widgetKind};`);
}
"#,
    );

    assert!(
        output.contains("var _loop_1 = function"),
        "renamed binding captured in a template substitution must still trigger the loop-function lowering.\nOutput:\n{output}"
    );
}

/// A `while` loop where the captured binding lives inside a template span of a
/// multi-substitution template literal.
#[test]
fn while_template_multi_span_capture_emits_loop_function() {
    let output = emit_es5(
        r#"
declare function next(): boolean;
declare function sink(s: () => string): void;
while (next()) {
    let cellValue = 1;
    let rowName = "r";
    sink(() => `cell ${rowName}=${cellValue} done`);
}
"#,
    );

    assert!(
        output.contains("var _loop_1 = function"),
        "captures inside multiple template spans must trigger the loop-function lowering.\nOutput:\n{output}"
    );
}

/// Tagged template variant: the captured binding sits inside a substitution of
/// a tagged template literal. The analyzer must descend into both the tag and
/// the template.
#[test]
fn for_of_tagged_template_substitution_capture_emits_loop_function() {
    let output = emit_es5(
        r#"
declare const rows: string[];
declare function tag(strings: TemplateStringsArray, ...vals: any[]): string;
declare function sink(s: () => string): void;
for (const tileId of rows) {
    sink(() => tag`tile-${tileId}-end`);
}
"#,
    );

    assert!(
        output.contains("var _loop_1 = function"),
        "capture inside a tagged-template substitution must trigger the loop-function lowering.\nOutput:\n{output}"
    );
}

/// Negative: a template literal that captures nothing block-scoped from the
/// loop must NOT produce a loop-function. Only an outer (non-loop) binding is
/// referenced inside the substitution, so no per-iteration capture is needed.
#[test]
fn for_of_template_substitution_without_capture_is_unchanged() {
    let output = emit_es5(
        r#"
declare const items: string[];
declare function sink(s: string): void;
const prefix = "p";
for (const item of items) {
    sink(`color: ${prefix}-${item};`);
}
"#,
    );

    assert!(
        !output.contains("_loop_1"),
        "a template literal with no captured block-scoped loop binding must not introduce a loop function.\nOutput:\n{output}"
    );
}

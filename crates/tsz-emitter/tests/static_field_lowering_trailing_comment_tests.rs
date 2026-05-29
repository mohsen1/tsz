//! Tests for trailing-comment preservation when class fields and object
//! spreads are lowered for pre-ES2018/pre-ES2022 targets.
//!
//! Structural rule 1: when a static class field is lowered out of the class
//! body into a `ClassName.field = <init>` assignment (target < ES2022 /
//! `useDefineForClassFields = false`), comments that live *inside* the field's
//! initializer expression (e.g. a trailing line comment on the last member of
//! `static A = class { m() {} // x }`) must still be emitted inline. The class
//! body skip used to advance the comment cursor past the whole field span,
//! including those nested comments, so the re-emitted initializer lost them.
//!
//! Structural rule 2: when an object literal with a spread is lowered to
//! `Object.assign(...)` (target < ES2018), a trailing line comment on the
//! literal's last element must be emitted after the final argument with the
//! closing `)` on the next line, matching `tsc`.

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_with_opts;
use tsz_emitter::output::printer::PrintOptions;

// ── Rule 1: nested-initializer trailing comments survive static lowering ─────

/// Reported repro shape: trailing comment on the last method of a class
/// expression assigned to a static field.
#[test]
fn static_field_class_expr_last_method_trailing_comment_inline() {
    let source = "class D {\n    static A = class {\n        m() {} // keep me\n    }\n    static B = 1;\n}\n";
    let output = parse_and_print_with_opts(source, PrintOptions::es6());
    assert!(
        output.contains("D.A = class"),
        "static field must be lowered to assignment\nOutput:\n{output}"
    );
    // The comment must trail the method on the same line, not float to a
    // standalone line after the assignment.
    assert!(
        output.contains("m() { } // keep me"),
        "trailing comment must stay inline on the method\nOutput:\n{output}"
    );
    // It must NOT have been hoisted to its own line before/after `D.B`.
    assert!(
        !output.contains("\n// keep me"),
        "comment must not be relocated to a standalone line\nOutput:\n{output}"
    );
}

/// Same rule with renamed identifiers and a different member name — the fix is
/// not keyed on any user-chosen name.
#[test]
fn static_field_class_expr_renamed_last_method_trailing_comment_inline() {
    let source = "class Widget {\n    static Inner = class {\n        render() {} // note here\n    }\n    static Count = 0;\n}\n";
    let output = parse_and_print_with_opts(source, PrintOptions::es6());
    assert!(
        output.contains("Widget.Inner = class"),
        "static field must be lowered\nOutput:\n{output}"
    );
    assert!(
        output.contains("render() { } // note here"),
        "trailing comment must stay inline on the renamed method\nOutput:\n{output}"
    );
}

/// Trailing comment on the last property of an object-literal initializer of a
/// lowered static field (no spread involved).
#[test]
fn static_field_object_literal_last_prop_trailing_comment_inline() {
    let source = "class D {\n    static C = {\n        a: 1,\n        b: 2 // last prop\n    };\n    static E = 3;\n}\n";
    let output = parse_and_print_with_opts(source, PrintOptions::es6());
    assert!(
        output.contains("D.C = {"),
        "static object-literal field must be lowered\nOutput:\n{output}"
    );
    assert!(
        output.contains("b: 2 // last prop"),
        "trailing comment must stay inline on the last property\nOutput:\n{output}"
    );
}

// ── Rule 2: trailing comments survive Object.assign spread lowering ──────────

/// Trailing comment on the last element of a spread object literal lowered to
/// `Object.assign(...)`. The comment lands after the final argument and the
/// closing `)` moves to the next line.
#[test]
fn object_spread_object_assign_trailing_comment_before_paren() {
    let source = "const g = {\n    a: 1,\n    ...{ b: 2 } // trailing\n};\n";
    let output = parse_and_print_with_opts(source, PrintOptions::es6());
    assert!(
        output.contains("Object.assign("),
        "spread must lower to Object.assign at ES2015\nOutput:\n{output}"
    );
    assert!(
        output.contains("// trailing\n"),
        "trailing comment must precede the closing paren on its own line\nOutput:\n{output}"
    );
    // Exactly one space before the comment (the emitter writes its own space).
    assert!(
        !output.contains("}  // trailing"),
        "comment must have a single leading space, not two\nOutput:\n{output}"
    );
}

/// Same rule inside a lowered static field initializer (both lowerings stack),
/// matching the reported `useBeforeDeclaration_propertyAssignment` witness.
#[test]
fn static_field_object_spread_object_assign_trailing_comment() {
    let source = "class D {\n    static C = {\n        x: 1,\n        ...{ y: 2 } // should be an error\n    };\n}\n";
    let output = parse_and_print_with_opts(source, PrintOptions::es6());
    assert!(
        output.contains("D.C = Object.assign("),
        "static spread field must lower to Object.assign\nOutput:\n{output}"
    );
    assert!(
        output.contains("// should be an error\n"),
        "trailing comment must survive both lowerings\nOutput:\n{output}"
    );
}

// ── Negative cases: no spurious output without a comment ─────────────────────

/// No trailing comment present: lowering must not invent a comment, a stray
/// space, or a newline before the closing paren.
#[test]
fn object_spread_object_assign_no_comment_no_spurious_output() {
    let source = "const g = {\n    a: 1,\n    ...{ b: 2 }\n};\n";
    let output = parse_and_print_with_opts(source, PrintOptions::es6());
    assert!(
        output.contains("Object.assign({ a: 1 }, { b: 2 })"),
        "no-comment spread must lower compactly with closing paren inline\nOutput:\n{output}"
    );
    assert!(
        !output.contains("} \n)") && !output.contains("}\n)"),
        "no-comment case must not move the closing paren to its own line\nOutput:\n{output}"
    );
}

/// Static field whose initializer has no inner comment: lowering must not
/// relocate or fabricate comments.
#[test]
fn static_field_class_expr_no_inner_comment_clean() {
    let source = "class D {\n    static A = class {\n        m() {}\n    }\n    static B = 1;\n}\n";
    let output = parse_and_print_with_opts(source, PrintOptions::es6());
    assert!(
        output.contains("D.A = class"),
        "static field must be lowered\nOutput:\n{output}"
    );
    assert!(
        !output.contains("//"),
        "no comment must be emitted when source has none\nOutput:\n{output}"
    );
}

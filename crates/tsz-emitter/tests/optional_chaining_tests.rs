//! Integration tests for optional chaining downlevel emit

use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::{parse_and_lower_print, parse_and_print_with_opts};

fn emit_es2016(source: &str) -> String {
    let opts = PrintOptions {
        target: ScriptTarget::ES2016,
        ..Default::default()
    };
    parse_and_lower_print(source, opts)
}

/// super.method?.() should capture `super.method` as a unit and use `.call(this)`.
/// See GH#34952.
#[test]
fn super_property_access_optional_call() {
    let source = r#"class Base { method() {} }
class Derived extends Base {
    method1() { return super.method?.(); }
}"#;
    let output = emit_es2016(source);
    // Must capture `super.method` as a unit, not `(_a = super).method`
    assert!(
        output.contains("(_a = super.method) === null || _a === void 0 ? void 0 : _a.call(this)"),
        "super.method?.() should capture super.method as a unit.\nOutput:\n{output}"
    );
    // Must NOT contain (_a = super)
    assert!(
        !output.contains("(_a = super)") && !output.contains("= super)"),
        "super must not be captured in a temp variable.\nOutput:\n{output}"
    );
}

/// super["method"]?.() should capture `super["method"]` as a unit.
#[test]
fn super_element_access_optional_call() {
    let source = r#"class Base { method() {} }
class Derived extends Base {
    method2() { return super["method"]?.(); }
}"#;
    let output = emit_es2016(source);
    assert!(
        output.contains(
            "(_a = super[\"method\"]) === null || _a === void 0 ? void 0 : _a.call(this)"
        ),
        "super[\"method\"]?.() should capture super[\"method\"] as a unit.\nOutput:\n{output}"
    );
}

/// Hoisted var declarations should appear inline in single-line function bodies.
#[test]
fn hoisted_var_in_single_line_body() {
    let source = "class Base { method() {} }
class Derived extends Base {
    method1() { return super.method?.(); }
}";
    let opts = PrintOptions {
        target: ScriptTarget::ES2016,
        ..Default::default()
    };
    // parse_and_print_with_opts calls set_source_text (needed for single-line detection)
    let output = parse_and_print_with_opts(source, opts);
    assert!(
        output.contains("{ var _a; return"),
        "Single-line body should include inline var declaration.\nOutput:\n{output}"
    );
}

#[test]
fn concise_arrow_optional_method_call_gets_temp_prologue() {
    let source = "const typeHandlers = {};\nconst onSomeEvent = (p) => typeHandlers[p.t]?.(p);";
    let output = emit_es2016(source);

    assert!(
        output.contains(
            "const onSomeEvent = (p) => {\n    var _a;\n    return (_a = typeHandlers[p.t]) === null || _a === void 0 ? void 0 : _a.call(typeHandlers, p);\n};"
        ),
        "Concise arrow optional-call temp should be declared inside a synthesized block.\nOutput:\n{output}"
    );
}

/// Regression: `(foo?.m as any).length` previously emitted
/// `foo?.m.length` — the outer parens around the optional chain were
/// stripped along with the type assertion. The two forms are NOT
/// semantically equivalent:
///   `(foo?.m).length` — chain ends at `)`, then `.length` accesses
///     the result. If foo is nullish, accesses `.length` on `undefined`
///     and throws a `TypeError`.
///   `foo?.m.length` — chain continues through `.length`. If foo is
///     nullish, the whole chain short-circuits to `undefined`.
/// The fix in `emit_parenthesized` preserves the parens when the
/// type-erased inner expression is an optional chain *and* we are in
/// an access position.
#[test]
fn type_asserted_optional_chain_in_access_preserves_outer_parens() {
    let source = r#"class Foo { m() {} }
const foo: Foo | undefined = undefined;
(foo?.m as any).length;
(<any>foo?.m).length;
(foo?.["m"] as any).length;
(<any>foo?.["m"]).length;
"#;
    let opts = PrintOptions {
        target: ScriptTarget::ESNext,
        ..Default::default()
    };
    let output = parse_and_print_with_opts(source, opts);
    assert!(
        output.contains("(foo?.m).length"),
        "Outer parens around optional property chain must be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(foo?.[\"m\"]).length"),
        "Outer parens around optional element chain must be preserved.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("foo?.m.length"),
        "Optional chain must not be allowed to continue through stripped parens.\nOutput:\n{output}"
    );
}

/// Regression: `(foo.m as any)?.()` previously emitted `foo.m()` —
/// the `?.()` optional-call marker was silently dropped. The cause:
/// `find_call_open_paren_position` searched for the first `(` between
/// `call_node.pos` and `call_node.end`, but for a parenthesized callee
/// that first `(` belonged to the *callee*, not the call's argument
/// list. The backward scan for `?.` from the wrong `(` saw the callee's
/// content and bailed, so the `?.` token was never emitted. The fix
/// pins the scan start to right after the callee.
#[test]
fn parenthesized_type_assertion_callee_preserves_optional_call() {
    let source = r#"class Foo { m() {} }
const foo = new Foo();
(foo.m as any)?.();
(<any>foo.m)?.();
"#;
    let opts = PrintOptions {
        target: ScriptTarget::ESNext,
        ..Default::default()
    };
    let output = parse_and_print_with_opts(source, opts);
    assert!(
        output.contains("foo.m?.()"),
        "Parenthesized type-asserted callee must preserve `?.()`, not drop the `?.` token.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("foo.m();"),
        "The `?.` token must not be dropped to a plain call.\nOutput:\n{output}"
    );
}

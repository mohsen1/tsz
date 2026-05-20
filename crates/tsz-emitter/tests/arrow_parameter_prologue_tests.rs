use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn emit_with_target(source: &str, target: ScriptTarget) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            target,
            ..PrintOptions::default()
        },
    )
    .code
}

#[test]
fn es2015_recovered_arrow_line_terminator_stays_canonical_outside_enum() {
    let output = emit_with_target("var fn = ()\n    => 10;", ScriptTarget::ES2015);

    assert!(
        output.contains("var fn = () => 10;"),
        "Recovered native arrow should be canonically printed outside enum lowering.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("()\n    =>"),
        "Illegal arrow line break should not leak into ordinary expression output.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_arrow_binding_param_class_static_uses_native_prologue() {
    let output = emit_with_target(
        "(({ [class { static x = 1 }.x]: b = \"\" }) => {})();",
        ScriptTarget::ES2015,
    );

    assert!(
        output.contains("((_a) => { var _b; var { [(_b = class {"),
        "Arrow binding parameter should be moved to a native body prologue.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_b.x = 1,"),
        "Class static field side effect should stay inside the binding key expression.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_b).x]: b = \"\" } = _a; })();"),
        "Native destructuring should read from the forwarded parameter temp.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("(({ [("),
        "Original destructuring parameter should not remain in the arrow head.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_arrow_binding_param_nullish_key_uses_native_prologue() {
    let output = emit_with_target(
        "const a = () => undefined;\n(({ [a() ?? \"d\"]: c = \"\" }) => {})();",
        ScriptTarget::ES2015,
    );

    assert!(
        output.contains(
            "((_a) => { var _b; var { [(_b = a()) !== null && _b !== void 0 ? _b : \"d\"]: c = \"\" } = _a; })();"
        ),
        "Arrow binding parameter with a downlevel nullish key should move the pattern into a native body prologue.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("(({ [(_a = a())"),
        "Downlevel nullish temp should not be hoisted outside the arrow parameter prologue.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_arrow_binding_param_wrapped_nullish_key_uses_native_prologue() {
    let output = emit_with_target(
        "const a = () => undefined;\n(({ [+(a() ?? \"d\")]: c = \"\" }) => {})();",
        ScriptTarget::ES2015,
    );

    assert!(
        output.contains(
            "((_a) => { var _b; var { [+((_b = a()) !== null && _b !== void 0 ? _b : \"d\")]: c = \"\" } = _a; })();"
        ),
        "Arrow binding parameter with a wrapped downlevel nullish key should move the pattern into a native body prologue.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("(({ [+((_a = a())"),
        "Wrapped downlevel nullish temp should not be hoisted outside the arrow parameter prologue.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_arrow_binding_param_optional_chain_key_uses_native_prologue() {
    let output = emit_with_target(
        "const a = () => undefined;\n(({ [a()?.d]: c = \"\" }) => {})();",
        ScriptTarget::ES2015,
    );

    assert!(
        output.contains(
            "((_a) => { var _b; var { [(_b = a()) === null || _b === void 0 ? void 0 : _b.d]: c = \"\" } = _a; })();"
        ),
        "Arrow binding parameter with a downlevel optional-chain key should move the pattern into a native body prologue.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("(({ [(_a = a())"),
        "Downlevel optional-chain temp should not be hoisted outside the arrow parameter prologue.\nOutput:\n{output}"
    );
}

#[test]
fn es5_arrow_binding_param_nullish_key_allocates_inner_temp_first() {
    let output = emit_with_target(
        "const a = () => undefined;\n(({ [a() ?? \"d\"]: c = \"\" }) => {})();",
        ScriptTarget::ES5,
    );

    assert!(
        output.contains(
            "var _b;\n    var _c = (_b = a()) !== null && _b !== void 0 ? _b : \"d\", _d = _a[_c], c = _d === void 0 ? \"\" : _d;"
        ),
        "ES5 arrow parameter destructuring should allocate the nullish temp before the computed-key temp.\nOutput:\n{output}"
    );
}

#[test]
fn es5_arrow_binding_param_wrapped_nullish_key_allocates_inner_temp_first() {
    let output = emit_with_target(
        "const a = () => undefined;\n(({ [+(a() ?? \"d\")]: c = \"\" }) => {})();",
        ScriptTarget::ES5,
    );

    assert!(
        output.contains(
            "var _b;\n    var _c = +((_b = a()) !== null && _b !== void 0 ? _b : \"d\"), _d = _a[_c], c = _d === void 0 ? \"\" : _d;"
        ),
        "ES5 arrow parameter destructuring should allocate the wrapped nullish temp before the computed-key temp.\nOutput:\n{output}"
    );
}

#[test]
fn es5_arrow_binding_param_array_wrapped_nullish_key_allocates_inner_temp_first() {
    let output = emit_with_target(
        "const a = () => undefined;\n(({ [[a() ?? \"d\"][0]]: c = \"\" }) => {})();",
        ScriptTarget::ES5,
    );

    assert!(
        output.contains(
            "var _b;\n    var _c = [(_b = a()) !== null && _b !== void 0 ? _b : \"d\"][0], _d = _a[_c], c = _d === void 0 ? \"\" : _d;"
        ),
        "ES5 arrow parameter destructuring should capture downlevel temps inside array/access wrappers before the computed-key temp.\nOutput:\n{output}"
    );
}

#[test]
fn es5_arrow_binding_param_object_wrapped_nullish_key_allocates_inner_temp_first() {
    let output = emit_with_target(
        "const a = () => undefined;\n(({ [{ value: a() ?? \"d\" }.value]: c = \"\" }) => {})();",
        ScriptTarget::ES5,
    );

    assert!(
        output.contains(
            "var _b;\n    var _c = { value: (_b = a()) !== null && _b !== void 0 ? _b : \"d\" }.value, _d = _a[_c], c = _d === void 0 ? \"\" : _d;"
        ),
        "ES5 arrow parameter destructuring should capture downlevel temps inside object/member wrappers before the computed-key temp.\nOutput:\n{output}"
    );
}

#[test]
fn es5_arrow_binding_param_optional_chain_key_allocates_inner_temp_first() {
    let output = emit_with_target(
        "const a = () => undefined;\n(({ [a()?.d]: c = \"\" }) => {})();",
        ScriptTarget::ES5,
    );

    assert!(
        output.contains(
            "var _b;\n    var _c = (_b = a()) === null || _b === void 0 ? void 0 : _b.d, _d = _a[_c], c = _d === void 0 ? \"\" : _d;"
        ),
        "ES5 arrow parameter destructuring should allocate the optional-chain temp before the computed-key temp.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_simple_optional_chain_arrow_stays_concise_with_comment() {
    let output = emit_with_target(
        "// https://github.com/microsoft/TypeScript/issues/41814\n\
         const test = (names: string[]) =>\n\
             // single-line comment\n\
             names?.filter(x => x);",
        ScriptTarget::ES2015,
    );

    assert!(
        output.contains("const test = (names) => \n// single-line comment\nnames === null || names === void 0 ? void 0 : names.filter(x => x);"),
        "Simple optional-chain arrows should stay concise and preserve the body comment.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("=> {"),
        "Simple optional-chain arrows should not be block-wrapped just to lower `?.`.\nOutput:\n{output}"
    );
}

#[test]
fn es5_optional_chain_arrow_body_comment_precedes_return() {
    let output = emit_with_target(
        "// https://github.com/microsoft/TypeScript/issues/41814\n\
         const test = (names: string[]) =>\n\
             // single-line comment\n\
             names?.filter(x => x);",
        ScriptTarget::ES5,
    );

    assert!(
        output.contains(
            "var test = function (names) {\n    // single-line comment\n    return names === null || names === void 0 ? void 0 : names.filter(function (x) { return x; });\n};"
        ),
        "ES5 arrow lowering should place body-leading line comments before the synthesized return.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("names.filter(// single-line comment"),
        "Body-leading comments must not leak into the optional-call argument list.\nOutput:\n{output}"
    );
}

// ── computed instance field in a function parameter default ──────────────────
// tsc pattern: (_classTemp = IIFE, _propNameTemp = expr, _classTemp)
// with var _propNameTemp, _classTemp; hoisted before the if-block.

#[test]
fn es5_param_default_class_computed_instance_field_uses_comma_pattern() {
    let output = emit_with_target(
        "function f(y = class { [x] = x }, x = 1) {}",
        ScriptTarget::ES5,
    );

    assert!(
        output.contains("var _a, _b;"),
        "Computed instance-field and class temps must be hoisted before the if-block.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(_b = /** @class */"),
        "Class temp must be assigned inline in a comma expression.\nOutput:\n{output}"
    );
    assert!(
        output.contains("this[_a] = x;"),
        "Constructor must reference the computed-prop-name temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = x,"),
        "Prop-name temp must be initialized after the class IIFE in the comma expression.\nOutput:\n{output}"
    );
    assert!(
        output.contains(",\n        _b); }"),
        "Class temp must be the final value in the comma expression.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var _a;\n    _a = x;"),
        "Computed prop temp must not appear inside the IIFE body.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__setFunctionName"),
        "No-static anonymous computed-field class expression must not add named-evaluation helper output.\nOutput:\n{output}"
    );
}

#[test]
fn es5_param_default_class_computed_instance_field_name_agnostic() {
    // Renamed binding variable (k/v instead of x/y) - the structural rule must hold
    // regardless of what names the user chose.
    let output = emit_with_target(
        "function g(v = class { [k] = k }, k = 1) {}",
        ScriptTarget::ES5,
    );

    assert!(
        output.contains("var _a, _b;"),
        "Computed instance-field temps must be hoisted whatever the user names the params.\nOutput:\n{output}"
    );
    assert!(
        output.contains("this[_a] = k;"),
        "Constructor must use the temp for the computed key.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = k,"),
        "Prop-name temp must be initialized to the key expression after the IIFE.\nOutput:\n{output}"
    );
}

#[test]
fn es5_param_default_class_computed_instance_field_indentation() {
    // The IIFE body content must be indented relative to the function body,
    // not flattened to a single continuation level.
    let output = emit_with_target(
        "function h(y = class { [x] = x }, x = 1) {}",
        ScriptTarget::ES5,
    );

    // The constructor body line `this[_a] = x;` should be one more level in than
    // `function class_1() {`  — not at the same level.
    let class_line_pos = output
        .find("function class_")
        .expect("must have class function");
    let ctor_body_pos = output
        .find("this[_a] = x;")
        .expect("must have constructor body assignment");
    let class_indent = output[..class_line_pos]
        .rfind('\n')
        .map(|p| output[p + 1..class_line_pos].len())
        .unwrap_or(0);
    let body_indent = output[..ctor_body_pos]
        .rfind('\n')
        .map(|p| output[p + 1..ctor_body_pos].len())
        .unwrap_or(0);
    assert!(
        body_indent > class_indent,
        "Constructor body must be indented deeper than the constructor declaration (class: {class_indent}, body: {body_indent}).\nOutput:\n{output}"
    );
}

#[test]
fn es5_param_default_class_computed_instance_field_leading_comment() {
    let output = emit_with_target(
        "function documented(y = /** class comment */ class { [x] = x }, x = 1) {}",
        ScriptTarget::ES5,
    );

    assert!(
        output.contains("/** class comment */"),
        "Class-expression leading comments should be preserved when the IIFE expression is scheduled in a comma expression.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = x,"),
        "Computed-key temp must still be initialized after the class IIFE.\nOutput:\n{output}"
    );
}

#[test]
fn es5_param_default_named_class_computed_instance_field_uses_comma_pattern() {
    let output = emit_with_target(
        "function named(y = class C { [x] = x }, x = 1) {}",
        ScriptTarget::ES5,
    );

    assert!(
        output.contains("function C()"),
        "Named class expression must preserve its constructor name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = x,"),
        "Named class expression computed-key temp must be initialized after the class IIFE in the surrounding comma expression.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var _a;\n    _a = x;"),
        "Named class expression computed-key temp must not be initialized inside the class IIFE.\nOutput:\n{output}"
    );
}

#[test]
fn es5_param_default_static_class_computed_instance_field_uses_comma_pattern() {
    let output = emit_with_target(
        "function stat(y = class { [x] = x; static s = 1 }, x = 1) {}",
        ScriptTarget::ES5,
    );

    assert!(
        output.contains("_a = x,"),
        "Static class expression computed-key temp must be initialized after the class IIFE in the surrounding comma expression.\nOutput:\n{output}"
    );
    assert!(
        output.contains(".s = 1"),
        "Static class expression must still emit its static member tail.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__setFunctionName"),
        "Static class expression still needs named-evaluation helper output while routed through a class temp.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var _a;\n    _a = x;"),
        "Static class expression computed-key temp must not be initialized inside the class IIFE.\nOutput:\n{output}"
    );
}

#[test]
fn es5_statement_class_computed_instance_field_uses_outer_temp() {
    // Statement-level class declarations with instance-only computed fields
    // close over an outer temp (tsc behavior).
    let output = emit_with_target("class C { [x] = x }", ScriptTarget::ES5);

    assert!(
        output.contains("var _a;\nvar C"),
        "Computed instance-field temp must be declared before the statement class IIFE.\nOutput:\n{output}"
    );
    assert!(
        output.contains("}());\n_a = x;"),
        "Computed instance-field temp must be initialized after the statement class IIFE.\nOutput:\n{output}"
    );
}

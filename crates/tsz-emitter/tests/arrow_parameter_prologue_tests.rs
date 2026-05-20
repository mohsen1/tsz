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

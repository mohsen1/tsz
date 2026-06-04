#[test]
fn valid_typeof_property_call_does_not_emit_extra_statement() {
    // A method literally named `typeof` is a valid JS property. The emitter must
    // not treat it as a recovered type-annotation tail and emit a duplicate
    // `typeof (arg);` statement.
    let source = r#"const obj = {
    typeof(value) {
        return value;
    }
};
const result = obj.typeof("ok");
result;"#;

    let output = parse_and_print(source);

    assert!(
        output.contains(r#"obj.typeof("ok")"#),
        "Valid .typeof() property call should be preserved.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(r#"typeof ("ok");"#),
        "Valid .typeof() call must not produce a spurious typeof statement.\nOutput:\n{output}"
    );
}

#[test]
fn async_arrow_recovery_ignores_string_literal_initializer() {
    let source = r#"var x = "async (a): Foo = await =>";"#;

    let output = parse_and_print(source);

    assert!(
        output.contains(r#"var x = "async (a): Foo = await =>";"#),
        "String literal initializer should be preserved.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(", Foo"),
        "Recovery must not add a return-type binding from string contents.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("{\n}"),
        "Recovery must not emit an extra block from string contents.\nOutput:\n{output}"
    );
}

#[test]
fn recovered_interface_function_type_body_return_emits() {
    let source = r#"class Foo {
    get Z() {
        return 1;
    }
}

interface I2 extends Foo {
    a: {
        toString: () => {
            return 1;
        };
    }
}"#;

    let output = parse_and_print(source);

    assert!(
        output.contains("get Z()"),
        "Value-side class should still emit.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return 1;\n;"),
        "Recovered return from erased interface type body should emit before the leftover semicolon.\nOutput:\n{output}"
    );
}

#[test]
fn es5_for_in_single_array_binding_with_default_inlines_void0() {
    let source = "for (let [x = 'a' in {}] in { '': 0 }) console.log(x)";
    let output = parse_and_emit_strict_target(source, "forin.ts", ScriptTarget::ES5);

    assert!(
        output.contains("(void 0)[0]"),
        "Single-element array for-in head should inline (void 0)[0].\nOutput:\n{output}"
    );
    assert!(
        !output.contains("= void 0,"),
        "Single-element array for-in head must not allocate a source temp.\nOutput:\n{output}"
    );
}

#[test]
fn es5_for_in_single_object_binding_with_default_inlines_void0() {
    // Renamed iteration variable (`y` not `x`) to prove the rule is structural.
    let source = "for (let {y = 'a' in {}} in { '': 0 }) console.log(y)";
    let output = parse_and_emit_strict_target(source, "forin.ts", ScriptTarget::ES5);

    assert!(
        output.contains("(void 0).y"),
        "Single-element object for-in head should inline (void 0).y.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("= void 0,"),
        "Single-element object for-in head must not allocate a source temp.\nOutput:\n{output}"
    );
}

#[test]
fn es5_for_in_single_array_binding_no_default_inlines_void0() {
    // Single element without a default still reads the source once -> inline.
    let source = "for (var [first] in []) {}";
    let output = parse_and_emit_strict_target(source, "forin.ts", ScriptTarget::ES5);

    assert!(
        output.contains("(void 0)[0]"),
        "Single-element no-default array for-in head should inline (void 0)[0].\nOutput:\n{output}"
    );
    assert!(
        !output.contains("= void 0,"),
        "Single-element for-in head must not allocate a source temp.\nOutput:\n{output}"
    );
}

#[test]
fn es5_for_in_multi_array_binding_uses_source_temp() {
    // Two elements read the source twice, so tsc binds it to a shared temp.
    let source = "for (var [a, b] in []) {}";
    let output = parse_and_emit_strict_target(source, "forin.ts", ScriptTarget::ES5);

    assert!(
        output.contains("= void 0,"),
        "Multi-element array for-in head should allocate a shared source temp.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("(void 0)["),
        "Multi-element array for-in head must not inline the synthetic source.\nOutput:\n{output}"
    );
}

#[test]
fn es5_for_in_multi_object_binding_uses_source_temp() {
    // Renamed members (`p`/`q`) prove the multi-element fallback is structural.
    let source = "for (var {p, q} in []) {}";
    let output = parse_and_emit_strict_target(source, "forin.ts", ScriptTarget::ES5);

    assert!(
        output.contains("= void 0,"),
        "Multi-element object for-in head should allocate a shared source temp.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("(void 0)."),
        "Multi-element object for-in head must not inline the synthetic source.\nOutput:\n{output}"
    );
}

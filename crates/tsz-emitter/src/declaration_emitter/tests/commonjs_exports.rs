use super::*;

#[test]
fn define_property_jsdoc_typeof_module_exports_function_param_emits_callable_type() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
Object.defineProperty(module.exports, "factory", { value: function factory() {} });

/**
 * @param {typeof module.exports.factory} input
 */
function use(input) {}
Object.defineProperty(module.exports, "use", { value: use });
"#,
    );

    assert!(
        output.contains("export function use(input: () => void): void;"),
        "Expected direct JSDoc module.exports function reference to emit a callable type: {output}"
    );
}

#[test]
fn define_property_jsdoc_object_typeof_module_exports_function_param_emits_callable_member() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
Object.defineProperty(module.exports, "make", { value: function make() {} });

/**
 * @param {{value: typeof module.exports.make}} input
 */
function consume(input) {}
Object.defineProperty(module.exports, "consume", { value: consume });
"#,
    );

    assert!(
        output.contains("export function consume(input: {\n    value: () => void;\n}): void;"),
        "Expected object JSDoc module.exports function reference to emit a callable member: {output}"
    );
}

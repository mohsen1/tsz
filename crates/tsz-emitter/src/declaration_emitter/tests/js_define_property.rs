use super::*;

#[test]
fn test_js_define_property_jsdoc_typeof_function_export_emits_callable_type() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
function task() {}
Object.defineProperty(module.exports, "task", { value: task });

/**
 * @param {typeof module.exports.task} callback
 */
function use(callback) {
    callback();
}
Object.defineProperty(module.exports, "use", { value: use });
"#,
    );

    assert!(
        output.contains("export function use(callback: () => void): void;"),
        "Expected defineProperty JSDoc typeof reference to emit callable type: {output}"
    );
}

#[test]
fn test_js_define_property_jsdoc_nested_typeof_function_export_emits_callable_type() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
function task() {}
Object.defineProperty(module.exports, "task", { value: task });

/**
 * @param {Array<typeof module.exports.task>} callbacks
 */
function useAll(callbacks) {
    callbacks[0]();
}
Object.defineProperty(module.exports, "useAll", { value: useAll });
"#,
    );

    assert!(
        output.contains("export function useAll(callbacks: Array<() => void>): void;"),
        "Expected nested defineProperty JSDoc typeof reference to emit callable type: {output}"
    );
}

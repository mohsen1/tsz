use super::*;

#[test]
fn mutable_function_expando_variable_keeps_callable_signature() {
    let output = emit_dts_with_usage_analysis(
        r#"
const ConstFn = function (n: number) {
    return n.toString();
};
ConstFn.prop = 1;

var VarFn = function (n: number) {
    return n.toString();
};
VarFn.prop = 1;

let RenamedArrow = (flag: boolean) => flag ? 1 : 0;
RenamedArrow.meta = "value";
"#,
    );

    assert!(
        output.contains("declare const ConstFn: {\n    (n: number): any;\n    prop: number;\n};"),
        "Expected const function expression to keep expando static surface: {output}"
    );
    assert!(
        output.contains("declare var VarFn: (n: number) => any;"),
        "Expected mutable function expression to emit its own callable signature: {output}"
    );
    assert!(
        output.contains("declare let RenamedArrow: (flag: boolean) => any;"),
        "Expected mutable arrow function to emit its own callable signature: {output}"
    );
    assert!(
        !output.contains("declare var VarFn: {\n    (n: number): any;\n    prop: number;\n};"),
        "Did not expect mutable function expression to merge expando static surface: {output}"
    );
    assert!(
        !output.contains(
            "declare let RenamedArrow: {\n    (flag: boolean): any;\n    meta: string;\n};"
        ),
        "Did not expect mutable arrow function to merge expando static surface: {output}"
    );
}

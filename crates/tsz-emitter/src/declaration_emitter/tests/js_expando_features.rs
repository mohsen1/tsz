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

#[test]
fn returned_local_function_expando_keeps_assigned_member_type() {
    let output = emit_dts_with_usage_analysis(
        r#"
function makeCounter(seed: number) {
    const next = function (step: number) {
        return seed + step;
    };
    next.total = seed + 1;
    return next;
}

function makeFlagged(flag: boolean) {
    let choose = (value: string) => value.length;
    choose.ready = flag;
    return choose;
}
"#,
    );

    assert!(
        output.contains(
            "declare function makeCounter(seed: number): {\n    (step: number): number;\n    total: number;\n};"
        ),
        "Expected returned function expression expando property to keep assigned member type: {output}"
    );
    assert!(
        output.contains(
            "declare function makeFlagged(flag: boolean): {\n    (value: string): any;\n    ready: boolean;\n};"
        ),
        "Expected returned arrow expando property to keep assigned member type: {output}"
    );
}

#[test]
fn repeated_object_expando_assignments_emit_normalized_union() {
    let output = emit_dts_with_usage_analysis(
        r#"
const Build = function (label: string) {
    return label.length;
};
Build.config = { count: 1 };
Build.config = { name: "ready" };

const Renamed = (enabled: boolean) => enabled;
Renamed.state = { ok: true };
Renamed.state = { code: 200 };
"#,
    );

    assert!(
        output.contains(
            "config: {\n        count: number;\n        name?: undefined;\n    } | {\n        name: string;\n        count?: undefined;\n    };"
        ),
        "Expected repeated object expando assignment to emit normalized object union: {output}"
    );
    assert!(
        output.contains(
            "state: {\n        ok: boolean;\n        code?: undefined;\n    } | {\n        code: number;\n        ok?: undefined;\n    };"
        ),
        "Expected renamed arrow expando assignment to emit normalized object union: {output}"
    );
}

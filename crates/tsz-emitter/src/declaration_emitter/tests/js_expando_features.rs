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
        output.contains("declare var VarFn: (n: number) => string;"),
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

#[test]
fn namespace_returned_local_function_expando_emits_typeof_merge() {
    let output = emit_dts_with_usage_analysis(
        r#"
namespace Boxed {
    function Builder(): void {}
    Builder.count = 1;
    export function make() {
        return Builder;
    }
}

namespace Renamed {
    function Factory(flag: boolean) {
        return flag;
    }
    Factory.label = "ready";
    export function getFactory() {
        return Factory;
    }
}
"#,
    );

    assert!(
        output.contains(
            "function Builder(): void;\n    namespace Builder {\n        var count: number;\n    }\n    export function make(): typeof Builder;"
        ),
        "Expected returned namespace-local function expando to emit a function/namespace merge and typeof return: {output}"
    );
    assert!(
        output.contains(
            "function Factory(flag: boolean): boolean;\n    namespace Factory {\n        var label: string;\n    }\n    export function getFactory(): typeof Factory;"
        ),
        "Expected renamed namespace-local function expando to emit a function/namespace merge and typeof return: {output}"
    );
}

#[test]
fn expando_arithmetic_initializer_uses_member_call_and_instance_facts() {
    let output = emit_dts_with_usage_analysis(
        r#"
function Count(n: number) {
    return n.toString();
}
Count.value = 1;
Count.next = function(n: number) {
    return n + 1;
};
var total = Count.value + Count.next(2) + Count(3).length;

const Make = function(name: string) {
    return name;
};
Make.info = { size: 1 };
Make.info = { label: "" };
Make.bump = function(value: number) {
    return value + 1;
};
var score = (Make.info.size || 0) + Make.bump(1) + Make("x").length;

class Box {
    n = 1;
}
Box.extra = 2;
Box.pick = function(n: number) {
    return n;
};
var classTotal = Box.extra + Box.pick(1) + new Box().n;

var Expr = class {
    n = 1;
};
Expr.extra = 2;
Expr.pick = function(n: number) {
    return n;
};
var exprTotal = Expr.extra + Expr.pick(1) + new Expr().n;
"#,
    );

    assert!(
        output.contains("declare var total: number;"),
        "Expected expando arithmetic initializer to use member/call facts: {output}"
    );
    assert!(
        output.contains("declare var score: number;"),
        "Expected nested object expando arithmetic initializer to use union member facts: {output}"
    );
    assert!(
        output.contains("declare var classTotal: number;"),
        "Expected class expando arithmetic initializer to use instance member facts: {output}"
    );
    assert!(
        output.contains("declare var exprTotal: number;"),
        "Expected class-expression expando arithmetic initializer to use instance member facts: {output}"
    );
}

#[test]
fn repeated_expando_arithmetic_var_declarations_keep_number_facts() {
    let output = emit_dts_with_usage_analysis(
        r#"
function Decl(n: number) {
    return n.toString();
}
Decl.prop = 2;
Decl.m = function(n: number) {
    return n + 1;
};
var n = Decl.prop + Decl.m(12) + Decl(101).length;

const Expr = function(n: number) {
    return n.toString();
};
Expr.prop = { x: 2 };
Expr.prop = { y: "" };
Expr.m = function(n: number) {
    return n + 1;
};
var n = (Expr.prop.x || 0) + Expr.m(12) + Expr(101).length;

function Merge(n: number) {
    return n * 100;
}
Merge.p1 = 111;
namespace Merge {
    export var p2 = 222;
}
namespace Merge {
    export var p3 = 333;
}
var n = Merge.p1 + Merge.p2 + Merge.p3 + Merge(1);
var merged = Merge.p1 + Merge.p2 + Merge.p3 + Merge(1);
var fromP1 = Merge.p1;
var fromP1P2 = Merge.p1 + Merge.p2;
var fromP1P2P3 = Merge.p1 + Merge.p2 + Merge.p3;
var fromP1Call = Merge.p1 + Merge(1);
var fromCall = Merge(1);
var fromNs = Merge.p2 + Merge.p3;
"#,
    );

    assert!(
        output.matches("declare var n: number;").count() >= 3,
        "Expected repeated expando arithmetic declarations to keep number facts: {output}"
    );
    assert!(
        output.contains("declare var merged: number;"),
        "Expected namespace-merged arithmetic declarations to keep number facts: {output}"
    );
    assert!(
        output.contains("declare var fromP1: number;"),
        "Expected late-bound namespace members to keep value facts: {output}"
    );
    assert!(
        output.contains("declare var fromP1P2: number;"),
        "Expected late-bound and namespace members to compose in arithmetic: {output}"
    );
    assert!(
        output.contains("declare var fromP1P2P3: number;"),
        "Expected three namespace members to compose in arithmetic: {output}"
    );
    assert!(
        output.contains("declare var fromP1Call: number;"),
        "Expected late-bound members and direct calls to compose in arithmetic: {output}"
    );
    assert!(
        output.contains("declare var fromCall: number;"),
        "Expected namespace-merged direct calls to keep return facts: {output}"
    );
    assert!(
        output.contains("declare var fromNs: number;"),
        "Expected namespace exports to keep value facts: {output}"
    );
}

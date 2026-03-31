#[cfg(test)]
mod test_utils {
    pub fn check_and_collect(source: &str, error_code: u32) -> Vec<(u32, String)> {
        crate::test_utils::check_source_diagnostics(source)
            .iter()
            .filter(|d| d.code == error_code)
            .map(|d| (d.start, d.message_text.clone()))
            .collect()
    }
}

#[cfg(test)]
mod ts2481_tests {
    use super::test_utils::check_and_collect;

    #[test]
    fn var_in_for_of_with_let() {
        let source = "for (let v of []) {\n    var v = 0;\n}";
        let errors = check_and_collect(source, 2481);
        assert_eq!(errors.len(), 1, "Expected 1 TS2481: {errors:?}");
        assert!(errors[0].1.contains("'v'"));
    }

    #[test]
    fn var_in_for_of_without_initializer() {
        let source = "for (let v of []) {\n    var v;\n}";
        let errors = check_and_collect(source, 2481);
        assert_eq!(errors.len(), 1, "Expected 1 TS2481: {errors:?}");
    }

    #[test]
    fn var_in_nested_block_with_let() {
        let source = "{\n    let x;\n    {\n        var x = 1;\n    }\n}";
        let errors = check_and_collect(source, 2481);
        assert_eq!(errors.len(), 1, "Expected 1 TS2481: {errors:?}");
    }

    #[test]
    fn var_in_for_in_with_let() {
        let source = "function test() {\n    for (let v in {}) { var v; }\n}";
        let errors = check_and_collect(source, 2481);
        assert_eq!(errors.len(), 1, "Expected 1 TS2481: {errors:?}");
    }

    #[test]
    fn var_in_for_with_let() {
        let source = "function test() {\n    for (let v; ; ) { var v; }\n}";
        let errors = check_and_collect(source, 2481);
        assert_eq!(errors.len(), 1, "Expected 1 TS2481: {errors:?}");
    }

    #[test]
    fn no_error_when_names_share_function_scope() {
        // function f() { let x; var x; } — no TS2481 (names share function scope)
        let source = "function f() {\n    let x = 1;\n    var x = 2;\n}";
        let errors = check_and_collect(source, 2481);
        assert_eq!(
            errors.len(),
            0,
            "No TS2481 when names share function scope: {errors:?}"
        );
    }

    #[test]
    fn no_error_when_var_in_child_block_of_function() {
        // function f() { let x; { var x; } } — no TS2481 (names share function scope)
        // tsc emits TS2451, not TS2481, for this case
        let source = "function f() {\n    let x;\n    {\n        var x;\n    }\n}";
        let errors = check_and_collect(source, 2481);
        assert_eq!(
            errors.len(),
            0,
            "No TS2481 when var in child block of function: {errors:?}"
        );
    }

    #[test]
    fn no_error_for_let_only() {
        let source = "{\n    let x;\n    {\n        let x;\n    }\n}";
        let errors = check_and_collect(source, 2481);
        assert_eq!(errors.len(), 0, "No TS2481 for let-to-let: {errors:?}");
    }

    #[test]
    fn deeply_nested_var() {
        let source = "{\n    let x;\n    {\n        {\n            var x = 1;\n        }\n    }\n}";
        let errors = check_and_collect(source, 2481);
        assert_eq!(
            errors.len(),
            1,
            "Expected 1 TS2481 for deeply nested var: {errors:?}"
        );
    }
}

#[cfg(test)]
mod ts2481_same_block_tests {
    use super::test_utils::check_and_collect;
    use crate::test_utils::check_source_diagnostics;

    #[test]
    fn const_and_var_same_block_emits_ts2481() {
        // { const x = 0; var x = ""; } → TS2481 only (not TS2451 or TS2403)
        let source = "{\n    const x = 0;\n    var x = \"\";\n}";
        let ts2481 = check_and_collect(source, 2481);
        assert_eq!(ts2481.len(), 1, "Expected 1 TS2481: {ts2481:?}");
        assert!(ts2481[0].1.contains("'x'"));
    }

    #[test]
    fn let_and_var_same_block_emits_ts2481() {
        // { let x; var x = 1; } → TS2481 only
        let source = "{\n    let x;\n    var x = 1;\n}";
        let ts2481 = check_and_collect(source, 2481);
        assert_eq!(ts2481.len(), 1, "Expected 1 TS2481: {ts2481:?}");
    }

    #[test]
    fn same_block_no_false_ts2451() {
        // { const x = 0; var x = ""; } → no TS2451
        let source = "{\n    const x = 0;\n    var x = \"\";\n}";
        let ts2451 = check_and_collect(source, 2451);
        assert_eq!(
            ts2451.len(),
            0,
            "No TS2451 for same-block shadow: {ts2451:?}"
        );
    }

    #[test]
    fn same_block_no_false_ts2403() {
        // { const x = 0; var x = ""; } → no TS2403
        let source = "{\n    const x = 0;\n    var x = \"\";\n}";
        let ts2403 = check_and_collect(source, 2403);
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 for same-block shadow: {ts2403:?}"
        );
    }

    #[test]
    fn multiple_blocks_each_get_ts2481() {
        let source = r#"
var x: string;
{
    const x = 0;
    var x = "";
}
function f() {
    {
        let y = 1;
        {
            var y: string = "";
        }
    }
}
{
    let z = 1;
    var z = 2;
}
"#;
        let ts2481 = check_and_collect(source, 2481);
        assert_eq!(ts2481.len(), 3, "Expected 3 TS2481 errors: {ts2481:?}");
    }

    #[test]
    fn function_scope_still_emits_ts2451_not_ts2481() {
        // function f() { let x; var x; } → TS2451, not TS2481
        let source = "function f() {\n    let x = 1;\n    var x = 2;\n}";
        let ts2481 = check_and_collect(source, 2481);
        assert_eq!(
            ts2481.len(),
            0,
            "No TS2481 for function-scope shared: {ts2481:?}"
        );
        let ts2451 = check_and_collect(source, 2451);
        assert_eq!(
            ts2451.len(),
            2,
            "Expected 2 TS2451 for function-scope: {ts2451:?}"
        );
    }

    #[test]
    fn only_ts2481_errors_present() {
        // Ensure ONLY TS2481 is emitted for this case, nothing else
        let source = "{\n    const x = 0;\n    var x = \"\";\n}";
        let all_diags = check_source_diagnostics(source);
        let relevant: Vec<_> = all_diags
            .iter()
            .filter(|d| d.code == 2451 || d.code == 2403 || d.code == 2481)
            .collect();
        assert_eq!(
            relevant.len(),
            1,
            "Expected exactly 1 diagnostic (TS2481): {relevant:?}"
        );
        assert_eq!(relevant[0].code, 2481);
    }
}

#[cfg(test)]
mod ts2397_tests {
    use super::test_utils::check_and_collect;

    #[test]
    fn var_undefined_emits_ts2397() {
        let errors = check_and_collect("var undefined = null;", 2397);
        assert_eq!(errors.len(), 1, "Expected 1 TS2397: {errors:?}");
        assert!(errors[0].1.contains("'undefined'"));
    }

    #[test]
    fn var_global_this_emits_ts2397() {
        let errors = check_and_collect("var globalThis;", 2397);
        assert_eq!(errors.len(), 1, "Expected 1 TS2397: {errors:?}");
        assert!(errors[0].1.contains("'globalThis'"));
    }

    #[test]
    fn let_undefined_emits_ts2397() {
        let errors = check_and_collect("let undefined = 1;", 2397);
        assert_eq!(errors.len(), 1, "Expected 1 TS2397: {errors:?}");
    }

    #[test]
    fn namespace_global_this_emits_ts2397() {
        let errors = check_and_collect("namespace globalThis { export function foo() {} }", 2397);
        assert_eq!(errors.len(), 1, "Expected 1 TS2397: {errors:?}");
        assert!(errors[0].1.contains("'globalThis'"));
    }

    #[test]
    fn normal_var_no_ts2397() {
        let errors = check_and_collect("var x = 1;", 2397);
        assert_eq!(errors.len(), 0, "No TS2397 for normal var: {errors:?}");
    }

    #[test]
    fn const_undefined_emits_ts2397() {
        let errors = check_and_collect("const undefined = void 0;", 2397);
        assert_eq!(errors.len(), 1, "Expected 1 TS2397: {errors:?}");
    }
}

#[cfg(test)]
mod ts2403_false_positive_tests {
    use crate::test_utils::check_source_diagnostics;

    #[test]
    fn recursive_types_with_typeof_no_false_ts2403() {
        // From recursiveTypesWithTypeof.ts
        let source = r#"
var c: typeof c;
var c: any;
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 expected for circular typeof: {ts2403:?}"
        );
    }

    #[test]
    fn var_redecl_with_interface_no_false_ts2403() {
        // From TwoInternalModulesWithTheSameNameAndSameCommonRoot.ts (part3)
        let source = r#"
interface Point { x: number; y: number; }
var o: { x: number; y: number };
var o: Point;
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 expected for structurally identical types: {ts2403:?}"
        );
    }

    #[test]
    fn var_redecl_with_unknown_initializer_emits_ts2403() {
        let source = r#"
declare const u: unknown;
var x: number | string;
var x = u;
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            1,
            "Expected TS2403 for unknown redeclaration: {ts2403:?}"
        );
    }

    #[test]
    fn typeof_module_no_false_ts2403() {
        // From nonInstantiatedModule.ts
        let source = r#"
namespace M {
    export var a = 1;
}
var m: typeof M;
var m = M;
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 expected for typeof module: {ts2403:?}"
        );
    }

    #[test]
    fn optional_tuple_elements_no_false_ts2403() {
        // From optionalTupleElementsAndUndefined.ts
        let source = r#"
var v: [1, 2?];
var v: [1, (2 | undefined)?];
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 expected for optional tuple elements: {ts2403:?}"
        );
    }

    #[test]
    fn type_alias_application_no_false_ts2403() {
        // Type alias applications should be compatible with their evaluated forms.
        // Uses simple alias (no lib types needed) to test that Application types
        // are evaluated before TS2403 comparison.
        let source = r#"
type Pair<A, B> = [A, B];
var v: [1, 2];
var v: Pair<1, 2>;
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 expected for type alias application: {ts2403:?}"
        );
    }

    #[test]
    fn identity_mapped_type_no_false_ts2403() {
        // Mapped type application should evaluate to same structure.
        // Uses inline identity mapped type (no lib dependency).
        let source = r#"
type Id<T> = { [K in keyof T]: T[K] };
var v: { x: number };
var v: Id<{ x: number }>;
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 expected for identity mapped type: {ts2403:?}"
        );
    }

    #[test]
    fn typeof_var_redecl_no_false_ts2403() {
        // From typeofANonExportedType.ts
        let source = r#"
interface I { foo: string; }
var i: I;
var i2: I;
var r5: typeof i;
var r5: typeof i2;
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 expected for typeof var redecl: {ts2403:?}"
        );
    }

    #[test]
    fn namespace_merged_var_no_false_ts2403() {
        // From TwoInternalModulesThatMergeEachWithExportedAndNonExportedLocalVarsOfTheSameName
        let source = r#"
namespace A {
    export interface Point { x: number; y: number; }
    export var Origin: Point = { x: 0, y: 0 };
}
namespace A {
    var Origin: string = "0,0";
}
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 expected for merged namespace vars: {ts2403:?}"
        );
    }

    #[test]
    fn namespace_merged_var_redecl_no_false_ts2403() {
        // From TwoInternalModulesWithTheSameNameAndSameCommonRoot.ts (part3 vars)
        let source = r#"
namespace A {
    export interface Point { x: number; y: number; }
    export var Origin: Point = { x: 0, y: 0 };
}
var o: { x: number; y: number };
var o: A.Point;
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 for interface/object-literal redecl: {ts2403:?}"
        );
    }

    #[test]
    fn non_instantiated_module_redecl_no_false_ts2403() {
        // From nonInstantiatedModule.ts
        let source = r#"
namespace M {
    export var a = 1;
}
var a1: number;
var a1 = M.a;
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 for module property access: {ts2403:?}"
        );
    }

    #[test]
    fn typeof_namespace_vs_object_literal_no_false_ts2403() {
        // From nonInstantiatedModule.ts:
        // `var p2: { Origin() : { x: number; y: number; } };`
        // `var p2: typeof M2.Point;`
        // The namespace Lazy(DefId) type must be resolved to its structural Object
        // form for the bidirectional subtype check to succeed.
        let source = r#"
namespace NS {
    export function foo(): number { return 1; }
}
var x: { foo(): number };
var x: typeof NS;
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 for typeof namespace vs structurally equivalent object: {ts2403:?}"
        );
    }

    #[test]
    fn typeof_namespace_member_vs_object_literal_no_false_ts2403() {
        // When `typeof NS.Sub` resolves to a namespace member that is structurally
        // an object, it should be compatible with the equivalent object literal type.
        let source = r#"
namespace M2 {
    export namespace Point {
        export function Origin(): { x: number; y: number } { return { x: 0, y: 0 }; }
    }
}
var p2: { Origin(): { x: number; y: number } };
var p2: typeof M2.Point;
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 for typeof namespace member vs object literal: {ts2403:?}"
        );
    }

    #[test]
    fn enum_var_redecl_emits_ts2403() {
        // From duplicateLocalVariable4.ts: var x = E; var x = E.a;
        // First x is `typeof E`, second x is `E` — types differ, should emit TS2403.
        let source = r#"
enum E { a }
var x = E;
var x = E.a;
"#;
        let all_diags = check_source_diagnostics(source);
        let ts2403 = all_diags
            .iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(ts2403.len(), 1, "Expected 1 TS2403 for enum var redecl");
    }

    #[test]
    fn for_loop_var_redecl_emits_ts2403() {
        // var declarations in for-loop initializers should trigger TS2403
        // when re-declared with incompatible types.
        let source = r#"
for(var a: any;;) break;
for(var a: number;;) break;
for(var a: string;;) break;
"#;
        let all_diags = check_source_diagnostics(source);
        let ts2403 = all_diags
            .iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            2,
            "Expected 2 TS2403 for for-loop var redecls: {ts2403:?}"
        );
    }

    #[test]
    fn for_loop_var_redecl_with_initializer_emits_ts2403() {
        // var with initializer in for-loop also triggers TS2403
        let source = r#"
for(var a: any;;) break;
for(var a = 1;;) break;
"#;
        let all_diags = check_source_diagnostics(source);
        let ts2403 = all_diags
            .iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            1,
            "Expected 1 TS2403 for for-loop var with initializer: {ts2403:?}"
        );
    }

    #[test]
    fn for_loop_var_compatible_no_ts2403() {
        // Same type should NOT trigger TS2403
        let source = r#"
for(var a: number;;) break;
for(var a: number;;) break;
"#;
        let all_diags = check_source_diagnostics(source);
        let ts2403 = all_diags
            .iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 for compatible for-loop var redecls: {ts2403:?}"
        );
    }

    #[test]
    fn nested_for_loop_var_emits_ts2403() {
        // var in nested block scopes (if inside for) still hoists to function scope
        let source = r#"
var a: string;
if (true) {
    for(var a: number;;) break;
}
"#;
        let all_diags = check_source_diagnostics(source);
        let ts2403 = all_diags
            .iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            1,
            "Expected 1 TS2403 for nested for-loop var: {ts2403:?}"
        );
    }

    #[test]
    fn for_loop_let_no_cross_scope_ts2403() {
        // let declarations in for-loops are block-scoped, should NOT trigger TS2403
        let source = r#"
for(let a: any;;) break;
for(let a: number;;) break;
"#;
        let all_diags = check_source_diagnostics(source);
        let ts2403 = all_diags
            .iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 for let in separate for-loops: {ts2403:?}"
        );
    }

    #[test]
    fn module_scoped_declare_var_no_ts2403_against_lib() {
        // In module files (with import/export), `declare var` is module-scoped
        // and should NOT trigger TS2403 against lib globals.
        let source = r#"
export const x = 1;
declare var console: { log(msg?: any): void; };
console.log("test");
"#;
        let all_diags = check_source_diagnostics(source);
        let ts2403 = all_diags
            .iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 for module-scoped declare var vs lib global: {ts2403:?}"
        );
    }
}

#[cfg(test)]
mod fundule_ts2403_tests {
    use crate::test_utils::check_source_diagnostics;

    #[test]
    fn fundule_redecl_emits_ts2403() {
        // When a function+namespace merge (fundule) is assigned to a var
        // that was previously declared with just the function signature type,
        // TS2403 should fire because `typeof Point` includes namespace exports
        // that `() => { x: number; y: number }` does not have.
        let source = r#"
namespace B {
    export function Point() {
        return { x: 0, y: 0 };
    }
    export namespace Point {
        export var Origin = { x: 0, y: 0 };
    }
}
var fn2: () => { x: number; y: number };
var fn2 = B.Point;
"#;
        let all_diags = check_source_diagnostics(source);
        let ts2403 = all_diags
            .iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            1,
            "Expected 1 TS2403 for fundule redecl (typeof Point != () => {{ ... }}): {ts2403:?}\nAll diags: {all_diags:?}"
        );
    }

    #[test]
    fn fundule_compatible_redecl_no_ts2403() {
        // When the function type matches (no namespace exports), no TS2403.
        let source = r#"
function Point() {
    return { x: 0, y: 0 };
}
var fn2: () => { x: number; y: number };
var fn2 = Point;
"#;
        let all_diags = check_source_diagnostics(source);
        let ts2403 = all_diags
            .iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 for compatible function redecl: {ts2403:?}"
        );
    }
}

#[cfg(test)]
mod mapped_type_validation_tests {
    use crate::test_utils::check_source_diagnostics;

    #[test]
    fn mapped_template_invalid_key_index_reports_ts2536() {
        let source = r#"
type Foo2<T, F extends keyof T> = {
    pf: { [P in F]?: T[P] },
    pt: { [P in T]?: T[P] },
};
"#;

        let ts2536 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2536)
            .collect::<Vec<_>>();
        assert_eq!(ts2536.len(), 1, "Expected one TS2536 from pt: {ts2536:?}");
        assert!(
            ts2536[0]
                .message_text
                .contains("Type 'P' cannot be used to index type 'T'"),
            "TS2536 message mismatch: {ts2536:?}"
        );
    }

    #[test]
    fn mapped_property_access_with_generic_key_reports_ts2339() {
        let source = r#"
function test<T, K extends keyof T>(obj: Pick<T, K>) {
    let value = obj.foo;
}
"#;
        let ts2339 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2339)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2339.len(),
            1,
            "Expected one TS2339 on Pick property access: {ts2339:?}"
        );
        assert!(
            ts2339[0].message_text.contains("foo"),
            "TS2339 message should reference foo: {ts2339:?}"
        );
    }

    /// When a mapped type iterates over `keyof T` with T unconstrained, any
    /// specific property access should report TS2339 because the key set is
    /// not known.  This test uses an inline mapped type (no stdlib Pick needed).
    #[test]
    fn inline_mapped_type_unconstrained_keyof_reports_ts2339() {
        // { [K in keyof T]: T[K] } is equivalent to identity-mapped T.
        // With T unconstrained, accessing `.foo` should be an error.
        let source = r#"
function test<T>(obj: { [K in keyof T]: T[K] }) {
    let value = obj.foo;
}
"#;
        let ts2339 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2339)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2339.len(),
            1,
            "Expected TS2339 for .foo on mapped type with unconstrained keyof T: {ts2339:?}"
        );
        assert!(
            ts2339[0].message_text.contains("foo"),
            "TS2339 message should reference 'foo': {ts2339:?}"
        );
    }

    #[test]
    fn mapped_type_index_access_constraint_exceeds_keyof_reports_ts2322() {
        // When a mapped type constraint is an indexed access like AB[S] and S's
        // constraint exceeds keyof AB, tsc emits TS2322 for the mapped type constraint.
        // This tests that evaluation doesn't mask the issue by eagerly resolving AB[S].
        let source = r#"
type AB = {
    a: 'a'
    b: 'a'
};
type T5<S extends 'a'|'b'|'extra'> = {[key in AB[S]]: true}[S];
"#;

        let ts2322 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2322)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2322.len(),
            1,
            "Expected one TS2322 for AB[S] not assignable to string | number | symbol: {ts2322:?}"
        );
        assert!(
            ts2322[0]
                .message_text
                .contains("is not assignable to type 'string | number | symbol'"),
            "TS2322 message mismatch: {ts2322:?}"
        );
    }

    #[test]
    fn mapped_type_index_access_valid_constraint_no_ts2322() {
        // When S's constraint is within keyof AB, no TS2322 should be emitted.
        let source = r#"
type AB = {
    a: 'a'
    b: 'a'
};
type T7<S extends 'a'|'b', L extends 'a'> = {[key in AB[S]]: true}[L];
"#;

        let ts2322 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2322)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2322.len(),
            0,
            "Expected no TS2322 when index constraint is within keyof: {ts2322:?}"
        );
    }
}

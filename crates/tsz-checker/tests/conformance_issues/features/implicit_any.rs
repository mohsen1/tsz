use crate::core::*;

#[test]
fn test_ts7022_not_emitted_for_destructured_parameter_with_concrete_default_source() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function foo({
    x = 1,
    y = x
}) {}
        "#,
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 7022),
        "Did not expect TS7022 when a sibling default reads a binding with its own concrete initializer.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7022 should NOT fire when noImplicitAny is off (like all 7xxx diagnostics).
#[test]
fn test_ts7022_not_emitted_without_no_implicit_any() {
    let opts = CheckerOptions {
        no_implicit_any: false,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
var a = { f: a };
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7022),
        "Should NOT emit TS7022 when noImplicitAny is off.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7022 should NOT fire when the self-reference is in a function body (deferred context).
/// From: declFileTypeofFunction.ts
#[test]
fn test_ts7022_not_emitted_for_function_body_reference() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
var foo3 = function () {
    return foo3;
}
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7022),
        "Should NOT emit TS7022 for self-reference in function body (deferred context).\nActual errors: {diagnostics:#?}"
    );
}

/// TS7022 should NOT fire for class expression initializers with method body references.
/// From: classExpression4.ts
#[test]
fn test_ts7022_not_emitted_for_class_expression_body_reference() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
let C = class {
    foo() {
        return new C();
    }
};
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7022),
        "Should NOT emit TS7022 for self-reference in class method body (deferred context).\nActual errors: {diagnostics:#?}"
    );
}

/// TS7022 should NOT fire for arrow function body self-references.
/// From: simpleRecursionWithBaseCase3.ts
#[test]
fn test_ts7022_not_emitted_for_arrow_body_reference() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
const fn1 = () => {
  if (Math.random() > 0.5) {
    return fn1()
  }
  return 0
}
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7022),
        "Should NOT emit TS7022 for self-reference in arrow function body (deferred context).\nActual errors: {diagnostics:#?}"
    );
}

// TS7023: Function implicitly has return type 'any' because it does not have a return
// type annotation and is referenced directly or indirectly in one of its return expressions.

/// TS7023 should fire for function expression variables that call themselves in return.
/// From: implicitAnyFromCircularInference.ts
#[test]
fn test_ts7023_function_expression_self_call() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
var f1 = function () {
    return f1();
};
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7023),
        "Should emit TS7023 for function expression self-call.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 7022),
        "Should NOT emit TS7022 for function expression (deferred context).\nActual errors: {diagnostics:#?}"
    );
}

/// TS7023 should fire for arrow function variables that call themselves in return.
/// From: implicitAnyFromCircularInference.ts
#[test]
fn test_ts7023_arrow_function_self_call() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
var f2 = () => f2();
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7023),
        "Should emit TS7023 for arrow function self-call.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7023 should NOT fire when the recursive function has a non-recursive base case.
/// tsc infers the return type from the base case (`return 0` → `number`), ignoring
/// the circular self-reference. From: simpleRecursionWithBaseCase3.ts
#[test]
fn test_ts7023_not_emitted_with_base_case() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
const fn1 = () => {
  if (Math.random() > 0.5) {
    return fn1()
  }
  return 0
}
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7023),
        "Should NOT emit TS7023 when recursive function has a base case.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7023_emitted_for_function_declaration_wrapped_self_call() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        strict: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function fn5() {
    return [fn5][0]();
}
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7023),
        "Should emit TS7023 when a function declaration calls itself through an immediate wrapper in a return expression.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7023_not_emitted_for_direct_function_declaration_self_call() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        strict: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function fn2(n: number) {
    return fn2(n);
}
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7023),
        "Should NOT emit TS7023 for a direct self-call in a function declaration.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7022_and_ts7024_emitted_for_nested_callback_circular_return() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        strict: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
declare function fn1<T>(cb: () => T): string;
const res1 = fn1(() => res1);
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7022),
        "Should emit TS7022 for callback-driven circular initializer inference.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 7024),
        "Should emit TS7024 for the anonymous callback return circularity.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7022_and_ts7023_emitted_for_object_property_callback_circular_return() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        strict: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
declare const box: <T>(input: { fields: () => T }) => T;
const value = box({
    fields: () => value,
});
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7022),
        "Should emit TS7022 when a contextual callback return reads the variable being inferred.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 7023),
        "Should emit TS7023 on the named property callback.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7022_not_emitted_for_stored_arrow_property_returning_self() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        strict: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
const value = {
    fields: () => value,
};
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7022),
        "Should NOT emit TS7022 for a stored deferred callback.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 7023),
        "Should NOT emit TS7023 for a stored deferred callback.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 7024),
        "Should NOT emit TS7024 for a stored deferred callback.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7023 should NOT fire when noImplicitAny is off.
#[test]
fn test_ts7023_not_emitted_without_no_implicit_any() {
    let opts = CheckerOptions {
        no_implicit_any: false,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
var f1 = function () {
    return f1();
};
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7023),
        "Should NOT emit TS7023 when noImplicitAny is off.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7023_object_literal_method_this_property_uses_inferred_method_type() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        strict: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_raw_diagnostics_named(
        "test.ts",
        r#"
var obj = {
    f() {
        return this.spaaace;
    }
};
"#,
        opts,
    );

    assert!(
        diagnostics.iter().any(|diag| diag.code == 7023),
        "Should emit TS7023 for object literal methods whose return expressions read `this` through the under-construction object type.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| { diag.code == 2339 && diag.message_text.contains("{ f(): any; }") }),
        "Expected the `this` property-access error to see the inferred `any` return type for the method.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7023_object_literal_computed_name_this_reference_keeps_inferred_return_shape() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        strict: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_raw_diagnostics_named(
        "test.ts",
        r#"
export const thing = {
    doit() {
        return {
            [this.a]: "",
        }
    }
};
"#,
        opts,
    );

    assert!(
        diagnostics.iter().any(|diag| diag.code == 7023),
        "Should emit TS7023 for object literal methods whose computed return shapes reference `this` while the object is still being inferred.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 2339
                && diag
                    .message_text
                    .contains("{ doit(): { [x: number]: string; }; }")
        }),
        "Expected the `this` property-access error to retain the inferred return shape for `doit`.\nActual diagnostics: {diagnostics:#?}"
    );
}

// TS2487: The left-hand side of a 'for...of' statement must be a variable or a property access.
// From: for-of3.ts

/// `for (v++ of [])` should emit TS2487 because `v++` is not a valid assignment target.
#[test]
fn test_ts2487_invalid_for_of_lhs() {
    let diagnostics = compile_and_get_diagnostics(
        r"
var v: any;
for (v++ of []) { }
        ",
    );
    assert!(
        has_error(&diagnostics, 2487),
        "Should emit TS2487 for invalid for-of LHS.\nActual errors: {diagnostics:#?}"
    );
}

/// Valid for-of LHS patterns should NOT emit TS2487.
#[test]
fn test_ts2487_valid_for_of_lhs() {
    let diagnostics = compile_and_get_diagnostics(
        r"
var v: any;
var arr: any[] = [];
for (v of arr) { }
        ",
    );
    assert!(
        !has_error(&diagnostics, 2487),
        "Should NOT emit TS2487 for valid for-of LHS.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7022_and_ts7023_emitted_for_for_of_iterator_method_self_reference() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
declare const Symbol: { readonly iterator: unique symbol };
class MyIterator {
    [Symbol.iterator]() {
        return v;
    }
}

for (var v of new MyIterator()) {}
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7022),
        "Should emit TS7022 for a for-of iterator method that returns the loop variable.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 7023),
        "Should emit TS7023 for the named iterator method.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7022_and_ts7023_emitted_for_for_of_next_value_self_reference() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
declare const Symbol: { readonly iterator: unique symbol };
class MyIterator {
    next() {
        return {
            done: true,
            value: v,
        };
    }

    [Symbol.iterator]() {
        return this;
    }
}

for (var v of new MyIterator()) {}
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7022),
        "Should emit TS7022 when next().value reads the loop variable.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 7023),
        "Should emit TS7023 for next() when its return expression is circular.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2448_and_ts7022_emitted_for_for_of_header_shadowing_self_reference() {
    let opts = CheckerOptions {
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
let v = [1];
for (let v of v) {
    v;
}
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 2448),
        "Should emit TS2448 when a for-of header expression reads the loop binding in its own TDZ.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 7022),
        "Should emit TS7022 when a for-of header expression circularly infers the loop binding.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7022_not_emitted_for_type_only_reference_inside_initializer() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
namespace Translation {
    export type TranslationKeyEnum = 'translation1' | 'translation2';
    export const TranslationKeyEnum = {
        Translation1: 'translation1' as TranslationKeyEnum,
        Translation2: 'translation2' as TranslationKeyEnum,
    };
}
        ",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 7022),
        "Should NOT emit TS7022 when an initializer only mentions the symbol name in type position.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7022 should NOT fire when a variable references a namespace/enum import-equals alias
/// with the same name. The initializer name-match is not a real circularity because the
/// symbol resolves to a different entity (the imported alias).
/// From: declarationEmitEnumReferenceViaImportEquals.ts
#[test]
fn test_ts7022_not_emitted_for_namespace_enum_import_equals_same_name() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
namespace Translation {
    export type TranslationKeyEnum = 'translation1' | 'translation2';
    export const TranslationKeyEnum = {
        Translation1: 'translation1' as TranslationKeyEnum,
        Translation2: 'translation2' as TranslationKeyEnum,
    };
}
import TranslationKeyEnum = Translation.TranslationKeyEnum;
const x = TranslationKeyEnum;
        ",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 7022),
        "Should NOT emit TS7022 for import-equals alias with same name as variable.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7022 should NOT fire when a `var` redeclaration references the already-established
/// type from a prior declaration with a type annotation. E.g.:
///   var o: { x: number; y: number };
///   var o = A.Utils.mirror(o);
/// The second `var o` is NOT circular because `o` already has a concrete type.
/// From: TwoInternalModulesWithTheSameNameAndSameCommonRoot.ts
#[test]
fn test_ts7022_not_emitted_for_var_redeclaration_with_prior_type() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function mirror(p: { x: number; y: number }): { x: number; y: number } {
    return { x: p.y, y: p.x };
}
var o: { x: number; y: number };
var o = mirror(o);
        ",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 7022),
        "Should NOT emit TS7022 for var redeclaration when prior declaration has a type.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7022 should NOT fire when a `var` has a `typeof` type annotation and a subsequent
/// redeclaration assigns from itself. The type is established by the first annotation.
/// From: recursiveTypesWithTypeof.ts
#[test]
fn test_ts7022_not_emitted_for_typeof_annotated_var_reassignment() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
var h: () => typeof h;
var h = h();
        ",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 7022),
        "Should NOT emit TS7022 for typeof-annotated var with self-assignment.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7022 should NOT fire for generic function type assertions. The name `x` in
/// `<T>(x: T) => { x }` is a parameter, not a reference to the outer variable.
/// From: typeAssertionToGenericFunctionType.ts
#[test]
fn test_ts7022_not_emitted_for_generic_function_type_assertion() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
var x = {
    a: < <T>(x: T) => T > ((x: any) => 1),
    b: <T>(x: T) => { x }
};
        ",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 7022),
        "Should NOT emit TS7022 when inner `x` is a parameter, not the outer variable.\nActual errors: {diagnostics:#?}"
    );
}

// TS1360: `satisfies` with `as const` should accept readonly-to-mutable arrays.
// From: typeSatisfaction_asConstArrays.ts

/// tsc 6.0 accepts `[1,2,3] as const satisfies unknown[]` because `satisfies`
/// checks structural shape, not mutability constraints. The readonly modifier
/// from `as const` should not cause a TS1360 failure.
#[test]
fn test_ts1360_not_emitted_for_as_const_satisfies_mutable_array() {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
const arr1 = [1, 2, 3] as const satisfies readonly unknown[]
const arr2 = [1, 2, 3] as const satisfies unknown[]
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 1360),
        "Should NOT emit TS1360 for `as const satisfies` readonly-to-mutable.\nActual errors: {diagnostics:#?}"
    );
}

// TS7034: Variable implicitly has type 'any' in some locations where its type cannot be determined.

/// TS7034 should fire for variables without type annotation that are captured by nested functions.
/// From: implicitAnyDeclareVariablesWithoutTypeAndInit.ts
#[test]
fn test_ts7034_captured_variable_in_nested_function() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
var y;
function func(k: any) { y };
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7034),
        "Should emit TS7034 for variable captured by nested function.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7034 should NOT fire for variables used only at the same scope level.
#[test]
fn test_ts7034_not_emitted_for_same_scope_usage() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
var x;
function func(k: any) {};
func(x);
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7034),
        "Should NOT emit TS7034 for variable used at same scope level.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7034_emitted_for_evolving_array_same_scope_read() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f() {
    let x = [];
    let y = x;
}
        ",
        opts,
    );
    let ts7005_count = diagnostics.iter().filter(|(code, _)| *code == 7005).count();

    assert!(
        has_error(&diagnostics, 7034),
        "Should emit TS7034 for evolving array same-scope read.\nActual errors: {diagnostics:#?}"
    );
    assert_eq!(
        ts7005_count, 1,
        "Should emit exactly one TS7005 at the unsafe evolving-array read.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7034_emitted_after_empty_array_assignment_before_read() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f() {
    let x;
    x = [];
    let y = x;
}
        ",
        opts,
    );
    let ts7005_count = diagnostics.iter().filter(|(code, _)| *code == 7005).count();

    assert!(
        has_error(&diagnostics, 7034),
        "Should emit TS7034 once an unannotated variable is read as an evolving array.\nActual errors: {diagnostics:#?}"
    );
    assert_eq!(
        ts7005_count, 1,
        "Should emit exactly one TS7005 at the unsafe read after `x = []`.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7034_evolving_array_same_scope_read_after_push_is_stable() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r"
function f() {
    let x = [];
    x.push(1);
    let y = x;
}
        ",
        opts,
    );

    assert!(
        !has_error(&diagnostics, 7034),
        "Should NOT emit TS7034 after same-scope array mutation stabilizes the element type.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 7005),
        "Should NOT emit TS7005 after same-scope array mutation stabilizes the element type.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7034_evolving_array_skips_length_and_push_sites() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        strict: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r"
let bar = [];
bar?.length;
bar.push('baz');
        ",
        opts,
    );

    assert!(
        !has_error(&diagnostics, 7034),
        "Should NOT emit TS7034 for `.length`/`push`-only evolving-array usage.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 7005),
        "Should NOT emit TS7005 for `.length`/`push`-only evolving-array usage.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7034_evolving_array_reports_element_read_not_length_probe() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        strict: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
let foo = [];
foo?.length;
foo[0];
        ",
        opts,
    );
    let ts7005_count = diagnostics.iter().filter(|(code, _)| *code == 7005).count();

    assert!(
        has_error(&diagnostics, 7034),
        "Should emit TS7034 once an evolving array is read through an element access.\nActual errors: {diagnostics:#?}"
    );
    assert_eq!(
        ts7005_count, 1,
        "Should emit TS7005 only for the element access, not the `.length` probe.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_control_flow_unannotated_loop_incrementor_reads_assignment_union() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        strict: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
{
    let iNext;
    for (let i = 0; i < 10; i = iNext) {
        if (i == 5) {
            iNext = "bad";
            continue;
        }
        iNext = i + 1;
    }
}
        "#,
        opts,
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322 for the incrementor read, got: {diagnostics:#?}"
    );
    assert!(
        ts2322[0].1.contains("string | number"),
        "Expected TS2322 about the incrementor type, got: {ts2322:#?}"
    );
}

#[test]
fn test_const_null_alias_equality_reports_null_not_number() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
const myNull: null = null;

function f(x: number | null) {
    if (x === myNull) {
        const s: string = x;
    }
}
        "#,
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one TS2322, got: {diagnostics:#?}"
    );
    assert!(
        ts2322[0]
            .1
            .contains("Type 'null' is not assignable to type 'string'"),
        "Expected null-based TS2322 after const-null alias narrowing, got: {ts2322:#?}"
    );
}

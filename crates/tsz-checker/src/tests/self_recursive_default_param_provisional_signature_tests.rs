//! Regression coverage for issue #16037: a `const`-bound self-recursive arrow
//! (or function expression) with an explicit return-type annotation lost that
//! annotation's benefit for in-body self-references as soon as ANY parameter
//! in its own parameter list had a default-value initializer instead of an
//! explicit type annotation — even when that parameter was unrelated to, and
//! unused by, the recursive call.
//!
//! Structural rule: `tsc` resolves an in-body self-reference of an annotated
//! function-like variable binding to the initializer's declared signature,
//! which is computable without analyzing the body — a default-valued
//! parameter's type comes from its own initializer expression, not from the
//! enclosing function's body or return type, so it needs no circular
//! resolution any more than an explicitly annotated parameter does. tsz's
//! `provisional_circular_variable_function_symbol_type`
//! (`state/type_analysis/symbol_type_helpers.rs`) already implements this
//! short-circuit for fully-annotated signatures; its gate,
//! `function_has_explicit_param_and_return_annotations`, required every
//! parameter to carry an explicit type annotation, so a signature with a
//! default-valued (but unannotated) parameter fell through to the general
//! cycle-breaking path, which caches the self-reference as `TypeId::ERROR`.
//! That `ERROR` result then defeats guard-based narrowing on the recursive
//! call's result downstream, so a sibling generic-callback inference site
//! finds no evidence for its type parameter and silently defaults it to
//! `unknown`, producing a false `TS2322` (superjson's `plainer.ts` canary).
//!
//! Owner layer: checker (`state/type_analysis/symbol_type_helpers.rs`),
//! `provisional_circular_variable_function_symbol_type`'s gate.

use crate::test_utils::check_source_diagnostics;

fn assert_clean(src: &str) {
    let diags = check_source_diagnostics(src);
    assert!(
        diags.is_empty(),
        "expected no diagnostics, got {diags:?} for:\n{src}"
    );
}

/// Shared fixture: a self-recursive arrow walker over a user-defined
/// recursive `Tree`/dict shape, narrowed through two type guards before a
/// nested generic callback consumes the narrowed member. `extra_param`
/// varies per case; `head`/`tail` let a caller swap the arrow for an
/// equivalent function-expression form.
fn walker_source_with(extra_param: &str, head: &str, tail: &str) -> String {
    format!(
        r#"
type Dict<T> = {{ [key: string]: T }};
type Tree<T> = InnerNode<T> | Leaf<T>;
type Leaf<T> = [T];
type InnerNode<T> = [T, Dict<Tree<T>>];
type MinimisedTree<T> = Tree<T> | Dict<Tree<T>> | undefined;

declare const isArray: (payload: any) => payload is any[];
declare const isPlainObject: (payload: any) => payload is {{ [key: string]: any }};
declare function forEach<T>(record: Dict<T>, run: (v: T, key: string) => void): void;
declare function transformValue(object: any): {{ value: any }} | undefined;

interface Result {{
  transformedValue: any;
  annotations?: MinimisedTree<string>;
}}

export const walker = {head}(
  object: any{extra_param}
): Result {tail} {{
  const transformationResult = transformValue(object);
  const transformed = transformationResult?.value ?? object;
  const innerAnnotations: Dict<Tree<string>> = {{}};

  forEach(transformed, (value: any, index: string) => {{
    const recursiveResult = walker(value);

    if (isArray(recursiveResult.annotations)) {{
      innerAnnotations[index] = recursiveResult.annotations;
    }} else if (isPlainObject(recursiveResult.annotations)) {{
      forEach(recursiveResult.annotations, (tree, key) => {{
        innerAnnotations[index + '.' + key] = tree;
      }});
    }}
  }});

  return {{ transformedValue: transformed, annotations: innerAnnotations }};
}};
"#,
        head = head,
        tail = tail,
        extra_param = extra_param,
    )
}

fn walker_source(extra_param: &str) -> String {
    walker_source_with(extra_param, "", "=>")
}

/// Baseline: no extra parameter at all. Must stay clean (pre-existing).
#[test]
fn self_recursive_arrow_no_extra_param_is_clean() {
    assert_clean(&walker_source(""));
}

/// The exact #16037 repro: an extra default-valued, unannotated `boolean`
/// parameter must not corrupt the recursive call's inferred type.
#[test]
fn self_recursive_arrow_with_unannotated_default_param_is_clean() {
    assert_clean(&walker_source(", flag = false"));
}

/// The parameter's default-inferred type is irrelevant to the trigger: an
/// object-typed default reproduces identically to a boolean one.
#[test]
fn self_recursive_arrow_with_unrelated_object_default_param_is_clean() {
    assert_clean(&walker_source(", seen = { marker: 1 }"));
}

/// An explicit type annotation on the extra parameter was already the
/// documented escape hatch; must remain clean (adjacent positive control).
#[test]
fn self_recursive_arrow_with_explicitly_annotated_extra_param_is_clean() {
    assert_clean(&walker_source(", flag: boolean = false"));
}

/// Renamed-binder control: a differently-named recursive variable and
/// differently-named default parameter must behave identically.
#[test]
fn self_recursive_arrow_renamed_binder_with_default_param_is_clean() {
    let src = r#"
type Dict<T> = { [key: string]: T };
type Tree<T> = InnerNode<T> | Leaf<T>;
type Leaf<T> = [T];
type InnerNode<T> = [T, Dict<Tree<T>>];
type MinimisedTree<T> = Tree<T> | Dict<Tree<T>> | undefined;

declare const isArray: (payload: any) => payload is any[];
declare const isPlainObject: (payload: any) => payload is { [key: string]: any };
declare function forEach<T>(record: Dict<T>, run: (v: T, key: string) => void): void;
declare function transformValue(object: any): { value: any } | undefined;

interface Outcome {
  transformedValue: any;
  annotations?: MinimisedTree<string>;
}

export const traverse = (
  payload: any,
  seenObjects = 0
): Outcome => {
  const transformed = transformValue(payload)?.value ?? payload;
  const collected: Dict<Tree<string>> = {};

  forEach(transformed, (value: any, index: string) => {
    const nested = traverse(value);

    if (isArray(nested.annotations)) {
      collected[index] = nested.annotations;
    } else if (isPlainObject(nested.annotations)) {
      forEach(nested.annotations, (leaf, key) => {
        collected[index + '.' + key] = leaf;
      });
    }
  });

  return { transformedValue: transformed, annotations: collected };
};
"#;
    assert_clean(src);
}

/// Function-expression form of the initializer (not just arrow) must also
/// benefit from the provisional signature.
#[test]
fn self_recursive_function_expression_with_default_param_is_clean() {
    assert_clean(&walker_source_with(", flag = false", "function ", ""));
}

/// Negative/fallback control: a parameter with NEITHER an annotation NOR a
/// default initializer genuinely needs body inference, so it must still fall
/// through to the pre-existing cycle-breaking behavior (no crash; `tsc`
/// itself reports TS7006 here under `noImplicitAny`, so this only asserts
/// tsz does not panic or hang, not that it is silent).
#[test]
fn self_recursive_arrow_with_bare_unannotated_param_does_not_panic() {
    let src = r#"
interface Result {
  transformedValue: any;
}

export const walkerBareParam = (
  object: any,
  extra
): Result => {
  const r = walkerBareParam(object, extra);
  return { transformedValue: r };
};
"#;
    // Only guards against a panic/hang; diagnostics content for this
    // genuinely-circular shape is out of scope for this suite.
    let _ = check_source_diagnostics(src);
}

/// Negative control: no return-type annotation at all still requires real
/// body inference and must not be affected by this provisional short-circuit.
#[test]
fn self_recursive_arrow_without_return_annotation_does_not_panic() {
    let src = r#"
type Dict<T> = { [key: string]: T };

declare function forEach<T>(record: Dict<T>, run: (v: T, key: string) => void): void;

export const walkerNoReturnAnnotation = (
  object: any,
  flag = false
) => {
  const innerAnnotations: Dict<any> = {};
  forEach(innerAnnotations, (value: any, index: string) => {
    const recursiveResult = walkerNoReturnAnnotation(value);
    innerAnnotations[index] = recursiveResult;
  });
  return { annotations: innerAnnotations };
};
"#;
    let _ = check_source_diagnostics(src);
}

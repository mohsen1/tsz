#[test]
fn noinfer_blocks_inferred_generic_call_candidates() {
    let source = r#"
declare function choose<T>(value: T, fallback: NoInfer<T>): T;
choose("a", "b");
choose("a", "a");

type NI<T> = NoInfer<T>;
declare function chooseAlias<T>(value: T, fallback: NI<T>): T;
chooseAlias("a", "b");

declare function choosePlain<T>(value: T, fallback: T): T;
choosePlain("a", "b");

choose<"a">("a", "b");
"#;
    let diags = relevant_diagnostics(source);
    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        ts2345.len(),
        3,
        "NoInfer fallback positions and explicit type args should reject \"b\", while plain T should infer from both arguments. Diagnostics: {diags:#?}"
    );
}

#[test]
fn explicit_boolean_literal_type_arguments_stay_literal() {
    let source = r#"
declare function id<T>(value: T): T;
id<true>(true);
id<true>(false);
id<false>(false);
id<false>(true);

declare let zero: { <T>(): T };
const zeroTrue: true = zero<true>(true);
const zeroFalse: false = zero<false>(false);

declare let f: { <T>(): T, g<U>(): U };
const inferred = f<true>(true);
const keepTrue: true = inferred;
const rejectFalse: false = inferred;
"#;
    let diags = relevant_diagnostics(source);
    let ts2345 = diagnostics_with_code(&diags, 2345);
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2345.len(),
        2,
        "Explicit true/false type arguments should remain boolean literal types. Diagnostics: {diags:#?}"
    );
    assert!(
        ts2322.len() == 1,
        "Instantiation expression call results should not widen boolean literals. Diagnostics: {diags:#?}"
    );
}

#[test]
fn noinfer_blocks_candidates_nested_in_object_properties() {
    let source = r#"
declare function chooseProp<T extends string>(value: T, fallback: { x: NoInfer<T> }): void;
chooseProp("a", { x: "b" });
chooseProp("a", { x: "a" });
"#;
    let diags = relevant_diagnostics(source);
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "NoInfer nested in an object property should block fallback inference and reject only the \"b\" NoInfer property. Diagnostics: {diags:#?}"
    );
}

#[test]
fn noinfer_blocks_candidates_nested_in_object_properties_with_lib_intrinsic() {
    let source = r#"
declare function chooseProp<T extends string>(value: T, fallback: { x: NoInfer<T> }): void;
chooseProp("a", { x: "b" });
chooseProp("a", { x: "a" });
"#;
    let diags = relevant_lib_diagnostics(source);
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "Lib intrinsic NoInfer nested in an object property should reject only the \"b\" property. Diagnostics: {diags:#?}"
    );
}

#[test]
fn noinfer_array_argument_widens_to_primitive() {
    let source = r#"
declare function choose<T>(options: T[], fallback: NoInfer<T>): T;
choose(["a", "b", "c"], "d");
choose(["a", "b", "c"], "a");
choose([1, 2, 3], 4);
choose([true, false], true);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "array literal widens T to primitive so NoInfer fallback passes. Diagnostics: {diags:#?}"
    );
}

#[test]
fn noinfer_array_single_element_widens_to_primitive() {
    let source = r#"
declare function choose<T>(options: T[], fallback: NoInfer<T>): T;
choose(["a"], "b");
choose([1], 2);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "single-element array widens to primitive for NoInfer fallback. Diagnostics: {diags:#?}"
    );
}

#[test]
fn noinfer_array_renamed_type_param_widens_to_primitive() {
    let source = r#"
declare function pick<U>(candidates: U[], default_value: NoInfer<U>): U;
pick(["x", "y", "z"], "w");
pick([10, 20], 30);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "widening is not name-sensitive: different type-param name. Diagnostics: {diags:#?}"
    );
}

#[test]
fn noinfer_scalar_literal_still_preserved() {
    let source = r#"
declare function choose<T>(value: T, fallback: NoInfer<T>): T;
choose("a", "b");
choose("a", "a");
"#;
    let diags = relevant_diagnostics(source);
    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        ts2345.len(),
        1,
        "scalar direct argument keeps literal narrow; NoInfer fallback rejects mismatch. Diagnostics: {diags:#?}"
    );
}

#[test]
fn noinfer_complex_return_widens_scalar_literal() {
    let source = r#"
function fn1<T>(a: T, b: NoInfer<T>): T {
  return a;
}

function fn2<T>(a: T, b: NoInfer<T>): { v: T } {
  return { v: a };
}

fn1("a", "b");
fn2("a", "b");
"#;
    let diags = relevant_diagnostics(source);
    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        ts2345.len(),
        1,
        "Only direct scalar return should preserve the literal and reject the NoInfer fallback. Diagnostics: {diags:#?}"
    );
}

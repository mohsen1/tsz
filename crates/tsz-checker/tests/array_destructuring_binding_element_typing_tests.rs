//! Array/object destructuring binds every element from the source through the
//! same rule `tsc` uses in `getBindingElementTypeFromParentType`, for **all**
//! declaration forms (variable declarations and annotated parameters):
//!
//! - tuple sources slice (`sliceTupleType`) for `...rest` and index positional
//!   elements;
//! - a fresh array literal is a tuple when the pattern has a leading fixed
//!   element (`[a, ...r]`) and an array when the pattern is pure-rest (`[...r]`),
//!   because the pattern supplies the contextual type;
//! - non-tuple, non-array iterable sources (string, `Set`, `Map`, generator)
//!   bind the destructuring-iterated element type (`E` positionally, `E[]` for
//!   rest).
//!
//! Every expectation is pinned against `tsc` 6.0.2 with `--strict`. Binder names
//! are varied across cases so the behaviour is structural, not name-driven.

use tsz_checker::test_utils::check_source_diagnostics;

/// The rendered `TS2322` target-mismatch messages for `source`. Each divergence
/// row uses `const probe: 12345 = <binding>;`, so the binding's inferred type is
/// rendered as the assignment source in the message.
fn messages_2322(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text)
        .collect()
}

fn renders_type(source: &str, rendered: &str) -> bool {
    let needle = format!("Type '{rendered}' is not assignable to type '12345'.");
    messages_2322(source).contains(&needle)
}

// --- Fresh array-literal sources (rows a–d) ------------------------------------

#[test]
fn fresh_literal_fixed_rest_slices_residual_tuple_const() {
    // row a: `const [lead, ...tail] = [1, 'x', true]` → tail: [string, boolean].
    let src = "const [lead, ...tail] = [1, 'x', true];\nconst probe: 12345 = tail;\n";
    assert!(
        renders_type(src, "[string, boolean]"),
        "row a: {:?}",
        messages_2322(src)
    );
}

#[test]
fn fresh_literal_fixed_rest_slices_residual_tuple_let() {
    // row b: `let` widens the source identically to `const` at the slice site.
    let src = "let [lead, ...remainder] = [1, 'x', true];\nconst probe: 12345 = remainder;\n";
    assert!(
        renders_type(src, "[string, boolean]"),
        "row b: {:?}",
        messages_2322(src)
    );
}

#[test]
fn fresh_literal_fixed_rest_with_default_slices_residual_tuple() {
    // row c: a positional default does not change the rest slice.
    let src = "const [first = 9, ...others] = [1, 'x', true];\nconst probe: 12345 = others;\n";
    assert!(
        renders_type(src, "[string, boolean]"),
        "row c: {:?}",
        messages_2322(src)
    );
}

#[test]
fn fresh_literal_nested_rest_slices_at_each_level() {
    // row d: nested rest patterns slice the residual at each level.
    let src =
        "const [head, ...[neck, ...spine]] = [1, 'x', true, 'z'];\nconst probe: 12345 = spine;\n";
    assert!(
        renders_type(src, "[boolean, string]"),
        "row d: {:?}",
        messages_2322(src)
    );
}

#[test]
fn fresh_literal_pure_rest_widens_to_array() {
    // A pure-rest pattern takes an array contextual type, so the literal widens.
    let src = "const [...everything] = [1, 2, 3];\nconst probe: 12345 = everything;\n";
    assert!(
        renders_type(src, "number[]"),
        "pure-rest widens: {:?}",
        messages_2322(src)
    );
}

// --- Annotated function parameters (rows e–i) ----------------------------------

#[test]
fn annotated_param_array_pattern_indexes_and_slices() {
    // row e: `function fn([a, ...r]: [number, string, boolean])`.
    let src = "function pick([alpha, ...beta]: [number, string, boolean]) {\n  const p1: 12345 = alpha;\n  const p2: 12345 = beta;\n}\n";
    assert!(
        renders_type(src, "number"),
        "row e positional: {:?}",
        messages_2322(src)
    );
    assert!(
        renders_type(src, "[string, boolean]"),
        "row e rest: {:?}",
        messages_2322(src)
    );
}

#[test]
fn annotated_param_readonly_tuple_slices_to_mutable_tuple() {
    // row f: readonly source slices to a mutable residual tuple.
    let src = "function take([one, ...rest]: readonly [number, string, boolean]) {\n  const p: 12345 = rest;\n}\n";
    assert!(
        renders_type(src, "[string, boolean]"),
        "row f: {:?}",
        messages_2322(src)
    );
}

#[test]
fn annotated_rest_param_with_variadic_tuple_binds_array_slice() {
    // row g: `function fn(...[a, ...r]: [number, ...string[]])` → r: string[].
    let src = "function collect(...[lead, ...more]: [number, ...string[]]) {\n  const p: 12345 = more;\n}\n";
    assert!(
        renders_type(src, "string[]"),
        "row g: {:?}",
        messages_2322(src)
    );
}

#[test]
fn annotated_param_array_pattern_in_all_function_forms() {
    // row h: arrow, object-literal method, and class method forms of row e all
    // bind the rest to the sliced tuple (previously they fell to `any`).
    let arrow =
        "const fx = ([a1, ...b1]: [number, string, boolean]) => {\n  const p: 12345 = b1;\n};\n";
    assert!(
        renders_type(arrow, "[string, boolean]"),
        "row h arrow: {:?}",
        messages_2322(arrow)
    );

    let method = "const holder = {\n  run([a2, ...b2]: [number, string, boolean]) {\n    const p: 12345 = b2;\n  },\n};\n";
    assert!(
        renders_type(method, "[string, boolean]"),
        "row h object method: {:?}",
        messages_2322(method)
    );

    let class_method = "class Runner {\n  run([a3, ...b3]: [number, string, boolean]) {\n    const p: 12345 = b3;\n  }\n}\n";
    assert!(
        renders_type(class_method, "[string, boolean]"),
        "row h class method: {:?}",
        messages_2322(class_method)
    );
}

#[test]
fn annotated_param_object_rest_binds_remaining_properties() {
    // row i: `function fn({ p, ...rest }: { p: number; q: string })`.
    let src = "function shape({ picked, ...leftover }: { picked: number; q: string }) {\n  const p: 12345 = leftover;\n}\n";
    assert!(
        renders_type(src, "{ q: string; }"),
        "row i: {:?}",
        messages_2322(src)
    );
}

// --- Non-array-like iterable sources (rows j–n) --------------------------------

#[test]
fn string_source_rest_binds_string_array() {
    // row j: `const [...r] = 'hello'` → r: string[].
    let src = "const [...letters] = 'hello';\nconst probe: 12345 = letters;\n";
    assert!(
        renders_type(src, "string[]"),
        "row j: {:?}",
        messages_2322(src)
    );
}

#[test]
fn string_source_positional_binds_string() {
    // row k: `const [c] = 'hi'` → c: string.
    let src = "const [initial] = 'hi';\nconst probe: 12345 = initial;\n";
    assert!(
        renders_type(src, "string"),
        "row k: {:?}",
        messages_2322(src)
    );
}

// Rows l (`Set`), m (`Map`), and n (generator) exercise the same iterable
// fallback as rows j/k but require the standard library, which this unit harness
// does not load (`set_lib_contexts(Vec::new())`). They are verified end-to-end
// against `tsc` 6.0.2 in the PR's differential matrix; string iteration (rows
// j/k) covers the iterable path here because it is intrinsic.

// --- Negative controls (must stay unchanged) -----------------------------------

#[test]
fn declared_tuple_source_still_slices() {
    let src = "declare const triple: [number, string, boolean];\nconst [x, ...y] = triple;\nconst probe: 12345 = y;\n";
    assert!(
        renders_type(src, "[string, boolean]"),
        "declared tuple: {:?}",
        messages_2322(src)
    );
}

#[test]
fn as_const_source_still_slices_literals() {
    let src = "const frozen = [1, 'x', true] as const;\nconst [x, ...y] = frozen;\nconst probe: 12345 = y;\n";
    // `as const` preserves literal element types in the residual slice.
    assert!(
        renders_type(src, "[\"x\", true]"),
        "as const: {:?}",
        messages_2322(src)
    );
}

#[test]
fn array_source_rest_stays_an_array() {
    let src = "declare const nums: number[];\nconst [x, ...y] = nums;\nconst probe: 12345 = y;\n";
    assert!(
        renders_type(src, "number[]"),
        "array source: {:?}",
        messages_2322(src)
    );
}

#[test]
fn union_of_tuples_slices_each_member() {
    let src = "declare const u: [number, string] | [number, boolean];\nconst [x, ...y] = u;\nconst probe: 12345 = y;\n";
    assert!(
        renders_type(src, "[string] | [boolean]"),
        "union of tuples: {:?}",
        messages_2322(src)
    );
}

#[test]
fn assignment_destructuring_over_declared_tuple_is_unchanged() {
    // Assignment destructuring (not a binding) keeps binding the declared targets;
    // no spurious TS2322 from the source slice.
    let src = "declare const triple: [number, string, boolean];\nlet head: number;\nlet tail: [string, boolean];\n[head, ...tail] = triple;\n";
    assert!(
        !messages_2322(src)
            .iter()
            .any(|m| m.contains("is not assignable")),
        "assignment destructuring should not report TS2322: {:?}",
        messages_2322(src)
    );
}

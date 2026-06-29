//! Tuple rest-destructuring binds the residual tuple slice, not `ElementType[]`.
//!
//! Regression: an array binding pattern with a `...rest` element over a finite
//! (or mixed-variadic) tuple collapsed the residual to `ElementType[]` (an
//! array of the tuple's element type) instead of slicing the tuple from the
//! rest position, matching tsc's `sliceTupleType`. This produced false `TS2322`
//! whenever the rest binding flowed into a tuple-typed position.
//!
//! All inferred-type expectations are pinned against `tsc` 6.0.x. Binder names
//! are varied across cases so the behaviour is structural, not name-driven.

use tsz_checker::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn messages_2322(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text)
        .collect()
}

#[test]
fn fixed_tuple_skip_one_slices_to_residual_tuple() {
    // `rest` is `[string, boolean]`, not `boolean[]`.
    let source = r#"
const tup: [number, string, boolean] = [1, "a", true];
const [lead, ...tail] = tup;
const ok: [string, boolean] = tail;
"#;
    assert!(
        !codes(source).contains(&2322),
        "expected no TS2322 assigning the sliced tuple rest; got {:?}",
        codes(source)
    );
}

#[test]
fn fixed_tuple_rest_rejects_wrong_residual_tuple() {
    // The residual is `[string, boolean]`; assigning to `[number, boolean]`
    // must still report TS2322 (the slice is precise, not widened to an array).
    let source = r#"
const tup: [number, string, boolean] = [1, "a", true];
const [first, ...others] = tup;
const wrong: [number, boolean] = others;
"#;
    assert!(
        codes(source).contains(&2322),
        "expected TS2322 for a mismatched residual tuple; got {:?}",
        codes(source)
    );
}

#[test]
fn fixed_tuple_skip_two_and_skip_all() {
    let skip_two = r#"
const triple: [number, string, boolean] = [1, "a", true];
const [a, b, ...rem] = triple;
const ok: [boolean] = rem;
"#;
    assert!(
        !codes(skip_two).contains(&2322),
        "skip-two rest should be [boolean]; got {:?}",
        codes(skip_two)
    );

    let skip_all = r#"
const triple: [number, string, boolean] = [1, "a", true];
const [a, b, c, ...empty] = triple;
const ok: [] = empty;
"#;
    assert!(
        !codes(skip_all).contains(&2322),
        "skip-all rest should be the empty tuple []; got {:?}",
        codes(skip_all)
    );
}

#[test]
fn readonly_tuple_rest_is_a_mutable_slice() {
    // tsc slices a readonly source into a mutable residual tuple, so the rest
    // accepts mutation and is assignable to the mutable tuple type.
    let source = r#"
const ro: readonly [number, string, boolean] = [1, "a", true];
const [headEl, ...restEls] = ro;
restEls.push("more");
const ok: [string, boolean] = restEls;
"#;
    assert!(
        !codes(source).contains(&2322) && !codes(source).contains(&2540),
        "readonly source should slice to a mutable tuple; got {:?}",
        codes(source)
    );
}

#[test]
fn mixed_variadic_tuple_preserves_leading_fixed_and_rest() {
    // `[number, boolean, ...string[]]` skipping one binds `[boolean, ...string[]]`.
    let source = r#"
const mixed: [number, boolean, ...string[]] = [1, true, "x"];
const [n, ...rest] = mixed;
const ok: [boolean, ...string[]] = rest;
"#;
    assert!(
        !codes(source).contains(&2322),
        "expected [boolean, ...string[]] residual; got {:?}",
        codes(source)
    );
}

#[test]
fn binding_past_leading_fixed_yields_array_form() {
    // Consuming past the leading fixed region drops into the variadic rest, so
    // the residual is the array form `string[]`, exactly like tsc.
    let source = r#"
const mixed: [boolean, number, ...string[]] = [true, 1, "x"];
const [a, b, c, ...rest] = mixed;
const ok: string[] = rest;
const stillTuple: [string] = rest;
"#;
    let cs = codes(source);
    // `rest` is `string[]`: assignable to `string[]` (no error on that line),
    // but NOT to a fixed `[string]` tuple (TS2322 on the last line).
    assert!(
        cs.contains(&2322),
        "expected TS2322 assigning string[] to [string]; got {cs:?}"
    );
}

#[test]
fn array_source_rest_stays_an_array() {
    // A genuine array source must keep producing `E[]`, unchanged.
    let source = r#"
const arr: number[] = [1, 2, 3];
const [a, ...rest] = arr;
const ok: number[] = rest;
"#;
    assert!(
        !codes(source).contains(&2322),
        "array source rest should remain number[]; got {:?}",
        codes(source)
    );
}

#[test]
fn parameter_destructuring_slices_the_tuple() {
    // The shared binding-element path also covers parameter destructuring.
    let source = r#"
function take([lead, ...tail]: [number, string, boolean]): [string, boolean] {
  return tail;
}
"#;
    assert!(
        !codes(source).contains(&2322),
        "parameter rest should slice to [string, boolean]; got {:?}",
        codes(source)
    );
}

#[test]
fn union_of_tuples_slices_each_member() {
    let source = r#"
declare const u: [number, string] | [number, string, boolean];
const [a, ...rest] = u;
const ok: [string] | [string, boolean] = rest;
"#;
    assert!(
        !codes(source).contains(&2322),
        "union members should each slice independently; got {:?}",
        codes(source)
    );
}

#[test]
fn residual_tuple_display_matches_tsc() {
    // Pin the exact rendered residual against tsc's `sliceTupleType` output.
    let source = r#"
const tup: [number, string, boolean] = [1, "a", true];
const [lead, ...tail] = tup;
const show: "X" = tail;
"#;
    let msgs = messages_2322(source);
    assert!(
        msgs.iter().any(|m| m.contains("[string, boolean]")),
        "expected residual rendered as [string, boolean]; got {msgs:?}"
    );
}

#[test]
fn fresh_array_literal_rest_widens_to_array_not_a_slice() {
    // A *fresh* array-literal destructuring source is widened by tsc
    // (`getWidenedTypeForVariableLikeDeclaration`), so the `...rest` binds the
    // element array `number[]`, NOT the residual tuple slice `[number, number]`
    // that a declared tuple would produce.
    let assignable_to_array = r#"
const [head, ...spread] = [1, 2, 3];
const ok: number[] = spread;
"#;
    assert!(
        !codes(assignable_to_array).contains(&2322),
        "fresh-literal rest should be number[] (assignable to number[]); got {:?}",
        codes(assignable_to_array)
    );

    let not_a_tuple = r#"
const [lead, ...trailing] = [1, 2, 3];
const wrong: [number, number] = trailing;
"#;
    assert!(
        codes(not_a_tuple).contains(&2322),
        "fresh-literal rest is number[], not the tuple [number, number]; got {:?}",
        codes(not_a_tuple)
    );

    // `var` fresh literal widens identically.
    let var_form = r#"
var [v, ...vrest] = [1, 2, 3];
const okv: number[] = vrest;
const wrongv: [number, number] = vrest;
"#;
    let cs = codes(var_form);
    assert!(
        cs.iter().filter(|&&c| c == 2322).count() == 1,
        "var fresh-literal rest is number[]: ok for number[], TS2322 for tuple; got {cs:?}"
    );
}

#[test]
fn rest_element_object_pattern_over_fresh_literal_reports_number_array() {
    // restElementWithBindingPattern2.ts conformance shape: tsc reports
    //   TS2339 Property 'b' does not exist on type 'number[]'.
    // The rest source `[0, 1]` widens to `number[]`, so the nested object
    // pattern resolves the missing property against `number[]`, not against a
    // tuple `[number, number]`.
    let source = "var [...{0: a, b }] = [0, 1];\n";
    let msgs: Vec<String> = check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2339)
        .map(|d| d.message_text)
        .collect();
    assert!(
        msgs.iter()
            .any(|m| m.contains("'b'") && m.contains("number[]")),
        "expected TS2339 against number[] (tsc parity); got {msgs:?}"
    );
}

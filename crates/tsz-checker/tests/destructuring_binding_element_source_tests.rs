//! Binding-element types from annotated parameters and iterated sources.
//!
//! tsc's `getBindingElementTypeFromParentType` types every element of an
//! array binding pattern from the pattern's source: tuple sources index
//! positionally and slice for rest (`sliceTupleType`); non-array-like
//! iterable sources (string, `Iterable<T>`, `Map`, generators) use the
//! destructuring-iterated element type; a union with a non-array-like member
//! switches every position to the iterated element type of the whole union.
//! The same rules apply to annotated function parameters, which previously
//! bound `any` for array-pattern elements and object rest.
//!
//! All expectations are pinned against `tsc` 6.0.2. Binder names vary across
//! cases so the behaviour is structural, not name-driven.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{
    check_source_diagnostics, check_source_with_libs_code_messages, load_default_lib_files,
};

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

/// TS2322 messages with the default lib loaded, for sources that need
/// `Iterable` / `Set` / `Map` / generator types to resolve.
fn lib_messages_2322(source: &str) -> Vec<String> {
    let lib_files = load_default_lib_files();
    check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &lib_files)
        .into_iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message)
        .collect()
}

// ---------------------------------------------------------------------------
// Annotated parameters: array patterns
// ---------------------------------------------------------------------------

#[test]
fn annotated_param_array_pattern_positional_element_is_typed() {
    // Previously `lead` bound `any`, silently accepting any assignment.
    let source = r#"
function pick([lead, second]: [number, string]) {
  const wrong: boolean = lead;
  const alsoWrong: boolean = second;
}
"#;
    let cs = codes(source);
    assert!(
        cs.iter().filter(|&&c| c == 2322).count() == 2,
        "annotated param array elements are number/string, not any; got {cs:?}"
    );
}

#[test]
fn annotated_param_array_rest_slices_the_tuple() {
    let source = r#"
function grab([head, ...tail]: [number, string, boolean]) {
  const show: "X" = tail;
}
"#;
    let msgs = messages_2322(source);
    assert!(
        msgs.iter().any(|m| m.contains("[string, boolean]")),
        "annotated param rest slices to [string, boolean]; got {msgs:?}"
    );
}

#[test]
fn annotated_param_readonly_tuple_rest_slices() {
    let source = r#"
function fromRo([one, ...more]: readonly [number, string, boolean]) {
  const show: "X" = more;
}
"#;
    let msgs = messages_2322(source);
    assert!(
        msgs.iter().any(|m| m.contains("[string, boolean]")),
        "readonly tuple param rest slices to a mutable [string, boolean]; got {msgs:?}"
    );
}

#[test]
fn rest_parameter_tuple_pattern_rest_binds_array_form() {
    // `...[first, ...remaining]: [number, ...string[]]` binds `string[]`.
    let source = r#"
function spreadForm(...[first, ...remaining]: [number, ...string[]]) {
  const wrong: boolean = remaining[0];
  const alsoWrong: boolean = first;
}
"#;
    let cs = codes(source);
    assert!(
        cs.iter().filter(|&&c| c == 2322).count() == 2,
        "rest-parameter tuple pattern binds string[]/number; got {cs:?}"
    );
}

#[test]
fn annotated_arrow_and_method_param_patterns_are_typed() {
    let arrow = r#"
const fn = ([alpha, ...omega]: [number, string]) => {
  const wrong: boolean = omega[0];
};
"#;
    assert!(
        codes(arrow).contains(&2322),
        "arrow param rest element is string; got {:?}",
        codes(arrow)
    );

    let method = r#"
class Holder {
  consume([x, ...ys]: [number, string]) {
    const wrong: boolean = ys[0];
  }
}
"#;
    assert!(
        codes(method).contains(&2322),
        "method param rest element is string; got {:?}",
        codes(method)
    );

    let object_literal_method = r#"
const bag = {
  eat([p, ...qs]: [number, string]) {
    const wrong: boolean = qs[0];
  },
};
"#;
    assert!(
        codes(object_literal_method).contains(&2322),
        "object-literal method param rest element is string; got {:?}",
        codes(object_literal_method)
    );
}

#[test]
fn annotated_param_object_rest_omits_named_siblings() {
    // Previously `remainder` bound `any`.
    let source = r#"
function strip({ keep, ...remainder }: { keep: number; other: string }) {
  const show: "X" = remainder;
}
"#;
    let msgs = messages_2322(source);
    assert!(
        msgs.iter().any(|m| m.contains("{ other: string; }")),
        "annotated param object rest is {{ other: string; }}; got {msgs:?}"
    );
}

#[test]
fn annotated_param_nested_array_in_object_pattern_is_typed() {
    // Nested chains previously resolved only pure object paths.
    let source = r#"
function nest({ inner: [count, ...labels] }: { inner: [number, string, string] }) {
  const wrong: boolean = count;
  const alsoWrong: boolean = labels[0];
}
"#;
    let cs = codes(source);
    assert!(
        cs.iter().filter(|&&c| c == 2322).count() == 2,
        "nested array pattern under object pattern is typed; got {cs:?}"
    );
}

#[test]
fn unannotated_param_pattern_stays_implicit_any() {
    // Negative control: no annotation and no contextual type still reports
    // implicit-any (TS7031), and must not invent element types.
    let source = r#"
function loose([a, ...rest]) {
  return rest;
}
"#;
    let cs = codes(source);
    assert!(
        cs.contains(&7031),
        "unannotated pattern elements stay implicitly any (TS7031); got {cs:?}"
    );
}

#[test]
fn param_default_initializer_types_the_pattern() {
    // Unannotated parameter with a default: the pattern's source is the
    // initializer's type, and a fresh literal infers as a tuple.
    let cast_form = r#"
function withCast([lo, ...hi] = [1, "x"] as [number, string]) {
  const show: "X" = hi;
}
"#;
    let msgs = messages_2322(cast_form);
    assert!(
        msgs.iter().any(|m| m.contains("[string]")),
        "cast default slices to [string]; got {msgs:?}"
    );

    let literal_form = r#"
function withLiteral([lo, ...hi] = [1, "x"]) {
  const wrong: boolean = lo;
}
"#;
    assert!(
        codes(literal_form).contains(&2322),
        "fresh-literal default binds lo: number; got {:?}",
        codes(literal_form)
    );
}

// ---------------------------------------------------------------------------
// Iterated (non-array-like) sources
// ---------------------------------------------------------------------------

#[test]
fn string_source_binds_string_elements() {
    let positional = r#"
const [ch] = "hi";
const wrong: boolean = ch;
"#;
    assert!(
        codes(positional).contains(&2322),
        "string positional element is string; got {:?}",
        codes(positional)
    );

    let rest = r#"
const [...chars] = "hello";
const show: "X" = chars;
"#;
    let msgs = messages_2322(rest);
    assert!(
        msgs.iter().any(|m| m.contains("string[]")),
        "string rest binds string[]; got {msgs:?}"
    );
}

#[test]
fn iterable_map_set_and_generator_sources_bind_element_arrays() {
    let iterable = r#"
declare const nums: Iterable<number>;
const [...gathered] = nums;
const show: "X" = gathered;
"#;
    assert!(
        lib_messages_2322(iterable)
            .iter()
            .any(|m| m.contains("number[]")),
        "Iterable<number> rest binds number[]; got {:?}",
        lib_messages_2322(iterable)
    );

    let set = r#"
const [...uniques] = new Set([1, 2]);
const show: "X" = uniques;
"#;
    assert!(
        lib_messages_2322(set)
            .iter()
            .any(|m| m.contains("number[]")),
        "Set<number> rest binds number[]; got {:?}",
        lib_messages_2322(set)
    );

    let map = r#"
declare const table: Map<string, number>;
const [...entries] = table;
const show: "X" = entries;
"#;
    assert!(
        lib_messages_2322(map)
            .iter()
            .any(|m| m.contains("[string, number][]")),
        "Map rest binds [string, number][]; got {:?}",
        lib_messages_2322(map)
    );

    let generator = r#"
function* words() { yield "a"; }
const [...taken] = words();
const show: "X" = taken;
"#;
    assert!(
        lib_messages_2322(generator)
            .iter()
            .any(|m| m.contains("string[]")),
        "generator rest binds string[]; got {:?}",
        lib_messages_2322(generator)
    );
}

// ---------------------------------------------------------------------------
// Unions with non-array-like members
// ---------------------------------------------------------------------------

#[test]
fn union_with_string_member_uses_iterated_element_type_everywhere() {
    // `string | [number, boolean]`: every position binds
    // `string | number | boolean` (not per-member indexed access).
    let source = r#"
declare const mixed: string | [number, boolean];
const [firstEl, secondEl] = mixed;
const show: "X" = firstEl;
const show2: "X" = secondEl;
"#;
    let msgs = messages_2322(source);
    assert!(
        msgs.iter()
            .filter(|m| m.contains("string | number | boolean"))
            .count()
            == 2,
        "both positions bind string | number | boolean; got {msgs:?}"
    );
}

#[test]
fn union_with_iterable_member_rest_binds_iterated_array() {
    let source = r#"
declare const source: Set<symbol> | boolean[];
const [lead, ...others] = source;
const show: "X" = others;
"#;
    let msgs = lib_messages_2322(source);
    assert!(
        msgs.iter()
            .any(|m| m.contains("(boolean | symbol)[]") || m.contains("(symbol | boolean)[]")),
        "rest over Set|array union binds the iterated element array; got {msgs:?}"
    );
}

#[test]
fn all_array_like_union_keeps_distributed_indexing() {
    // Negative control: no non-array-like member, so per-member indexed
    // access is unchanged (`number | boolean`, not the full iterated union).
    let source = r#"
declare const pair: [number, string] | boolean[];
const [headOf] = pair;
const show: "X" = headOf;
"#;
    let msgs = messages_2322(source);
    assert!(
        msgs.iter()
            .any(|m| m.contains("number | boolean") && !m.contains("string")),
        "all-array-like union keeps per-member indexing; got {msgs:?}"
    );
}

// ---------------------------------------------------------------------------
// Declared-source negative controls
// ---------------------------------------------------------------------------

#[test]
fn declared_tuple_and_as_const_sources_unchanged() {
    let declared = r#"
declare const trio: [number, string, boolean];
const [x, ...xs] = trio;
const show: "X" = xs;
"#;
    assert!(
        messages_2322(declared)
            .iter()
            .any(|m| m.contains("[string, boolean]")),
        "declared tuple rest still slices; got {:?}",
        messages_2322(declared)
    );

    let as_const = r#"
const [y, ...ys] = [1, "x"] as const;
const show: "X" = ys;
"#;
    assert!(
        messages_2322(as_const).iter().any(|m| m.contains("\"x\"")),
        "as-const rest keeps literal slice elements; got {:?}",
        messages_2322(as_const)
    );
}

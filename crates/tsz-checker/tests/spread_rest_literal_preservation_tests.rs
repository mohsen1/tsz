//! Literal preservation for generic rest parameters packed from trailing
//! arguments (tsc's `getSpreadArgumentType`).
//!
//! When `tsc` infers a rest type parameter from individual arguments, each
//! literal argument keeps its literal type iff the per-index contextual type
//! `T[i]` preserves it: while `T` is unfixed the *base constraint* decides
//! (`string`/`number`/`boolean`/`keyof`/template-literal/literal-union
//! constituents preserve their matching literal kinds; `any`/`unknown`/
//! `object` widen). These tests pin the uncontexted (unfixed) family across
//! constraint shapes and the widening fallbacks.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};

fn relevant_default_lib_diagnostics(source: &str) -> Vec<(u32, String)> {
    let lib_files = load_default_lib_files();
    check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &lib_files)
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect()
}

fn assert_no_diagnostics(source: &str, what: &str) {
    let diagnostics = relevant_default_lib_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "{what}: expected no diagnostics, got: {diagnostics:#?}"
    );
}

fn assert_single_ts2322(source: &str, what: &str) {
    let diagnostics = relevant_default_lib_diagnostics(source);
    let ts2322_count = diagnostics.iter().filter(|(code, _)| *code == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "{what}: expected exactly one TS2322, got: {diagnostics:#?}"
    );
}

#[test]
fn string_array_constraint_preserves_string_literals() {
    assert_no_diagnostics(
        r#"
function tagList<Parts extends string[]>(...parts: Parts): Parts {
    return parts;
}
const picked = tagList("north", "south");
const expected: ["north", "south"] = picked;
"#,
        "string[] constraint should keep string literal elements",
    );
}

#[test]
fn number_array_constraint_preserves_numeric_literals() {
    assert_no_diagnostics(
        r#"
function coords<Axes extends number[]>(...axes: Axes): Axes {
    return axes;
}
const pair = coords(3, 7);
const expected: [3, 7] = pair;
"#,
        "number[] constraint should keep numeric literal elements",
    );
}

#[test]
fn boolean_array_constraint_preserves_boolean_literals() {
    assert_no_diagnostics(
        r#"
function flags<Bits extends boolean[]>(...bits: Bits): Bits {
    return bits;
}
const set = flags(true, false);
const expected: [true, false] = set;
"#,
        "boolean[] constraint should keep boolean literal elements",
    );
}

#[test]
fn keyof_array_constraint_preserves_string_literals() {
    assert_no_diagnostics(
        r#"
function keyPath<Keys extends (keyof any)[]>(...segments: Keys): Keys {
    return segments;
}
const path = keyPath("outer", "inner");
const expected: ["outer", "inner"] = path;
"#,
        "(keyof any)[] constraint should keep string literal elements",
    );
}

#[test]
fn template_literal_array_constraint_preserves_matching_literals() {
    assert_no_diagnostics(
        r#"
function events<Names extends `on${string}`[]>(...names: Names): Names {
    return names;
}
const handlers = events("onClick", "onHover");
const expected: ["onClick", "onHover"] = handlers;
"#,
        "template-literal array constraint should keep matching string literals",
    );
}

#[test]
fn literal_union_array_constraint_preserves_literals() {
    assert_no_diagnostics(
        r#"
function moves<Dirs extends ("up" | "down" | "left")[]>(...dirs: Dirs): Dirs {
    return dirs;
}
const combo = moves("up", "left");
const expected: ["up", "left"] = combo;
"#,
        "literal-union array constraint should keep literal elements",
    );
}

#[test]
fn mixed_primitive_union_array_constraint_preserves_each_kind() {
    assert_no_diagnostics(
        r#"
function cells<Row extends (string | number)[]>(...row: Row): Row {
    return row;
}
const record = cells("id", 42);
const expected: ["id", 42] = record;
"#,
        "(string | number)[] constraint should keep both literal kinds",
    );
}

#[test]
fn any_array_constraint_still_widens_literals() {
    assert_no_diagnostics(
        r#"
function pack<Items extends any[]>(...items: Items): Items {
    return items;
}
const bag = pack("a", "b");
const expected: [string, string] = bag;
"#,
        "any[] constraint should widen literal elements to their primitive",
    );
    assert_single_ts2322(
        r#"
function pack<Items extends any[]>(...items: Items): Items {
    return items;
}
const bag = pack("a", "b");
const tooNarrow: ["a", "b"] = bag;
"#,
        "any[] constraint must NOT keep literal elements",
    );
}

#[test]
fn unknown_array_constraint_still_widens_literals() {
    assert_no_diagnostics(
        r#"
function stash<Items extends unknown[]>(...items: Items): Items {
    return items;
}
const bin = stash(1, 2);
const expected: [number, number] = bin;
"#,
        "unknown[] constraint should widen literal elements to their primitive",
    );
    assert_single_ts2322(
        r#"
function stash<Items extends unknown[]>(...items: Items): Items {
    return items;
}
const bin = stash(1, 2);
const tooNarrow: [1, 2] = bin;
"#,
        "unknown[] constraint must NOT keep literal elements",
    );
}

#[test]
fn object_array_constraint_widens_fresh_object_properties() {
    assert_no_diagnostics(
        r#"
function shapes<Objs extends object[]>(...objs: Objs): Objs {
    return objs;
}
const box = shapes({ width: 4 });
const expected: [{ width: number }] = box;
"#,
        "object[] constraint should deep-widen fresh object literal elements",
    );
}

#[test]
fn concrete_tuple_constraint_preserves_literals_per_position() {
    assert_no_diagnostics(
        r#"
function entry<Pair extends [string, number]>(...pair: Pair): Pair {
    return pair;
}
const row = entry("total", 10);
const expected: ["total", 10] = row;
"#,
        "[string, number] tuple constraint should keep literals per position",
    );
}

#[test]
fn tuple_typed_rest_param_preserves_variadic_slice_literals() {
    assert_no_diagnostics(
        r#"
function labeled<Tail extends string[]>(...args: [number, ...Tail]): Tail {
    return args.slice(1) as Tail;
}
const tail = labeled(1, "alpha", "beta");
const expected: ["alpha", "beta"] = tail;
"#,
        "tuple-typed rest param should keep literals in the variadic slice",
    );
}

#[test]
fn readonly_array_constraint_preserves_literals() {
    assert_no_diagnostics(
        r#"
function frozen<Items extends readonly string[]>(...items: Items): Items {
    return items;
}
const listed = frozen("one", "two");
const expected: ["one", "two"] = listed;
"#,
        "readonly string[] constraint should keep string literal elements",
    );
}

#[test]
fn chained_type_param_constraint_preserves_literals() {
    assert_no_diagnostics(
        r#"
function chained<Elem extends string, List extends Elem[]>(...list: List): List {
    return list;
}
const linked = chained("head", "tail");
const expected: ["head", "tail"] = linked;
"#,
        "a rest constraint chained through a primitive-constrained param should keep literals",
    );
}

#[test]
fn array_application_constraint_preserves_literals() {
    assert_no_diagnostics(
        r#"
function generics<Items extends Array<string>>(...items: Items): Items {
    return items;
}
const wrapped = generics("x", "y");
const expected: ["x", "y"] = wrapped;
"#,
        "Array<string> application constraint should keep string literal elements",
    );
}

#[test]
fn union_of_arrays_constraint_preserves_literals() {
    assert_no_diagnostics(
        r#"
function either<Items extends string[] | number[]>(...items: Items): Items {
    return items;
}
const chosen = either("a", "b");
const expected: ["a", "b"] = chosen;
"#,
        "string[] | number[] union constraint should keep literal elements",
    );
}

#[test]
fn spread_tail_keeps_fixed_prefix_literal() {
    assert_no_diagnostics(
        r#"
declare const tail: string[];
function headTail<Parts extends string[]>(...parts: Parts): Parts {
    return parts;
}
const merged = headTail("first", ...tail);
const expected: ["first", ...string[]] = merged;
const alsoOk: [string, ...string[]] = merged;
"#,
        "a spread tail should not stop the fixed prefix from keeping its literal",
    );
}

#[test]
fn spread_tail_prefix_literal_is_not_widened_to_plain_string_slot() {
    assert_single_ts2322(
        r#"
declare const tail: string[];
function headTail<Parts extends string[]>(...parts: Parts): Parts {
    return parts;
}
const merged = headTail("first", ...tail);
const tooNarrow: ["second", ...string[]] = merged;
"#,
        "the preserved prefix literal must still mismatch a different literal",
    );
}

#[test]
fn enum_member_arguments_keep_enum_literal_elements() {
    assert_no_diagnostics(
        r#"
enum Mode { On = "on", Off = "off" }
function switches<States extends Mode[]>(...states: States): States {
    return states;
}
const toggled = switches(Mode.On, Mode.Off);
const expected: [Mode.On, Mode.Off] = toggled;
"#,
        "enum member arguments should keep their enum literal element types",
    );
}

#[test]
fn contextual_tuple_annotation_still_preserves_literals() {
    assert_no_diagnostics(
        r#"
function direct<Parts extends string[]>(...parts: Parts): Parts {
    return parts;
}
const annotated: ["red", "green"] = direct("red", "green");
"#,
        "a compatible tuple annotation should keep literal elements",
    );
}

#[test]
fn incompatible_contextual_annotation_still_reports_ts2322() {
    assert_single_ts2322(
        r#"
function direct<Parts extends string[]>(...parts: Parts): Parts {
    return parts;
}
const wrong: 5 = direct("red", "green");
"#,
        "an incompatible annotation still fails assignability",
    );
}

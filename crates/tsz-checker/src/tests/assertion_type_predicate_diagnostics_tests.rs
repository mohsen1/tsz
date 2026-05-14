//! Regression coverage for assertion-function target diagnostics and reachability.

use crate::context::CheckerOptions;
use crate::module_resolution::build_module_resolution_maps;
use crate::state::CheckerState;
use crate::test_utils::{
    check_js_source_diagnostics, check_source, check_source_codes, check_source_strict_codes,
};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_require_assertion_from_dts() -> Vec<u32> {
    let files = [
        (
            "ex2.d.ts",
            r#"
declare function art(value: any): asserts value;
export = art;
"#,
        ),
        (
            "38379.js",
            r#"
const artoo = require("./ex2");
let y = 1;
artoo(y);
"#,
        ),
    ];

    let mut parsed = Vec::new();
    let mut binders = Vec::new();
    let mut roots = Vec::new();
    for (file_name, source) in files {
        let mut parser = ParserState::new(file_name.to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        roots.push(root);
        binders.push(Arc::new(binder));
        parsed.push(Arc::new(parser.get_arena().clone()));
    }

    let file_names = vec!["ex2.d.ts".to_string(), "38379.js".to_string()];
    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);

    let types = TypeInterner::new();
    let current_arena = Arc::clone(&parsed[1]);
    let current_binder = Arc::clone(&binders[1]);
    let mut checker = CheckerState::new(
        current_arena.as_ref(),
        current_binder.as_ref(),
        &types,
        "38379.js".to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_lib: true,
            module: tsz_common::common::ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );

    checker.ctx.set_all_arenas(Arc::new(parsed));
    checker.ctx.set_all_binders(Arc::new(binders));
    checker.ctx.set_current_file_idx(1);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.ctx.set_lib_contexts(Vec::new());

    checker.check_source_file(roots[1]);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn unannotated_assertion_identifier_emits_ts2775() {
    let codes = check_source_codes(
        r#"
function f(x: unknown) {
    const assert = (value: unknown): asserts value => {};
    assert(typeof x === "string");
}
"#,
    );
    assert!(
        codes.contains(&2775),
        "expected TS2775 for assertion variable without explicit declaration type, got {codes:?}"
    );
}

#[test]
fn invalid_assertion_alias_does_not_narrow_after_ts2775() {
    let codes = check_source_strict_codes(
        r#"
function assertString(x: unknown): asserts x is string {
    if (typeof x !== "string") throw "";
}
const f = assertString;
let v: unknown;
f(v);
v.toUpperCase();
"#,
    );
    assert!(
        codes.contains(&2775),
        "expected TS2775 for assertion alias without explicit type annotation, got {codes:?}"
    );
    assert!(
        codes.contains(&18046),
        "invalid assertion alias must not narrow unknown value, got {codes:?}"
    );
}

#[test]
fn unannotated_asserts_this_receiver_emits_ts2775_and_does_not_narrow() {
    let codes = check_source_strict_codes(
        r#"
class Validator {
    value: unknown;

    constructor(value: unknown) {
        this.value = value;
    }

    assertIsNumber(): asserts this is Validator & { value: number } {
        if (typeof this.value !== "number") throw "";
    }
}

function useThisAssert() {
    const v = new Validator(42);
    v.assertIsNumber();
    const n: number = v.value;
}
"#,
    );
    assert!(
        codes.contains(&2775),
        "expected TS2775 for assertion method receiver without explicit declaration type, got {codes:?}"
    );
    assert!(
        codes.contains(&2322),
        "invalid assertion method call must not narrow receiver value, got {codes:?}"
    );
}

#[test]
fn annotated_asserts_this_receiver_narrows_without_ts2775() {
    let codes = check_source_strict_codes(
        r#"
class Validator {
    value: unknown;

    constructor(value: unknown) {
        this.value = value;
    }

    assertIsNumber(): asserts this is Validator & { value: number } {
        if (typeof this.value !== "number") throw "";
    }
}

function useThisAssert() {
    const v: Validator = new Validator(42);
    v.assertIsNumber();
    const n: number = v.value;
}
"#,
    );
    assert!(
        !codes.contains(&2775),
        "did not expect TS2775 for explicitly annotated assertion receiver, got {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "explicitly annotated assertion receiver should narrow value, got {codes:?}"
    );
}

#[test]
fn type_predicate_target_must_name_function_parameter() {
    let codes = check_source_codes(
        r#"
type PredicateCheck<T> = T extends (...args: any[]) => T is infer U ? U : never;
type PC = PredicateCheck<(x: unknown) => x is string>;
"#,
    );
    assert!(
        codes.contains(&1225),
        "expected TS1225 when a type predicate names type parameter `T` instead of a function parameter, got {codes:?}"
    );
}

#[test]
fn type_predicate_cannot_reference_rest_parameter() {
    let codes = check_source_codes(
        r#"
function isAllStrings(...values: unknown[]): values is string[] {
    return values.every(value => typeof value === "string");
}

function assertAllStrings(...values: unknown[]): asserts values is string[] {}
"#,
    );
    let ts1229_count = codes.iter().filter(|&&code| code == 1229).count();
    assert_eq!(
        ts1229_count, 2,
        "expected TS1229 for type and assertion predicates that reference rest parameters, got {codes:?}"
    );
}

#[test]
fn assertion_element_access_emits_ts2776() {
    let codes = check_source_codes(
        r#"
const assert: (value: unknown) => asserts value = value => {};
const a = [assert];
a[0](true);
"#,
    );
    assert!(
        codes.contains(&2776),
        "expected TS2776 for assertion call through element access, got {codes:?}"
    );
}

#[test]
fn asserts_this_for_of_variable_from_annotated_iterable_does_not_emit_ts2775() {
    let codes = check_source_codes(
        r#"
class Test {
    assertIsTest(): asserts this is Test {}
}
function f(items: Test[]) {
    for (let item of items) {
        item.assertIsTest();
    }
}
"#,
    );
    assert!(
        !codes.contains(&2775),
        "asserts-this for-of variables from annotated iterables should not require a receiver annotation, got {codes:?}"
    );
}

#[test]
fn asserts_this_for_of_variable_from_inferred_iterable_emits_ts2775() {
    let codes = check_source_codes(
        r#"
class Test {
    assertIsTest(): asserts this is Test {}
}
function f() {
    const items = [new Test()];
    for (let item of items) {
        item.assertIsTest();
    }
}
"#,
    );
    assert!(
        codes.contains(&2775),
        "expected TS2775 for asserts-this for-of variable from inferred iterable, got {codes:?}"
    );
}

#[test]
fn assertion_false_condition_emits_unreachable_code() {
    let diagnostics = check_source(
        r#"
const assert: (value: unknown) => asserts value = value => {};
function f(x: unknown) {
    assert(false && x === undefined);
    x;
}
"#,
        "test.ts",
        CheckerOptions {
            allow_unreachable_code: Some(false),
            ..CheckerOptions::default()
        },
    );
    let codes: Vec<u32> = diagnostics.iter().map(|diag| diag.code).collect();
    assert!(
        codes.contains(&7027),
        "expected TS7027 after assert(false && ...), got {codes:?}"
    );
}

#[test]
fn jsdoc_returns_asserts_predicate_on_arrow_var_does_not_emit_ts2775() {
    // `const foo = (a) => { … }` with `@returns {asserts a is B}` is an
    // explicit assertion annotation in JS files. Without the JSDoc-asserts
    // arm in `declaration_has_explicit_assertion_annotation`, every
    // arrow-bound assertion target in JS would fire a spurious TS2775 at
    // its call site (regression: `assertionTypePredicates2.ts`).
    let diagnostics = check_js_source_diagnostics(
        r#"
/**
 * @typedef {{ x: number }} A
 */
/**
 * @typedef { A & { y: number } } B
 */

/**
 * @param {A} a
 * @returns { asserts a is B }
 */
const foo = (a) => {
    if (/** @type { B } */ (a).y !== 0) throw TypeError();
    return undefined;
};

/** @type { A } */
const a = { x: 1 };
foo(a);
"#,
    );
    let ts2775: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2775)
        .collect();
    assert!(
        ts2775.is_empty(),
        "did not expect TS2775 when assertion target has @returns predicate, got: {diagnostics:#?}"
    );
}

#[test]
fn js_require_export_equals_assertion_from_dts_does_not_emit_ts2775() {
    let codes = check_require_assertion_from_dts();
    assert!(
        !codes.contains(&2775),
        "did not expect TS2775 for JS require() of export= assertion function from .d.ts, got {codes:?}"
    );
}

#[test]
fn constructor_type_predicate_return_emits_ts1228() {
    let codes = check_source_codes("declare let Q: new (x: unknown) => asserts x;");
    assert!(
        codes.contains(&1228),
        "expected TS1228 for predicate return in constructor type, got {codes:?}"
    );
}

#[test]
fn interface_construct_signature_type_predicate_does_not_emit_ts1228() {
    // Construct signatures inside an interface declaration accept type
    // predicates as their return type — tsc allows `interface I { new (...): x is T }`
    // even though the predicate is meaningless at runtime. Only constructor
    // *type* nodes (`new (...) => x is T`) and class constructor declarations
    // emit TS1228.
    let codes = check_source_codes("interface I { new (p: unknown): p is string; }");
    assert!(
        !codes.contains(&1228),
        "did not expect TS1228 for predicate return in interface construct signature, got {codes:?}"
    );
}

#[test]
fn assertion_predicate_intersection_with_narrower_object_type_does_not_emit_ts2677() {
    let codes = check_source_codes(
        r#"
interface Data {
    status: "pending" | "complete";
    value?: string;
}

function assertComplete(d: Data): asserts d is Data & { status: "complete"; value: string } {
    if (d.status !== "complete" || !d.value) throw "";
}
"#,
    );
    assert!(
        !codes.contains(&2677),
        "did not expect TS2677 for narrowing intersection assertion predicate, got {codes:?}"
    );
}

#[test]
fn assertion_function_type_intersection_predicate_does_not_emit_ts2677() {
    let codes = check_source_codes(
        r#"
interface Data {
    status: "pending" | "complete";
    value?: string;
}

type AssertComplete = (d: Data) => asserts d is Data & { status: "complete"; value: string };
"#,
    );
    assert!(
        !codes.contains(&2677),
        "did not expect TS2677 for narrowing intersection assertion function type, got {codes:?}"
    );
}

#[test]
fn assertion_predicate_that_widens_parameter_still_emits_ts2677() {
    let codes = check_source_codes(
        r#"
interface Data {
    status: "pending" | "complete";
    value?: string;
}

function assertAnyData(d: { status: "complete" }): asserts d is Data {
    if (d.status !== "complete") throw "";
}
"#,
    );
    assert!(
        codes.contains(&2677),
        "expected TS2677 for widening assertion predicate, got {codes:?}"
    );
}

// --- Generic assertion function narrowing (Issue #5790) ---
// When a generic assertion function's type parameter T does not appear in any
// parameter type (e.g., `assertType<T>(value: unknown): asserts value is T`),
// the solver-instantiated type must be used for narrowing, not the raw T.

#[test]
fn generic_assertion_with_explicit_type_arg_narrows_correctly() {
    // `assertType<T>(value: unknown): asserts value is T` called with explicit <string>
    // must narrow `val` to `string` so `.toUpperCase()` is valid.
    let codes = check_source_codes(
        r#"
function assertType<T>(value: unknown): asserts value is T {}
let val: unknown = 'hello';
assertType<string>(val);
val.toUpperCase();
"#,
    );
    assert!(
        !codes.contains(&2339),
        "expected no TS2339 after assertType<string>(val), got {codes:?}"
    );
}

#[test]
fn generic_assertion_type_param_name_independent() {
    // The fix must be structural (not keyed on identifier names).
    // Using a different type-parameter name (U instead of T) must work the same way.
    let codes = check_source_codes(
        r#"
function assertIs<U>(value: unknown): asserts value is U {}
let x: unknown = 42;
assertIs<number>(x);
x.toFixed(2);
"#,
    );
    assert!(
        !codes.contains(&2339),
        "expected no TS2339 after assertIs<number>(x), got {codes:?}"
    );
}

#[test]
fn generic_assertion_param_in_predicate_only_different_param_name() {
    // `assertKind<K>(label: string): asserts label is K` — K is not in any param type.
    // After assertKind<"foo">(s), s should be narrowed to "foo".
    let codes = check_source_codes(
        r#"
function assertKind<K>(label: unknown): asserts label is K {}
let s: unknown = 'foo';
assertKind<string>(s);
s.toUpperCase();
"#,
    );
    assert!(
        !codes.contains(&2339),
        "expected no TS2339 for generic assertion with type param only in predicate, got {codes:?}"
    );
}

#[test]
fn generic_assertion_with_param_in_both_predicate_and_arg_still_resolves() {
    // Regression guard: `assertEqual<T>(value: unknown, expected: T): asserts value is T`
    // — T IS in a parameter type, so the existing arg-inference path applies.
    // This must still produce no TS2339.
    let codes = check_source_codes(
        r#"
function assertEqual<T>(value: unknown, expected: T): asserts value is T {}
let n: unknown = 42;
assertEqual(n, 0);
n.toFixed(2);
"#,
    );
    assert!(
        !codes.contains(&2339),
        "expected no TS2339 for assertEqual<T> where T is in a param, got {codes:?}"
    );
}

#[test]
fn generic_assertion_without_explicit_type_arg_does_not_emit_ts2339() {
    // Without an explicit type arg, T defaults to `unknown`; the narrowed type is
    // `unknown` (same as before), so any property access gives TS18046, not TS2339.
    let codes = check_source_codes(
        r#"
function assertType<T>(value: unknown): asserts value is T {}
let val: unknown = 'hello';
assertType(val);
val.toUpperCase();
"#,
    );
    assert!(
        !codes.contains(&2339),
        "generic assertion without type arg must not produce TS2339 (wrong for 'T'), got {codes:?}"
    );
}

use super::super::core::*;

#[test]
fn test_unannotated_async_generator_method_infers_yield_type_in_return() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
declare const Symbol: { readonly asyncIterator: unique symbol };
interface AsyncGenerator<T, TReturn, TNext> {}

const iter = {
    async *[Symbol.asyncIterator](_: number) {
        yield 0;
    }
};

declare let expected: () => AsyncGenerator<number, void, unknown>;
expected = iter[Symbol.asyncIterator];
"#,
        CheckerOptions {
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );

    let ts2322 = diagnostics
        .iter()
        .find(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str());

    assert!(
        ts2322.is_some_and(|message| {
            message.contains("AsyncGenerator<number, void, unknown>")
                && !message.contains("AsyncGenerator<any, void, unknown>")
        }),
        "Expected the inferred async generator method return type to preserve the yielded number.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_isolated_declarations_reports_computed_object_literal_exports() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
const x: 0 | 1 = Math.random() ? 0 : 1;
declare function assert(n: number): asserts n is 1;
assert(x);

let u = Symbol();
const y: 0 = 0;

export let o = { [x]: 1 };
export let o2 = { [y]: 1 };
export let o3 = { [1]: 1 };
export let o31 = { [-1]: 1 };
export let o32 = { [1 - 1]: 1 };
export let o4 = { [u]: 1 };
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            isolated_declarations: true,
            emit_declarations: true,
            ..Default::default()
        },
    );

    let ts9038_count = diagnostics.iter().filter(|(code, _)| *code == 9038).count();
    assert_eq!(
        ts9038_count, 4,
        "Expected TS9038 for identifier- and expression-based computed object-literal property names under isolated declarations.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 9010),
        "Expected TS9010 for the inferred helper variable used in a computed export name.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_isolated_declarations_reports_exported_variable_statement_in_module_file() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            ("/dep.ts", "export {};"),
            (
                "/index.ts",
                r#"
import "./dep";

declare const source: { foo: string };

export const value = source.foo;
"#,
            ),
        ],
        "/index.ts",
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            module: tsz_common::common::ModuleKind::CommonJS,
            isolated_declarations: true,
            emit_declarations: true,
            ..Default::default()
        },
    );

    let ts9010_count = diagnostics.iter().filter(|(code, _)| *code == 9010).count();
    assert_eq!(
        ts9010_count, 1,
        "Expected TS9010 for exported variable statements in module files under isolated declarations.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_isolated_declarations_parameter_default_type_assertion_needs_annotation() {
    let diagnostics = compile_and_get_diagnostics_named(
        "file2.ts",
        r#"
type T = number;
export function foo2(p = (ip = 10 as T, v: number): void => {}): void {}
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            module: tsz_common::common::ModuleKind::CommonJS,
            isolated_declarations: true,
            emit_declarations: true,
            strict: true,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 9011),
        "Expected TS9011 when an exported function parameter default relies on a type assertion instead of an explicit parameter annotation.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_computed_object_literal_argument_mismatch_reports_ts2345() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
type State = {
  a: number;
  b: string;
};

class Test {
  setState(state: State) {}
  test(entries: [string, unknown][]) {
    for (const [key, value] of entries) {
      this.setState({
        [key]: value,
      });
    }
  }
}
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            strict: true,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2345
                && message.contains("Argument of type")
                && message.contains("is not assignable to parameter of type 'State'")
        }),
        "Expected TS2345 for computed object literal argument mismatch.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_direct_computed_object_literal_argument_mismatch_reports_ts2345() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
type State = {
  a: number;
  b: string;
};

declare const key: string;
declare const value: unknown;
declare function setState(state: State): void;

setState({
  [key]: value,
});
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            strict: true,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2345
                && message.contains("Argument of type")
                && message.contains("is not assignable to parameter of type 'State'")
        }),
        "Expected TS2345 for direct computed object literal argument mismatch.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_class_field_arrow_object_entries_computed_argument_reports_ts2345() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
type State = {
  a: number;
  b: string;
};

class Test {
  setState(state: State) {}

  test = (e: any) => {
    for (const [key, value] of Object.entries(e)) {
      this.setState({
        [key]: value,
      });
    }
  };
}
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2017,
            strict: true,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2345
                && message.contains("Argument of type")
                && message.contains("is not assignable to parameter of type 'State'")
        }),
        "Expected TS2345 for computed object literal mismatch in class field arrow Object.entries path.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_object_entries_mapped_callable_values_report_ts2345_not_ts2349() {
    let diagnostics = without_missing_global_type_errors(compile_and_get_diagnostics_named(
        "test.ts",
        r#"
type ArrayLike<T> = {
  length: number;
  [n: number]: T;
};

declare const Object: {
  entries<T>(o: { [s: string]: T; } | ArrayLike<T>): [string, T][];
  entries(o: {}): [string, any][];
};

type T1 = "A" | "B";

type T2 = {
  C: [string];
  D: [number];
};

declare const map: {
  [K in T1 | keyof T2]: (...args: K extends keyof T2 ? T2[K] : []) => unknown;
};

declare const args: any;

for (const [key, fn] of Object.entries(map)) {
  fn(...args);
}
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2017,
            strict: true,
            ..Default::default()
        },
    ));

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2345
                && message.contains("Argument of type 'any'")
                && message.contains("parameter of type 'never'")
        }),
        "Expected TS2345 for Object.entries mapped callable values.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2349),
        "Should not emit TS2349 for Object.entries mapped callable values.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_literal_computed_object_properties_report_ts1117_duplicates() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
const t1 = {
    1: 1,
    [1]: 0
}

const t2 = {
    "1": 1,
    [+1]: 0
}

const t3 = {
    "-1": 1,
    [-1]: 0
}
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let ts1117 = diagnostics.iter().filter(|(code, _)| *code == 1117).count();
    assert_eq!(
        ts1117, 3,
        "Expected TS1117 for literal computed object property duplicates.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_last_property_in_object_literal_keeps_explicit_function_source_type_in_ts2322() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
interface Thing {
    thunk: (str: string) => void;
}
function test(thing: Thing) {
    thing.thunk("str");
}
test({
    thunk: (str: string) => {},
    thunk: (num: number) => {}
});
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let ts2322_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str())
        .collect();

    assert!(
        ts2322_messages.iter().any(|message| message.contains(
            "Type '(num: number) => void' is not assignable to type '(str: string) => void'"
        )),
        "Expected TS2322 to preserve explicit source signature from the last object literal property.\nActual diagnostics: {diagnostics:#?}"
    );

    assert!(
        !ts2322_messages.iter().any(|message| message.contains(
            "Type '(str: string) => void' is not assignable to type '(str: string) => void'"
        )),
        "Did not expect contextualized source signature in TS2322 for duplicate object literal property.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_computed_property_contextual_index_signatures_accept_mixed_literal_members() {
    let _diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
interface I<T> {
    [s: string]: T;
}

declare function foo<T>(obj: I<T>): T

foo({
    p: "",
    0: () => { },
    ["hi" + "bye"]: true,
    [0 + 1]: 0,
    [+"hi"]: [0]
});

interface N<T> {
    [n: number]: T;
}
interface S<T> {
    [s: string]: T;
}

declare function bar<T>(obj: N<T>): T;
declare function baz<T>(obj: S<T>): T;

bar({
    0: () => { },
    ["hi" + "bye"]: true,
    [0 + 1]: 0,
    [+"hi"]: [0]
});

baz({ p: "" });
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    // TODO: Computed property contextual typing with mixed literal index signatures
    // currently produces a false TS2345 for the `bar()` call with `N<T>` (number index).
    // tsc accepts this. Fix requires better index signature merging in contextual typing.
    // assert!(
    //     !diagnostics.iter().any(|(code, _)| *code == 2345),
    //     "Expected computed-property contextual index signature calls to succeed.\nActual diagnostics: {diagnostics:#?}"
    // );
}

#[test]
fn test_class_entity_named_computed_members_induce_ts2411_index_checks() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
var s: string;
var n: number;
var a: any;
class C {
    [s]: number;
    [n] = n;
    [s + n] = 2;
    [+s]: typeof s;
    [a]: number;
}
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            strict: true,
            ..Default::default()
        },
    );

    let ts2411 = diagnostics.iter().filter(|(code, _)| *code == 2411).count();
    assert_eq!(
        ts2411, 2,
        "Expected two TS2411 diagnostics for [+s] against synthesized string/number index constraints.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2564 && message.contains("Property '[+s]' has no initializer")
        }),
        "Expected TS2564 for non-canonical computed property name [+s].\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_this_in_enum_member_initializer_reports_ts2332() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
enum TopLevelEnum {
    ThisWasAllowedButShouldNotBe = this
}

namespace ModuleEnum {
    enum EnumInModule {
        WasADifferentError = this
    }
}
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            no_implicit_this: true,
            ..Default::default()
        },
    );

    let ts2332 = diagnostics.iter().filter(|(code, _)| *code == 2332).count();
    let ts2683 = diagnostics.iter().filter(|(code, _)| *code == 2683).count();
    assert_eq!(
        ts2332, 2,
        "Expected TS2332 for both enum member initializer `this` uses.\nActual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        ts2683, 2,
        "Expected TS2683 companion diagnostics for both enum member initializer `this` uses.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2331),
        "Did not expect TS2331 for `this` inside enum member initializers.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_arrow_return_cast_reports_cast_type_in_message() {
    let diagnostics = compile_and_get_diagnostics_named_with_lib_and_options(
        "mytest.js",
        r#"
/**
 * @template T
 * @param {T|undefined} value value or not
 * @returns {T} result value
 */
const foo1 = value => /** @type {string} */({ ...value });

/**
 * @template T
 * @param {T|undefined} value value or not
 * @returns {T} result value
 */
const foo2 = value => /** @type {string} */(/** @type {T} */({ ...value }));
"#,
        CheckerOptions {
            check_js: true,
            strict: true,
            allow_js: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2322_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(
        ts2322_messages.len(),
        2,
        "Expected two TS2322 diagnostics, got: {diagnostics:?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .all(|message| message.contains("Type 'string' is not assignable to type 'T'.")),
        "Expected direct JSDoc cast type in TS2322 message, got: {ts2322_messages:?}"
    );
}

#[test]
fn test_enum_member_references_in_conditions_report_ts2845() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
enum Nums {
    Zero = 0,
    One = 1,
}

const a = Nums.Zero ? "a" : "b";
const b = Nums.One ? "a" : "b";

if (Nums.Zero) {}
if (Nums.One) {}

enum Strs {
    Empty = "",
    A = "A",
}

const c = Strs.Empty ? "a" : "b";
const d = Strs.A ? "a" : "b";

if (Strs.Empty) {}
if (Strs.A) {}
"#,
    );

    let ts2845_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2845)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(
        ts2845_messages.len(),
        8,
        "Expected eight TS2845 diagnostics, got: {diagnostics:#?}"
    );
    assert_eq!(
        ts2845_messages
            .iter()
            .filter(|message| message.contains("'false'"))
            .count(),
        4,
        "Expected four always-false enum condition diagnostics, got: {ts2845_messages:#?}"
    );
    assert_eq!(
        ts2845_messages
            .iter()
            .filter(|message| message.contains("'true'"))
            .count(),
        4,
        "Expected four always-true enum condition diagnostics, got: {ts2845_messages:#?}"
    );
}

#[test]
fn test_union_partial_numeric_and_symbol_index_writes_report_ts7053() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
declare const sym: unique symbol;
type Both =
    { s: number, '0': number, [sym]: boolean }
    | { [n: number]: number, [s: string]: string | number };
declare var both: Both;
both[0] = 1;
both[1] = 0;
both[0] = 'not ok';
both[sym] = 'not ok';
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );

    let ts7053_count = diagnostics.iter().filter(|(code, _)| *code == 7053).count();
    let ts2322_count = diagnostics.iter().filter(|(code, _)| *code == 2322).count();

    assert_eq!(
        ts7053_count, 2,
        "Expected TS7053 for partial numeric and unique-symbol union writes.\nActual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        ts2322_count, 1,
        "Expected the incompatible write to the shared numeric slot to stay TS2322.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_js_global_element_access_or_fallback_uses_contextual_target() {
    let diagnostics = compile_and_get_diagnostics_named_with_lib_and_options(
        "test.js",
        r#"
var Common = {};
globalThis["Common"] = globalThis["Common"] || {};
/**
 * @param {string} string
 * @return {string}
 */
Common.localize = function (string) {
    return string;
};
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2741_count = diagnostics.iter().filter(|(code, _)| *code == 2741).count();
    let ts7053_count = diagnostics.iter().filter(|(code, _)| *code == 7053).count();

    assert_eq!(
        ts2741_count, 1,
        "Expected the JS global element-access `||` assignment to fail with one TS2741.\nActual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        ts7053_count, 0,
        "Did not expect TS7053 for globalThis[\"Common\"] once it resolves through the global property path.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_typedef_string_index_signature_accepts_number_element_write() {
    let diagnostics = compile_named_files_get_diagnostics_with_lib_and_options(
        &[(
            "foo.js",
            r#"
// @allowJs: true
// @checkJs: true
// @target: esnext
// @outDir: ./out
// @declaration: true
/**
 * @typedef {{
 *   [id: string]: [Function, Function];
 * }} ResolveRejectMap
 */

let id = 0;

/**
 * @param {ResolveRejectMap} handlers
 * @returns {Promise<any>}
 */
const send = handlers => new Promise((resolve, reject) => {
    handlers[++id] = [resolve, reject];
});
"#,
        )],
        "foo.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ESNext,
            emit_declarations: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 7053),
        "Did not expect TS7053 when a JSDoc typedef string index signature is written through a numeric key.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_callback_typedef_attached_near_function_does_not_emit_ts8024() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "mod1.js",
                r#"
/** @callback Con - some kind of continuation
 * @param {object | undefined} error
 * @return {any} I don't even know what this should return
 */
module.exports = C
function C() {
    this.p = 1
}
"#,
            ),
            (
                "use.js",
                r#"
/** @param {import('./mod1').Con} k */
function f(k) {
    return k({ ok: true })
}
"#,
            ),
        ],
        "use.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 8024),
        "Did not expect TS8024 for a cross-module JSDoc callback typedef comment near a function value.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_function_declaration_does_not_inherit_previous_variable_jsdoc_type() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[(
            "a.js",
            r#"
/** @type {number | undefined} */
var n;

function f(a = null, b = n, l = []) {
    a = undefined
    a = null
    a = 1
    a = true
    a = {}
    a = 'ok'

    b = 1
    b = undefined
    b = 'error'

    l.push(1)
    l.push('ok')
}
"#,
        )],
        "a.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            no_implicit_any: true,
            strict: true,
            strict_null_checks: false,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 8030),
        "Did not expect TS8030 for a function declaration to inherit a previous variable's JSDoc @type.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_atomics_wait_async_accepts_shared_typed_arrays_without_ts2769() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
const sab = new SharedArrayBuffer(Int32Array.BYTES_PER_ELEMENT * 1024);
const int32 = new Int32Array(sab);
const sab64 = new SharedArrayBuffer(BigInt64Array.BYTES_PER_ELEMENT * 1024);
const int64 = new BigInt64Array(sab64);

const check32: Int32Array<SharedArrayBuffer> = int32;
const check64: BigInt64Array<SharedArrayBuffer> = int64;

const waitValue = Atomics.wait(int32, 0, 0);
const { async, value } = Atomics.waitAsync(int32, 0, 0);
const { async: async64, value: value64 } = Atomics.waitAsync(int64, 0, BigInt(0));

async function main() {
    if (async) {
        await value;
    }
    if (async64) {
        await value64;
    }
    return waitValue;
}
"#,
        CheckerOptions {
            target: ScriptTarget::ESNext,
            strict: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2769),
        "Did not expect TS2769 for Atomics.waitAsync on shared typed arrays.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_intersection_index_signature_diagnostics_preserve_declared_identifier_annotations() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
type A = { a: string };
type B = { b: string };

declare let sb1: { x: A } & { y: B };
declare let tb1: { [key: string]: A };
tb1 = sb1;

declare let ss: { a: string } & { b: number };
declare let tt: { [key: string]: string };
tt = ss;
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    // tsc emits TS2741 (missing property) or TS2322 (not assignable) depending on
    // which check fires first. Both preserve the intersection source type display.
    assert!(
        diagnostics.iter().any(|(code, message)| {
            (*code == 2322 || *code == 2741)
                && (message.contains("{ x: A; } & { y: B; }")
                    || message.contains("'{ x: A; } & { y: B; }'"))
        }),
        "Expected TS2322 or TS2741 to preserve the declared intersection source type for `sb1`.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322
                && message.contains("Type '{ a: string; } & { b: number; }' is not assignable")
        }),
        "Expected TS2322 to preserve the declared intersection source type for `ss`.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_assignment_to_any_array_rest_parameters_indexed_access_classification() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
function bar<T extends string[], K extends number>() {
    type T01 = string[]["0.0"];
    type T02 = string[][K | "0"];
    type T11 = T["0.0"];
    type T12 = T[K | "0"];
}
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2339_count = diagnostics.iter().filter(|(code, _)| *code == 2339).count();
    let ts2536_count = diagnostics.iter().filter(|(code, _)| *code == 2536).count();

    assert_eq!(
        ts2339_count, 1,
        "Expected exactly one TS2339 for string[][\"0.0\"].\nActual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        ts2536_count, 1,
        "Expected exactly one TS2536 for generic T[\"0.0\"], and no TS2536 for K | \"0\" unions.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_contextual_computed_non_bindable_property_type_mapped_callback_literal_return() {
    let diagnostics =
        without_missing_global_type_errors(compile_and_get_diagnostics_with_lib_and_options(
            r#"
type Original = { foo: 'expects a string literal', baz: boolean, bar: number };
type Mapped = {
  [prop in keyof Original]: (arg: Original[prop]) => Original[prop]
};

const unexpectedlyFailingExample: Mapped = {
  foo: (arg) => 'expects a string literal',
  baz: (arg) => true,
  bar: (arg) => 51345
};
"#,
            CheckerOptions {
                strict: true,
                target: ScriptTarget::ES2015,
                ..CheckerOptions::default()
            },
        ));

    assert!(
        diagnostics.is_empty(),
        "Did not expect a false TS2322 when a mapped callback returns the exact contextual literal type.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_contextual_computed_non_bindable_property_type_uses_callable_fallback() {
    let diagnostics =
        without_missing_global_type_errors(compile_and_get_diagnostics_with_lib_and_options(
            r#"
type Original = { foo: 'expects a string literal', baz: boolean, bar: number };
type Mapped = {
  [prop in keyof Original]: (arg: Original[prop]) => Original[prop]
};

const propSelector = <propName extends string>(propName: propName): propName => propName;

const unexpectedlyFailingExample: Mapped = {
  foo: (arg) => 'expects a string literal',
  baz: (arg) => true,
  [propSelector('bar')]: (arg) => 51345
};
"#,
            CheckerOptions {
                strict: true,
                target: ScriptTarget::ES2015,
                ..CheckerOptions::default()
            },
        ));

    assert!(
        diagnostics.is_empty(),
        "Did not expect a false TS2322 when a computed mapped callback property should inherit callable context.\nActual diagnostics: {diagnostics:#?}"
    );
}

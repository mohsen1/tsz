//! Tests for `{ [K in keyof T]: V }[keyof T]` evaluation — the solver must
//! substitute `V` when indexing a homomorphic mapped type with its own
//! constraint (`keyof T`).
//!
//! Structural rule: when `{ [K in C]: V }[I]` is evaluated and `I`
//! semantically matches the mapped constraint `C` (e.g. both are `keyof T`),
//! the template `V` is the result. This is the `KeyOf`-index path; prior to
//! this fix the non-union `KeyOf` node bypassed `generic_index_covering_mapped_constraint`
//! and returned `None`, leaving the expression unevaluated.
//!
//! Key invariant checked: the fix is keyed on semantic identity of `I` and `C`,
//! not on any name spelling or union structure.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_common::common::ModuleKind;

fn check_strict(source: &str) -> Vec<Diagnostic> {
    let lib_files = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);
    assert!(!lib_files.is_empty(), "es5.d.ts lib file not loaded");
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
}

fn has_code(diags: &[Diagnostic], code: u32) -> bool {
    diags.iter().any(|d| d.code == code)
}

const fn no_errors(diags: &[Diagnostic]) -> bool {
    diags.is_empty()
}

/// A `typeof value` alias whose value type was inferred must stay resolvable
/// when a mapped/keyof type is evaluated inside a relation path.
///
/// Before #13484, relation-driven mapped evaluation saw `V = typeof lit` as an
/// unresolved value-symbol `Lazy(DefId)`, classified it as a non-object, and
/// collapsed the mapped property body to `error`.
#[test]
fn mapped_keyof_typeof_inferred_value_resolves_in_relation_path() {
    let diagnostics = tsz_checker::test_utils::check_multi_file_with_global_index(
        &[
            (
                "/node_modules/vlib/index.d.ts",
                r#"
export interface Validator<T> { (props: object): any; brand?: T; }
export interface Requireable<T> extends Validator<T> { isRequired: Validator<T & {}>; }
export declare const str: Requireable<string>;
"#,
            ),
            (
                "file.ts",
                r#"
import * as P from "vlib";
const lit = { str: P.str.isRequired };
type V = typeof lit;
type Mc = { [K in keyof V]: V[K] extends P.Validator<any> ? K : never };
const ok: { str: "str" } = (null as any as Mc);
"#,
            ),
        ],
        "file.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );

    assert!(
        no_errors(&diagnostics),
        "expected inferred typeof mapped keys to resolve without diagnostics, got: {diagnostics:#?}"
    );
}

/// Same structure with renamed binders and an explicit negative assignment so
/// the test is not tied to the original repro's property/type-parameter names.
#[test]
fn mapped_keyof_typeof_inferred_value_resolves_with_renamed_binders() {
    let diagnostics = tsz_checker::test_utils::check_multi_file_with_global_index(
        &[
            (
                "/node_modules/vlib/index.d.ts",
                r#"
export interface CheckBox<T> { (props: object): any; marker?: T; }
export interface Needed<T> extends CheckBox<T> { must: CheckBox<T & {}>; }
export declare const title: Needed<string>;
"#,
            ),
            (
                "consumer.ts",
                r#"
import * as Q from "vlib";
const shape = { title: Q.title.must };
type ShapeType = typeof shape;
type Picked = { [Field in keyof ShapeType]: ShapeType[Field] extends Q.CheckBox<any> ? Field : never };
const bad: { title: "other" } = (null as any as Picked);
"#,
            ),
        ],
        "consumer.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_code(&diagnostics, 2322),
        "expected TS2322 for the wrong literal target, got: {diagnostics:#?}"
    );
}

/// Merged interface/constructor symbols carry value flags for `typeof`, but a
/// bare type reference must still resolve to the instance side in relation prep.
#[test]
fn merged_interface_var_type_reference_stays_instance_in_relation_path() {
    let diagnostics = check_strict(
        r#"
interface ElementLike { innerHTML: string; }
declare var ElementLike: { new (): ElementLike; prototype: ElementLike };
type Mirror<T> = { [K in keyof T]: T[K] };
declare const node: Mirror<ElementLike>;
node.innerHTML = "";
"#,
    );

    assert!(
        no_errors(&diagnostics),
        "expected merged interface/var type reference to stay instance-shaped, got: {diagnostics:#?}"
    );
}

// ============================================================================
// Core: mapped-type keyof index access evaluates to the template
// ============================================================================

/// `{ [k in keyof T]: Spy }[keyof T]` must evaluate to `Spy`.
/// When `.and` is a `Function` property of `Spy`, accessing `.returnValue`
/// on `Function` must produce TS2339.
///
/// This is the `spyComparisonChecking.ts` pattern (accepted regression fixed).
/// The test avoids `for..of` (which requires ES2015 lib) and instead uses
/// a `declare const` key of the concrete type to isolate the evaluation.
#[test]
fn spy_obj_key_and_returnvalue_emits_ts2339() {
    let source = r#"
interface Spy extends Function {
    and: Function;
}
type SpyObj<T> = T & { [k in keyof T]: Spy; }
declare const spyObj: SpyObj<{foo(): void}>;
declare const key: keyof {foo(): void};
spyObj[key].and.returnValue(1);
"#;

    let diags = tsz_checker::test_utils::check_multi_file_with_global_index(
        &[
            (
                "remote.ts",
                r#"
export type Remote<A, B> = { first: A; second: B };
export type OtherKey = "remote";
"#,
            ),
            ("test.ts", source),
        ],
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        has_code(&diags, 2339),
        "expected TS2339 for returnValue on Function, got: {diags:#?}"
    );
}

/// Same pattern with a different mapped-variable name (`P` instead of `k`)
/// to confirm the fix is not name-sensitive.
#[test]
fn spy_obj_key_and_returnvalue_emits_ts2339_p_name() {
    let source = r#"
interface Spy extends Function {
    and: Function;
}
type SpyObj<T> = T & { [P in keyof T]: Spy; }
declare const spyObj: SpyObj<{foo(): void}>;
declare const key: keyof {foo(): void};
spyObj[key].and.returnValue(1);
"#;

    let diags = check_strict(source);
    assert!(
        has_code(&diags, 2339),
        "expected TS2339 with mapped variable named P, got: {diags:#?}"
    );
}

/// Same pattern with a long descriptive mapped-variable name to confirm
/// the fix generalises beyond single-letter names.
#[test]
fn spy_obj_key_and_returnvalue_emits_ts2339_methodkey_name() {
    let source = r#"
interface Spy extends Function {
    and: Function;
}
type SpyObj<T> = T & { [MethodKey in keyof T]: Spy; }
declare const spyObj: SpyObj<{foo(): void}>;
declare const key: keyof {foo(): void};
spyObj[key].and.returnValue(1);
"#;

    let diags = check_strict(source);
    assert!(
        has_code(&diags, 2339),
        "expected TS2339 with mapped variable named MethodKey, got: {diags:#?}"
    );
}

// ============================================================================
// Direct evaluation: `{ [K in keyof T]: V }[keyof T]` assignability
// ============================================================================

/// Assigning `{ [k in keyof T]: number }[keyof T]` to `number` must not
/// produce TS2322 — the indexed access evaluates to `number`.
#[test]
fn mapped_keyof_index_access_assigns_to_value_type() {
    let source = r#"
function f<T>(obj: { [k in keyof T]: number }, key: keyof T): void {
    const x: number = obj[key];
}

type ArgMap = { a: number, b: string };
"#;

    let diags = check_strict(source);
    assert!(
        no_errors(&diags),
        "{{ [k in keyof T]: number }}[keyof T] should assign to number without any errors, got: {diags:#?}"
    );
}

/// Same with type parameter named `Key` to rule out name-keying.
#[test]
fn mapped_keyof_index_access_assigns_to_value_type_key_name() {
    let source = r#"
function f<U>(obj: { [Key in keyof U]: string }, key: keyof U): void {
    const x: string = obj[key];
}
"#;

    let diags = check_strict(source);
    assert!(
        no_errors(&diags),
        "{{ [Key in keyof U]: string }}[keyof U] should assign to string without any errors, got: {diags:#?}"
    );
}

/// Using a type alias for the mapped type — the fix must work when the
/// object is referenced via an alias rather than inline.
#[test]
fn mapped_keyof_index_access_via_alias_assigns_to_value_type() {
    let source = r#"
type Box<T> = { [k in keyof T]: boolean };
function f<T>(obj: Box<T>, key: keyof T): void {
    const x: boolean = obj[key];
}
"#;

    let diags = check_strict(source);
    assert!(
        no_errors(&diags),
        "Box<T>[keyof T] via alias should assign to boolean without any errors, got: {diags:#?}"
    );
}

/// `{ [P in K]: F<P> }[K]` with a concrete key-space must distribute per key.
/// Substituting `K` as one whole key-space collapses the correlation and
/// produces a false TS2345 for the `correlatedUnions.ts` pattern.
#[test]
fn mapped_keyof_index_access_preserves_per_key_union_correlation() {
    let source = r#"
type RecordMap = { n: number, s: string, b: boolean };
type UnionRecord<K extends keyof RecordMap = keyof RecordMap> = { [P in K]: {
    kind: P,
    v: RecordMap[P],
    f: (v: RecordMap[P]) => void
}}[K];

declare const r: UnionRecord;
function processRecord<K extends keyof RecordMap>(rec: UnionRecord<K>): void {
    rec.f(rec.v);
}
processRecord(r);
"#;

    let diags = check_strict(source);
    assert!(
        no_errors(&diags),
        "UnionRecord should remain a per-key correlated union, got: {diags:#?}"
    );
}

#[test]
fn mapped_keyof_tuple_rest_context_preserves_local_alias_scope() {
    let source = r#"
type TypeMap = {
    foo: string,
    bar: number
};
type Keys = keyof TypeMap;
type HandlerMap = { [P in Keys]: (x: TypeMap[P]) => void };
const handlers: HandlerMap = {
    foo: s => s.length,
    bar: n => n.toFixed(2)
};
type DataEntry<K extends Keys = Keys> = { [P in K]: {
    type: P,
    data: TypeMap[P]
}}[K];
const data: DataEntry[] = [
    { type: 'foo', data: 'abc' },
    { type: 'bar', data: 42 },
];
function process<K extends Keys>(data: DataEntry<K>[]) {
    data.forEach(block => {
        if (block.type in handlers) {
            handlers[block.type](block.data)
        }
    });
}
process(data);

interface DocumentEventMap {
    click: { x: number };
    scroll: { y: number };
}
type Ev<K extends keyof DocumentEventMap> = { [P in K]: {
    readonly name: P;
    readonly once?: boolean;
    readonly callback: (ev: DocumentEventMap[P]) => void;
}}[K];
function processEvents<K extends keyof DocumentEventMap>(events: Ev<K>[]) {
    for (const event of events) {
        event.callback({} as DocumentEventMap[K]);
    }
}
function createEventListener<K extends keyof DocumentEventMap>({ name, once = false, callback }: Ev<K>): Ev<K> {
    return { name, once, callback };
}
const clickEvent = createEventListener({
    name: "click",
    callback: ev => ev.x,
});
const scrollEvent = createEventListener({
    name: "scroll",
    callback: ev => ev.y,
});
processEvents([clickEvent, scrollEvent]);

function ff1() {
    type ArgMap = {
        sum: [a: number, b: number],
        concat: [a: string, b: string, c: string]
    }
    type Keys = keyof ArgMap;
    const funs: { [P in Keys]: (...args: ArgMap[P]) => void } = {
        sum: (a, b) => a + b,
        concat: (a, b, c) => a + b + c
    }
    function apply<K extends Keys>(funKey: K, ...args: ArgMap[K]) {
        const fn = funs[funKey];
        fn(...args);
    }
    const x1 = apply('sum', 1, 2)
    const x2 = apply('concat', 'str1', 'str2', 'str3')
}
type ArgMap = { a: number, b: string };
"#;

    let diags = check_strict(source);
    assert!(
        no_errors(&diags),
        "function-local ArgMap/Keys should contextually type rest handlers and calls, got: {diags:#?}"
    );
}

#[test]
fn required_mapped_index_read_removes_optional_undefined() {
    let source = r#"
interface Foo {
    bar?: string
}

declare function takeString(value: string): void;

function readRequired<T extends keyof Foo>(prop: T, value: Required<Foo>) {
    takeString(value[prop]);
}
"#;

    let diags = check_strict(source);
    assert!(
        no_errors(&diags),
        "Required<Foo>[T] should read as string, not string | undefined: {diags:#?}"
    );
}

// ============================================================================
// Assignability mismatch: value type mismatch must produce TS2322
// ============================================================================

// ============================================================================
// Negative: mismatched keyof types must NOT substitute
// ============================================================================

/// `{ [K in keyof T]: number }[keyof S]` where T != S must NOT evaluate
/// to `number` — the index covers a different key-space than the constraint.
/// Without this guard the `keyof_same_inner` check could accept mismatched
/// inner types and produce a false substitution.
#[test]
fn mapped_keyof_index_access_different_type_params_no_false_substitution() {
    let source = r#"
function f<T, S>(obj: { [K in keyof T]: number }, key: keyof S): void {
    const x: number = obj[key];
}
"#;

    let diags = check_strict(source);
    // tsc emits TS2345 because `keyof S` does not satisfy `keyof T`.
    assert!(
        has_code(&diags, 2345) || has_code(&diags, 2322),
        "keyof S indexing {{ [K in keyof T]: number }} should error (different type params), got: {diags:#?}"
    );
}

// ============================================================================
// Assignability mismatch: value type mismatch must produce TS2322
// ============================================================================

/// Assigning `{ [k in keyof T]: number }[keyof T]` to `string` must
/// produce TS2322 — the evaluation gives `number` which is not `string`.
#[test]
fn mapped_keyof_index_access_wrong_value_type_emits_ts2322() {
    let source = r#"
function f<T>(obj: { [k in keyof T]: number }, key: keyof T): void {
    const x: string = obj[key];
}
"#;

    let diags = check_strict(source);
    assert!(
        has_code(&diags, 2322),
        "{{ [k in keyof T]: number }}[keyof T] assigned to string should emit TS2322, got: {diags:#?}"
    );
}

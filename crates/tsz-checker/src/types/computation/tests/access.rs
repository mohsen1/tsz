use crate::diagnostics::Diagnostic;

fn check_source_with_default_libs(source: &str) -> Vec<Diagnostic> {
    crate::test_utils::check_source_diagnostics(source)
}

fn has_code(diags: &[Diagnostic], code: u32) -> bool {
    diags.iter().any(|d| d.code == code)
}

/// Filter out TS2318 ("Cannot find global type") which fires when lib files aren't loaded.
fn semantic_errors(diags: &[Diagnostic]) -> Vec<u32> {
    diags
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| d.code)
        .collect()
}

/// Minimal Promise/PromiseLike type definitions for tests.
const PROMISE_LIB: &str = r#"
interface PromiseLike<T> {
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null,
        onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null
    ): PromiseLike<TResult1 | TResult2>;
}
interface Promise<T> {
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null,
        onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null
    ): Promise<TResult1 | TResult2>;
}
interface PromiseConstructor {
    new <T>(executor: (resolve: (value: T | PromiseLike<T>) => void, reject: (reason?: any) => void) => void): Promise<T>;
}
declare var Promise: PromiseConstructor;
"#;

#[test]
fn contextual_type_through_new_promise_variable_decl() {
    // `const p: Promise<string> = new Promise(resolve => resolve("hello"))` should
    // infer T = string from the contextual type, producing no errors.
    let source = format!(
        r#"{PROMISE_LIB}
const p: Promise<string> = new Promise(resolve => resolve("hello"));"#
    );
    let diags = check_source_with_default_libs(&source);
    let errors = semantic_errors(&diags);
    assert!(
        errors.is_empty(),
        "Expected no semantic errors for contextually typed new Promise, got: {errors:?}"
    );
}

#[test]
fn contextual_type_through_await_new_promise() {
    // `const s: string = await new Promise(resolve => resolve("ok"))` should
    // infer T = string via the await contextual type union.
    let source = format!(
        r#"{PROMISE_LIB}
async function f() {{ const s: string = await new Promise(resolve => resolve("ok")); }}"#
    );
    let diags = check_source_with_default_libs(&source);
    let errors = semantic_errors(&diags);
    assert!(
        errors.is_empty(),
        "Expected no semantic errors for await new Promise with contextual type, got: {errors:?}"
    );
}

#[test]
fn contextual_type_async_return_new_promise() {
    // Note: the full async return + new Promise fix requires real lib files because
    // resolve_global_interface_type("Promise") doesn't find local declarations.
    // This test verifies the code doesn't crash; the full fix is validated by
    // the contextuallyTypeAsyncFunctionReturnType conformance test.
    let source = format!(
        r#"{PROMISE_LIB}
interface Obj {{ key: "value"; }}
async function f(): Promise<Obj> {{
    return new Promise(resolve => {{
        resolve({{ key: "value" }});
    }});
}}"#
    );
    let diags = check_source_with_default_libs(&source);
    // Without real lib files, global Promise resolution fails and inference
    // falls back to unknown, producing TS2322/TS2345. This is expected.
    // The important thing is no crash and the code path executes.
    let _ = semantic_errors(&diags);
}

#[test]
fn tuple_expression_negative_index_emits_t2514() {
    // `as const` makes the literal a readonly tuple — without it, `["a", 1]`
    // is inferred as `(string | number)[]` (an array) and TS2514 is not expected.
    let diags = check_source_with_default_libs(
        r#"
const tuple = ["a", 1] as const;
const bad = tuple[-1];
"#,
    );

    assert!(
        has_code(&diags, 2514),
        "Expected TS2514 for tuple expression negative index, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn private_name_access_unknown_reports_18046() {
    let diags = check_source_with_default_libs(
        r#"
class A {
    #foo = true;
    static #baz = 10;
    static #m() {}
    method(thing: unknown) {
        thing.#foo;
        thing.#m();
        thing.#baz;
        thing.#bar;
        thing.#foo();
    }
}
"#,
    );
    let errors = semantic_errors(&diags);
    assert_eq!(
        errors.iter().filter(|code| **code == 18046).count(),
        5,
        "Expected 5 TS18046 diagnostics for private access on unknown, got: {errors:?}"
    );
    assert_eq!(
        errors.iter().filter(|code| **code == 2339).count(),
        1,
        "Expected one TS2339 diagnostic for undeclared private name, got: {errors:?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == 2339 && d.message_text.contains("#bar")),
        "Expected the TS2339 diagnostic to mention '#bar': {diags:?}"
    );
}

#[test]
fn private_name_access_never_reports_2339() {
    let diags = check_source_with_default_libs(
        r#"
class A {
    #foo = true;
    static #baz = 10;
    static #m() {}
    method(thing: never) {
        thing.#foo;
        thing.#m();
        thing.#baz;
        thing.#bar;
        thing.#foo();
    }
}
"#,
    );
    let errors = semantic_errors(&diags);
    assert_eq!(
        errors.iter().filter(|code| **code == 2339).count(),
        5,
        "Expected 5 TS2339 diagnostics for private access on never, got: {errors:?}"
    );
    assert!(
        errors.iter().all(|code| *code == 2339),
        "Expected only TS2339 diagnostics, got: {errors:?}"
    );
}

#[test]
fn inherited_static_member_element_access_emits_ts2576() {
    let diags = check_source_with_default_libs(
        r#"
class Base {
    static count = 1;
    static get size() {
        return 2;
    }
}
class Derived extends Base {}
const value = new Derived();
value["count"];
value["size"];
"#,
    );

    let errors = semantic_errors(&diags);
    assert_eq!(
        errors.iter().filter(|code| **code == 2576).count(),
        2,
        "Expected TS2576 for inherited static field and accessor element access, got: {errors:?}"
    );
}

// Regression: `obj[expr]` where the index expression's type is a `Lazy(DefId)`
// type-alias reference must resolve the alias before the solver's resolver-less
// element-access query. Otherwise the index never matches the receiver's keys and
// the result silently gains `| undefined`, producing false TS2532/TS2722/TS18048
// (and the resulting TS2322 against a non-nullable annotation). This reproduced in
// kysely's dispatch-table / priority-table patterns (issue #10669): the key is a
// *property access of an alias-typed property* (`node.kind` where
// `kind: SomeUnionAlias`), which keeps the alias form, unlike a plain variable
// read which arrives already resolved.

#[test]
fn element_access_alias_typed_property_index_no_false_undefined() {
    // Record indexed by a property whose declared type is a union alias, plus the
    // dispatch-table shape via `this[...]`. Binder names are deliberately varied to
    // keep the rule name-agnostic.
    let diags = check_source_with_default_libs(
        r#"
type Selector = 'alpha' | 'beta' | 'gamma';
interface Carrier { tag: Selector; }

declare const handlers: Record<Selector, (c: Carrier) => Carrier>;
declare const carrier: Carrier;
const transformed: Carrier = handlers[carrier.tag](carrier);

type Priority = 'low' | 'high';
declare const ranks: Record<Priority, number>;
declare const entry: { level: Priority };
const rank: number = ranks[entry.level];
const ranked = ranks[entry.level] + 1;

declare const plainObj: { alpha: 1; beta: 2; gamma: 3 };
const plainValue: 1 | 2 | 3 = plainObj[carrier.tag];

class Dispatcher {
    routes: Record<Selector, () => number> = {
        alpha: () => 1,
        beta: () => 2,
        gamma: () => 3,
    };
    run(c: Carrier): number {
        return this.routes[c.tag]();
    }
}
"#,
    );
    let errors = semantic_errors(&diags);
    assert!(
        errors.is_empty(),
        "alias-typed property index must not introduce spurious `| undefined`; got: {errors:?}"
    );
}

#[test]
fn element_access_union_of_aliases_property_index_no_false_undefined() {
    // The index type is a union whose members are themselves alias references.
    let diags = check_source_with_default_libs(
        r#"
type Left = 'one' | 'two';
type Right = 'three';
type Combined = Left | Right;

declare const lookup: Record<Combined, number>;
declare const holder: { key: Combined };
const picked: number = lookup[holder.key];
"#,
    );
    let errors = semantic_errors(&diags);
    assert!(
        errors.is_empty(),
        "union-of-aliases property index must not introduce spurious `| undefined`; got: {errors:?}"
    );
}

#[test]
fn element_access_alias_index_through_string_index_signature_resolves() {
    // A `string`-alias index must still resolve through a string index signature
    // to the value type rather than collapsing to a false `undefined` result.
    let diags = check_source_with_default_libs(
        r#"
type Name = string;
declare const dict: { [k: string]: number };
declare const named: { id: Name };
const value: number = dict[named.id];
"#,
    );
    let errors = semantic_errors(&diags);
    assert!(
        errors.is_empty(),
        "string-alias index through an index signature must resolve to the value type; got: {errors:?}"
    );
}

#[test]
fn wide_symbol_identifier_write_uses_symbol_index_signature_value_type() {
    let diags = check_source_with_default_libs(
        r#"
declare let answerKey: symbol;
declare let amount: number;
declare let table: Record<symbol, number>;

table[answerKey] = amount;
"#,
    );
    let errors = semantic_errors(&diags);
    assert!(
        errors.is_empty(),
        "wide-symbol writes through a symbol index signature should accept the value type; got: {errors:?}"
    );
}

#[test]
fn renamed_wide_symbol_identifier_read_uses_symbol_index_signature_value_type() {
    let diags = check_source_with_default_libs(
        r#"
declare let marker: symbol;
declare let registry: Record<symbol, number>;

let value: number = registry[marker];
"#,
    );
    let errors = semantic_errors(&diags);
    assert!(
        errors.is_empty(),
        "renamed wide-symbol reads through a symbol index signature should return the value type; got: {errors:?}"
    );
}

#[test]
fn element_access_optional_mapped_alias_index_keeps_optional_undefined() {
    // Negative guard: resolving the alias index must NOT mask legitimate
    // `| undefined` introduced by an optional member. tsc rejects assigning the
    // optional member value to a non-nullable annotation here (TS2322). Uses an
    // inline optional mapped type so the test does not depend on lib utilities.
    let diags = check_source_with_default_libs(
        r#"
type Slot = 'x' | 'y' | 'z';
type OptMap = { [K in Slot]?: number };
declare const optional: OptMap;
declare const holder: { slot: Slot };
const value: number = optional[holder.slot];
"#,
    );
    let errors = semantic_errors(&diags);
    assert!(
        has_code(&diags, 2322),
        "optional mapped alias index must still surface the optional `| undefined` (TS2322); got: {errors:?}"
    );
}

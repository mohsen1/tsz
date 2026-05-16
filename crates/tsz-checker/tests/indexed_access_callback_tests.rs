//! Tests for generic indexed-access callback parameter inference.
//!
//! Covers the `T[K]` pattern where a type parameter `K extends keyof T` is used
//! as an index in a callback parameter type. When `K` is inferred from a string
//! literal argument, the callback parameter should resolve to the specific property
//! type (e.g. `T["name"]` = `string`) rather than the full union `T[keyof T]`.
//!
//! See: <https://github.com/mohsen1/tsz/issues/6978>

use tsz_checker::context::CheckerOptions;

fn relevant_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .filter(|(code, _)| *code != 2318)
        .collect()
}

/// Baseline: `T[K]` in callback param resolves to the specific property type when
/// `K` is inferred from a string literal argument.
///
/// `map(person, "name", (n) => n.toUpperCase())` — K = "name", so n: string.
#[test]
fn generic_callback_indexed_access_key_param_specific_property() {
    let source = r#"
function map<T, K extends keyof T, U>(
    obj: T,
    key: K,
    fn: (val: T[K]) => U
): U {
    return fn(obj[key]);
}
const person = { name: "John", age: 30 };
const result1 = map(person, "name", (n) => n.toUpperCase());
const result2 = map(person, "age", (a) => a * 2);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "K inferred as literal so T[K] should be specific property type, not union. Got: {diags:#?}"
    );
}

/// Renamed type parameters — the rule is structural, not sensitive to parameter names.
///
/// `pick(rec, "x", (v) => v + 1)` with `Obj/Key/Result` names must behave identically
/// to `T/K/U`.
#[test]
fn generic_callback_indexed_access_renamed_params_specific_property() {
    let source = r#"
function pick<Obj, Key extends keyof Obj, Result>(
    obj: Obj,
    key: Key,
    transform: (val: Obj[Key]) => Result
): Result {
    return transform(obj[key]);
}
const rec = { x: 1, y: "hello" };
const r1 = pick(rec, "x", (v) => v + 1);
const r2 = pick(rec, "y", (v) => v.toUpperCase());
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "T[K] with renamed params must still resolve to specific property. Got: {diags:#?}"
    );
}

/// Two independent key type params — each resolves to its own literal-indexed type.
///
/// When K1="count" and K2="label", the callback receives `number` and `string`
/// respectively, not the union `number | string` for both.
#[test]
fn generic_callback_indexed_access_two_independent_keys() {
    let source = r#"
function combine<T, K1 extends keyof T, K2 extends keyof T>(
    obj: T,
    k1: K1,
    k2: K2,
    fn: (v1: T[K1], v2: T[K2]) => string
): string {
    return fn(obj[k1], obj[k2]);
}
const data = { count: 42, label: "hello" };
const r = combine(data, "count", "label", (n, s) => n.toFixed(0) + s.trim());
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "two independent K params should each resolve to their own literal-indexed type. Got: {diags:#?}"
    );
}

/// `T[K]` nested inside an object wrapper in the callback parameter type.
///
/// `fn: (wrapper: { value: T[K] }) => void` — the literal-key preservation
/// must look through the wrapping object to detect the indexed access.
#[test]
fn generic_callback_indexed_access_wrapped_in_object_param() {
    let source = r#"
function wrapAndCall<T, K extends keyof T>(
    obj: T,
    key: K,
    fn: (wrapper: { value: T[K] }) => void
): void {
    fn({ value: obj[key] });
}
wrapAndCall({ score: 100 }, "score", (w) => { const n: number = w.value; });
wrapAndCall({ tag: "alpha" }, "tag", (w) => { const s: string = w.value; });
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "T[K] wrapped in an object type in the callback param must still resolve to the specific property. Got: {diags:#?}"
    );
}

/// When the key argument is a type-parameter (not a literal), the callback must
/// still compile without false positives — K propagates as a type parameter.
#[test]
fn generic_callback_indexed_access_variable_key_no_false_positive() {
    let source = r#"
function applyKey<T, K extends keyof T, U>(
    obj: T,
    key: K,
    fn: (val: T[K]) => U
): U {
    return fn(obj[key]);
}
function withDynKey<T, K extends keyof T>(obj: T, key: K): void {
    applyKey(obj, key, (v) => v);
}
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "variable key (not a literal) must not produce false positives on T[K] callback. Got: {diags:#?}"
    );
}

/// `T[K]` in an array return type resolves to the literal property type.
#[test]
fn generic_callback_indexed_access_array_return_type() {
    let source = r#"
function pluckOne<T, K extends keyof T>(item: T, key: K): T[K][] {
    return [item[key]];
}
const scores: number[] = pluckOne({ score: 1 }, "score");
const labels: string[] = pluckOne({ label: "ok" }, "label");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "T[K] in an array return type must resolve to the specific property type. Got: {diags:#?}"
    );
}

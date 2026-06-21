//! Regression tests for issue #14321.
//!
//! When inferring a callee's type argument and the argument's type is itself a
//! generic type parameter we are *not* inferring (e.g. `obj: T` passed to
//! `keyOf<K>(obj: Record<K, unknown>)`), `tsc` infers the callee parameter from
//! the source's apparent type — its constraint. So `K` of `Record<K, unknown>`
//! is inferred from a `T extends Record<string, unknown>` source as `string`,
//! not the constraint default `PropertyKey`.
//!
//! tsz previously left such a `K` at its `PropertyKey` constraint default,
//! producing an over-wide key and a downstream false `TS2339`/`TS2322` (mined
//! from radash `lowerize`). These tests pin the corrected apparent-type
//! inference for the `Record<K, V>` constructor form, the equivalent mapped
//! `{ [P in K]: V }` form, and a concrete index-signature source, with binder
//! names varied so no fix can key on a particular spelling.

use tsz_checker::test_utils::check_source_code_messages as compile_and_get_diagnostics;

fn ts2322_count(source: &str) -> usize {
    compile_and_get_diagnostics(source)
        .iter()
        .filter(|(code, _)| *code == 2322)
        .count()
}

// Self-contained `Record`/`PropertyKey` so the test runs without lib contexts.
const PRELUDE: &str = r#"
type PKey = string | number | symbol;
type Rec<K extends PKey, V> = { [P in K]: V };
"#;

#[test]
fn record_key_param_inferred_from_source_constraint_key() {
    // `K` of `Rec<K, unknown>` must infer `string` from `T`'s constraint
    // `Rec<string, unknown>`, so `const s: string = k` is clean.
    let source = format!(
        "{PRELUDE}
declare function keyOf<K extends PKey>(obj: Rec<K, unknown>): K;
function lowerize<T extends Rec<string, unknown>>(obj: T) {{
  const k = keyOf(obj);
  const s: string = k;
}}
"
    );
    assert_eq!(
        ts2322_count(&source),
        0,
        "K should infer `string` from the source parameter's apparent type, not `PropertyKey`"
    );
}

#[test]
fn record_key_param_is_string_not_widened_to_propertykey() {
    // The dual of the previous test: if `K` were `string`, assigning to a
    // narrower `1` must still fail — proving `K` is `string`, not `1` and not a
    // suppressed `any`.
    let source = format!(
        "{PRELUDE}
declare function keyOf<K extends PKey>(obj: Rec<K, unknown>): K;
function lowerize<T extends Rec<string, unknown>>(obj: T) {{
  const k = keyOf(obj);
  const bad: 1 = k;
}}
"
    );
    assert_eq!(
        ts2322_count(&source),
        1,
        "K = string is not assignable to the literal `1`"
    );
}

#[test]
fn number_key_constraint_infers_number() {
    // A `Rec<number, unknown>` constraint must infer `K = number`.
    let source = format!(
        "{PRELUDE}
declare function keyOf<K extends PKey>(obj: Rec<K, unknown>): K;
function f<Src extends Rec<number, unknown>>(obj: Src) {{
  const k = keyOf(obj);
  const n: number = k;
  const bad: 1 = k;
}}
"
    );
    assert_eq!(
        ts2322_count(&source),
        1,
        "K = number is clean against `number` but rejected against `1`"
    );
}

#[test]
fn propertykey_constraint_source_stays_propertykey() {
    // Negative control: a source whose constraint key is the full `PKey` must
    // keep `K = PKey`, so assigning to `string` *does* error.
    let source = format!(
        "{PRELUDE}
declare function keyOf<K extends PKey>(obj: Rec<K, unknown>): K;
function f<Src extends Rec<PKey, unknown>>(obj: Src) {{
  const k = keyOf(obj);
  const s: string = k;
}}
"
    );
    assert_eq!(
        ts2322_count(&source),
        1,
        "a PropertyKey-constrained source must not be narrowed to `string`"
    );
}

#[test]
fn mapped_key_param_inferred_from_concrete_index_signature() {
    // The mapped `{ [P in K]: V }` form over a concrete index-signature source
    // infers `K` from the index key type.
    let source = "
declare function keyOf2<Key extends string | number | symbol>(obj: { [P in Key]: unknown }): Key;
declare const o: { [x: string]: number };
const k = keyOf2(o);
const s: string = k;
const bad: 1 = k;
";
    assert_eq!(
        ts2322_count(source),
        1,
        "K should infer `string` from the source index-signature key, clean against `string`"
    );
}

#[test]
fn plain_generic_identity_inference_is_unchanged() {
    // Guard against over-firing: a non-structured target still infers the
    // source directly (`T = 5`), not its (absent) constraint.
    let source = "
declare function id<T>(x: T): T;
const r = id(5);
const ok: 5 = r;
const bad: 6 = r;
";
    assert_eq!(
        ts2322_count(source),
        1,
        "identity inference must keep T = 5 (only the `6` assignment fails)"
    );
}

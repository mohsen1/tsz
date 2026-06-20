//! TS7053 — indexing a type parameter by a `unique symbol` that keys a member
//! of the parameter's constraint.
//!
//! Structural rule: when the indexed object is a type parameter, indexability is
//! governed by its base constraint's apparent type. A `unique symbol` that names
//! a member of that constraint is a valid index, so `x[fooProp]` on
//! `T extends Foo<number>` (where `Foo` declares `[fooProp]: T`) has the deferred
//! type `T[typeof fooProp]` — never a TS7053. The string-keyed property path
//! already resolves the constraint for member lookup; the symbol-keyed path
//! previously indexed the bare, unresolved type parameter (whose element access
//! fails) and produced a false implicit-any element access. The rule is purely
//! structural: it keys on the constraint shape, not on any identifier spelling.
//!
//! Witness: `lateBoundConstraintTypeChecksCorrectly.ts` (upstream conformance).
use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::check_source;

fn codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

// 1. Reported repro: a unique-symbol-keyed member of the constraint is a valid
//    index — no TS7053.
#[test]
fn unique_symbol_key_of_constraint_no_ts7053() {
    let result = codes(
        r#"
declare const fooProp: unique symbol;
interface Foo<T> { [fooProp]: T; }
function f<T extends Foo<number>>(x: T) { return x[fooProp]; }
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 indexing a constrained param by a constraint symbol key, got: {result:?}"
    );
}

// 2. Renamed symbol / interface / parameter — proves the rule is structural,
//    not name-based.
#[test]
fn unique_symbol_constraint_renamed_is_structural() {
    let renamed = codes(
        r#"
declare const tag: unique symbol;
interface Bag<V> { [tag]: V; }
function pull<Row extends Bag<string>>(value: Row) { return value[tag]; }
"#,
    );
    assert!(
        !renamed.contains(&7053),
        "expected no TS7053 with renamed symbol/interface/param, got: {renamed:?}"
    );
}

// 3. The deferred result type is `T[typeof sym]`: a wrong-typed assignment
//    target still errors (TS2322), and the matching annotation stays clean.
#[test]
fn unique_symbol_index_result_is_deferred_indexed_access() {
    // Constraint declares the symbol-keyed member as `T` (a number here), so the
    // access type is `number`: assigning to `string` must report TS2322.
    let wrong = codes(
        r#"
declare const slot: unique symbol;
interface Holder<T> { [slot]: T; }
function g<T extends Holder<number>>(x: T) { const bad: string = x[slot]; }
"#,
    );
    assert!(
        wrong.contains(&2322),
        "expected TS2322 assigning T[typeof slot] (number) to string, got: {wrong:?}"
    );
    assert!(
        !wrong.contains(&7053),
        "the deferred indexed access must not also report TS7053, got: {wrong:?}"
    );

    let ok = codes(
        r#"
declare const slot: unique symbol;
interface Holder<T> { [slot]: T; }
function g<T extends Holder<number>>(x: T) {
    const fine: T[typeof slot] = x[slot];
}
"#,
    );
    assert!(
        ok.is_empty(),
        "expected no diagnostics for the matching annotation, got: {ok:?}"
    );
}

// 4. Multiple symbol-keyed members, including one whose value is concrete
//    (`string`) rather than the type parameter.
#[test]
fn multiple_unique_symbol_keys_no_ts7053() {
    let result = codes(
        r#"
declare const fooProp: unique symbol;
declare const barProp: unique symbol;
interface Foo<T> { [fooProp]: T; [barProp]: string; }
function f<T extends Foo<number>>(x: T) {
    const a: T[typeof fooProp] = x[fooProp];
    const b: T[typeof barProp] = x[barProp];
}
"#,
    );
    assert!(
        result.is_empty(),
        "expected no diagnostics for multiple constraint symbol keys, got: {result:?}"
    );
}

// 5. Transitive constraint chain: `T extends U`, `U extends Foo<number>`.
#[test]
fn unique_symbol_key_through_constraint_chain_no_ts7053() {
    let result = codes(
        r#"
declare const key: unique symbol;
interface Foo<T> { [key]: T; }
function f<U extends Foo<number>, T extends U>(x: T) { return x[key]; }
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 when the symbol keys a transitive constraint, got: {result:?}"
    );
}

// 6. Negative control: a unique symbol that is NOT a key of the constraint must
//    still report TS7053 (the fix resolves the constraint, it does not accept
//    arbitrary symbol keys).
#[test]
fn unrelated_unique_symbol_still_emits_ts7053() {
    let result = codes(
        r#"
declare const onProp: unique symbol;
declare const offProp: unique symbol;
interface Foo<T> { [onProp]: T; }
function f<T extends Foo<number>>(x: T) { return x[offProp]; }
"#,
    );
    assert!(
        result.contains(&7053),
        "expected TS7053 for a symbol that does not key the constraint, got: {result:?}"
    );
}

// 6b. Negative control: an unconstrained parameter indexed by a unique symbol
//     still reports TS7053.
#[test]
fn unconstrained_param_unique_symbol_still_emits_ts7053() {
    let result = codes(
        r#"
declare const k: unique symbol;
function f<T>(x: T) { return x[k]; }
"#,
    );
    assert!(
        result.contains(&7053),
        "expected TS7053 for an unconstrained param indexed by a unique symbol, got: {result:?}"
    );
}

// 7. Control: a well-known symbol (`Symbol.iterator`) member on the constraint
//    was already accepted and must stay clean.
#[test]
fn well_known_symbol_key_of_constraint_no_ts7053() {
    let result = codes(
        r#"
interface Foo { [Symbol.iterator](): number; }
function f<T extends Foo>(x: T) { return x[Symbol.iterator]; }
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 indexing a constrained param by a well-known symbol, got: {result:?}"
    );
}

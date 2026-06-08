//! Regression coverage for the fresh-object-literal call-argument recovery gate.
//!
//! A fresh object literal passed as a call argument whose function-typed
//! *property* member has a contravariantly-incompatible parameter must be
//! rejected under `strictFunctionTypes`, exactly as `tsc` does (TS2322 on the
//! property), and exactly as the same literal is rejected in assignment, return,
//! array-element, and `satisfies` positions. The call path used to recover such
//! arguments through `is_fresh_subtype_of` alone — the raw subtype relation
//! compares property-position function members bivariantly — so a real
//! parameter-variance mismatch was silently accepted only in argument position.
//!
//! Binder names are varied across cases so the behavior cannot key off any
//! particular identifier.

use tsz_checker::test_utils::{check_source_strict, diagnostic_codes};

const TS2322: u32 = 2322;
const TS2345: u32 = 2345;

fn assert_has_code(source: &str, code: u32) {
    let diags = check_source_strict(source);
    assert!(
        diags.iter().any(|diag| diag.code == code),
        "expected TS{code}, got {:?}",
        diagnostic_codes(&diags)
    );
}

fn assert_clean(source: &str) {
    let diags = check_source_strict(source);
    assert!(
        diags.is_empty(),
        "expected no diagnostics, got {:?}",
        diagnostic_codes(&diags)
    );
}

// --- Negative cases: the mismatch must surface (was a false negative) ---

#[test]
fn fresh_object_literal_arg_rejects_contravariant_callback_param() {
    // `(p: Square) => void` is not assignable to `(p: Shape) => void`.
    let source = r#"
interface Shape { kind: string; }
interface Square extends Shape { side: number; }
declare function render(opts: { draw: (p: Shape) => void }): void;
render({ draw: (p: Square) => {} });
"#;
    assert_has_code(source, TS2322);
}

#[test]
fn fresh_object_literal_arg_rejects_unrelated_callback_param() {
    let source = r#"
interface Shape { kind: string; }
declare function render(opts: { draw: (p: Shape) => void }): void;
render({ draw: (p: number) => {} });
"#;
    assert_has_code(source, TS2322);
}

#[test]
fn nested_fresh_object_literal_arg_rejects_contravariant_callback_param() {
    let source = r#"
interface Node1 { id: string; }
interface Leaf extends Node1 { value: number; }
declare function mount(cfg: { inner: { visit: (n: Node1) => void } }): void;
mount({ inner: { visit: (n: Leaf) => {} } });
"#;
    assert_has_code(source, TS2322);
}

#[test]
fn spread_fresh_object_literal_arg_rejects_contravariant_callback_param() {
    // The spread member carries the narrower annotated signature; the merged
    // fresh literal must still be rejected (TS2345 on the whole argument).
    let source = r#"
interface Animal2 { name: string; }
interface Cat extends Animal2 { whiskers: number; }
declare function adopt(o: { feed: (a: Animal2) => void }): void;
const partial = { feed: (a: Cat) => {} };
adopt({ ...partial });
"#;
    assert_has_code(source, TS2345);
}

#[test]
fn multiple_args_each_fresh_literal_is_checked() {
    let source = r#"
interface Base1 { tag: string; }
interface Derived1 extends Base1 { extra: number; }
declare function pair(
    a: { on: (x: Base1) => void },
    b: { on: (x: Base1) => void },
): void;
pair({ on: (x: Base1) => {} }, { on: (x: Derived1) => {} });
"#;
    assert_has_code(source, TS2322);
}

// --- Positive controls: legitimate recoveries must stay clean ---

#[test]
fn fresh_object_literal_arg_accepts_exact_callback() {
    let source = r#"
interface Evt { type: string; }
declare function listen(o: { handler: (e: Evt) => void }): void;
listen({ handler: (e: Evt) => {} });
"#;
    assert_clean(source);
}

#[test]
fn fresh_object_literal_arg_accepts_wider_callback_param() {
    // A wider parameter (`{}`) is sound for a `(e: Evt) => void` slot.
    let source = r#"
interface Evt { type: string; }
declare function listen(o: { handler: (e: Evt) => void }): void;
listen({ handler: (e: {}) => {} });
"#;
    assert_clean(source);
}

#[test]
fn fresh_object_literal_arg_accepts_unannotated_callback_param() {
    // The parameter type is supplied only by the contextual refresh — this is
    // exactly the recovery the gate must preserve.
    let source = r#"
interface Evt { type: string; }
declare function listen(o: { handler: (e: Evt) => void }): void;
listen({ handler: (e) => { e.type; } });
"#;
    assert_clean(source);
}

#[test]
fn fresh_object_literal_arg_accepts_method_target_bivariance() {
    // A method-signature *target* is bivariant, matching tsc: a narrower
    // parameter is accepted.
    let source = r#"
interface Shape { kind: string; }
interface Square extends Shape { side: number; }
declare function render(opts: { draw(p: Shape): void }): void;
render({ draw: (p: Square) => {} });
"#;
    assert_clean(source);
}

#[test]
fn generic_call_object_literal_callback_inference_stays_clean() {
    let source = r#"
declare function run<T, U>(o: { items: T[]; map: (x: T) => U }): U[];
const out = run({ items: [1, 2, 3], map: (x) => x.toFixed() });
"#;
    assert_clean(source);
}

#[test]
fn fresh_object_literal_arg_accepts_covariant_callback_return() {
    let source = r#"
interface Shape { kind: string; }
interface Square extends Shape { side: number; }
declare function make(o: { build: () => Shape }): void;
make({ build: (): Square => ({ kind: "s", side: 1 }) });
"#;
    assert_clean(source);
}

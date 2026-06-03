//! Regression coverage for discriminated-union narrowing through *nested*
//! property accesses.
//!
//! Structural rule (matches TypeScript 6.0.x): a union reference is narrowed
//! only by a discriminant property accessed **directly** on that reference
//! (`x.kind === "a"`). TypeScript does NOT narrow an outer union through a
//! nested discriminant access — `x.meta.kind === "a"` narrows `x.meta` (and
//! `x.meta.kind`) but never `x`. Previously tsz walked the full property path
//! (`["meta", "kind"]`) and narrowed the outer reference, accepting code that
//! `tsc` rejects with TS2339/TS2322.
//!
//! Binder names are varied per case so the rule is structural, not keyed to a
//! particular identifier.

use tsz_checker::test_utils::check_source_strict_codes;

const TS2339: u32 = 2339; // Property does not exist on type
const TS2322: u32 = 2322; // Type not assignable

fn codes(source: &str) -> Vec<u32> {
    check_source_strict_codes(source)
}

// ---------------------------------------------------------------------------
// Positive cases: legitimate direct-discriminant narrowing must keep working.
// ---------------------------------------------------------------------------

#[test]
fn top_level_discriminant_still_narrows() {
    // `tsc`: clean.
    let diags = codes(
        r#"
type Shape = { kind: "circle"; radius: number } | { kind: "square"; side: string };
function area(shape: Shape): number | string {
    if (shape.kind === "circle") return shape.radius;
    return shape.side;
}
"#,
    );
    assert!(
        !diags.contains(&TS2339),
        "direct discriminant `shape.kind` must still narrow `shape`, got {diags:?}"
    );
}

#[test]
fn unit_typed_intermediate_property_is_a_direct_discriminant() {
    // `meta` itself is the unit discriminant (`"on" | "off"`), a direct property.
    // `tsc`: clean.
    let diags = codes(
        r#"
type Toggle = { meta: "on"; power: number } | { meta: "off"; reason: string };
function read(node: Toggle): number | string {
    if (node.meta === "on") return node.power;
    return node.reason;
}
"#,
    );
    assert!(
        !diags.contains(&TS2339),
        "unit-typed direct property `node.meta` must narrow `node`, got {diags:?}"
    );
}

#[test]
fn narrowing_the_inner_reference_still_works() {
    // The discriminant is direct *relative to the inner reference* `holder.inner`,
    // so `tsc` narrows `holder.inner` and both branches type-check.
    let diags = codes(
        r#"
type Inner = { kind: "a"; av: number } | { kind: "b"; bv: string };
function pick(holder: { inner: Inner }): number | string {
    if (holder.inner.kind === "a") return holder.inner.av;
    return holder.inner.bv;
}
"#,
    );
    assert!(
        !diags.contains(&TS2339),
        "inner reference `holder.inner` must still narrow by its direct discriminant, got {diags:?}"
    );
}

#[test]
fn aliased_top_level_discriminant_still_narrows() {
    // Aliased *direct* discriminant (TS 4.4 feature). `tsc`: clean.
    let diags = codes(
        r#"
type Signal = { tag: "click"; x: number } | { tag: "key"; code: string };
function handle(ev: Signal): number | string {
    const t = ev.tag;
    if (t === "click") return ev.x;
    return ev.code;
}
"#,
    );
    assert!(
        !diags.contains(&TS2339),
        "aliased direct discriminant `const t = ev.tag` must narrow `ev`, got {diags:?}"
    );
}

#[test]
fn switch_on_direct_discriminant_still_narrows() {
    // `tsc`: clean.
    let diags = codes(
        r#"
type Cmd = { op: "add"; lhs: number } | { op: "neg"; value: string };
function run(cmd: Cmd): number | string {
    switch (cmd.op) {
        case "add": return cmd.lhs;
        case "neg": return cmd.value;
    }
}
"#,
    );
    assert!(
        !diags.contains(&TS2339),
        "switch on direct discriminant `cmd.op` must narrow `cmd`, got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative cases: nested discriminants must NOT narrow the outer union.
// Each mirrors a `tsc` TS2339/TS2322 result.
// ---------------------------------------------------------------------------

#[test]
fn nested_discriminant_does_not_narrow_outer_union() {
    // `tsc`: TS2339 on `node.a` and `node.b` (no narrowing of `node`).
    let diags = codes(
        r#"
type Variant = { meta: { kind: "a" }; a: number } | { meta: { kind: "b" }; b: string };
function visit(node: Variant): number | string {
    if (node.meta.kind === "a") return node.a;
    return node.b;
}
"#,
    );
    assert!(
        diags.contains(&TS2339),
        "nested discriminant `node.meta.kind` must NOT narrow `node`; expected TS2339, got {diags:?}"
    );
}

#[test]
fn deeply_nested_discriminant_does_not_narrow_outer_union() {
    // `tsc`: TS2339 (two levels of nesting).
    let diags = codes(
        r#"
type Wrap =
    | { outer: { inner: { kind: "a" } }; a: number }
    | { outer: { inner: { kind: "b" } }; b: string };
function take(w: Wrap): number | string {
    if (w.outer.inner.kind === "a") return w.a;
    return w.b;
}
"#,
    );
    assert!(
        diags.contains(&TS2339),
        "deep nested discriminant must NOT narrow the root; expected TS2339, got {diags:?}"
    );
}

#[test]
fn aliased_nested_discriminant_does_not_narrow_outer_union() {
    // `const disc = action.meta.type` does not narrow `action`. `tsc`: TS2339.
    let diags = codes(
        r#"
type Action = { meta: { type: "inc" }; amount: number } | { meta: { type: "dec" }; id: string };
function reduce(action: Action): number | string {
    const disc = action.meta.type;
    if (disc === "inc") return action.amount;
    return action.id;
}
"#,
    );
    assert!(
        diags.contains(&TS2339),
        "aliased nested discriminant must NOT narrow the root; expected TS2339, got {diags:?}"
    );
}

#[test]
fn aliased_intermediate_object_does_not_narrow_outer_union() {
    // `const m = item.meta; if (m.kind === ...)` narrows `m`, not `item`. `tsc`: TS2339.
    let diags = codes(
        r#"
type Item = { meta: { kind: "a" }; a: number } | { meta: { kind: "b" }; b: string };
function use(item: Item): number | string {
    const m = item.meta;
    if (m.kind === "a") return item.a;
    return item.b;
}
"#,
    );
    assert!(
        diags.contains(&TS2339),
        "aliasing the intermediate object must NOT narrow the root; expected TS2339, got {diags:?}"
    );
}

#[test]
fn switch_on_nested_discriminant_does_not_narrow_outer_union() {
    // `tsc`: TS2339 — switching on `state.meta.phase` does not narrow `state`.
    let diags = codes(
        r#"
type State = { meta: { phase: "load" }; progress: number } | { meta: { phase: "done" }; result: string };
function summarize(state: State): number | string {
    switch (state.meta.phase) {
        case "load": return state.progress;
        case "done": return state.result;
    }
    return 0;
}
"#,
    );
    assert!(
        diags.contains(&TS2339),
        "switch on nested discriminant must NOT narrow the root; expected TS2339, got {diags:?}"
    );
}

#[test]
fn nested_discriminant_does_not_assign_narrowed_member() {
    // The narrowed member is not assignable from the un-narrowed union. `tsc`: TS2322.
    let diags = codes(
        r#"
type Add = { meta: { kind: "a" }; a: number };
type Op = Add | { meta: { kind: "b" }; b: string };
function go(op: Op): void {
    if (op.meta.kind === "a") {
        const only: Add = op;
        void only;
    }
}
"#,
    );
    assert!(
        diags.contains(&TS2322),
        "outer union must stay un-narrowed through a nested discriminant; expected TS2322, got {diags:?}"
    );
}

#[test]
fn typeof_nested_property_does_not_narrow_outer_union() {
    // `typeof box.payload.value === "string"` narrows `box.payload`, never `box`.
    // `tsc`: TS2339 on `box.s` / `box.n`.
    let diags = codes(
        r#"
type Box =
    | { payload: { value: string }; s: number }
    | { payload: { value: number }; n: string };
function unwrap(box: Box): number | string {
    if (typeof box.payload.value === "string") return box.s;
    return box.n;
}
"#,
    );
    assert!(
        diags.contains(&TS2339),
        "typeof on a nested property must NOT narrow the root; expected TS2339, got {diags:?}"
    );
}

//! Regression coverage for discriminated-union narrowing of a **member-rooted**
//! reference — most importantly a `this.`-rooted property-access reference.
//!
//! Structural rule (matches TypeScript 6.0.x): a discriminant guard accessed
//! *directly* on a reference narrows that reference. The narrowed reference need
//! not be a bare identifier — `this.state.matched` narrows `this.state`, exactly
//! as `s.matched` narrows a local `s`. Previously tsz measured the discriminant
//! property path from the *syntactic root* (`this`) and required a single
//! segment, so any member-rooted target (two or more hops from the root) was
//! silently skipped: the guard `if (this.state.matched)` left `this.state` typed
//! as the full union and produced false `TS2322`/`TS2339`.
//!
//! The fix measures the path *relative to the narrowed reference*, so the rule
//! is uniform across truthiness, `===`, `switch`, and assertion-predicate
//! discriminants, and applies whether the reference is `s`, `this.state`, or a
//! deeper `this.a.b` chain. The "direct property only" guarantee is preserved:
//! a nested access (`x.meta.kind`) still narrows `x.meta`, never the outer `x`.
//!
//! Binder names are varied per case so the coverage is structural, not keyed to
//! a particular identifier.

use tsz_checker::test_utils::check_source_strict_codes;

const TS2322: u32 = 2322; // Type not assignable
const TS2339: u32 = 2339; // Property does not exist on type

fn codes(source: &str) -> Vec<u32> {
    check_source_strict_codes(source)
}

// ---------------------------------------------------------------------------
// Positive cases: a discriminant directly on a `this.`-rooted reference narrows.
// `tsc` is clean on all of these.
// ---------------------------------------------------------------------------

#[test]
fn truthiness_discriminant_on_this_param_property_generic() {
    // The ts-pattern witness: a generic constructor-parameter property.
    let diags = codes(
        r#"
type MatchState<out> = { matched: true; value: out } | { matched: false; value: undefined };
class Matcher<inp, out> {
  constructor(private input: inp, private state: MatchState<out>) {}
  otherwise(handler: (v: inp) => out): out {
    if (this.state.matched) return this.state.value;
    return handler(this.input);
  }
}
"#,
    );
    assert!(
        !diags.contains(&TS2322),
        "`if (this.state.matched)` must narrow `this.state`, got {diags:?}"
    );
}

#[test]
fn truthiness_discriminant_on_this_plain_field_nongeneric() {
    // Plain field (not a parameter property), concrete value type.
    let diags = codes(
        r#"
type Result = { ok: true; data: number } | { ok: false; data: undefined };
class Box {
  private slot: Result;
  constructor(seed: Result) { this.slot = seed; }
  read(): number { if (this.slot.ok) return this.slot.data; throw 0; }
}
"#,
    );
    assert!(
        !diags.contains(&TS2322),
        "plain `this.`-field truthiness discriminant must narrow, got {diags:?}"
    );
}

#[test]
fn equality_discriminant_on_this_field() {
    let diags = codes(
        r#"
type Figure = { tag: "round"; radius: number } | { tag: "square"; edge: number };
class Drawing {
  constructor(private figure: Figure) {}
  metric(): number {
    if (this.figure.tag === "round") return this.figure.radius;
    return this.figure.edge;
  }
}
"#,
    );
    assert!(
        !diags.iter().any(|c| *c == TS2322 || *c == TS2339),
        "`this.figure.tag === \"round\"` must narrow `this.figure`, got {diags:?}"
    );
}

#[test]
fn switch_discriminant_on_this_field() {
    let diags = codes(
        r#"
type Node = { sort: "leaf"; weight: number } | { sort: "branch"; span: number };
class Tree {
  constructor(private node: Node) {}
  size(): number {
    switch (this.node.sort) {
      case "leaf": return this.node.weight;
      case "branch": return this.node.span;
    }
  }
}
"#,
    );
    assert!(
        !diags.iter().any(|c| *c == TS2322 || *c == TS2339),
        "`switch (this.node.sort)` must narrow `this.node`, got {diags:?}"
    );
}

#[test]
fn truthiness_discriminant_on_deeper_this_chain() {
    // The narrowed reference is itself two hops deep: `this.wrap.cell.flag`
    // narrows `this.wrap.cell`.
    let diags = codes(
        r#"
type Cell = { flag: true; payload: number } | { flag: false; payload: undefined };
class Holder {
  constructor(private wrap: { cell: Cell }) {}
  fetch(): number { if (this.wrap.cell.flag) return this.wrap.cell.payload; throw 0; }
}
"#,
    );
    assert!(
        !diags.contains(&TS2322),
        "`this.wrap.cell.flag` must narrow `this.wrap.cell`, got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Control: narrowing the identical union through a local copy already worked and
// must keep working (isolates the defect to member-rooted references).
// ---------------------------------------------------------------------------

#[test]
fn local_copy_discriminant_still_narrows() {
    let diags = codes(
        r#"
type Outcome = { hit: true; value: number } | { hit: false; value: undefined };
class Runner {
  constructor(private outcome: Outcome) {}
  resolve(): number { const local = this.outcome; if (local.hit) return local.value; throw 0; }
}
"#,
    );
    assert!(
        !diags.contains(&TS2322),
        "local-copy discriminant narrowing must keep working, got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative cases: a discriminant on a *nested* access must NOT narrow the outer
// union. `tsc` reports the noted error; tsz must too (no over-narrowing).
// ---------------------------------------------------------------------------

#[test]
fn nested_access_does_not_narrow_outer_union_equality() {
    // `pair.lead.key === "x"` narrows `pair.lead`, never the outer `pair`, so
    // `pair.first` (present on only one member) is still a union-wide access.
    let diags = codes(
        r#"
type Pair = { lead: { key: "x" }; first: number } | { lead: { key: "y" }; second: number };
function pick(pair: Pair): number {
  if (pair.lead.key === "x") { return pair.first; }
  return 0;
}
"#,
    );
    assert!(
        diags.contains(&TS2339),
        "nested discriminant must not narrow the outer union, expected TS2339, got {diags:?}"
    );
}

#[test]
fn nested_access_does_not_narrow_outer_union_on_this() {
    // `this.frame.inner.matched` narrows `this.frame.inner`, never `this.frame`.
    let diags = codes(
        r#"
type Inner = { matched: true; value: number } | { matched: false; value: undefined };
class Stage {
  constructor(private frame: { inner: Inner; tail: number } | { inner: Inner; head: number }) {}
  run(): number {
    if (this.frame.inner.matched) { return this.frame.tail; }
    return 0;
  }
}
"#,
    );
    assert!(
        diags.contains(&TS2339),
        "nested `this.frame.inner` discriminant must not narrow `this.frame`, expected TS2339, got {diags:?}"
    );
}

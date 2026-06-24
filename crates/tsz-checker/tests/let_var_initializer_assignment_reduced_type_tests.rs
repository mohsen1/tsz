//! Regression coverage for control-flow narrowing of a `let`/`var` declaration
//! by its *initializer*, when the declared type is a union of object/class
//! types.
//!
//! Structural rule (matches `tsc`): `let x: A | B = value` gives `x` the
//! initializer-reduced flow type for the rest of the declaration scope, exactly
//! as `tsc`'s `getAssignmentReducedType` does — the reduction is **not**
//! `const`-specific. The narrowed type is only the *initial* flow type: the flow
//! graph re-widens `x` on any later assignment, and the loop fixed-point
//! re-widens it across back-edges, so mutable-variable and loop semantics are
//! preserved.
//!
//! Owner: `tsz_checker` control-flow assignment typing
//! (`flow/control_flow/assignment.rs::get_assigned_type`). Previously the
//! var-decl-with-annotation branch returned the *declared* union for any
//! non-`const` declaration (`if !is_const { return None; }`), so an
//! object/class-typed `let`/`var` kept the un-narrowed union and produced false
//! `TS2322` (assignability) and `TS2339` (property access on a union member)
//! diagnostics. Primitive-union initializers already narrowed through the
//! literal-type branch, masking the gap. The fix narrows never-reassigned
//! standalone `let`/`var` declarations like `const`, while preserving tsc's
//! older function-scoped `var` redeclaration behavior for symbols merged with
//! earlier parameters.
//!
//! Binder names are varied per case so coverage is structural, not keyed to a
//! particular identifier.

use tsz_checker::test_utils::{check_source_codes, check_source_strict_codes};

const TS2322: u32 = 2322; // Type X is not assignable to type Y
const TS2339: u32 = 2339; // Property does not exist on type
const TS2403: u32 = 2403; // Subsequent variable declarations must have the same type

fn codes(source: &str) -> Vec<u32> {
    check_source_strict_codes(source)
}

fn default_codes(source: &str) -> Vec<u32> {
    check_source_codes(source)
}

// ---------------------------------------------------------------------------
// Positive cases: a `let` whose initializer pins a single union member must
// narrow exactly like a `const`. `tsc` is clean on all of these.
// ---------------------------------------------------------------------------

#[test]
fn let_object_literal_initializer_narrows_union_member() {
    let diags = codes(
        r#"
let x: { k: "a" } | { k: "b" } = { k: "a" };
const t: { k: "a" } = x;
"#,
    );
    assert!(
        diags.is_empty(),
        "let object-literal initializer must narrow to {{ k: \"a\" }}, got {diags:?}"
    );
}

#[test]
fn var_object_literal_initializer_narrows_union_member_renamed() {
    // `var` and different binder names: the rule is not keyed to `let`/`x`.
    let diags = codes(
        r#"
var payload: { tag: "lo"; bound: number } | { tag: "hi"; label: string } = { tag: "lo", bound: 3 };
const got: { tag: "lo"; bound: number } = payload;
"#,
    );
    assert!(
        diags.is_empty(),
        "var object-literal initializer must narrow to the assigned member, got {diags:?}"
    );
}

#[test]
fn let_property_access_after_initializer_narrowing_has_no_ts2339() {
    // The narrowed member exposes its own properties; reading them must not
    // report TS2339 against the *other* (un-narrowed) union member.
    let diags = codes(
        r#"
let payload: { tag: "x"; n: number } | { tag: "y"; s: string } = { tag: "x", n: 1 };
const nn: number = payload.n;
"#,
    );
    assert!(
        !diags.contains(&TS2339),
        "property access on the narrowed member must not report TS2339, got {diags:?}"
    );
    assert!(
        !diags.contains(&TS2322),
        "narrowed member access must not report TS2322, got {diags:?}"
    );
}

#[test]
fn let_class_instance_initializer_narrows_union_member() {
    // Non-literal initializer (`new Cat()`): must narrow through the same path
    // as `const`, not just object/array literals.
    let diags = codes(
        r#"
class Cat { meow = 1; }
class Dog { bark = 2; }
let pet: Cat | Dog = new Cat();
const c: Cat = pet;
"#,
    );
    assert!(
        diags.is_empty(),
        "let class-instance initializer must narrow to Cat, got {diags:?}"
    );
}

#[test]
fn let_reference_initializer_narrows_union_member() {
    // Non-literal initializer that is another reference.
    let diags = codes(
        r#"
declare const seed: { k: "a" };
let x: { k: "a" } | { k: "b" } = seed;
const t: { k: "a" } = x;
"#,
    );
    assert!(
        diags.is_empty(),
        "let reference initializer must narrow to {{ k: \"a\" }}, got {diags:?}"
    );
}

#[test]
fn let_initializer_narrowing_makes_impossible_branch_never() {
    // Downstream consequence of narrowing: once `x` is `{ a: 1 }`, the
    // `x.a === 1` guard is always-true, so the `else` branch is `never` and
    // reading `x.a` there reports TS2339 — exactly as `tsc` does.
    let diags = codes(
        r#"
let x: { a: 1 } | { a: 2 } = { a: 1 };
if (x.a === 1) {
  const y: 1 = x.a;
} else {
  const z: 2 = x.a;
}
"#,
    );
    assert!(
        diags.contains(&TS2339),
        "the impossible else branch must report TS2339 on x.a (never), got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative / guard cases: narrowing must NOT over-fire.
// ---------------------------------------------------------------------------

#[test]
fn let_full_union_initializer_does_not_over_narrow() {
    // The initializer's type is the whole union, so no member is eliminated and
    // `x` keeps the declared union — assigning it to one member is TS2322.
    let diags = codes(
        r#"
declare const v: { k: "a" } | { k: "b" };
let x: { k: "a" } | { k: "b" } = v;
const t: { k: "a" } = x;
"#,
    );
    assert!(
        diags.contains(&TS2322),
        "a full-union initializer must not narrow away a member, got {diags:?}"
    );
}

#[test]
fn reassigned_let_initializer_is_not_narrowed() {
    // A `let` that is reassigned anywhere is *not* an effectively constant
    // reference, so the initializer does not narrow (the flow graph owns its
    // evolution). `x` keeps the declared union, so the later read against one
    // member is TS2322. (Narrowing reassigned mutable locals is deferred: it
    // requires loop-fixed-point widening to stay sound — see the #8513 case
    // below.)
    let diags = codes(
        r#"
let x: { k: "a" } | { k: "b" } = { k: "a" };
const u: { k: "a" } = x;
x = { k: "b" };
"#,
    );
    assert!(
        diags.contains(&TS2322),
        "a reassigned let must keep its declared union, got {diags:?}"
    );
}

#[test]
fn loop_reassigned_generic_optional_has_no_spurious_error() {
    // Regression guard for the #8513 `Optional<r>` repro
    // (conformance `typeGuardsAsAssertions.ts`). `result` is reassigned inside
    // the loop, so it must keep its declared `Optional<r>` type rather than
    // narrow to `None`. Narrowing the initializer here would evaluate the
    // loop body's first fixed-point pass against `None`, degrading inference of
    // the `someFrom` type argument and producing a spurious TS2322. tsc is
    // clean.
    let diags = codes(
        r#"
declare let cond: boolean;
interface None { readonly none: string; }
interface Some<a> { readonly some: a; }
type Optional<a> = Some<a> | None;
declare const none: None;
declare function isSome<a>(value: Optional<a>): value is Some<a>;
function someFrom<a>(some: a) { return { some }; }
function fn<r>(makeSome: () => r): void {
  let result: Optional<r> = none;
  while (cond) {
    result = someFrom(isSome(result) ? result.some : makeSome());
  }
}
"#,
    );
    assert!(
        !diags.contains(&TS2322),
        "loop-reassigned generic Optional must not produce a spurious TS2322, got {diags:?}"
    );
}

#[test]
fn parameter_merged_var_initializer_does_not_narrow_parameter_symbol() {
    // `var` declarations merge with function parameters in the same function
    // scope. The merged symbol's checking surface remains the earlier parameter
    // declaration, so the `var` annotation/initializer must not assignment-reduce
    // the parameter to `Beta`. This is the structural form of conformance
    // `functionArgShadowing.ts`.
    let diags = default_codes(
        r#"
class Alpha { foo() {} }
class Beta { bar() {} }
function use(param: Alpha) {
  var param: Beta = new Beta();
  param.bar();
}
"#,
    );
    assert!(
        diags.contains(&TS2403),
        "merged parameter/var must report the redeclaration mismatch, got {diags:?}"
    );
    assert!(
        diags.contains(&TS2339),
        "the merged parameter surface must remain Alpha, so param.bar is TS2339; got {diags:?}"
    );
}

#[test]
fn constructor_parameter_property_merged_var_keeps_parameter_type() {
    // Vary the binder name and include the parameter-property shape from the
    // TypeScript fixture. The `var` declaration conflicts with the constructor
    // parameter's value symbol; reads still use the parameter's number surface.
    let diags = default_codes(
        r#"
class Holder {
  constructor(public value: number) {
    var value: string;
    var n: number = value;
  }
}
"#,
    );
    assert!(
        diags.contains(&TS2403),
        "parameter-property var redeclaration must report TS2403, got {diags:?}"
    );
    assert!(
        !diags.contains(&TS2322),
        "value keeps the constructor parameter's number type, so assigning to number is clean; got {diags:?}"
    );
}

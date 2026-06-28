//! Control-flow property inference for un-annotated, un-initialized class
//! fields (tsc's `getFlowTypeInConstructor`).
//!
//! Structural rule: when a class instance property has neither a type
//! annotation nor an initializer, tsc infers its type from the
//! `this.<name> = <value>` assignments in the constructor (the widened union of
//! the assigned value types, plus `undefined` when the field is not definitely
//! assigned on every path). tsz previously left such a field `any`, which
//! cascaded into spurious diagnostics such as `TS7053` when the field was used
//! to index a typed map (witness: tRPC `TRPCError.code`). Owner:
//! `types/class_type/instance.rs` (instance-type builder) and
//! `state_checking_members/member_access.rs`
//! (`infer_property_type_from_constructor_flow`).
//!
//! The fix is structural, not name-based: these tests vary the class, property,
//! and parameter binder names to confirm no identifier string drives the logic.

use crate::test_utils::check_source_strict_codes as check_strict;

const TS2322: u32 = 2322; // Type X is not assignable to type Y.
const TS7008: u32 = 7008; // Member implicitly has an 'any' type.
const TS7053: u32 = 7053; // Element implicitly has an 'any' type (index).

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

#[test]
fn constructor_assignment_preserves_declared_literal_union_no_ts7053() {
    // The reported witness: a readonly field typed only by `this.code = o.code`
    // (`o.code: "A" | "B" | "C"`) must keep that union so indexing the const map
    // is valid. tsc is clean.
    let codes = check_strict(
        r#"
type Code = "A" | "B" | "C";
const MAP = { A: 1, B: 2, C: 3 } as const;
class E {
  public readonly code;
  constructor(opts: { code: Code }) { this.code = opts.code; }
}
function f(e: E) { const v = MAP[e.code]; }
"#,
    );
    assert_eq!(count(&codes, TS7053), 0, "no spurious TS7053: {codes:?}");
    assert_eq!(
        count(&codes, TS7008),
        0,
        "field is inferred, not any: {codes:?}"
    );
}

#[test]
fn constructor_inference_is_not_name_based() {
    // Same shape as above with every binder renamed: still clean. Guards against
    // any property/class/parameter name literal sneaking into the logic.
    let codes = check_strict(
        r#"
type Status = "open" | "closed" | "pending";
const TABLE = { open: 0, closed: 1, pending: 2 } as const;
class Ticket {
  readonly state;
  constructor(input: { state: Status }) { this.state = input.state; }
}
function lookup(t: Ticket) { const n = TABLE[t.state]; }
"#,
    );
    assert_eq!(count(&codes, TS7053), 0, "{codes:?}");
    assert_eq!(count(&codes, TS7008), 0, "{codes:?}");
}

#[test]
fn fresh_literal_assignment_widens_to_primitive() {
    // `this.n = 1` infers `number` (fresh literal widened), so assigning the
    // field to `string` is a TS2322 — and the field is NOT `any` (which would
    // suppress the error).
    let codes = check_strict(
        r#"
class Counter {
  n;
  constructor() { this.n = 1; }
}
const c = new Counter();
const ok: number = c.n;
const bad: string = c.n;
"#,
    );
    assert_eq!(
        count(&codes, TS2322),
        1,
        "number not assignable to string: {codes:?}"
    );
    assert_eq!(count(&codes, TS7008), 0, "{codes:?}");
}

#[test]
fn branch_assignments_union() {
    // Both constructor branches assign, so the field is `string | number`.
    let codes = check_strict(
        r#"
class U {
  v;
  constructor(b: boolean) { if (b) { this.v = 1; } else { this.v = "s"; } }
}
const u = new U(true);
const ok: string | number = u.v;
const bad: number = u.v;
"#,
    );
    assert_eq!(
        count(&codes, TS2322),
        1,
        "string|number not assignable to number: {codes:?}"
    );
}

#[test]
fn conditionally_assigned_field_adds_undefined() {
    // Only one branch assigns, so under strictNullChecks the flow type is
    // `number | undefined`; assigning it to `number` is a TS2322.
    let codes = check_strict(
        r#"
class Maybe {
  m;
  constructor(b: boolean) { if (b) { this.m = 1; } }
}
const mb = new Maybe(true);
const ok: number | undefined = mb.m;
const bad: number = mb.m;
"#,
    );
    assert_eq!(
        count(&codes, TS2322),
        1,
        "number|undefined not assignable to number: {codes:?}"
    );
    assert_eq!(
        count(&codes, TS7008),
        0,
        "conditional assignment still infers: {codes:?}"
    );
}

#[test]
fn generic_constructor_parameter_is_preserved() {
    // `this.val = p` with `p: T` infers the field as the type parameter `T`,
    // so returning it where `T` is expected is allowed and where `string` is
    // expected is a TS2322.
    let codes = check_strict(
        r#"
class Box<T> {
  val;
  constructor(p: T) { this.val = p; }
}
function unbox<T>(b: Box<T>): T { return b.val; }
function wrong<T>(b: Box<T>): string { return b.val; }
"#,
    );
    assert_eq!(
        count(&codes, TS2322),
        1,
        "T not assignable to string: {codes:?}"
    );
}

#[test]
fn derived_class_assignment_after_super_is_inferred() {
    // A field assigned after `super()` in a derived class is inferred normally.
    let codes = check_strict(
        r#"
class Base {}
class Derived extends Base {
  tag;
  constructor() { super(); this.tag = 1; }
}
const d = new Derived();
const bad: string = d.tag;
"#,
    );
    assert_eq!(
        count(&codes, TS2322),
        1,
        "number not assignable to string: {codes:?}"
    );
    assert_eq!(count(&codes, TS7008), 0, "{codes:?}");
}

#[test]
fn null_only_assignment_stays_any_and_reports_ts7008() {
    // When every assignment only ever produces `null`, tsc widens the flow type
    // back to `any` and reports TS7008 — and there is no spurious cascade from a
    // bare `null` field type.
    let codes = check_strict(
        r#"
class Nully {
  z;
  constructor() { this.z = null; }
}
const ny = new Nully();
const s: string = ny.z;
"#,
    );
    assert_eq!(
        count(&codes, TS7008),
        1,
        "null-only field stays implicit any: {codes:?}"
    );
    assert_eq!(
        count(&codes, TS2322),
        0,
        "any field is assignable, no cascade: {codes:?}"
    );
}

#[test]
fn no_constructor_assignment_remains_implicit_any() {
    // A field assigned only in a method (not the constructor) is not inferred;
    // it stays implicit `any` and reports TS7008, matching tsc.
    let codes = check_strict(
        r#"
class Late {
  q;
  set() { this.q = 1; }
}
"#,
    );
    assert_eq!(
        count(&codes, TS7008),
        1,
        "method-only assignment is not constructor flow: {codes:?}"
    );
}

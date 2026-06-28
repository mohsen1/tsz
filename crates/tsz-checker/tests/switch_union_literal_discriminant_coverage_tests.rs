//! Regression coverage for switch narrowing of a discriminated-union member
//! whose discriminant property is itself a **union of literals**.
//!
//! Structural rule (matches `tsc`): when a `switch` on a discriminant property
//! handles every literal a union member's discriminant can take, `tsc`
//! eliminates that member in the `default`/post-switch flow (residual `never`).
//! Previously tsz tested the member's discriminant property against each
//! excluded literal *individually* (`("a" | "b") <: "a"` is false), so a member
//! whose discriminant is a union (`{ kind: "a" | "b" }`) was never excluded even
//! when `"a"`, `"b"` were both handled. That produced a spurious `TS2322` on the
//! standard `const _: never = x` exhaustiveness check (false positive) and
//! silently accepted property access on the should-be-`never` residual (false
//! negative). The fix adds a union-coverage test: exclude the member when its
//! discriminant property type is a subtype of the union of ALL excluded
//! literals. It is gated on a genuine multi-member union so a lone non-union
//! object is not over-narrowed (`tsc` keeps it).
//!
//! Binder/property names are varied per case so the coverage is structural, not
//! keyed to a particular identifier.

use tsz_checker::test_utils::check_source_strict_codes;

const TS2322: u32 = 2322; // Type not assignable
const TS2339: u32 = 2339; // Property does not exist on type

fn codes(source: &str) -> Vec<u32> {
    check_source_strict_codes(source)
}

// ---------------------------------------------------------------------------
// False-positive forms: every literal of a union discriminant is handled, so
// the member narrows to `never` and `const _: never = x` is clean. `tsc` ok.
// ---------------------------------------------------------------------------

#[test]
fn string_union_discriminant_fully_covered_default_is_never() {
    let diags = codes(
        r#"
type Shape = { kind: "a" | "b" } | { kind: "c" };
function classify(shape: Shape) {
  switch (shape.kind) {
    case "a": return 1;
    case "b": return 2;
    case "c": return 3;
    default:
      const leftover: never = shape;
      return leftover;
  }
}
"#,
    );
    assert!(
        !diags.contains(&TS2322),
        "fully covered union discriminant must narrow `default` to never, got {diags:?}"
    );
}

#[test]
fn numeric_union_discriminant_fully_covered_default_is_never() {
    let diags = codes(
        r#"
type Cell = { code: 1 | 2 } | { code: 3 };
function route(cell: Cell) {
  switch (cell.code) {
    case 1: return "one";
    case 2: return "two";
    case 3: return "three";
    default:
      const residual: never = cell;
      return residual;
  }
}
"#,
    );
    assert!(
        !diags.contains(&TS2322),
        "numeric union discriminant fully covered must narrow to never, got {diags:?}"
    );
}

#[test]
fn enum_union_discriminant_property_fully_covered_default_is_never() {
    // Distinct from the enum-SUBJECT path (#6823/#9659): here the enum union is a
    // discriminant PROPERTY of an object union.
    let diags = codes(
        r#"
enum Tag { First, Second, Third }
type Token = { mark: Tag.First | Tag.Second } | { mark: Tag.Third };
function name(token: Token) {
  switch (token.mark) {
    case Tag.First: return "first";
    case Tag.Second: return "second";
    case Tag.Third: return "third";
    default:
      const rest: never = token;
      return rest;
  }
}
"#,
    );
    assert!(
        !diags.contains(&TS2322),
        "enum union discriminant property fully covered must narrow to never, got {diags:?}"
    );
}

#[test]
fn grouped_fallthrough_cases_fully_cover_union_discriminant() {
    let diags = codes(
        r#"
type Packet = { sort: "alpha" | "beta" } | { sort: "gamma" };
function dispatch(packet: Packet) {
  switch (packet.sort) {
    case "alpha":
    case "beta":
      return 0;
    case "gamma":
      return 1;
    default:
      const tail: never = packet;
      return tail;
  }
}
"#,
    );
    assert!(
        !diags.contains(&TS2322),
        "grouped fall-through cases covering a union discriminant must narrow to never, got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// False-negative form: exhaustive switch with no `default`; post-switch flow is
// `never`, so accessing a property must report `TS2339` (as `tsc` does).
// ---------------------------------------------------------------------------

#[test]
fn exhaustive_switch_makes_postflow_never_property_access_errors() {
    let diags = codes(
        r#"
type Entry = { tag: "x" | "y"; v: number } | { tag: "z"; w: string };
function read(entry: Entry) {
  switch (entry.tag) {
    case "x":
    case "y":
      return entry.v;
    case "z":
      return entry.w;
  }
  return entry.v; // entry is `never` here
}
"#,
    );
    assert!(
        diags.contains(&TS2339),
        "post-exhaustive-switch access on never residual must report TS2339, got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Controls that must NOT regress (no over-narrowing).
// ---------------------------------------------------------------------------

#[test]
fn lone_non_union_object_with_union_discriminant_is_kept() {
    // A single (non-union) object whose `kind` is a 3-literal union is NOT
    // narrowed away by switch coverage in `tsc` — it stays, so `const _: never`
    // still reports TS2322. The fix is gated on a genuine multi-member union.
    let diags = codes(
        r#"
type Only = { kind: "p" | "q" | "r" };
function inspect(only: Only) {
  switch (only.kind) {
    case "p": return 1;
    case "q": return 2;
    case "r": return 3;
    default:
      const leftover: never = only;
      return leftover;
  }
}
"#,
    );
    assert!(
        diags.contains(&TS2322),
        "lone non-union object must not be over-narrowed; expected TS2322, got {diags:?}"
    );
}

#[test]
fn partial_coverage_keeps_member_with_union_discriminant() {
    // Only `"a"` and `"c"` are handled; the `{ kind: "a" | "b" }` member is NOT
    // covered (its `"b"` value is unhandled), so it survives into `default` and
    // `const _: never` reports TS2322 — exactly as `tsc` does. The fix must not
    // narrow a partially covered union discriminant.
    let diags = codes(
        r#"
type Variant = { kind: "a" | "b" } | { kind: "c" };
function handle(variant: Variant) {
  switch (variant.kind) {
    case "a": return 1;
    case "c": return 3;
    default:
      const leftover: never = variant;
      return leftover;
  }
}
"#,
    );
    assert!(
        diags.contains(&TS2322),
        "partially covered union discriminant member must be kept; expected TS2322, got {diags:?}"
    );
}

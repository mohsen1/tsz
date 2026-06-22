//! Regression tests for issue #14530.
//!
//! Block-body return-type inference must union the UNWIDENED return-expression
//! types and widen the result only when it collapses to a single (fresh)
//! literal — tsc's `getWidenedType(getUnionType(unwidenedReturnTypes))`.
//! Previously tsz widened each branch's literal independently, so a function
//! returning two distinct literals inferred `string`/`number` and a
//! literal-typed assignment of its result drew a spurious TS2322.
//!
//! Rule (matches tsc):
//! - two+ distinct literals  → preserved union (`"a" | "b"`, `1 | 2`)
//! - literal + fall-through  → `"a" | undefined`
//! - single literal          → widened (`return "x"` → `string`)
//! - two identical literals  → dedup to one → widened (`string`)
//! - literal + non-literal   → widened (`string`)
//! - `as const` / contextual → preserved (existing carve-outs unchanged)

use tsz_checker::test_utils::check_source_codes;

fn ts2322(source: &str) -> usize {
    check_source_codes(source)
        .into_iter()
        .filter(|&c| c == 2322)
        .count()
}

/// The reported repro: two distinct string literals infer `"positive" | "zero"`.
#[test]
fn two_distinct_string_literals_infer_literal_union() {
    let source = r#"
function classify(n: number) {
  if (n > 0) return "positive";
  return "zero";
}
const c: "positive" | "zero" = classify(0);
"#;
    assert_eq!(
        ts2322(source),
        0,
        "two distinct literal returns must infer the literal union, not `string`: {:?}",
        check_source_codes(source)
    );
}

/// Distinct numeric literals are preserved as `1 | 2`. Renamed binders to keep
/// the rule structural, not identifier-driven.
#[test]
fn two_distinct_number_literals_infer_literal_union() {
    let source = r#"
function pick(flag: number) {
  if (flag > 0) return 1;
  return 2;
}
const chosen: 1 | 2 = pick(0);
"#;
    assert_eq!(
        ts2322(source),
        0,
        "distinct numeric literal returns must infer `1 | 2`: {:?}",
        check_source_codes(source)
    );
}

/// A literal return plus an implicit `undefined` fall-through is `"a" | undefined`,
/// not `string | undefined`.
#[test]
fn literal_plus_fallthrough_undefined_preserves_literal() {
    let source = r#"
function maybeTag(n: number) {
  if (n > 0) return "tagged";
}
const t: "tagged" | undefined = maybeTag(0);
"#;
    assert_eq!(
        ts2322(source),
        0,
        "literal + fall-through must infer `\"tagged\" | undefined`: {:?}",
        check_source_codes(source)
    );
}

/// A single literal return STILL widens to its base (tsc widens a lone fresh
/// literal). Assigning it to that literal must still error.
#[test]
fn single_literal_return_still_widens() {
    let source = r#"
function only() { return "x"; }
const v: "x" = only();
"#;
    assert!(
        ts2322(source) >= 1,
        "a single fresh-literal return widens to `string`, so `const v: \"x\"` must error: {:?}",
        check_source_codes(source)
    );
}

/// Two IDENTICAL literals dedup to a single literal and therefore widen to the
/// base — not preserved.
#[test]
fn two_identical_literals_dedup_and_widen() {
    let source = r#"
function same(n: number) {
  if (n > 0) return "dup";
  return "dup";
}
const s: "dup" = same(0);
"#;
    assert!(
        ts2322(source) >= 1,
        "two identical literals dedup to one and widen to `string`, so `const s: \"dup\"` must error: {:?}",
        check_source_codes(source)
    );
}

/// A literal mixed with a non-literal contribution widens (the union collapses
/// to `string`, which is not a literal).
#[test]
fn literal_plus_non_literal_widens() {
    let source = r#"
function mix(n: number, s: string) {
  if (n > 0) return "lit";
  return s;
}
const out: string = mix(0, "z");
const bad: "lit" = mix(0, "z");
"#;
    // `out: string` is clean; `bad: "lit"` must error (inferred `string`).
    assert!(
        ts2322(source) >= 1,
        "literal + non-literal infers `string`, so `const bad: \"lit\"` must error: {:?}",
        check_source_codes(source)
    );
}

/// Negative control: a distinct-literal union is still rejected when assigned to
/// a NARROWER single literal — the fix preserves the union, it does not widen
/// away genuine errors.
#[test]
fn distinct_literal_union_still_errors_against_narrower_target() {
    let source = r#"
function two(n: number) {
  if (n > 0) return "a";
  return "b";
}
const narrow: "a" = two(0);
"#;
    assert!(
        ts2322(source) >= 1,
        "`\"a\" | \"b\"` must still be rejected against target `\"a\"`: {:?}",
        check_source_codes(source)
    );
}

/// Carve-out preserved: a single `return x as const` stays its literal type and
/// is NOT widened.
#[test]
fn single_const_assertion_return_stays_literal() {
    let source = r#"
function asConst() { return "kept" as const; }
const k: "kept" = asConst();
"#;
    assert_eq!(
        ts2322(source),
        0,
        "`return x as const` must keep the literal type (no widen): {:?}",
        check_source_codes(source)
    );
}

/// Carve-out preserved: a contextually-typed single literal return stays literal.
#[test]
fn contextual_single_literal_return_stays_literal() {
    let source = r#"
const f: () => "ctx" = () => { return "ctx"; };
void f;
"#;
    assert_eq!(
        ts2322(source),
        0,
        "a contextually-typed literal return must stay literal: {:?}",
        check_source_codes(source)
    );
}

/// `true | false` normalizes to `boolean` (not a literal), so it is not widened
/// further and assigning to `boolean` is clean.
#[test]
fn distinct_boolean_literals_normalize_to_boolean() {
    let source = r#"
function flag(n: number) {
  if (n > 0) return true;
  return false;
}
const b: boolean = flag(0);
"#;
    assert_eq!(
        ts2322(source),
        0,
        "`true | false` normalizes to `boolean`, assignable to `boolean`: {:?}",
        check_source_codes(source)
    );
}

/// Adjacent shape: multi-return object literals keep per-property `as const`
/// discriminants across the union while widening non-asserted siblings.
#[test]
fn multi_return_object_literal_preserves_per_property_const() {
    let source = r#"
function node(n: number) {
  if (n > 0) return { kind: "a" as const, v: 1 };
  return { kind: "b" as const, v: 2 };
}
const r: { kind: "a" | "b"; v: number } = node(0);
"#;
    assert_eq!(
        ts2322(source),
        0,
        "per-property `as const` discriminants must survive the return union: {:?}",
        check_source_codes(source)
    );
}

//! Conditional (ternary) expressions must apply `tsc`'s
//! `UnionReduction.Subtype`: when one branch type is a subtype of the other,
//! the result collapses to the supertype. Without this, a `cond ? sub : super`
//! expression keeps a stray subtype member (e.g. `{} | { [k: string]: unknown
//! }`) whose non-indexable arm triggers a false `TS7053` on later element
//! access. Regression coverage for issue #10674 (kysely `parseObject` /
//! `mapRow` false positives).

use crate::test_utils::check_source_strict_codes;

/// Reported repro shape (write target): `target = cond ? {} : obj` where `obj`
/// has a string index signature. tsc reduces the conditional to the index-sig
/// type, so the `for...in` write `target[key] = ...` is clean. tsz used to keep
/// `{} | { [k: string]: unknown }` and fire a false TS7053 on the `{}` arm.
#[test]
fn conditional_subtype_reduction_suppresses_false_ts7053_on_write() {
    let codes = check_source_strict_codes(
        r#"
interface Bag { [k: string]: unknown; }
function parseObject(obj: Bag, create: boolean): Bag {
  const target = create ? {} : obj;
  for (const key in obj) {
    target[key] = obj[key];
  }
  return target;
}
"#,
    );
    assert!(
        !codes.contains(&7053),
        "conditional `cond ? {{}} : indexSig` should reduce to the index-sig supertype; unexpected TS7053: {codes:?}",
    );
}

/// Same rule, read access: `const v = (cond ? {} : obj)[key]`.
#[test]
fn conditional_subtype_reduction_suppresses_false_ts7053_on_read() {
    let codes = check_source_strict_codes(
        r#"
interface Bag { [k: string]: unknown; }
function readShape(obj: Bag, create: boolean, key: string): unknown {
  const t = create ? {} : obj;
  return t[key];
}
"#,
    );
    assert!(
        !codes.contains(&7053),
        "reading `(cond ? {{}} : indexSig)[key]` should be allowed; unexpected TS7053: {codes:?}",
    );
}

/// The reduction keys on the structural subtype relation, not on identifier or
/// type-parameter spelling. Renamed binders and a generic index-signature
/// wrapper must behave identically.
#[test]
fn conditional_subtype_reduction_is_name_independent() {
    let codes = check_source_strict_codes(
        r#"
type Dict<V> = { [Key in string]: V };
function pick<V>(source: Dict<V>, flag: boolean, k: string): V | {} {
  const chosen = flag ? {} : source;
  return chosen[k];
}
"#,
    );
    assert!(
        !codes.contains(&7053),
        "generic index-sig wrapper with renamed binders should still reduce; unexpected TS7053: {codes:?}",
    );
}

/// Order-independence, observably: the reduction must drop the `{}` arm
/// regardless of which branch it appears in. Both `cond ? {} : bag` and
/// `cond ? bag : {}` must allow `[key]` without TS7053. (A plain "no TS2322"
/// assertion on a nominal `Sub | Super` would pass even without the fix,
/// because `Sub | Super` is itself assignable to `Super`; element access is
/// the behaviour that actually distinguishes `{} | Bag` from `Bag`.)
#[test]
fn conditional_subtype_reduction_is_branch_order_independent() {
    let codes = check_source_strict_codes(
        r#"
interface Bag { [k: string]: unknown; }
function trueFirst(obj: Bag, c: boolean, key: string): unknown {
  const t = c ? {} : obj;
  return t[key];
}
function falseFirst(obj: Bag, c: boolean, key: string): unknown {
  const t = c ? obj : {};
  return t[key];
}
"#,
    );
    assert!(
        !codes.contains(&7053),
        "subtype reduction must drop the non-indexable `{{}}` arm in either branch order; unexpected TS7053: {codes:?}",
    );
}

/// Negative / fallback: when neither branch subsumes the other, the union is
/// preserved. Two disjoint object literals with no string index signature must
/// still produce TS7053 on element access (no over-suppression).
#[test]
fn conditional_without_subtype_relation_still_reports_ts7053() {
    let codes = check_source_strict_codes(
        r#"
function disjoint(c: boolean, key: string): unknown {
  const t = c ? { a: 1 } : { b: 2 };
  return t[key];
}
"#,
    );
    assert!(
        codes.contains(&7053),
        "disjoint object branches have no string index signature and must still report TS7053: {codes:?}",
    );
}

/// Negative: two fresh object literals that share a property are preserved as a
/// union (tsc's fresh-literal complement behavior), so excess/index access on
/// the union still reports the implicit-any element access.
#[test]
fn conditional_both_fresh_object_literals_are_not_collapsed() {
    let codes = check_source_strict_codes(
        r#"
function complement(c: boolean, key: string): unknown {
  const t = c ? { a: 1 } : { a: 1, b: 2 };
  return t[key];
}
"#,
    );
    assert!(
        codes.contains(&7053),
        "fresh-literal complement union has no string index signature and must still report TS7053: {codes:?}",
    );
}

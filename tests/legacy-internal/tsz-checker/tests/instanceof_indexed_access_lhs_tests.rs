//! A deferred indexed-access `T[K]` is a valid `instanceof` left operand.
//!
//! Structural rule: tsc's instanceof-LHS check accepts `any`, an object type, or
//! a type parameter; for a deferred indexed access `T[K]` it looks through to the
//! apparent (object-like constraint) type, so `shape[k] instanceof C` is legal
//! when `T extends { [k: string]: Base }`. tsz's `InstanceofLeftOperandVisitor`
//! had no `visit_index_access` arm, so a `T[K]` fell to the default (invalid) and
//! produced a false TS2358. A *concrete* indexed access is already evaluated to
//! its element type before the visitor, so a primitive element stays invalid.
//! Owner: `tsz-solver` `operations/binary_ops.rs` `InstanceofLeftOperandVisitor`.

use crate::test_utils::check_source_strict_codes as check_strict;

const TS2358: u32 = 2358; // LHS of instanceof must be any/object/type-parameter.

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

#[test]
fn deferred_indexed_access_is_valid_instanceof_lhs() {
    // `shape[k]` has type `T[keyof T]` (a deferred indexed access) — a valid LHS.
    let codes = check_strict(
        r#"
class Base { x!: number; }
class Sub extends Base { y!: number; }
type Shape = { [k: string]: Base };
function f<T extends Shape>(shape: T, k: keyof T) {
  const v = shape[k];
  if (v instanceof Sub) {
    v.y;
  }
}
"#,
    );
    assert_eq!(
        count(&codes, TS2358),
        0,
        "deferred T[K] is a valid instanceof LHS: {codes:?}"
    );
}

#[test]
fn deferred_indexed_access_lhs_is_name_independent() {
    // Anti-hardcoding cover: renamed binders, same rule.
    let codes = check_strict(
        r#"
class P { a!: number; }
class Q extends P { b!: number; }
type Rec = { [key: string]: P };
function g<U extends Rec>(rec: U, key: keyof U) {
  if (rec[key] instanceof Q) {}
}
"#,
    );
    assert_eq!(count(&codes, TS2358), 0, "{codes:?}");
}

#[test]
fn concrete_primitive_indexed_access_stays_invalid_instanceof_lhs() {
    // Negative control: a concrete `o["name"]` evaluates to `string` before the
    // visitor, so it is still an invalid instanceof LHS (TS2358), matching tsc.
    let codes = check_strict(
        r#"
declare const o: { name: string };
class X {}
const r = o["name"] instanceof X;
"#,
    );
    assert_eq!(
        count(&codes, TS2358),
        1,
        "concrete string indexed access stays invalid: {codes:?}"
    );
}

//! TS2352 false-positive: a both-callable `as`-assertion skips the
//! construct/call-signature structural overlap (#14325, mined from arktype).
//!
//! When an `as`-assertion has a callable/constructor source and a
//! callable/constructor target and neither is assignable to the other, tsc
//! decides overlap by structurally comparing call/construct signatures
//! (erasing the target's generic type parameters). tsz short-circuited via a
//! `both_callable` guard that skipped the comparison, leaving overlap false →
//! a spurious TS2352.

use crate::test_utils::check_source_strict_codes as check_strict;

fn ts2352(source: &str) -> Vec<u32> {
    check_strict(source)
        .into_iter()
        .filter(|c| *c == 2352)
        .collect()
}

/// The arktype witness: an anonymous class source asserted to a construct
/// signature returning a concrete object type. `{}` is not assignable to the
/// target instance and the constructor arities differ, so neither side is
/// assignable to the other — but the construct signatures structurally
/// overlap, so tsc is clean.
#[test]
fn anon_class_to_concrete_construct_sig_no_ts2352() {
    let source = r#"
type T = { x: number };
const B = class {} as new (base: T) => T;
export { B };
"#;
    assert!(
        ts2352(source).is_empty(),
        "no TS2352 expected — construct signatures overlap. Got: {:?}",
        check_strict(source)
    );
}

/// Construct-signature target carrying its own generic type parameter. tsc
/// erases the target's type parameter when computing overlap.
#[test]
fn anon_class_to_generic_construct_sig_no_ts2352() {
    let source = r#"
const B = class {} as new <U>(base: U) => U;
export { B };
"#;
    assert!(
        ts2352(source).is_empty(),
        "no TS2352 expected — generic construct signatures overlap after erasure. Got: {:?}",
        check_strict(source)
    );
}

/// Construct-signature target whose return type is a wider object than the
/// source instance. The empty-class instance `{}` is not assignable to the
/// richer instance, and the constructor arities differ, so neither side is
/// assignable to the other — but the construct return types are mutually
/// comparable, so tsc is clean.
#[test]
fn anon_class_to_richer_construct_sig_no_ts2352() {
    let source = r#"
interface Node { kind: number; next: Node | null }
const B = class {} as new (base: Node) => Node;
export { B };
"#;
    assert!(
        ts2352(source).is_empty(),
        "no TS2352 expected — construct signatures overlap. Got: {:?}",
        check_strict(source)
    );
}

/// Negative control: a plain function value (call signature only) asserted to
/// a construct signature has no comparable signature, so TS2352 must remain.
#[test]
fn plain_function_to_construct_sig_still_emits_ts2352() {
    let source = r#"
type T = { x: number };
declare const f: (base: number) => number;
const c = f as new (base: number) => T;
export { c };
"#;
    assert!(
        !ts2352(source).is_empty(),
        "TS2352 expected — a call-only function is not comparable to a construct signature. Got: {:?}",
        check_strict(source)
    );
}

/// Negative control: construct signatures whose return types genuinely do not
/// overlap (disjoint primitives) must still error.
#[test]
fn construct_sig_disjoint_returns_still_emits_ts2352() {
    let source = r#"
declare const a: new (base: number) => number;
const b = a as new (base: number) => string;
export { b };
"#;
    assert!(
        !ts2352(source).is_empty(),
        "TS2352 expected — `number` and `string` construct returns do not overlap. Got: {:?}",
        check_strict(source)
    );
}

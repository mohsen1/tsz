//! Without `strictNullChecks`, `null` is assignable to every non-union target
//! — including `void`.
//!
//! tsc's `isSimpleTypeRelatedTo`:
//!
//! ```text
//! s & Undefined && (!strictNullChecks && !(t & UnionOrIntersection) || t & (Undefined | Void))
//! s & Null      && (!strictNullChecks && !(t & UnionOrIntersection) || t & Null)
//! ```
//!
//! The `undefined`-may-hit-`void`-but-`null`-may-not asymmetry lives entirely
//! in the **strict** half of those conditions (`t & (Undefined | Void)` vs
//! `t & Null`). tsz had it applied inside the *non-strict* branch, which made
//! `var x: void = null` an error under `strict: false` while `number`,
//! `string` and `undefined` targets were all correctly accepted.

use tsz_checker::test_utils::{check_source_non_strict_codes, check_source_strict_codes};

fn non_strict_codes(source: &str) -> Vec<u32> {
    check_source_non_strict_codes(source)
}

/// The regression: `void` was the one target that wrongly rejected `null`.
#[test]
fn null_is_assignable_to_void_without_strict_null_checks() {
    assert!(
        non_strict_codes("var a: void = null;\n").is_empty(),
        "non-strict `null` -> `void` must be accepted"
    );
}

/// The targets that already worked — guard against a fix that over-corrects in
/// the other direction.
#[test]
fn null_is_assignable_to_ordinary_targets_without_strict_null_checks() {
    // Lib-dependent targets (`Object`, `Date`) are covered by the
    // conformance row; the unit harness loads no lib.
    for target in ["number", "string", "undefined", "boolean", "{}"] {
        let source = format!("var a: {target} = null;\n");
        assert!(
            non_strict_codes(&source).is_empty(),
            "non-strict `null` -> `{target}` must be accepted"
        );
    }
}

#[test]
fn undefined_is_assignable_to_void_without_strict_null_checks() {
    assert!(non_strict_codes("var a: void = undefined;\n").is_empty());
}

/// Strict mode keeps the asymmetry: `undefined` reaches `void`, `null` does not.
#[test]
fn strict_mode_still_rejects_null_to_void() {
    assert!(
        check_source_strict_codes("var a: void = null;\n").contains(&2322),
        "strict `null` -> `void` must still be an error"
    );
}

#[test]
fn strict_mode_still_accepts_undefined_to_void() {
    assert!(
        !check_source_strict_codes("var a: void = undefined;\n").contains(&2322),
        "strict `undefined` -> `void` is legal"
    );
}

/// Strict mode still rejects the ordinary targets, so the change is confined to
/// the non-strict branch.
#[test]
fn strict_mode_still_rejects_null_to_ordinary_targets() {
    for target in ["number", "string", "undefined"] {
        let source = format!("var a: {target} = null;\n");
        assert!(
            check_source_strict_codes(&source).contains(&2322),
            "strict `null` -> `{target}` must still be an error"
        );
    }
}

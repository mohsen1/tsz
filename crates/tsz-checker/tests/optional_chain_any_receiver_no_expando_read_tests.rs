//! An optional-chain property read off an `any`-typed receiver must not
//! recover an "expando" assigned type; it stays `any`.
//!
//! Closes #16710. The structural rule:
//!
//! > When a property is read through an optional chain (`recv?.p`) and the
//! > receiver's type is `any`, `tsc` keeps the result `any` — a plain `any`
//! > receiver is not an expando root, so a preceding `recv.p = <value>`
//! > assignment does not synthesize a member. `tsz` computed the same `any`
//! > through the property solver, but the optional-chain property *fast path*
//! > then refined that `any` by walking the file for a matching
//! > `recv.p = <value>` assignment and adopting the written value's type,
//! > narrowing `recv?.p` away from `any`.
//!
//! The non-optional spelling (`recv.p`) already stayed `any` because its read
//! path gates expando recovery on `is_expando_property_read`. The fix applies
//! the same gate inside `refine_expando_property_read_type`
//! (`crates/tsz-checker/src/types/property_access_helpers/expando.rs`), the only
//! caller of which is the optional-chain property fast path, so genuine expando
//! roots (functions/namespaces/`checkJs` variables) are unaffected while a plain
//! `any` receiver is left `any`.
//!
//! `obj?.a = 1` is itself the invalid-optional-write-target error TS2779; the
//! rows below assert that the *read* no longer adds a spurious TS2322 on top of
//! it (the written `1` must not become the read type of `obj?.a`).

use tsz_checker::test_utils::check_source_strict_codes;

/// `obj?.a = 1; obj?.a` — the reported repro. Only the invalid-write-target
/// TS2779 fires; the read stays `any`, so assigning it to `string` is clean.
#[test]
fn optional_property_write_read_stays_any() {
    let codes = check_source_strict_codes(
        "declare const obj: any;\nobj?.a = 1;\nlet x: string = obj?.a;\n",
    );
    assert_eq!(codes, vec![2779], "obj?.a must stay `any`, got {codes:?}");
}

/// A valid (non-optional) write followed by an optional read: no TS2779 because
/// the write target is valid, and no TS2322 because the read stays `any`.
#[test]
fn plain_write_optional_read_stays_any() {
    let codes =
        check_source_strict_codes("declare const obj: any;\nobj.a = 1;\nlet x: string = obj?.a;\n");
    assert!(codes.is_empty(), "obj?.a must stay `any`, got {codes:?}");
}

/// Nested optional chain over an `any` receiver: `obj?.a?.b` stays `any`.
#[test]
fn nested_optional_read_stays_any() {
    let codes = check_source_strict_codes(
        "declare const obj: any;\nobj.a.b = 1;\nlet x: string = obj?.a?.b;\n",
    );
    assert!(codes.is_empty(), "obj?.a?.b must stay `any`, got {codes:?}");
}

/// The decision is structural, not keyed to any particular identifier: renamed
/// binders behave identically (guards against a name-based fast path).
#[test]
fn optional_property_read_stays_any_renamed_binders() {
    let codes = check_source_strict_codes(
        "declare const zqz: any;\nzqz?.qbq = 1;\nlet y: string = zqz?.qbq;\n",
    );
    assert_eq!(codes, vec![2779], "zqz?.qbq must stay `any`, got {codes:?}");
}

/// Regression guard: the element-access optional chain was already correct and
/// must remain so (`obj?.["a"]` stays `any`).
#[test]
fn optional_element_read_stays_any() {
    let codes = check_source_strict_codes(
        "declare const obj: any;\nobj?.[\"a\"] = 1;\nlet x: string = obj?.[\"a\"];\n",
    );
    assert_eq!(
        codes,
        vec![2779],
        "obj?.[\"a\"] must stay `any`, got {codes:?}"
    );
}

/// Regression guard: the non-optional property read already stayed `any`.
#[test]
fn plain_property_read_stays_any() {
    let codes =
        check_source_strict_codes("declare const obj: any;\nobj.a = 1;\nlet x: string = obj.a;\n");
    assert!(codes.is_empty(), "obj.a must stay `any`, got {codes:?}");
}

/// Regression guard: a plain-identifier `any` never narrows on assignment.
#[test]
fn plain_identifier_any_stays_any() {
    let codes = check_source_strict_codes("let obj: any;\nobj = 1;\nlet x: string = obj;\n");
    assert!(codes.is_empty(), "obj must stay `any`, got {codes:?}");
}

/// Positive guard: a genuine function-expando root still resolves the member
/// through the non-optional read — `F.p` is `number`, so assigning it to
/// `string` reports TS2322. (This is what the gate must *not* suppress.)
#[test]
fn function_expando_plain_read_resolves_member() {
    let codes = check_source_strict_codes("function F() {}\nF.p = 1;\nlet x: string = F.p;\n");
    assert_eq!(
        codes,
        vec![2322],
        "F.p must resolve to `number`, got {codes:?}"
    );
}

/// Positive guard: a genuine function-expando root still resolves the member
/// through the *optional* read too — `F?.p` is `number` (a function is never
/// nullish), so assigning it to `string` reports TS2322.
#[test]
fn function_expando_optional_read_resolves_member() {
    let codes = check_source_strict_codes("function F() {}\nF.p = 1;\nlet x: string = F?.p;\n");
    assert_eq!(
        codes,
        vec![2322],
        "F?.p must resolve to `number`, got {codes:?}"
    );
}

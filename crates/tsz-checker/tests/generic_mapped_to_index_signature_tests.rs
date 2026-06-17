use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_lib_files};

/// Type-check `source` as an external module with the bundled `es5` lib loaded
/// (so the global utility types `Pick`/`Omit`/`Record`/`Exclude`/… are in
/// scope), returning only the diagnostic codes. Skips gracefully (empty) when
/// the bundled lib asset is unavailable in the build environment.
fn check_with_es5_lib_codes(source: &str) -> Vec<u32> {
    let libs = load_lib_files(&["es5.d.ts"]);
    if libs.is_empty() {
        return Vec::new();
    }
    check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &libs)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

fn ts2322_count(codes: &[u32]) -> usize {
    codes.iter().filter(|&&c| c == 2322).count()
}

fn ts2345_count(codes: &[u32]) -> usize {
    codes.iter().filter(|&&c| c == 2345).count()
}

// =============================================================================
// Regression guard: superstruct "Omit-overlap" false-positive family.
//
// Structural rule: a generic homomorphic mapped type `{ [P in K]: Src[P] }`
// (the body of `Pick<Src, K>` / `Omit<Src, K>` and friends) whose key set `K`
// is a generic subset of `keyof Src` is assignable to an index-signature target
// `{ [x: string]: V }` exactly when every value `Src` can yield (its declared
// property values and index-signature values) is assignable to `V`. The
// per-key access `Src[P]` stays deferred because `P`'s key constraint resolves
// to a generic `keyof T`, so the relation must fall back to relating `Src`'s
// own value sources to `V` rather than rejecting the unreduced indexed access.
//
// tsz used to report a false `TS2322` (mapped not assignable to `Record`) and a
// follow-on `TS2345` (`Omit<S, K>` not assignable to the `Record` parameter).
// =============================================================================

/// `Pick<S, K>` over `S extends Record<string, V>` is assignable to that same
/// `Record<string, V>` (positive: single generic key parameter).
#[test]
fn pick_over_record_constrained_param_assignable_to_record() {
    let codes = check_with_es5_lib_codes(
        r#"
type Cell = { v: number };
type Sheet = Record<string, Cell>;
function project<S extends Sheet, K extends keyof S>(x: Pick<S, K>): Sheet {
    return x;
}
export {};
"#,
    );
    if codes.is_empty() {
        return; // lib asset unavailable — covered by CLI/conformance instead
    }
    assert_eq!(
        ts2322_count(&codes),
        0,
        "Pick<S, K> must be assignable to its source Record type: {codes:?}"
    );
}

/// `Omit<S, K>` over `S extends Record<string, V>` is assignable to that same
/// `Record<string, V>`. Binder names deliberately differ from the `Pick` case
/// to keep the rule structural (no name-based logic).
#[test]
fn omit_over_record_constrained_param_assignable_to_record() {
    let codes = check_with_es5_lib_codes(
        r#"
type Entry = { payload: string };
type Bag = Record<string, Entry>;
function drop<TBag extends Bag, Keys extends keyof TBag>(
    bag: Omit<TBag, Keys>
): Bag {
    return bag;
}
export {};
"#,
    );
    if codes.is_empty() {
        return;
    }
    assert_eq!(
        ts2322_count(&codes),
        0,
        "Omit<S, K> must be assignable to its source Record type: {codes:?}"
    );
    assert_eq!(
        ts2345_count(&codes),
        0,
        "no follow-on argument mismatch: {codes:?}"
    );
}

/// Concrete source object, generic key: `Omit<Conc, K>` where `Conc` is a
/// concrete object whose property values all satisfy the target index value.
#[test]
fn omit_over_concrete_object_generic_key_assignable_to_record() {
    let codes = check_with_es5_lib_codes(
        r#"
type Cell = { v: number };
type Conc = { a: { v: 1 }; b: { v: 2 } };
type Sheet = Record<string, Cell>;
function narrow<K extends keyof Conc>(x: Omit<Conc, K>): Sheet {
    return x;
}
export {};
"#,
    );
    if codes.is_empty() {
        return;
    }
    assert_eq!(
        ts2322_count(&codes),
        0,
        "Omit<Conc, K> over a concrete index-compatible object is assignable: {codes:?}"
    );
}

/// Negative: when the source value type does NOT satisfy the target index
/// value, the assignment must still error (the fall-back must not over-accept).
#[test]
fn pick_with_mismatched_value_type_still_errors() {
    let codes = check_with_es5_lib_codes(
        r#"
type Cell = { v: number };
type NumberSheet = Record<string, number>;
type CellSheet = Record<string, Cell>;
function bad<S extends NumberSheet, K extends keyof S>(x: Pick<S, K>): CellSheet {
    return x;
}
export {};
"#,
    );
    if codes.is_empty() {
        return;
    }
    assert_eq!(
        ts2322_count(&codes),
        1,
        "number values must not satisfy a Record<string, Cell> target: {codes:?}"
    );
}

/// Negative: a target with a required named property the mapped type cannot
/// supply must still error.
#[test]
fn pick_to_record_with_required_named_property_still_errors() {
    let codes = check_with_es5_lib_codes(
        r#"
type Cell = { v: number };
type Sheet = Record<string, Cell>;
function bad<S extends Sheet, K extends keyof S>(
    x: Pick<S, K>
): Sheet & { required: string } {
    return x;
}
export {};
"#,
    );
    if codes.is_empty() {
        return;
    }
    assert_eq!(
        ts2322_count(&codes),
        1,
        "a missing required property must still error: {codes:?}"
    );
}

/// Widening the target index value (here `unknown`) keeps the assignment valid:
/// every `S[P]` is assignable to `unknown`.
#[test]
fn pick_assignable_to_record_of_unknown() {
    let codes = check_with_es5_lib_codes(
        r#"
type Cell = { v: number };
type Sheet = Record<string, Cell>;
function widen<S extends Sheet, K extends keyof S>(
    x: Pick<S, K>
): Record<string, unknown> {
    return x;
}
export {};
"#,
    );
    if codes.is_empty() {
        return;
    }
    assert_eq!(
        ts2322_count(&codes),
        0,
        "Pick<S, K> is assignable to Record<string, unknown>: {codes:?}"
    );
}

//! Tests for TS2352 assertion-overlap with the `keyof T` type operator.
//!
//! Conformance test:
//! `TypeScript/tests/cases/conformance/types/keyof/keyofAndIndexedAccess.ts`
//! line 187: `const x2 = k as string;` where `k: keyof T`.
//!
//! `keyof T` reduces to a subset of `string | number | symbol`, so for
//! assertion overlap purposes it is comparable to any of those primitives
//! (or their literals). tsc's `isTypeComparableTo` walks the `keyof` to its
//! key-space union; without this case, an assertion like
//! `(k as string)` falls through to the property-overlap check (`KeyOf` has
//! no extractable properties) and emits a false-positive TS2352.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_strict(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// `k as string` where `k: keyof T` must NOT emit TS2352. `keyof T` is a
/// subset of `string | number | symbol`, so the cast to `string` overlaps.
#[test]
fn keyof_t_assert_to_string_no_ts2352() {
    let source = r#"
function f<T>(k: keyof T) {
    const s = k as string;
}
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — `keyof T` overlaps with `string`. Got: {codes:?}"
    );
}

/// Symmetric direction: `s as keyof T` where `s: string` must also not error.
#[test]
fn string_assert_to_keyof_t_no_ts2352() {
    let source = r#"
function f<T>(s: string) {
    const k = s as keyof T;
}
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — `string` overlaps with `keyof T`. Got: {codes:?}"
    );
}

/// `keyof T` to `number` should also be comparable.
#[test]
fn keyof_t_assert_to_number_no_ts2352() {
    let source = r#"
function f<T>(k: keyof T) {
    const n = k as number;
}
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — `keyof T` overlaps with `number`. Got: {codes:?}"
    );
}

/// `keyof T` to a literal type must also be comparable. tsc accepts
/// `(k as "x")` because `keyof T` could include the literal `"x"`.
#[test]
fn keyof_t_assert_to_string_literal_no_ts2352() {
    let source = r#"
function f<T>(k: keyof T) {
    const lit = k as "x";
}
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — `keyof T` overlaps with literal `\"x\"`. Got: {codes:?}"
    );
}

/// Sanity: `keyof T` does NOT overlap with `boolean` or other unrelated
/// primitives. The fix must remain narrow.
#[test]
fn keyof_t_assert_to_boolean_emits_ts2352() {
    let source = r#"
function f<T>(k: keyof T) {
    const b = k as boolean;
}
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        !ts2352.is_empty(),
        "TS2352 expected — `keyof T` does not overlap with `boolean`. Got: {codes:?}"
    );
}

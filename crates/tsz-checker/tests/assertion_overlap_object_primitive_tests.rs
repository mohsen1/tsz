//! Tests for TS2352 type-assertion overlap with the `object` primitive
//! and the `{}` empty object type.
//!
//! Conformance test:
//! `TypeScript/tests/cases/compiler/genericWithNoConstraintComparableWithCurlyCurly.ts`
//!
//! Before this fix, `{} as T` where `T extends object | null | undefined`
//! emitted a false-positive TS2352 because the assertion-overlap check
//! returned false when both sides had no extractable properties:
//! `types_have_common_properties_relaxed` short-circuits on the
//! "both empty" branch. tsc treats the `object` primitive as overlapping
//! with any object-like type, and walks a TypeParameter's constraint
//! when the source is the empty object type `{}`.
//!
//! Two narrow rules added in `types_are_comparable_for_assertion_inner`:
//!   1. `object` primitive ↔ Object/Array/Tuple/Callable/Function/Intersection
//!      → comparable.
//!   2. `{}` (empty object: no required props, no index sigs) ↔
//!      TypeParameter with constraint → recurse against the constraint.
//!
//! The "narrow to {}" gating is important: fully unwrapping any source's
//! type-parameter constraint would over-permit assertions like
//! `B as T extends A` (genericTypeAssertions4.ts).

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

/// `{} as T` with `T extends object | null | undefined` must NOT emit TS2352.
/// The constraint contains `object`, which the empty-object source overlaps with.
#[test]
fn empty_object_assert_to_typeparam_with_object_in_constraint_no_ts2352() {
    let source = r#"
function yes<T extends object | null | undefined>() {
    let x = {};
    x as T;
}
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — `{{}}` overlaps with `object` in T's constraint. Got: {codes:?}"
    );
}

/// `{} as T` with `T extends null | undefined` SHOULD emit TS2352.
/// The constraint has no object-like member; the empty-object source has no
/// overlap with null/undefined alone.
#[test]
fn empty_object_assert_to_typeparam_without_object_in_constraint_emits_ts2352() {
    let source = r#"
function no<T extends null | undefined>() {
    let x = {};
    x as T;
}
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        !ts2352.is_empty(),
        "TS2352 expected — `{{}}` does not overlap with `null | undefined`. Got: {codes:?}"
    );
}

/// `{} as T` with `T` (no constraint), `T extends unknown`, `T extends {}`,
/// `T extends object` — none of these emit TS2352. Each constraint either
/// has no upper bound (T = unknown) or an object-like upper bound.
#[test]
fn empty_object_assert_to_typeparam_no_or_object_constraint_no_ts2352() {
    let source = r#"
function foo<T>() {
    let x = {};
    x as T;
}
function bar<T extends unknown>() {
    let x = {};
    x as T;
}
function baz<T extends {}>() {
    let x = {};
    x as T;
}
function bat<T extends object>() {
    let x = {};
    x as T;
}
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected for any of foo/bar/baz/bat. Got: {codes:?}"
    );
}

/// Sanity: the empty-object special case must NOT cause `B as T extends A`
/// (where B is a specific subclass, not the empty object) to lose TS2352.
/// tsc emits TS2352 here because B is just one of many possible subtypes of A,
/// and T is opaque — B is not necessarily T.
#[test]
fn specific_subclass_assert_to_typeparam_emits_ts2352() {
    let source = r#"
class A { foo() { return ""; } }
class B extends A { bar() { return 1; } }
declare let b: B;
function foo2<T extends A>() {
    let y = b as T;
}
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        !ts2352.is_empty(),
        "TS2352 expected — B is one specific subtype of A, not opaque T. Got: {codes:?}"
    );
}

/// Sanity: `object` primitive overlaps with arbitrary object/array/tuple
/// shapes for assertion purposes.
#[test]
fn object_primitive_overlaps_array_assertion() {
    let source = r#"
declare let o: object;
let arr = o as number[];
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — `object` overlaps with array shapes. Got: {codes:?}"
    );
}

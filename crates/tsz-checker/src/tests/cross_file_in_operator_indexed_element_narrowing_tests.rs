//! `in`-operator narrowing of a cross-file discriminated-union element reached
//! through an indexed access (`xs[0]`, `m[k]`, `t[1]`).
//!
//! When a file checker lowers an imported alias body
//! (`type Either<E, A> = Left<E> | Right<A>`), the inner `Left`/`Right`
//! references stay interned as `Application(UnresolvedTypeName(..), args)` until
//! a later resolver pass. The solver-side `NarrowingContext` resolver is a
//! `TypeEnvironment` that only resolves names it was explicitly seeded with, so
//! when those still-unresolved members reach `narrow_by_property_presence` it
//! cannot read their property tables. `'right' in a` then fails to filter the
//! union, keeps every member, and a later `a.right` access surfaces a false
//! `TS2339` (or, in a typed return position, a `TS2322` `'unknown'`-vs-`A`).
//!
//! The equality/discriminant path (`a._tag === 'Right'`) already recovers this
//! residue through the `CheckerContext` resolver (#14992), but the `in` operator
//! (and the other `typeof`/`instanceof`/predicate guards) take the "solver-first"
//! guard path, which did not. The fix recovers the residue at the shared guard
//! application chokepoint (`narrow_with_guard_via_flow_boundary`) so every guard
//! form reads the resolved members (#14756).
//!
//! The cross-file cases vary binder names and cover
//! `Array`/`ReadonlyArray`/`Record`/tuple index receivers plus a 3-way
//! discriminant. Same-file negative controls (which do not need cross-file
//! recovery) assert that the narrowing logic itself is unchanged: a wrong-branch
//! access and a genuinely-absent property must still report `TS2339`, proving the
//! `in` check narrows to the correct single member rather than silencing the
//! union.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::{check_multi_file_with_libs_stamped, load_lib_files};
use tsz_common::common::ModuleKind;

const PROPERTY_DOES_NOT_EXIST: u32 = 2339;
const TYPE_NOT_ASSIGNABLE: u32 = 2322;

fn check(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file_with_libs_stamped(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            ..CheckerOptions::default()
        },
        &load_lib_files(&["es5.d.ts"]),
    )
}

fn narrowing_false_positives(diagnostics: &[Diagnostic]) -> Vec<(u32, String)> {
    diagnostics
        .iter()
        .filter(|d| d.code == PROPERTY_DOES_NOT_EXIST || d.code == TYPE_NOT_ASSIGNABLE)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Assert the cross-file `in` narrowing produces no false-positive `TS2339`/
/// `TS2322` (`./either.ts` carries the discriminated union, `./main.ts` the
/// narrowing site). `msg` describes the receiver shape under test.
fn assert_in_narrowing_clean(main: &str, msg: &str) {
    let diags = check(
        &[("./either.ts", EITHER_SRC), ("./main.ts", main)],
        "./main.ts",
    );
    let errors = narrowing_false_positives(&diags);
    assert!(errors.is_empty(), "{msg}, got: {errors:?}");
}

const EITHER_SRC: &str = r#"
export interface Left<E> { readonly _tag: 'Left'; readonly left: E }
export interface Right<A> { readonly _tag: 'Right'; readonly right: A }
export type Either<E, A> = Left<E> | Right<A>
export interface Both<E, A> { readonly _tag: 'Both'; readonly left: E; readonly right: A }
export type These<E, A> = Left<E> | Right<A> | Both<E, A>
"#;

#[test]
fn in_operator_narrows_array_indexed_cross_file_union() {
    let main = r#"
import { type Either } from './either'
export const f = <X, A>(xs: Array<Either<X, A>>): A | undefined => {
    const a = xs[0]
    if ('right' in a) { return a.right }
    return undefined
}
"#;
    assert_in_narrowing_clean(
        main,
        "expected `'right' in a` to narrow the indexed cross-file element to `Right<A>`",
    );
}

#[test]
fn in_operator_narrows_left_branch_indexed() {
    let main = r#"
import { type Either } from './either'
export const lhs = <X, A>(xs: Array<Either<X, A>>): X | undefined => {
    const a = xs[0]
    if ('left' in a) { return a.left }
    return undefined
}
"#;
    assert_in_narrowing_clean(
        main,
        "expected the positive `'left' in a` branch to narrow to `Left<X>`",
    );
}

#[test]
fn in_operator_narrows_readonly_array_indexed() {
    let main = r#"
import { type Either } from './either'
export const ro = <X, A>(xs: ReadonlyArray<Either<X, A>>): A | undefined => {
    const a = xs[0]
    if ('right' in a) { return a.right }
    return undefined
}
"#;
    assert_in_narrowing_clean(
        main,
        "expected ReadonlyArray indexed `in` narrowing to stay clean",
    );
}

#[test]
fn in_operator_narrows_record_value_indexed() {
    let main = r#"
import { type Either } from './either'
export const rec = <X, A>(m: Record<string, Either<X, A>>, k: string): A | undefined => {
    const a = m[k]
    if ('right' in a) { return a.right }
    return undefined
}
"#;
    assert_in_narrowing_clean(
        main,
        "expected Record value indexed `in` narrowing to stay clean",
    );
}

#[test]
fn in_operator_narrows_tuple_indexed() {
    let main = r#"
import { type Either } from './either'
export const tup = <X, A>(t: [Either<X, A>, Either<X, A>]): A | undefined => {
    const a = t[1]
    if ('right' in a) { return a.right }
    return undefined
}
"#;
    assert_in_narrowing_clean(main, "expected tuple indexed `in` narrowing to stay clean");
}

#[test]
fn in_operator_narrows_three_way_indexed() {
    let main = r#"
import { type These } from './either'
export const three = <X, A>(xs: Array<These<X, A>>): A | undefined => {
    const a = xs[0]
    if ('right' in a) { return a.right }
    return undefined
}
"#;
    assert_in_narrowing_clean(
        main,
        "expected 3-way discriminant indexed `in` narrowing to stay clean",
    );
}

#[test]
fn in_operator_concrete_array_index_narrows() {
    // No use-site generics: still a cross-file generic alias, so the same residue
    // path applies (per the issue's concrete-array witness).
    let main = r#"
import { type Either } from './either'
export const f = (xs: Array<Either<string, number>>): number | undefined => {
    const a = xs[0]
    if ('right' in a) { return a.right }
    return undefined
}
"#;
    assert_in_narrowing_clean(
        main,
        "expected the concrete `Array<Either<string, number>>` index `in` narrowing to be clean",
    );
}

#[test]
fn renamed_binders_in_operator_indexed_narrowing_stays_clean() {
    // Same shape with entirely different binder names, so the recovery cannot key
    // on `Either`/`Left`/`Right`/`_tag`/`right`.
    let shapes = r#"
export interface Ok<V> { readonly variant: 'ok'; readonly payload: V }
export interface Fail<E> { readonly variant: 'fail'; readonly reason: E }
export type Result<V, E> = Ok<V> | Fail<E>
"#;
    let main = r#"
import { type Result } from './shapes'
export const f = <V, E>(rs: Array<Result<V, E>>): V | undefined => {
    const r = rs[0]
    if ('payload' in r) { return r.payload }
    return undefined
}
"#;
    let diags = check(&[("./shapes.ts", shapes), ("./main.ts", main)], "./main.ts");
    let errors = narrowing_false_positives(&diags);
    assert!(
        errors.is_empty(),
        "expected renamed-binder indexed `in` narrowing to stay clean, got: {errors:?}",
    );
}

/// Same-file source so no cross-file residue recovery is involved: this exercises
/// the unchanged member-presence narrowing logic. In the `'l' in a` true branch
/// the element narrows to `L<X>`, so `.r` (only on `R<A>`) must still report
/// TS2339 — the fix must not silence genuine mismatches.
const SAME_FILE_DU: &str = r#"
interface L<E> { readonly _tag: 'L'; readonly l: E }
interface R<A> { readonly _tag: 'R'; readonly r: A }
type Ei<E, A> = L<E> | R<A>
"#;

#[test]
fn in_operator_wrong_branch_access_still_reports_ts2339() {
    let main = format!(
        "{SAME_FILE_DU}\nexport const f = <X, A>(xs: Array<Ei<X, A>>): A | undefined => {{\n    const a = xs[0]\n    if ('l' in a) {{ return a.r }}\n    return undefined\n}}\n"
    );
    let diags = check(&[("./main.ts", &main)], "./main.ts");
    assert!(
        diags
            .iter()
            .any(|d| d.code == PROPERTY_DOES_NOT_EXIST && d.message_text.contains('r')),
        "expected TS2339 for `.r` on the narrowed `L<X>` branch, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

#[test]
fn in_operator_genuinely_absent_property_still_reports_ts2339() {
    let main = format!(
        "{SAME_FILE_DU}\nexport const f = <X, A>(xs: Array<Ei<X, A>>): A | undefined => {{\n    const a = xs[0]\n    if ('r' in a) {{ return a.missing }}\n    return undefined\n}}\n"
    );
    let diags = check(&[("./main.ts", &main)], "./main.ts");
    assert!(
        diags
            .iter()
            .any(|d| d.code == PROPERTY_DOES_NOT_EXIST && d.message_text.contains("missing")),
        "expected TS2339 for the genuinely-absent `missing` property, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

//! Regression for issue #14261: a callee type argument must be inferable from
//! an argument matched against an **indexed-access parameter type** whose object
//! is a generic application carrying the type parameter (`Ord<A>['compare']`).
//!
//! Root cause: the generic-call constraint walker reduced an indexed-access
//! target through the bare interner (`evaluate_index_access`), which has no
//! resolver and therefore cannot expand a `Lazy`/`Application` object such as
//! `Ord<A>` to its body. The access stayed unevaluated, so no candidate was ever
//! collected for `A` and it collapsed to its default (`unknown`), surfacing as a
//! callback whose parameters were `unknown` (false `TS2345`/`TS18046`). The
//! walker now expands the object through the checker's resolver — which preserves
//! the inference placeholder rather than collapsing it to its constraint — and
//! re-indexes, so `Ord<A>['compare']` reduces to `(first: A, second: A) =>
//! Ordering` and the argument contributes `A = string`.
//!
//! Each test varies the binder names so the fix is exercised structurally, not
//! via any identifier/file-name predicate. Positive cases assert the inferred
//! argument flows (no spurious error); negative cases assert the parameter was
//! bound to the *concrete* inferred type (a mismatched annotation still reports
//! exactly one `TS2322`), proving the value is not a permissive `any`/`unknown`
//! that would suppress the error.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::check_source_with_libs_code_messages;

fn codes(source: &str) -> Vec<u32> {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    let opts = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    check_source_with_libs_code_messages(source, "test.ts", opts, &libs)
        .into_iter()
        .map(|(c, _)| c)
        .collect()
}

/// The issue witness: a contextual return type (`Ord<string>`) fixes `A`, and
/// the unannotated callback parameters must pick up `string` through the
/// indexed-access parameter type `Ord<A>['compare']`.
#[test]
fn issue_14261_contextual_return_indexed_access_param_callback() {
    let c = codes(
        r#"
type Ordering = -1 | 0 | 1
interface Ord<A> { readonly compare: (first: A, second: A) => Ordering }
declare const fromCompare: <A>(compare: Ord<A>['compare']) => Ord<A>
export const fromBlock = (O: Ord<string>): Ord<string> =>
  fromCompare((x, y) => { return x.length < y.length ? -1 : 1 })
"#,
    );
    assert!(
        c.is_empty(),
        "callback params must be typed `string` via the indexed-access parameter; got {c:?}"
    );
}

/// Pure inference (no contextual return): explicit `string` callback parameters
/// must infer `A = string` through `Ord<A>['compare']`. The mismatched
/// `Ord<number>` annotation proves `A` bound to `string`, not `unknown`.
#[test]
fn issue_14261_pure_inference_indexed_access_param() {
    let c = codes(
        r#"
type Ordering = -1 | 0 | 1
interface Ord<A> { readonly compare: (first: A, second: A) => Ordering }
declare const f: <A>(c: Ord<A>['compare']) => Ord<A>
const r = f((x: string, y: string) => 1)
const ok: Ord<string> = r
const bad: Ord<number> = r
"#,
    );
    assert_eq!(
        c.iter().filter(|&&x| x == 2322).count(),
        1,
        "A must infer to `string`: only the `Ord<number>` annotation mismatches. Got {c:?}"
    );
}

/// Renamed binders, explicit callback parameters: the fix is structural, so the
/// same inference holds when every identifier differs.
#[test]
fn issue_14261_renamed_binders_indexed_access_param() {
    let c = codes(
        r#"
type Cmp = -1 | 0 | 1
interface Comparator<Q> { readonly run: (a: Q, b: Q) => Cmp }
declare const build: <Q>(run: Comparator<Q>['run']) => Comparator<Q>
const made = build((zzz: string, www: string) => 1)
const ok: Comparator<string> = made
const bad: Comparator<number> = made
"#,
    );
    assert_eq!(
        c.iter().filter(|&&x| x == 2322).count(),
        1,
        "Q must infer to `string` regardless of binder names. Got {c:?}"
    );
}

/// Nested indexed access (`Foo<A>['x']['y']`): reduction must descend through
/// every index step to reach the callback inference site.
#[test]
fn issue_14261_nested_indexed_access_param() {
    let c = codes(
        r#"
interface Foo<A> { x: { y: (v: A) => void } }
declare const g: <A>(cb: Foo<A>['x']['y']) => A
const r: string = g((v) => { const s: string = v })
"#,
    );
    assert!(
        c.is_empty(),
        "nested indexed-access parameter must infer `A = string`; got {c:?}"
    );
}

/// Indexed access over a mapped type (`Pick<Box<A>, 'make'>['make']`): the
/// object expands through a mapped-type application and still exposes `A`.
#[test]
fn issue_14261_mapped_indexed_access_param() {
    let c = codes(
        r#"
interface Box<A> { make: (x: A) => A; other: number }
declare const h: <A>(fn: Pick<Box<A>, 'make'>['make']) => A
const r: string = h((x) => { const s: string = x; return x })
"#,
    );
    assert!(
        c.is_empty(),
        "mapped indexed-access parameter must infer `A = string`; got {c:?}"
    );
}

/// A direct deferred indexed-access source must match the indexed-access arm of
/// a union target before the naked fallback can capture it. This is the generic
/// form of tsc's `inferFromTypes` indexed-access arm: infer pairwise from object
/// and index (`Source` -> `Obj`, `Prop` -> `Key`) even when the parameter type
/// is `Obj[Key] | Fallback`.
#[test]
fn issue_14261_union_target_prefers_indexed_access_arm_for_generic_source() {
    let c = codes(
        r#"
declare function capture<Obj, Key extends keyof Obj, Fallback>(
  value: Obj[Key] | Fallback
): Obj[Key]

function use<Source, Prop extends keyof Source>(
  value: Source[Prop],
): Source[Prop] {
  return capture(value)
}
"#,
    );
    assert!(
        c.is_empty(),
        "deferred indexed-access arm must infer `Obj = Source` and `Key = Prop`; got {c:?}"
    );
}

/// Sibling `keyof` form: a deferred `keyof Source` source should match a
/// `keyof Obj` union arm before the naked fallback.
#[test]
fn issue_14261_union_target_prefers_keyof_arm_for_generic_source() {
    let c = codes(
        r#"
declare function capture<Obj, Fallback>(
  value: keyof Obj | Fallback
): keyof Obj

function use<Source>(
  value: keyof Source,
): keyof Source {
  return capture(value)
}
"#,
    );
    assert!(
        c.is_empty(),
        "deferred keyof arm must infer `Obj = Source` before fallback; got {c:?}"
    );
}

/// A direct function parameter (no indexed access) already worked; pin it so the
/// fix does not perturb the baseline path.
#[test]
fn issue_14261_direct_function_param_unaffected() {
    let c = codes(
        r#"
type Ordering = -1 | 0 | 1
interface Ord<A> { readonly compare: (first: A, second: A) => Ordering }
declare const f: <A>(c: (first: A, second: A) => Ordering) => Ord<A>
const r = f((x: string, y: string) => 1)
const ok: Ord<string> = r
const bad: Ord<number> = r
"#,
    );
    assert_eq!(
        c.iter().filter(|&&x| x == 2322).count(),
        1,
        "baseline direct-function-param inference must still bind `A = string`. Got {c:?}"
    );
}

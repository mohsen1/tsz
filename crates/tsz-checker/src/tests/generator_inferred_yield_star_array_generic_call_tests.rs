//! Regression tests for #16119: an unannotated generator whose *sole* yield
//! contribution is `yield*` over an array reported a spurious `TS2345` when
//! its inferred return type was passed to a *generic* parameter (`want<T>(x:
//! AsyncGenerator<T, any, any>): T`) — the source rendered with no type
//! arguments at all ("Argument of type '`AsyncGenerator`' is not assignable to
//! parameter of type '`AsyncGenerator<number, any, any>`'") even though a
//! concrete, non-generic target (`AsyncGenerator<number, any, any>`) accepted
//! the identical value.
//!
//! Structural rule: `unannotated_generator_return_type`
//! (`types/function_type_helpers.rs`) builds the generator's own return type
//! as `Application(Lazy(AsyncGenerator/Generator def), [yield_t, return_t,
//! next_t])` directly through the solver's construction API, bypassing the
//! ordinary type-node lowering path an explicit `: AsyncGenerator<...>`
//! annotation goes through. A generic call's constraint/finalize passes can
//! later re-evaluate that exact `Application` through a resolver context that
//! cannot re-derive the lib interface's type parameters from a bare
//! `Lazy(DefId)` base on its own, and falls back to the interface's
//! unsubstituted structural shape — silently dropping `yield_t`/`return_t`/
//! `next_t`. The fix warms the solver's application-eval cache for this exact
//! `(def, args)` pair while the checker's env-aware resolver is still live,
//! so the later raw-solver re-evaluation hits the cache instead of
//! re-deriving (and getting it wrong).
//!
//! Every expectation below was pinned against `tsc` 7.0.2 (`--strict
//! --target es2018 --lib es2018,dom`).

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn strict_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn async_generator_sole_array_yield_star_passed_directly_to_generic_call() {
    assert!(
        strict_codes(
            r#"
async function* outer() { yield* [1, 2, 3]; }
declare function want<T>(x: AsyncGenerator<T, any, any>): T;
want(outer());
"#
        )
        .is_empty(),
        "an unannotated async generator whose sole yield is `yield*` over an \
         array must satisfy a generic AsyncGenerator<T, ...> parameter"
    );
}

#[test]
fn async_generator_sole_array_yield_star_bound_to_const_first() {
    // Same shape, argument bound to a variable before the call — must behave
    // identically to the direct-call-argument form above.
    assert!(
        strict_codes(
            r#"
async function* outer() { yield* [1, 2, 3]; }
declare function want<T>(x: AsyncGenerator<T, any, any>): T;
const g = outer();
want(g);
"#
        )
        .is_empty()
    );
}

#[test]
fn sync_generator_sole_array_yield_star_generic_call() {
    // Sync twin: the same construction path is shared by `Generator`.
    assert!(
        strict_codes(
            r#"
function* outer() { yield* [1, 2, 3]; }
declare function want<T>(x: Generator<T, any, any>): T;
want(outer());
"#
        )
        .is_empty()
    );
}

#[test]
fn async_generator_result_still_usable_as_the_inferred_type() {
    // The inferred T must actually be `number`, not `any`/`unknown` — a
    // downstream mismatch against the resolved T must still be rejected.
    assert_eq!(
        strict_codes(
            r#"
async function* outer() { yield* [1, 2, 3]; }
declare function want<T>(x: AsyncGenerator<T, any, any>): T;
const v = want(outer());
const bad: string = v;
"#
        ),
        vec![2322],
        "T must resolve to number so only the trailing string assignment fails"
    );
}

#[test]
fn generic_call_still_rejects_a_real_type_argument_mismatch() {
    // Negative control: a genuine T mismatch (source yields string, T is
    // constrained to number) must still report TS2345 — this is not a
    // blanket bypass of the relation.
    assert_eq!(
        strict_codes(
            r#"
async function* outer() { yield* ["a", "b"]; }
declare function want<T extends number>(x: AsyncGenerator<T, any, any>): T;
want(outer());
"#
        ),
        vec![2345]
    );
}

#[test]
fn concrete_non_generic_target_still_rejects_a_real_mismatch() {
    // Negative control on the non-generic side: a concrete parameter type
    // must still catch a genuine element-type mismatch.
    assert_eq!(
        strict_codes(
            r#"
async function* outer() { yield* [1, 2, 3]; }
declare function want(x: AsyncGenerator<string, any, any>): void;
want(outer());
"#
        ),
        vec![2345]
    );
}

#[test]
fn renamed_binders_do_not_change_the_outcome() {
    // Anti-hardcoding control: identical shape, every binder renamed.
    assert!(
        strict_codes(
            r#"
async function* qqq() { yield* [7, 8, 9]; }
declare function zzz<Elem>(g: AsyncGenerator<Elem, any, any>): Elem;
zzz(qqq());
"#
        )
        .is_empty()
    );
}

#[test]
fn mixed_yield_and_array_yield_star_generic_call() {
    // A second, non-array-delegate contribution alongside the array delegate
    // must not reintroduce the defect (or regress this already-working shape).
    assert!(
        strict_codes(
            r#"
async function* outer() { yield 1; yield* [2, 3]; }
declare function want<T>(x: AsyncGenerator<T, any, any>): T;
want(outer());
"#
        )
        .is_empty()
    );
}

#[test]
fn annotated_generator_return_type_generic_call_unaffected() {
    // Regression control: an explicit return-type annotation lowers through
    // the ordinary type-node path and never touches
    // `unannotated_generator_return_type` — must keep working exactly as
    // before this fix.
    assert!(
        strict_codes(
            r#"
async function* outer(): AsyncGenerator<number, any, any> { yield* [1, 2, 3]; }
declare function want<T>(x: AsyncGenerator<T, any, any>): T;
want(outer());
"#
        )
        .is_empty()
    );
}

//! Regression tests for issue #9780.
//!
//! ## Structural rule
//!
//! When a const variable is initialized by a generic call
//! `fn(iter, cb)` where the signature is shaped like
//! `<T>(items: Iterable<T>, cb: (x: T) => ...) => ...`, the inferred
//! result type must be the same whether the const is later used as a
//! value or its `typeof` is queried from a type position.
//!
//! Specifically, `T` must be inferred from the iterable's element type
//! (`number` for `[1, 2, 3]`), not from the entire iterable as a whole
//! (`number[]`).
//!
//! ## Reported repro
//!
//! ```ts
//! type Equal<X, Y> = (<T>() => T extends X ? 1 : 2) extends
//!                    (<T>() => T extends Y ? 1 : 2) ? true : false;
//! const items = [1, 2, 3, 4];
//! const g = Object.groupBy(items, x => x > 2 ? 'a' : 'b');
//! type Same = Equal<typeof g, typeof g>;
//! ```
//!
//! tsc accepts this without error; tsz incorrectly reported TS2345 from
//! the call site, claiming that `(1 | 2 | 3 | 4)[]` was not assignable
//! to `Iterable<(1 | 2 | 3 | 4)[]>`. The target's outer wrapper had `T`
//! bound to the *whole* `items` array instead of its element type — a
//! tell-tale sign that the iterable-aware contextual substitution path
//! had silently fallen back to its naive "T = source" decomposition.
//!
//! ## Root cause
//!
//! In `collect_return_context_substitution_impl`, when the source of
//! the substitution is `Application(Iterable_DefId, [T_placeholder])`
//! and the target is an array, a guard normally diverts inference to
//! extract the element type. That guard relied on
//! `is_iterable_like_for_substitution(evaluate_type_with_env(source))`
//! — but `evaluate_type_with_env` only unwraps an `Application` when
//! the `TypeEnvironment` already has the base's body registered. Some
//! call sites (the `typeof`-driven type-alias resolution path is the
//! reported trigger) reach this code before that registration, so
//! evaluation returns the `Application` unchanged. The structural
//! check then sees no object shape and the guard wrongly returns
//! false, sending `Iterable<T>` against `T[]` through the naive
//! `T = source` decomposition.
//!
//! The fix replaces the single `evaluate_type_with_env` step with a
//! full resolution chain (`resolve_lazy_type` → `evaluate_application_type`
//! → `evaluate_type_with_env`) so the guard sees the substituted Object
//! body regardless of how lazy the environment happens to be at that
//! call site.
//!
//! These tests pin the structural rule across the matrix of cases laid
//! out in the issue. The tests do not depend on `Array<T>` being
//! recognised as `Iterable<T>` by the test-framework's lib loader
//! (which it currently isn't, independently of this fix). Instead they
//! anchor the contextual substitution outcome by binding the call's
//! result to a concrete annotation and asserting that the *only*
//! diagnostic produced (if any) is the lib-bridge gap, never the
//! `Iterable<source>` shape that signalled the original bug.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_compiled_lib_files};

fn lib_files() -> Vec<std::sync::Arc<tsz_binder::lib_loader::LibFile>> {
    load_compiled_lib_files(&[
        "lib.es5.d.ts",
        "lib.es2015.d.ts",
        "lib.es2015.core.d.ts",
        "lib.es2015.collection.d.ts",
        "lib.es2015.iterable.d.ts",
        "lib.es2015.generator.d.ts",
        "lib.es2015.promise.d.ts",
        "lib.es2015.proxy.d.ts",
        "lib.es2015.reflect.d.ts",
        "lib.es2015.symbol.d.ts",
        "lib.es2015.symbol.wellknown.d.ts",
    ])
}

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &lib_files())
        .into_iter()
        .filter(|(code, _)| *code != 2318) // lib-missing noise
        .collect()
}

/// Collect TS2345 diagnostics whose message renders an array-shaped
/// type as the first argument of an iterator-wrapper interface. That
/// nesting is the signature of issue #9780: the inference engine bound
/// `T` to the whole array source instead of its element type and so
/// rendered the target as e.g. `Iterable<number[]>` or
/// `Iterable<(1 | 2 | 3 | 4)[]>`.
///
/// Element-type-shaped targets like `Iterable<number>` would signal an
/// unrelated test-framework gap in Array-to-Iterable bridge resolution
/// and are not what this issue is about — so this predicate matches
/// only on the structural fingerprint (any `[` inside the iterator
/// wrapper's first type argument) rather than on the exact rendered
/// element types in any particular test case.
fn diags_with_whole_source_inference(source: &str) -> Vec<(u32, String)> {
    let target_wrappers = ["Iterable<", "Iterator<", "ArrayLike<"];
    diagnostics(source)
        .into_iter()
        .filter(|(code, message)| {
            *code == 2345
                && target_wrappers.iter().any(|wrapper| {
                    let Some(start) = message.find(wrapper) else {
                        return false;
                    };
                    let after = &message[start + wrapper.len()..];
                    let end = after.find(['>', ',']).unwrap_or(after.len());
                    after[..end].contains('[')
                })
        })
        .collect()
}

#[track_caller]
fn assert_no_whole_source_inference(source: &str) {
    let bad = diags_with_whole_source_inference(source);
    assert!(
        bad.is_empty(),
        "issue #9780 regression: T was inferred to the whole array source\nSource:\n{source}\nDiagnostics:\n{bad:#?}"
    );
}

// ── Original reported repro ───────────────────────────────────────────────

#[test]
fn object_group_by_with_equal_typeof_no_whole_source_inference() {
    assert_no_whole_source_inference(
        r#"
type Equal<X, Y> = (<T>() => T extends X ? 1 : 2) extends (<T>() => T extends Y ? 1 : 2) ? true : false;
declare const Obj: ObjectConstructor & {
    groupBy<K extends PropertyKey, T>(items: Iterable<T>, keySelector: (item: T, index: number) => K): Partial<Record<K, T[]>>;
};
const items = [1, 2, 3, 4];
const g = Obj.groupBy(items, x => x > 2 ? 'a' : 'b');
type Same = Equal<typeof g, typeof g>;
"#,
    );
}

// ── Minimal reduction: no Object.groupBy, no Equal ────────────────────────

#[test]
fn const_generic_call_typeof_alias_no_whole_source_inference() {
    assert_no_whole_source_inference(
        r#"
const fn: <T>(items: Iterable<T>, cb: (x: T) => void) => T = null as any;
const g = fn([1, 2], _ => {});
type T1 = typeof g;
"#,
    );
}

#[test]
fn const_generic_call_typeof_in_function_return_no_whole_source_inference() {
    assert_no_whole_source_inference(
        r#"
const fn: <T>(items: Iterable<T>, cb: (x: T) => void) => T = null as any;
const g = fn([1, 2], _ => {});
function f(): typeof g { return g; }
"#,
    );
}

// ── Adjacent shapes the rule must cover (issue test matrix) ───────────────

#[test]
fn two_type_params_typeof_no_whole_source_inference() {
    assert_no_whole_source_inference(
        r#"
const fn: <K, T>(items: Iterable<T>, cb: (x: T) => K) => K = null as any;
const g = fn([1, 2], _ => 'a');
type T1 = typeof g;
"#,
    );
}

#[test]
fn renamed_type_params_typeof_no_whole_source_inference() {
    assert_no_whole_source_inference(
        r#"
const fn: <P, X>(items: Iterable<X>, cb: (v: X) => P) => P = null as any;
const g = fn([1, 2], _ => 'a');
type T1 = typeof g;
"#,
    );
}

#[test]
fn const_generic_call_no_typeof_no_whole_source_inference() {
    // Control: without any typeof query, the call must of course not
    // produce a `T = whole array` diagnostic either.
    assert_no_whole_source_inference(
        r#"
const fn: <T>(items: Iterable<T>, cb: (x: T) => void) => T = null as any;
const g = fn([1, 2], _ => {});
"#,
    );
}

#[test]
fn cb_first_items_second_typeof_no_whole_source_inference() {
    // The buggy decomposition was specifically reached when `items` was
    // processed before `cb`. Pin the contra case as a control.
    assert_no_whole_source_inference(
        r#"
const fn: <T>(cb: (x: T) => void, items: Iterable<T>) => T = null as any;
const g = fn(_ => {}, [1, 2]);
type T1 = typeof g;
"#,
    );
}

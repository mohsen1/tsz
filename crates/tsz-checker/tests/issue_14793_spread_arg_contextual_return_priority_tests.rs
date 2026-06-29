//! Regression for issue #14793: a same-arity contextual return tuple must not
//! override a fully-inferred spread-argument binding for a rest type parameter.
//!
//! Root cause: after the solver resolved `f<T extends readonly unknown[]>(...args:
//! T): T` to a concrete `T = [number, string]` from the spread argument `f(...t)`,
//! the checker's generic-call finalize (`call/inner.rs`) re-applied the contextual
//! return type as `T`. Its "do the args fit the contextually-instantiated params?"
//! guard accepts position-swapped tuples, because the rest element type of
//! `[string, number]` is `string | number`, which both `number` and `string`
//! satisfy. The replaced result equalled the annotation, so the outer
//! variable-initializer assignability check trivially passed and the real `TS2322`
//! was lost.
//!
//! Fix: the contextual return type only fills the result when the solver left the
//! return type unresolved (still mentions a type parameter, an `infer`
//! placeholder, or `unknown`) — mirroring the sibling finalize path in
//! `call/mod.rs`. When inference already produced a concrete return type from the
//! arguments, that binding is authoritative (tsc's inference priority: a direct
//! argument candidate outranks a return-context candidate).
//!
//! Each test varies the callee/type-parameter/binding names so the fix is
//! exercised structurally, never through an identifier or file-name predicate.

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

#[track_caller]
fn expect_ts2322(source: &str) {
    let c = codes(source);
    assert_eq!(c, vec![2322], "expected exactly one TS2322; got {c:?}");
}

#[track_caller]
fn expect_clean(source: &str) {
    let c = codes(source);
    assert!(c.is_empty(), "expected no diagnostics; got {c:?}");
}

/// The issue witness: `t: [number, string]` spread into `f(...t)` whose result
/// is contextually typed by a position-swapped `[string, number]`. tsc reports
/// `TS2322`; the contextual tuple must not silently win.
#[test]
fn issue_14793_swapped_contextual_tuple_reports_ts2322() {
    expect_ts2322(
        r#"
declare function apply<Elems extends readonly unknown[]>(...items: Elems): Elems;
const pair: [number, string] = [1, "a"];
const swapped: [string, number] = apply(...pair);
"#,
    );
}

/// Control: the correctly-ordered annotation matches the spread inference and
/// must stay clean (the fix must not introduce a spurious error here).
#[test]
fn issue_14793_matching_order_is_clean() {
    expect_clean(
        r#"
declare function gather<Row extends readonly unknown[]>(...cells: Row): Row;
const tuple: [number, string] = [2, "b"];
const same: [number, string] = gather(...tuple);
"#,
    );
}

/// Disjoint element types were already caught; assert they stay caught (the
/// guard change must not regress the arity/element checks).
#[test]
fn issue_14793_disjoint_elements_still_report_ts2322() {
    expect_ts2322(
        r#"
declare function collect<Cols extends readonly unknown[]>(...vals: Cols): Cols;
const data: [number, string] = [3, "c"];
const wrong: [boolean, boolean] = collect(...data);
"#,
    );
}

/// A leading positional argument plus the spread changes the inferred arity, so
/// the swapped annotation must report `TS2322` (source has more elements than the
/// 2-element target).
#[test]
fn issue_14793_positional_plus_spread_reports_ts2322() {
    expect_ts2322(
        r#"
declare function build<Parts extends readonly unknown[]>(...segs: Parts): Parts;
const rest: [number, string] = [4, "d"];
const mismatch: [string, number] = build(1, ...rest);
"#,
    );
}

/// A contextual annotation that violates the rest type parameter's constraint
/// (`number` is not `readonly unknown[]`) must not be bound as `T`; the concrete
/// spread inference stays and reports `TS2322`.
#[test]
fn issue_14793_constraint_violating_contextual_reports_ts2322() {
    expect_ts2322(
        r#"
declare function spread<Items extends readonly unknown[]>(...xs: Items): Items;
const seq: [number, string] = [5, "e"];
const scalar: number = spread(...seq);
"#,
    );
}

/// Breaking the contextual link with an intermediate `const` already reported the
/// error; assert it remains reported (cross-check that the direct-initializer path
/// now matches the indirect one).
#[test]
fn issue_14793_intermediate_variable_reports_ts2322() {
    expect_ts2322(
        r#"
declare function relay<Tup extends readonly unknown[]>(...members: Tup): Tup;
const sourceTuple: [number, string] = [6, "f"];
const midResult = relay(...sourceTuple);
const swappedTarget: [string, number] = midResult;
"#,
    );
}

/// The legitimate contextual-fill case must keep working: when the return type is
/// a bare type parameter reached only through a callback's return position, the
/// contextual union narrows it (no direct argument pins it). tsc accepts this.
#[test]
fn issue_14793_callback_return_contextual_narrowing_stays_clean() {
    expect_clean(
        r#"
declare function run<R>(make: () => R): R;
const narrowed: 0 | 1 | 2 = run(() => 1);
"#,
    );
}

/// The other side of the legitimate case: when the callback body genuinely
/// conflicts with the contextual type, the error must still surface.
#[test]
fn issue_14793_callback_return_conflict_reports_ts2322() {
    expect_ts2322(
        r#"
declare function evaluate<Out>(producer: () => Out): Out;
const text: string = evaluate(() => 1);
"#,
    );
}

/// An unresolved return type (no argument candidate for the type parameter) must
/// still be filled from the contextual type — the guard only blocks override of a
/// *resolved* return type, never legitimate filling.
#[test]
fn issue_14793_unresolved_return_still_filled_from_contextual() {
    expect_clean(
        r#"
interface Holder<V> { value: V; }
declare function empty<V>(): Holder<V>;
const held: Holder<string> = empty();
"#,
    );
}

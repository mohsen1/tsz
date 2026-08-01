//! #16116 item 2 is **not a compiler defect**. It is an artifact of the unit
//! harness, and this suite pins the harness gap so it cannot swallow the same
//! class of diagnostic silently again.
//!
//! The issue reports that an unannotated generator yielding an inner generator
//! loses the inner's type argument: `tsc` reports TS2345 and tsz reports
//! nothing. That is true of `check_source_with_libs`. It is **false** of the
//! compiler. Run the issue's own repro through the CLI and tsz reports the
//! same TS2345 `tsc` does, having inferred the container correctly:
//!
//! ```text
//! $ tsz --noEmit --strict --pretty false item2.ts
//! item2.ts(9,7): error TS2345: Argument of type '{ next(..._: [] | [unknown]):
//!   Promise<IteratorResult<{ ... [Symbol.asyncIterator](): AsyncGenerator<string,
//!   void, unknown>; }, void>>; ... }' is not assignable to parameter of type
//!   'AsyncGenerator<AsyncGenerator<number, any, any>, any, any>'.
//! ```
//!
//! The `AsyncGenerator<string, void, unknown>` in that rendering is the inner's
//! type argument, intact. Nothing was degraded to `any`; no yield contribution
//! was lost.
//!
//! What *does* diverge is which shapes the unit harness can see the mismatch
//! on. Same fixture, `strict_codes` vs the release CLI:
//!
//! | yielded operand | container | CLI | harness |
//! | --- | --- | --- | --- |
//! | `AsyncGenerator<string>` | `async function*` | TS2345 | **silent** |
//! | `Generator<string>` | `function*` | TS2345 | **silent** |
//! | `Generator<string>` | `async function*` | TS2345 | TS2345 |
//! | `AsyncIterable`/`AsyncIterator`/`Iterator`/`Set`/`Iterable` | `async function*` | TS2345 | TS2345 |
//! | `Promise<string>`, `string[]`, `{ a: string }` | `async function*` | TS2345 | TS2345 |
//!
//! The harness loses the diagnostic on exactly the **self-similar** rows — the
//! ones where the yielded operand's alias is the same alias as the container's.
//! One alias away (`Generator` inside an *async* container) it agrees with the
//! CLI. So the gap is narrow and structural, not a blanket "the harness is
//! weaker".
//!
//! The two `#[ignore]`d rows below assert the CLI's (and `tsc`'s) answer. They
//! are red **because of the harness divergence**, not because of anything in
//! the container's inferred yield type — fixing a checker or solver path will
//! not turn them green. They go live when `check_source_with_libs` resolves
//! self-similar generator applications the way a real program does.
//!
//! Everything else here is a live control, green today, pinning the shapes the
//! harness *does* see so the boundary of the gap stays measured. Oracle for
//! every row: `tsc@7.0.2`
//! (`--noEmit --strict --pretty false --target es2018 --lib es2018,dom`),
//! cross-checked against `tsz --noEmit --strict --pretty false`.

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

/// The minimal form of the harness gap: `tsc` and the tsz CLI both report
/// TS2345; `check_source_with_libs` returns an empty diagnostic vector.
///
/// The `declare const` operand is deliberate — it strips the issue's inner
/// generator expression out of the picture entirely, so a reader cannot
/// mistake this for an inference problem.
#[test]
#[ignore = "unit-harness gap, NOT a checker defect: the CLI reports TS2345 here; \
            `check_source_with_libs` loses it on self-similar generator nesting"]
fn harness_sees_async_generator_operand_in_async_container() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncGenerator<string>;
declare function wants(h: AsyncGenerator<AsyncGenerator<number>>): void;
const d = async function* () {
    yield zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "the harness must see the mismatch the CLI reports: {codes:?}"
    );
}

/// The sync twin. Pins that the gap is a property of the self-similar nesting
/// rather than anything async-specific: no `await`, no `[Symbol.asyncIterator]`,
/// same divergence.
#[test]
#[ignore = "unit-harness gap, NOT a checker defect: the CLI reports TS2345 here; \
            `check_source_with_libs` loses it on self-similar generator nesting"]
fn harness_sees_sync_generator_operand_in_sync_container() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: Generator<string>;
declare function wants(h: Generator<Generator<number>>): void;
const d = function* () {
    yield zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "the harness must see the mismatch the CLI reports: {codes:?}"
    );
}

/// The load-bearing control, and the row that localises the gap: a **sync**
/// `Generator` operand inside an **async** container is one alias away from the
/// ignored rows and the harness sees it. A diagnosis blaming "yielding a
/// generator" or "yielding an iterable" would predict this fails too.
#[test]
fn sync_generator_operand_in_async_container_is_visible_to_the_harness() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: Generator<string>;
declare function wants(h: AsyncGenerator<Generator<number>>): void;
const d = async function* () {
    yield zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a `Generator` operand in an async container must stay visible: {codes:?}"
    );
}

/// `AsyncIterable` is `AsyncGenerator`'s shape minus the generator members, and
/// the harness sees it — so the gap is not "any async iterable operand".
#[test]
fn async_iterable_operand_is_visible_to_the_harness() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncIterable<string>;
declare function wants(h: AsyncGenerator<AsyncIterable<number>>): void;
const d = async function* () {
    yield zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "an `AsyncIterable` operand must stay visible: {codes:?}"
    );
}

/// A non-iterable generic operand: rules out "any nested generic argument is
/// invisible to the harness".
#[test]
fn promise_operand_is_visible_to_the_harness() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: Promise<string>;
declare function wants(h: AsyncGenerator<Promise<number>>): void;
const d = async function* () {
    yield zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a `Promise` operand must stay visible: {codes:?}"
    );
}

/// The container's own yield type is not lost in the harness either — only the
/// self-similar comparison is. Feeding the same container to a target whose
/// yield type is not a generator still reports, which is why the ignored rows
/// cannot be read as "the harness inferred `any` for the container".
#[test]
fn self_similar_container_still_rejects_a_non_generator_yield_type() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncGenerator<string>;
declare function wants(h: AsyncGenerator<{ x: number }>): void;
const d = async function* () {
    yield zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "the container's own yield type must still be a generator: {codes:?}"
    );
}

/// The relation itself is fine even inside the harness: the type the ignored
/// rows should have compared, written out by hand, is rejected against the same
/// target. So the gap is in how the harness builds or resolves the container,
/// not in the subtype check.
#[test]
fn written_out_self_similar_relation_reports_in_the_harness() {
    let codes = strict_codes(
        r#"
export {};
declare const a: AsyncGenerator<AsyncGenerator<string>, void, unknown>;
declare function wants(h: AsyncGenerator<AsyncGenerator<number>>): void;
wants(a);
"#,
    );
    assert!(
        codes.contains(&2345),
        "the written-out self-similar relation must report: {codes:?}"
    );
}

/// The sync half of the exoneration above.
#[test]
fn written_out_sync_self_similar_relation_reports_in_the_harness() {
    let codes = strict_codes(
        r#"
export {};
declare const a: Generator<Generator<string>, void, unknown>;
declare function wants(h: Generator<Generator<number>>): void;
wants(a);
"#,
    );
    assert!(
        codes.contains(&2345),
        "the written-out sync self-similar relation must report: {codes:?}"
    );
}

/// The depth-1 baseline the ignored rows are the depth-2 form of: without the
/// nesting the harness agrees with the CLI.
#[test]
fn non_nested_container_is_visible_to_the_harness() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncGenerator<string>;
declare function wants(h: AsyncGenerator<number>): void;
const d = async function* () {
    yield* zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "the non-nested delegate row must stay visible: {codes:?}"
    );
}

/// Annotating the operand's generator expression makes the row visible to the
/// harness. This is the row that made #16116's inference framing look
/// plausible: annotating "fixes" it, which reads like an inference problem.
/// It is not — the `declare const` rows above have no inference on the yielded
/// side at all and still diverge.
#[test]
fn annotated_inner_generator_expression_is_visible_to_the_harness() {
    let codes = strict_codes(
        r#"
export {};
declare function wants(h: AsyncGenerator<AsyncGenerator<number>>): void;
const d = async function* () {
    yield (async function* (): AsyncGenerator<string> {
        yield "a";
    })();
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "an annotated inner generator expression must stay visible: {codes:?}"
    );
}

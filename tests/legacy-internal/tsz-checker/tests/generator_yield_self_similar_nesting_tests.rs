//! #16116 item 2, tracked as #16125: an unannotated generator's inferred
//! container is silently accepted against a target that nests the **same**
//! generator interface inside itself. This suite pins the boundary so the class
//! cannot be swallowed silently again.
//!
//! This file originally headlined the divergence as "not a compiler defect, an
//! artifact of the unit harness". That framing is retracted throughout — see
//! the measured boundary further down. It is a real divergence that the harness
//! is merely the cheapest place to observe.
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
//! The *plain* `check_source_with_libs` harness lost the diagnostic on exactly
//! the **self-similar** rows — the ones where the yielded operand's alias is
//! the same alias as the container's. One alias away (`Generator` inside an
//! *async* container) it agreed with the CLI. So the gap was narrow and
//! structural, not a blanket "the harness is weaker".
//!
//! ## Mechanism (located, #16125) and the fix
//!
//! The mechanism is **not** in the checker or solver relation: the real CLI
//! reports `TS2345` on the exact repro. The divergence was that the plain
//! unit harness built its `CheckerState` with a bare `TypeInterner` and **no**
//! shared `DefinitionStore`, whereas the production driver attaches the
//! program's shared `DefinitionStore` to the `QueryCache`
//! (`crates/tsz-core/src/parallel/core/checking.rs`) so the solver's
//! `DefId`-keyed cross-arena declaration identity (issue #14344,
//! `TSZ_XARENA_BASE_DECL`) is available. Without the store, the lib generic
//! `AsyncGenerator`/`Generator`'s base cannot be unified across the user arena
//! and the lib arena, its variance goes unmeasured
//! (`try_variance_fast_path → None`), and the same-base relation falls back to
//! a lossy structural walk that wrongly accepts the mismatched nested yield.
//!
//! The fix is to route this suite through
//! [`crate::test_utils::check_source_with_libs_shared_def_store`], the
//! production-faithful harness that attaches a shared `DefinitionStore` — no
//! checker or solver change. Every row below (previously `#[ignore]`d or a
//! live control) now matches the CLI and `tsc`.
//!
//! What the *plain* `check_source_with_libs` harness measured (the gap this
//! suite now avoids by using the shared-`DefinitionStore` helper):
//!
//! | `d`'s declaration | harness |
//! | --- | --- |
//! | `const d = async function* () { ... }` (anonymous expression) | **silent** |
//! | `const d = async function* named() { ... }` (named expression) | TS2345 |
//! | `async function* d() { ... }` (declaration) | TS2345 |
//!
//! and the loss is not specific to the call-argument position — a plain
//! `const forced: AsyncGenerator<AsyncGenerator<number>> = d();` loses it too.
//!
//! The named rows do **not** report because they are correct. Rendering the
//! container out of the diagnostic text shows the named forms degrading to the
//! bare, unsubstituted interface, which fails against every target including
//! the ones it should pass:
//!
//! ```text
//!                target                     tsz renders the container as
//! anonymous   AsyncGenerator<{x:number}>    'AsyncGenerator<AsyncGenerator<string, any, any>, void, unknown>'
//! named       AsyncGenerator<{x:number}>    'AsyncGenerator'
//! named       AsyncGenerator<AsyncGenerator<number>>   'AsyncGenerator'
//! anonymous   AsyncGenerator<AsyncGenerator<number>>   (no diagnostic)
//! ```
//!
//! `tsc` renders the full application for both forms. The named form's bare
//! rendering is fixed in #16191 (see
//! `named_form_container_keeps_its_type_arguments_in_the_message` below): the
//! container was reduced to its structural shape at the call return without a
//! display back-reference to its `AsyncGenerator<...>` application. The
//! anonymous form always held the correct container type; the relation-side
//! self-similar loss on both forms was the #16125 defect, now fixed by routing
//! this suite through the shared-`DefinitionStore` harness (see above).
//!
//! Three *solver/checker-side* directions were measured dead before the root
//! cause (the missing harness `DefinitionStore`) was found — kept so they are
//! not re-attempted (each was built, measured on this suite, and reverted; all
//! three left the counts byte-identical):
//!
//! 1. Minting the `Application` base in `unannotated_generator_return_type`
//!    through the name-verified `get_or_create_def_id_for_symbol_name` instead
//!    of the raw `get_or_create_def_id`. That code is reached identically for
//!    anonymous and named forms, so it cannot explain an anonymity boundary.
//! 2. Gating `subtype/cache.rs`'s symbol-level cycle check on `def_pair`, so it
//!    honours the same-base-application exemption the `DefId`-level check
//!    already has.
//! 3. Widening `both_same_base_app` to cover two `DefId`s resolving to one
//!    `SymbolId` when the applications' type arguments differ.
//!
//! Everything else here is a live control, green today, pinning the shapes the
//! harness *does* see so the boundary of the gap stays measured. Oracle for
//! every row: `tsc@7.0.2`
//! (`--noEmit --strict --pretty false --target es2018 --lib es2018,dom`),
//! cross-checked against `tsz --noEmit --strict --pretty false`.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs_shared_def_store, load_default_lib_files};

fn strict_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    // Route through the production-faithful, shared-`DefinitionStore` harness:
    // the self-similar rows below need the solver's cross-arena `DefId`
    // identity for the lib generic (`AsyncGenerator`/`Generator`), which the
    // plain `check_source_with_libs` path lacks. See the module doc and #16125.
    check_source_with_libs_shared_def_store(
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

/// The named-expression form of the two ignored rows. Green today, and the
/// reason the boundary is a property of the container's declaration form rather
/// than of the target: only the binder-visible name changes between this row
/// and `harness_sees_async_generator_operand_in_async_container`.
#[test]
fn named_generator_function_expression_reports_the_self_similar_mismatch() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncGenerator<string>;
declare function wants(h: AsyncGenerator<AsyncGenerator<number>>): void;
const d = async function* named() {
    yield zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "the named-expression form must keep reporting: {codes:?}"
    );
}

/// The function-declaration form, third row of the boundary table. Green today.
#[test]
fn generator_function_declaration_reports_the_self_similar_mismatch() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncGenerator<string>;
declare function wants(h: AsyncGenerator<AsyncGenerator<number>>): void;
async function* d() {
    yield zzSource;
}
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "the function-declaration form must keep reporting: {codes:?}"
    );
}

/// The matching case stays clean. Without this row a "fix" that reported on
/// every self-similar comparison would look green on the two ignored rows while
/// trading the false negative for a false positive. `tsc` is clean here.
#[test]
fn self_similar_container_matching_the_target_stays_clean() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncGenerator<string>;
declare function wants(h: AsyncGenerator<AsyncGenerator<string>>): void;
const d = async function* () {
    yield zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.is_empty(),
        "the matching self-similar assignment must stay clean: {codes:?}"
    );
}

/// The same loss with every user binder renamed. Red for the same reason as the
/// two rows above; pinned so a future fix cannot be keyed on anything in the
/// user's naming without this row staying red.
#[test]
fn renamed_binders_async_generator_operand_in_async_container() {
    let codes = strict_codes(
        r#"
export {};
declare const qqFeed: AsyncGenerator<string>;
declare function accepts(pipe: AsyncGenerator<AsyncGenerator<number>>): void;
const emit = async function* () {
    yield qqFeed;
};
accepts(emit());
"#,
    );
    assert!(
        codes.contains(&2345),
        "renamed binders must not change the answer: {codes:?}"
    );
}

/// The loss is not specific to the call-argument path: a plain annotated
/// variable declaration loses it too, as TS2322. Pins the finding that
/// argument-position resolution is not the owner.
#[test]
fn variable_declaration_target_loses_the_self_similar_mismatch() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncGenerator<string>;
const d = async function* () {
    yield zzSource;
};
const forceMismatch: AsyncGenerator<AsyncGenerator<number>> = d();
"#,
    );
    assert!(
        codes.contains(&2322),
        "the variable-declaration form must report TS2322: {codes:?}"
    );
}

/// Depth 3. Pins that a fix has to hold when the interface nests inside itself
/// more than once, not just at the first level.
#[test]
fn depth_three_self_similar_nesting_reports() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncGenerator<AsyncGenerator<string>>;
declare function wants(h: AsyncGenerator<AsyncGenerator<AsyncGenerator<number>>>): void;
const d = async function* () {
    yield zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "depth-3 self-similar nesting must report: {codes:?}"
    );
}

/// The second, separate defect this suite's investigation surfaced: the **named**
/// forms report only because their container degraded to the bare, unsubstituted
/// interface. `tsc` renders
/// `AsyncGenerator<AsyncGenerator<string, any, any>, void, unknown>` here; tsz
/// renders a bare `AsyncGenerator`, dropping every type argument.
///
/// Fixed in #16191: the named form's container reached the diagnostic through
/// `finalize_call_return_like_success`, which eagerly reduces a monomorphic
/// `Application` return type to its structural shape at the call site. That
/// reduction (`instantiate_application_body_for_property_access`) dropped the
/// display back-reference to the originating `AsyncGenerator<...>` application,
/// so the printer showed a bare `AsyncGenerator`. Recording the back-reference
/// on that path — the checker-side counterpart to the solver's
/// `store_parametric_structural_back_reference` — restores the type arguments
/// without touching the structural type, so this is display-only and the
/// self-similar relation rows above (the separate #16125 defect) are unmoved.
#[test]
fn named_form_container_keeps_its_type_arguments_in_the_message() {
    let libs = load_default_lib_files();
    let messages = crate::test_utils::check_source_with_libs_code_messages(
        r#"
export {};
declare const zzSource: AsyncGenerator<string>;
declare function wants(h: AsyncGenerator<{ x: number }>): void;
async function* d() {
    yield zzSource;
}
wants(d());
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );
    let text = messages
        .iter()
        .find(|(code, _)| *code == 2345)
        .map(|(_, message)| message.clone())
        .unwrap_or_default();
    assert!(
        text.contains("AsyncGenerator<AsyncGenerator<string"),
        "the named form must keep its type arguments the way tsc renders them: {text:?}"
    );
}

/// Adjacent case for #16191: the **sync** `Generator` named form keeps its
/// type arguments too. The fix is on the container's display provenance, not on
/// anything async-specific, so `Generator<...>` must recover its arguments the
/// same way `AsyncGenerator<...>` does. Renders `Generator<string, ...>` on a
/// non-self-similar target so the relation genuinely rejects and the message is
/// the artifact under test.
#[test]
fn sync_named_form_container_keeps_its_type_arguments_in_the_message() {
    let libs = load_default_lib_files();
    let messages = crate::test_utils::check_source_with_libs_code_messages(
        r#"
export {};
declare function wants(h: Generator<{ x: number }>): void;
function* d() {
    yield "s";
}
wants(d());
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );
    let text = messages
        .iter()
        .find(|(code, _)| *code == 2345)
        .map(|(_, message)| message.clone())
        .unwrap_or_default();
    assert!(
        text.contains("Generator<string"),
        "the sync named form must keep its type arguments: {text:?}"
    );
}

/// Adjacent case for #16191: the binder names must not matter. A renamed
/// callee/target still recovers the container's type arguments, since the fix
/// keys on the nominal `Application` provenance and never on any identifier.
#[test]
fn renamed_binder_named_form_container_keeps_its_type_arguments_in_the_message() {
    let libs = load_default_lib_files();
    let messages = crate::test_utils::check_source_with_libs_code_messages(
        r#"
export {};
declare const zzOther: AsyncGenerator<string>;
declare function accept(container: AsyncGenerator<{ y: number }>): void;
async function* produce() {
    yield zzOther;
}
accept(produce());
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );
    let text = messages
        .iter()
        .find(|(code, _)| *code == 2345)
        .map(|(_, message)| message.clone())
        .unwrap_or_default();
    assert!(
        text.contains("AsyncGenerator<AsyncGenerator<string"),
        "renamed binders must not change the recovered display: {text:?}"
    );
}

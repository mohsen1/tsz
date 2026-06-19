//! Regression guard for the lazy lib-interface **heritage** materialization
//! rework (#13933 producer flip, #13935 consumer side, #13936 relation-input
//! side).
//!
//! Those changes defer materializing a lib interface's referenced/heritage
//! interfaces until a member or relation query forces them, so a Worker-RPC
//! program (comlink) stops eagerly interning the whole DOM/webworker graph.
//! The deferral is a documented conformance minefield — `merge_lib_interface_heritage`'s
//! own comments cite the prior reverts. These inputs pin the failure modes so
//! the rework can iterate **locally** (`cargo nextest run -p tsz-checker
//! lazy_lib_heritage_guard`) instead of only discovering regressions in a CI
//! round-trip:
//!
//! - **TS2339 / TS2740** — an inherited member becomes unresolvable because a
//!   heritage base was dropped instead of kept lazy (#12299, e.g.
//!   `HTMLElement.appendChild` from `Node`).
//! - **TS2488 / TS2345 / TS2322** — a base interface's type parameter leaks
//!   un-substituted through concrete iteration (#13652, `Map`/`Set` →
//!   `IteratorResult<T>` with a bare `T`).
//!
//! Each case must stay clean of those codes on every input below; the assertion
//! is shape-driven (inherited member / substituted iteration / messaging
//! interface), not tied to a particular identifier spelling.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

/// Diagnostic codes a lazy-heritage regression would surface on these inputs.
const HERITAGE_HAZARD_CODES: &[u32] = &[2339, 2740, 2488, 2345, 2322];

fn codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "guard.ts",
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

fn assert_no_heritage_hazard(label: &str, source: &str) {
    let got = codes(source);
    let hazards: Vec<u32> = got
        .iter()
        .copied()
        .filter(|code| HERITAGE_HAZARD_CODES.contains(code))
        .collect();
    assert!(
        hazards.is_empty(),
        "{label}: lazy-heritage hazard diagnostics surfaced {hazards:?} (all codes: {got:?}); \
         the lib-interface heritage-materialization rework (#13933/#13935/#13936) must keep this input clean",
    );
}

/// #12299 guard: a member inherited one heritage level (`Node.appendChild`)
/// must remain resolvable on a derived element. Dropping an in-progress base
/// instead of keeping it lazy is exactly what regressed in #12299.
#[test]
fn inherited_dom_member_resolves_one_level() {
    assert_no_heritage_hazard(
        "HTMLElement.appendChild (inherited from Node)",
        "declare const e: HTMLElement; const c = e.appendChild(e); void c;",
    );
}

/// #12299 guard, deeper chain: `EventTarget.addEventListener` is inherited
/// through the full DOM heritage graph (the `declarationFileForHtml*` family).
#[test]
fn inherited_dom_member_resolves_deep_chain() {
    assert_no_heritage_hazard(
        "HTMLElement.addEventListener (inherited from EventTarget)",
        "declare const e: HTMLElement; e.addEventListener(\"click\", () => {});",
    );
}

/// #13652 guard: destructuring `for-of` over `Map<K,V>` must yield substituted
/// `[K, V]`, not a base interface's bare type parameter.
#[test]
fn map_iteration_substitutes_element_type() {
    assert_no_heritage_hazard(
        "Map<string,number> destructuring iteration",
        "declare const m: Map<string, number>; for (const [k, v] of m) { const s: string = k; const n: number = v; void s; void n; }",
    );
}

/// #13652 guard, single-arg: `Set<number>` iteration yields `number`.
#[test]
fn set_iteration_substitutes_element_type() {
    assert_no_heritage_hazard(
        "Set<number> iteration",
        "declare const s: Set<number>; for (const v of s) { const n: number = v; void n; }",
    );
}

/// comlink fast-goal shape: the messaging interfaces whose eager heritage
/// materialization is the target. `Worker.addEventListener`/`postMessage` must
/// stay clean when their referenced-interface graph is deferred.
#[test]
fn worker_message_handler_and_post_clean() {
    assert_no_heritage_hazard(
        "Worker addEventListener/postMessage (comlink shape)",
        "declare const w: Worker; w.addEventListener(\"message\", (e) => { const d = e.data; void d; }); w.postMessage(1);",
    );
}

/// `MessagePort.onmessage` event-handler property (the `MessageEvent<T>`
/// reference that drives the over-materialization) must resolve clean.
#[test]
fn messageport_onmessage_event_clean() {
    assert_no_heritage_hazard(
        "MessagePort.onmessage handler + start()",
        "declare const p: MessagePort; p.onmessage = (e) => { const d = e.data; void d; }; p.start();",
    );
}

// ---------------------------------------------------------------------------
// Type-interner count harness (the perf side of #13937).
//
// `merge_lib_interface_heritage` over-materializing a lib-interface receiver's
// transitive `extends` closure is invisible to a diagnostic-only guard: the
// extra thousands of interned types are dropped after a single member lookup,
// so diagnostics stay byte-identical while `tsz --extendedDiagnostics`'s "Types"
// counter balloons. These guards make that count observable and bounded so the
// lazy-heritage rework (#13933 producer / #13935 consumer / #13936
// relation-input) can iterate locally on the actual lever instead of only the
// diagnostic surface.
//
// Counts are the in-process analogue of `--extendedDiagnostics`; their absolute
// value depends on the bundled stripped lib assets (smaller than the `dist`
// binary's full-lib numbers), but is deterministic for a fixed lib set, so the
// margins below are expressed relative to the trivial-file baseline rather than
// hardcoded totals.
// ---------------------------------------------------------------------------

/// Trivial source whose interned-type count is the per-lib-set baseline floor
/// every other count is measured against.
const TRIVIAL_SOURCE: &str = "const c = 1; export {};";

/// Max types a **lazy** lib-interface receiver may intern above the trivial
/// baseline. Today (default bundle) `Document`/`HTMLElement`/`Node`
/// annotations add ≤ ~440 over the floor, while the eager (un-deferred)
/// heritage path adds ≥ ~`4_600`; `2_000` sits cleanly between, so a regression
/// that re-materializes a lazy receiver's `extends` closure trips this bound
/// long before it reaches the eager cost.
const LAZY_RECEIVER_MARGIN: usize = 2_000;

/// Ceiling (above baseline) for the interfaces that **currently** over-
/// materialize — the lazy-heritage rework's target. Today the worst case
/// (`Worker` + member access) adds ~`7_800` over the floor; this only guards
/// against the count getting *worse*. When #13933/#13935/#13936 land and drop
/// these under [`LAZY_RECEIVER_MARGIN`], move the affected cases into the
/// lazy-floor guards above and tighten this ratchet.
const EAGER_RECEIVER_CEILING: usize = 10_000;

/// Interned-type count for `source` under the default lib bundle (strict),
/// asserting no lazy-heritage hazard diagnostic surfaced first (a dropped base
/// would corrupt both the count and the diagnostics).
fn type_count(source: &str) -> usize {
    use crate::test_utils::check_source_with_libs_type_count;
    let libs = load_default_lib_files();
    let (diagnostics, count) = check_source_with_libs_type_count(
        source,
        "guard.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );
    let hazards: Vec<u32> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .filter(|code| HERITAGE_HAZARD_CODES.contains(code))
        .collect();
    assert!(
        hazards.is_empty(),
        "type_count({source:?}) surfaced lazy-heritage hazard diagnostics {hazards:?}",
    );
    count
}

fn assert_lazy_receiver(label: &str, source: &str) {
    let base = type_count(TRIVIAL_SOURCE);
    let count = type_count(source);
    assert!(
        count <= base + LAZY_RECEIVER_MARGIN,
        "{label}: interned {count} types (baseline {base}), exceeding the lazy ceiling \
         {} — a lib-interface receiver that should resolve lazily is materializing its \
         full heritage closure (#12101/#13933 regression)",
        base + LAZY_RECEIVER_MARGIN,
    );
}

/// The trivial-file baseline must not silently balloon (a regression there
/// would mask every relative bound below).
#[test]
fn trivial_baseline_is_bounded() {
    let base = type_count(TRIVIAL_SOURCE);
    assert!(
        base <= 1_500,
        "trivial file interned {base} types; the baseline floor regressed",
    );
}

/// `Document` (the shipped #12101 lazy-receiver win) stays at the lazy floor
/// for both a bare annotation and a property read — member access must not
/// force the receiver's heritage closure.
#[test]
fn document_receiver_stays_lazy() {
    assert_lazy_receiver("Document annotation", "declare const d: Document; void d;");
    assert_lazy_receiver(
        "Document.title read",
        "declare const d: Document; const t = d.title; void t;",
    );
}

/// Property access on a lazy receiver must add no materialization over the bare
/// annotation — the invariant that makes `document.title` cheap.
#[test]
fn property_access_adds_no_materialization() {
    let annotation = type_count("declare const d: Document; void d;");
    let property = type_count("declare const d: Document; const t = d.title; void t;");
    assert!(
        property <= annotation,
        "Document.title interned {property} types vs {annotation} for the bare annotation; \
         member access is forcing receiver materialization (#12101 regression)",
    );
}

/// `HTMLElement` and `Node` resolve lazily today and must stay that way; the
/// heritage rework must not regress the receivers that already win.
#[test]
fn htmlelement_and_node_receivers_stay_lazy() {
    assert_lazy_receiver(
        "HTMLElement annotation",
        "declare const e: HTMLElement; void e;",
    );
    assert_lazy_receiver("Node annotation", "declare const n: Node; void n;");
}

/// Ratchet for the campaign-target interfaces (messaging / deep heritage) that
/// over-materialize today. They sit above the lazy margin now; this only fails
/// if the count grows past [`EAGER_RECEIVER_CEILING`]. The rework drives them
/// under [`LAZY_RECEIVER_MARGIN`], at which point they graduate to the
/// lazy-floor guards above.
#[test]
fn eager_receivers_stay_below_ratchet() {
    let base = type_count(TRIVIAL_SOURCE);
    let ceiling = base + EAGER_RECEIVER_CEILING;
    for (label, source) in [
        ("Worker annotation", "declare const w: Worker; void w;"),
        (
            "Worker.addEventListener",
            "declare const w: Worker; w.addEventListener(\"message\", (e) => { void e.data; });",
        ),
        (
            "MessagePort annotation",
            "declare const p: MessagePort; void p;",
        ),
        (
            "HTMLDivElement annotation",
            "declare const e: HTMLDivElement; void e;",
        ),
    ] {
        let count = type_count(source);
        assert!(
            count <= ceiling,
            "{label}: interned {count} types (baseline {base}), exceeding the over-\
             materialization ratchet {ceiling}; lib-interface heritage materialization \
             regressed further (#13933/#13935/#13936)",
        );
    }
}

/// Floor sanity (#12158): an already-lazy own property read stays clean — the
/// heritage rework must not regress the member-access laziness already shipped.
#[test]
fn document_property_read_stays_clean() {
    assert_no_heritage_hazard(
        "Document.title read",
        "declare const d: Document; const t: string = d.title; void t;",
    );
}

// =============================================================================
// On-demand forcing relation path (#12101 steps 5-7)
// =============================================================================
//
// The eager transitive `ensure_refs_resolved` pre-walk is dropped for
// force-eligible simple lib interfaces; their referenced tail is materialized on
// demand when a relation/evaluation structurally consumes it
// (`CheckerContext::force_def_on_miss`). These tests pin the relation path: a
// DOM type flowing through an *assignment relation* (not just an annotation or a
// member read) must still produce the exact `tsc` diagnostics and must not
// re-materialize its full heritage closure.

/// A DOM value assigned to an incompatible annotation drives the assignability
/// relation over the (now lazily-resolved) DOM interface. The relation must
/// still report TS2322 — the on-demand path resolves the real shape rather than
/// treating the unresolved `Lazy` as compatible (the #12144 conservative
/// fallback must never silence a genuine mismatch). Receiver/method spellings
/// are varied so the rule is structural.
#[test]
fn dom_relation_reports_mismatch_on_demand() {
    for (label, source, expect) in [
        (
            "createElement -> number",
            "declare const d: Document; const x: number = d.createElement(\"div\"); export {};",
            true,
        ),
        (
            "body -> string",
            "declare const d: Document; const s: string = d.body; export {};",
            true,
        ),
        (
            "createElement -> Element (compatible)",
            "declare const d: Document; const e: Element = d.createElement(\"p\"); export {};",
            false,
        ),
    ] {
        let got = codes(source);
        let has_2322 = got.contains(&2322);
        assert_eq!(
            has_2322, expect,
            "{label}: expected TS2322={expect} from the on-demand relation path, got {got:?}",
        );
    }
}

/// An inherited DOM member reached through heritage (`HTMLElement.appendChild`
/// from `Node`) must resolve on demand — the dropped transitive pre-walk must
/// not turn it into a spurious TS2339/TS2740 (the #12299 case). Clean both as a
/// bare statement and through a relation that consumes the return type.
#[test]
fn dom_inherited_member_resolves_on_demand() {
    assert_no_heritage_hazard(
        "HTMLElement.appendChild (statement)",
        "declare const e: HTMLElement; e.appendChild(e); void e;",
    );
    assert_no_heritage_hazard(
        "HTMLElement.appendChild return relation",
        "declare const e: HTMLElement; const n: Node = e.appendChild(e); void n;",
    );
}

/// The relation path must not re-introduce the eager heritage closure: a DOM
/// type consumed by an assignment relation stays under the same over-
/// materialization ratchet as a bare annotation. If forcing-on-miss walked the
/// transitive tail it would blow past the ratchet, re-creating the cost the
/// rework removed.
#[test]
fn dom_relation_stays_below_materialization_ratchet() {
    // This guards the on-demand *win*; with the kill-switch set the legacy eager
    // transitive pre-walk intentionally over-materializes, so skip it there.
    if crate::state_checking::lazy_lib_member::on_demand_forcing_disabled() {
        return;
    }
    let base = type_count(TRIVIAL_SOURCE);
    let ceiling = base + EAGER_RECEIVER_CEILING;
    for (label, source) in [
        (
            "HTMLElement assignment relation",
            "declare const e: HTMLElement; const x: HTMLElement = e; void x;",
        ),
        (
            "Node assignment relation",
            "declare const n: Node; const m: Node = n; void m;",
        ),
        (
            "Document createElement relation",
            "declare const d: Document; const e: Element = d.createElement(\"p\"); void e;",
        ),
    ] {
        let count = type_count(source);
        assert!(
            count <= ceiling,
            "{label}: interned {count} types (baseline {base}), exceeding the on-demand \
             ratchet {ceiling}; force-on-miss is walking the transitive heritage tail \
             instead of materializing only what the relation consumes (#12101 regression)",
        );
    }
}

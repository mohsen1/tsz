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

/// Floor sanity (#12158): an already-lazy own property read stays clean — the
/// heritage rework must not regress the member-access laziness already shipped.
#[test]
fn document_property_read_stays_clean() {
    assert_no_heritage_hazard(
        "Document.title read",
        "declare const d: Document; const t: string = d.title; void t;",
    );
}

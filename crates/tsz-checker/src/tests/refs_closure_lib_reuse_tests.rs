//! Coverage for the lib-pure refs-closure reuse fast path in
//! `ensure_refs_resolved` (issue #13936).
//!
//! `ensure_refs_resolved` walks the transitive ref/heritage closure of every
//! relation input. Relation-heavy programs re-walk the *same* lib/DOM heritage
//! subgraph (`EventTarget`, `Event`, `Node`, …) once per relation. The fast
//! path records a lib-pure closure the first time it is fully resolved (without
//! exhausting either fuel budget) and skips re-descending into it on later
//! traversals, since builtin-lib types are global, bound before checking, and
//! resolve identically in every arena/requester context.
//!
//! These tests assert the reuse is **behavior-preserving**: repeated relations
//! against the same lib heritage closure stay diagnostically correct (matching
//! `tsc`), genuine mismatches still error, and a closure that touches a *user*
//! type is never recorded for skip-descent (so the per-requester cross-arena
//! resolution those paths depend on keeps running). The cases vary the lib
//! interface and member spelling so behavior follows the structural shape, not
//! a particular identifier.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_lib_files};

fn dom_libs() -> Vec<std::sync::Arc<tsz_binder::lib_loader::LibFile>> {
    load_lib_files(&["es5.d.ts", "dom.d.ts", "dom.iterable.d.ts"])
}

fn dom_codes(source: &str) -> Vec<u32> {
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &dom_libs(),
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

/// Many relations against the same DOM heritage closure (each `addEventListener`
/// forces `EventTarget`/event-map heritage) must stay clean. The fast path skips
/// re-walking the shared lib closure on the 2nd..Nth relation; the result must be
/// identical to walking it every time.
#[test]
fn repeated_dom_heritage_relations_stay_clean() {
    let codes = dom_codes(
        r#"
declare const a: HTMLDivElement;
declare const b: HTMLAnchorElement;
declare const c: HTMLButtonElement;
const fn = (e: Event): void => { void e; };
a.addEventListener("click", fn);
b.addEventListener("click", fn);
c.addEventListener("click", fn);
const t1: EventTarget = a;
const t2: EventTarget = b;
const t3: EventTarget = c;
export {};
"#,
    );
    assert!(
        codes.is_empty(),
        "repeated relations against the shared DOM heritage closure should be clean, got {codes:?}",
    );
}

/// A genuine mismatch on a relation that follows an already-reused lib closure
/// must still error — recording the closure must not make later relations
/// permissive (the #12144 trap, guarded structurally here).
#[test]
fn mismatch_after_reused_lib_closure_still_errors() {
    let codes = dom_codes(
        r#"
declare const a: HTMLDivElement;
const ok: EventTarget = a;
void ok;
const bad: number = a;
export {};
"#,
    );
    assert!(
        codes.contains(&2322),
        "assigning an HTMLDivElement to number must still report TS2322 even after the \
         shared lib closure was reused, got {codes:?}",
    );
}

/// A user interface that references a DOM lib type must still type-check
/// correctly. The closure that reaches the user type is *not* lib-pure, so it is
/// never recorded for skip-descent — the user side keeps resolving normally.
#[test]
fn user_type_referencing_lib_type_still_checks() {
    let codes = dom_codes(
        r#"
interface Wrapper {
    el: HTMLDivElement;
    target: EventTarget;
}
declare const w: Wrapper;
const t: EventTarget = w.el;
void t;
const bad: number = w.el;
export {};
"#,
    );
    assert!(
        codes.contains(&2322) && !codes.contains(&2339),
        "user type referencing a lib type: lib member access resolves (no TS2339) and the \
         number mismatch still reports TS2322, got {codes:?}",
    );
}

//! Regression guard for the lazy-lib / refs-resolution-**fuel DETERMINISM**
//! invariant (issue #12144; full analysis in the non-committed
//! `LAZY_LIB_FUEL_DETERMINISM_FINDINGS.md` and the `lazy-lib-fuel-determinism`
//! memory).
//!
//! tsz materializes lib/DOM heritage by flattening each touched interface's
//! transitive closure into a flat `ObjectShape` (no `base_types`). That walk is
//! bounded by a global `REFS_RESOLUTION_FUEL` cap, which makes resolution
//! *completeness* order-dependent: a diagnostic can fire-or-not based on how much
//! fuel earlier statements burned. Issue #12144 was the witness — DOM-call-heavy
//! files silently dropped `TS2322`/`TS2345` when the first call exhausted the
//! budget and later identical assignments saw an unresolved `Lazy` (treated
//! leniently / compatible). It is patched, but the fragility is structural.
//!
//! The companion [`lazy_lib_heritage_guard_tests`] module pins the *member
//! resolution* failure modes of the heritage rework (#13933/#13935/#13936). This
//! module pins the orthogonal **determinism** invariant, which the planned
//! lazy-heritage / `base_types` per-member rework is most likely to regress:
//!
//! > A relation's diagnostic for a fixed `(source, target)` pair must be IDENTICAL
//! > regardless of prior resolution history or declaration order. Fuel may bound
//! > WORK, never ANSWERS.
//!
//! Concretely: N identical lib-type assignment errors must ALL report (no
//! fuel-exhaustion drop), reordering declarations must not change the diagnostic
//! set, inherited members must resolve through the full heritage closure, and a
//! genuinely-missing member must still be detected (a partial/lazy flatten that
//! left a base unresolved would treat it leniently and silently drop the error).
//! Assertions are shape-driven (DOM-call assignment / inherited member / missing
//! member), not tied to any identifier spelling.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    let mut c: Vec<u32> = check_source_with_libs(
        source,
        "fuel_determinism.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect();
    c.sort_unstable();
    c
}

/// #12144: every identical DOM-call assignment error must report. The first call
/// flattens the `HTMLElement` closure and consumes fuel; the rest must not be
/// silently suppressed by exhaustion. A regression here = silent diagnostic loss.
#[test]
fn identical_dom_call_assignments_all_report_no_fuel_drop() {
    let mut src = String::from("declare const d: Document;\n");
    let n = 8;
    for i in 0..n {
        // `Element` (createElement's return) is not assignable to `number`: TS2322.
        src.push_str(&format!("const a{i}: number = d.createElement(\"div\");\n"));
    }
    let n2322 = codes(&src).into_iter().filter(|&code| code == 2322).count();
    assert_eq!(
        n2322, n,
        "all {n} identical DOM-call assignment errors must report — no fuel-exhaustion \
         silent drop (#12144); got {n2322}. A lazy-heritage/base_types rework must keep \
         resolution completeness independent of prior fuel.",
    );
}

/// Fuel determinism: the diagnostic SET must not depend on declaration order.
/// Reordering the DOM-typed declarations + their erroring assignments must yield
/// the identical code multiset.
#[test]
fn dom_diagnostics_are_declaration_order_invariant() {
    let fwd = "declare const d: Document;\n\
               declare const w: Window;\n\
               const a: number = d.createElement(\"div\");\n\
               const b: number = w.document;\n";
    let rev = "declare const w: Window;\n\
               declare const d: Document;\n\
               const b: number = w.document;\n\
               const a: number = d.createElement(\"div\");\n";
    assert_eq!(
        codes(fwd),
        codes(rev),
        "DOM diagnostics must be declaration-order invariant (fuel determinism); \
         a fuel-burn-dependent resolution would diverge under reordering.",
    );
}

/// The heritage closure must be complete enough that inherited members resolve.
/// `addEventListener` (from `EventTarget`), `click` (from `HTMLElement`) and
/// `appendChild` (from `Node`) are all inherited onto `HTMLElement`. A rework
/// that left a base lazy/dropped would surface TS2339 here.
#[test]
fn inherited_lib_members_resolve_through_heritage() {
    let src = "declare const el: HTMLElement;\n\
               el.addEventListener(\"click\", () => {});\n\
               el.click();\n\
               el.appendChild(el);\n";
    let c = codes(src);
    assert!(
        !c.contains(&2339),
        "inherited DOM members (addEventListener/click/appendChild) must resolve through \
         the heritage closure; got {c:?}. A base_types/lazy-heritage rework must not drop them.",
    );
}

/// The dual of determinism: a genuinely-missing member must be DETECTED. `Window`
/// has no `__tsz_bogus_member`, so assigning it to an interface requiring that
/// member is TS2741 (matches tsc). A partial flatten that left `Window`
/// unresolved would treat it leniently and silently drop this error — the
/// #12144 failure shape.
#[test]
fn missing_member_detected_on_complete_flatten() {
    let src = "interface NeedsBogus { __tsz_bogus_member: number; }\n\
               declare const w: Window;\n\
               const p: NeedsBogus = w;\n";
    assert!(
        codes(src).contains(&2741),
        "a genuinely-missing member on a fully-flattened lib type must report TS2741 \
         (matches tsc); a partial/lazy flatten that drops it is the #12144 failure shape.",
    );
}

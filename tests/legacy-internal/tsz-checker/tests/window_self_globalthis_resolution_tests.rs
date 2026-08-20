//! Value-read and `typeof` of the self-referential globals `window`, `self`,
//! and `globalThis` resolve to their real lib types instead of collapsing to
//! `any` (regression for #14742).
//!
//! Structural rule: a `declare var X: Window & typeof globalThis` (the shape the
//! lib uses for `window`/`self`) and the ambient `globalThis` reference are read
//! in value position and via the `typeof X` type query as their concrete types —
//! `Window & typeof globalThis` and `typeof globalThis` — not `any`. The `any`
//! collapse previously poisoned every downstream expression (member reads,
//! indexed-access annotations), re-introducing false positives such as the
//! zustand TS2454 and `any | false` mistypes.
//!
//! Owners: `types/computation/type_operators.rs` and `types/type_node.rs` (the
//! `Window & typeof globalThis` intersection annotation no longer short-circuits
//! to `any`), `types/computation/identifier/resolution.rs` (the `globalThis`
//! value read resolves to the `typeof globalThis` surface), and `tsz_solver`
//! `ObjectFlags::GLOBAL_THIS_SURFACE` (the surface displays as `typeof
//! globalThis` even as an intersection member).
//!
//! Harness note: the unit checker harness does not resolve the *lib* `var
//! window`/`var self` declarations to their annotation type (it returns `any`
//! for lib value vars regardless of this fix), so the intersection-annotation
//! resolution is exercised here through an equivalent user-declared `var`. The
//! literal `window`/`self`/`globalThis` lib reads are verified end-to-end with
//! the CLI (see the PR's Verification section).

use crate::context::CheckerOptions;
use crate::test_utils::{check_multi_file_with_libs_stamped, load_lib_files};
use tsz_common::common::ModuleKind;
use tsz_common::diagnostics::Diagnostic;

const TS2322: u32 = 2322; // Type X is not assignable to type Y.
const TS2454: u32 = 2454; // Variable is used before being assigned.

fn strict() -> CheckerOptions {
    CheckerOptions {
        module: ModuleKind::ESNext,
        strict: true,
        ..CheckerOptions::default()
    }
}

/// Check `main` against the stripped es5 + dom libs. Returns `None` when the dom
/// lib is unavailable in this checkout (the caller skips rather than fails).
fn check_with_dom(main: &str) -> Option<Vec<Diagnostic>> {
    let libs = load_lib_files(&["es5.d.ts", "dom.d.ts"]);
    if libs.iter().all(|l| l.file_name != "dom.d.ts") {
        return None;
    }
    Some(check_multi_file_with_libs_stamped(
        &[("main.ts", main)],
        "main.ts",
        strict(),
        &libs,
    ))
}

fn messages(diags: &[Diagnostic], code: u32) -> Vec<&str> {
    diags
        .iter()
        .filter(|d| d.code == code)
        .map(|d| d.message_text.as_str())
        .collect()
}

fn count(diags: &[Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

#[test]
fn window_and_global_this_annotation_resolves_to_real_intersection() {
    // The core of the fix: a `Window & typeof globalThis` variable is read as the
    // real intersection (displayed verbatim), not `any`. `any` would silence the
    // `number` assignment below.
    let Some(diags) = check_with_dom(
        r#"
declare var globalScope: Window & typeof globalThis;
const captured = globalScope;
const probe: never = captured;
const asNumber: number = captured;
export {};
"#,
    ) else {
        return;
    };
    assert_eq!(
        count(&diags, TS2322),
        2,
        "an `any` collapse would drop both assignments: {diags:?}"
    );
    assert!(
        messages(&diags, TS2322)
            .iter()
            .all(|m| m.contains("Window & typeof globalThis")),
        "must display as `Window & typeof globalThis`, not `any`: {:?}",
        messages(&diags, TS2322)
    );
}

#[test]
fn renamed_alias_chain_preserves_intersection_type() {
    // Renamed binders and alias chains keep the resolved type (anti
    // fixture-name dependence): `const a = g; const b = a`.
    let Some(diags) = check_with_dom(
        r#"
declare var globalScope: Window & typeof globalThis;
const firstAlias = globalScope;
const secondAlias = firstAlias;
const probe: never = secondAlias;
export {};
"#,
    ) else {
        return;
    };
    assert!(
        messages(&diags, TS2322)
            .iter()
            .any(|m| m.contains("Window & typeof globalThis")),
        "alias chain must keep `Window & typeof globalThis`: {:?}",
        messages(&diags, TS2322)
    );
}

#[test]
fn window_and_global_this_member_read_resolves_concrete_member_type() {
    // A member shared by both arms resolves to its concrete type (`Window#origin`
    // is `string`), not `any`.
    let Some(diags) = check_with_dom(
        r#"
declare var globalScope: Window & typeof globalThis;
const probe: never = globalScope.origin;
export {};
"#,
    ) else {
        return;
    };
    assert!(
        messages(&diags, TS2322)
            .iter()
            .any(|m| m.contains("string")),
        "member of `Window & typeof globalThis` must keep its real type: {:?}",
        messages(&diags, TS2322)
    );
}

#[test]
fn global_this_value_read_resolves_to_typeof_global_this() {
    // The ambient `globalThis` read in value position is the `typeof globalThis`
    // surface, not `any`.
    let Some(diags) = check_with_dom(
        r#"
const g = globalThis;
const probe: never = g;
export {};
"#,
    ) else {
        return;
    };
    assert!(
        messages(&diags, TS2322)
            .iter()
            .any(|m| m.contains("typeof globalThis")),
        "globalThis must display as `typeof globalThis`, not `any`: {:?}",
        messages(&diags, TS2322)
    );
}

#[test]
fn typeof_indexed_access_unwraps_for_definite_assignment() {
    // zustand witness (`src/middleware/devtools.ts`): a `(typeof X)['opt']`
    // annotation must expose the declared `undefined`/`false` so the
    // definite-assignment check sees it. With the `any` collapse the `undefined`
    // was hidden inside `any`, re-introducing a false TS2454.
    let Some(diags) = check_with_dom(
        r#"
declare global { interface Window { opt?: { connect(): void } } }
export {};
declare var globalScope: Window & typeof globalThis;
function f() {
  let x: (typeof globalScope)['opt'] | false;
  try { x = (true as boolean) && globalScope.opt } catch {}
  if (!x) return;
  return x;
}
"#,
    ) else {
        return;
    };
    assert_eq!(
        count(&diags, TS2454),
        0,
        "definite-assignment must see the declared `undefined` through `(typeof X)['opt']`: {diags:?}"
    );
}

#[test]
fn ordinary_interface_globals_keep_their_named_types() {
    // Negative control: globals NOT declared as `Window & typeof globalThis`
    // keep resolving to their plain interface types and gain no globalThis
    // surface.
    let Some(diags) = check_with_dom(
        r#"
const d = document;
const dProbe: never = d;
const nav = navigator;
const navProbe: never = nav;
export {};
"#,
    ) else {
        return;
    };
    let msgs = messages(&diags, TS2322);
    assert!(
        msgs.iter().any(|m| m.contains("Document")),
        "document must stay `Document`: {msgs:?}"
    );
    assert!(
        msgs.iter().any(|m| m.contains("Navigator")),
        "navigator must stay `Navigator`: {msgs:?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("typeof globalThis")),
        "ordinary interface globals must not gain a globalThis surface: {msgs:?}"
    );
}

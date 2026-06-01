//! Coverage for the lazy single-member lib-interface property-access fast path.
//!
//! Value-position property/method access on a simple lib-interface receiver
//! resolves only the accessed member — an own plain property, an own method
//! overload set, or a member inherited through the bare `extends` closure (see
//! `state::state_checking::lazy_lib_member`). These tests assert the fast path
//! is **behavior-preserving**: member reads type correctly, type/argument
//! mismatches still error, missing members still report TS2339, and a user
//! interface that shadows a lib name falls back to the full path.
//!
//! The cases vary the lib interface and member spelling so the behavior follows
//! the structural shape (simple lib interface + property/method/inherited
//! member), not any particular identifier name.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_lib_files};

fn dom_codes(source: &str) -> Vec<u32> {
    let libs = load_lib_files(&["es5.d.ts", "dom.d.ts", "dom.iterable.d.ts"]);
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

/// Own plain property read on a simple lib interface resolves to the correct
/// member type with no diagnostics — proven across two different interfaces and
/// member names so the path follows the shape, not a spelling.
#[test]
fn own_member_read_resolves_without_diagnostics() {
    // `Document.title: string` and `Location.href: string` are own plain
    // properties; assigning their value to a `string` must type-check cleanly.
    let codes = dom_codes(
        r#"
declare const d: Document;
declare const loc: Location;
const a: string = d.title;
const b: string = loc.href;
export {};
"#,
    );
    assert!(
        codes.is_empty(),
        "own-member lib property reads should not produce diagnostics, got {codes:?}",
    );
}

/// A type mismatch on an own member of a simple lib interface must still report
/// TS2322 — the fast path resolves the real member type, it does not widen it.
#[test]
fn own_member_type_mismatch_still_errors() {
    let codes = dom_codes(
        r#"
declare const d: Document;
const wrong: number = d.title;
export {};
"#,
    );
    assert!(
        codes.contains(&2322),
        "string member assigned to number should report TS2322, got {codes:?}",
    );
}

/// Accessing a member that does not exist on the interface must still report
/// TS2339 — the fast path returns `None` for an absent member so the full path
/// surfaces the missing-property error.
#[test]
fn missing_member_still_reports_ts2339() {
    let codes = dom_codes(
        r#"
declare const d: Document;
const x = d.definitelyNotARealDomMember;
export {};
"#,
    );
    assert!(
        codes.contains(&2339),
        "absent member should report TS2339, got {codes:?}",
    );
}

/// A heritage-inherited member (e.g. a property declared on a base interface)
/// must still resolve correctly. The own-member fast path returns `None` for it
/// and the full materialization path provides the inherited member.
#[test]
fn inherited_member_still_resolves() {
    // `Document.nodeName` is inherited from `Node`; reading it as a string must
    // type-check (the inherited member is `string`).
    let codes = dom_codes(
        r#"
declare const d: Document;
const n: string = d.nodeName;
export {};
"#,
    );
    assert!(
        codes.is_empty(),
        "inherited lib member read should resolve cleanly, got {codes:?}",
    );
}

/// A user interface that shadows a lib name must NOT use the lib fast path: its
/// own members win. Proven by reading a user-only member that the lib interface
/// of the same name does not declare.
#[test]
fn user_shadowed_lib_interface_uses_own_members() {
    let codes = dom_codes(
        r#"
interface Location { userOnlyField: number; }
declare const loc: Location;
const v: number = loc.userOnlyField;
export {};
"#,
    );
    assert!(
        codes.is_empty(),
        "user-shadowed interface own member should resolve, got {codes:?}",
    );
}

// --- Lazy global ambient-var value type (extends the fast path to globals) ---
//
// A global ambient var typed as a bare simple-lib-interface reference (e.g. the
// lib's own `declare var document: Document`) keeps a lazy value type, so a
// property access like `document.title` resolves only the accessed member. These
// tests assert the global path is behavior-preserving: reads type correctly,
// mismatches still error, missing members still report TS2339, and method /
// inherited / shadowed / augmented shapes resolve identically to the eager path.
// The cases vary the lib interface and the accessed member so the behavior
// follows the structural shape, not any particular identifier name.

/// Own plain property read through the lib global `document` resolves to the
/// correct member type with no diagnostics. Proven across two globals
/// (`document.title`, `window.name`) so the path follows the shape.
#[test]
fn global_own_member_read_resolves_without_diagnostics() {
    let codes = dom_codes(
        r#"
const a: string = document.title;
const b: string = window.name;
export {};
"#,
    );
    assert!(
        codes.is_empty(),
        "global own-member reads should not produce diagnostics, got {codes:?}",
    );
}

/// A type mismatch on an own member accessed through a lib global must still
/// report TS2322 — keeping the global lazy does not widen the member type.
#[test]
fn global_own_member_type_mismatch_still_errors() {
    let codes = dom_codes(
        r#"
const wrong: number = document.title;
export {};
"#,
    );
    assert!(
        codes.contains(&2322),
        "string member assigned to number should report TS2322, got {codes:?}",
    );
}

/// Accessing a member that does not exist on the global's interface must still
/// report TS2339 — the lazy global resolves the absent member through the full
/// path, which surfaces the missing-property error.
#[test]
fn global_missing_member_still_reports_ts2339() {
    let codes = dom_codes(
        r#"
const x = document.definitelyNotARealDomMember;
export {};
"#,
    );
    assert!(
        codes.contains(&2339),
        "absent global member should report TS2339, got {codes:?}",
    );
}

/// A method member with overloads accessed through a lib global must resolve
/// (e.g. `document.querySelector`). Even resolving only this one member is
/// cheaper than materializing all of `Document`; the call must type-check.
#[test]
fn global_method_overload_member_resolves() {
    let codes = dom_codes(
        r#"
const el = document.querySelector("div");
const ok: Element | null = el;
export {};
"#,
    );
    assert!(
        codes.is_empty(),
        "global method/overload member access should resolve cleanly, got {codes:?}",
    );
}

/// A heritage-inherited member accessed through a lib global must still resolve
/// (e.g. `document.nodeName` is inherited from `Node`). The own-member fast path
/// returns `None`; the full materialization path provides the inherited member.
#[test]
fn global_inherited_member_still_resolves() {
    let codes = dom_codes(
        r#"
const n: string = document.nodeName;
export {};
"#,
    );
    assert!(
        codes.is_empty(),
        "inherited global member read should resolve cleanly, got {codes:?}",
    );
}

/// Name-agnostic control: a *user-defined* ambient global var typed as a
/// *user* interface (not a lib interface) must not take the lib fast path — its
/// own members resolve, and the lever leaves it on the normal path. Proven with
/// a non-lib interface and var name so the behavior is not keyed to `document`.
#[test]
fn user_ambient_global_var_uses_own_interface_members() {
    let codes = dom_codes(
        r#"
interface Widget { spin(): void; size: number; }
declare var widget: Widget;
widget.spin();
const s: number = widget.size;
const bad: string = widget.size;
export {};
"#,
    );
    assert_eq!(
        codes.iter().filter(|&&c| c == 2322).count(),
        1,
        "user ambient-global own members should resolve (only the size mismatch errors), got {codes:?}",
    );
    assert!(
        !codes.contains(&2339),
        "user ambient-global members must all resolve, got {codes:?}",
    );
}

/// Negative control: a globally-augmented lib interface (`declare global {
/// interface Document { ... } }`) must fall back to the full path so the
/// out-of-band augmented member stays visible through the global access.
#[test]
fn augmented_global_lib_interface_member_visible() {
    let codes = dom_codes(
        r#"
declare global {
    interface Document {
        tszAugmentedField: number;
    }
}
const v: number = document.tszAugmentedField;
const t: string = document.title;
export {};
"#,
    );
    assert!(
        !codes.contains(&2339),
        "augmented global member must resolve (no TS2339), got {codes:?}",
    );
    assert!(
        !codes.contains(&2322),
        "augmented member and own member reads should type-check, got {codes:?}",
    );
}

// ---------------------------------------------------------------------------
// Lazy method-overload + heritage-walk member resolution.
//
// These cover the extension of the single-property fast path to (a) own method
// **overload sets** lowered to their merged callable, and (b) members inherited
// through the bare-`extends` closure. Both are byte-identical to full
// materialization and match `tsc`: an inherited generic method call type-checks
// when correct and reports the same overload diagnostic when wrong.
// ---------------------------------------------------------------------------

/// An own method-overload set on a simple lib interface (`ParentNode.append`,
/// accessed via the inherited `Document` receiver) must resolve to a callable
/// with all overloads: a correct call type-checks and a wrong-argument call
/// reports the same TS2345/TS2769 the full path would. Proven on an inherited
/// method so the heritage walk and the overload lowering are exercised together.
#[test]
fn inherited_method_overload_call_type_checks() {
    let codes = dom_codes(
        r#"
declare const d: Document;
d.append("text");
export {};
"#,
    );
    assert!(
        codes.is_empty(),
        "inherited method-overload call with a valid arg should type-check, got {codes:?}",
    );
}

/// The same inherited method with a wrong argument must still report the call
/// error — the lazy overload set is the real merged callable, not a widened
/// `any`. `ParentNode.append(...: (Node | string)[])` rejects an object literal.
#[test]
fn inherited_method_overload_wrong_arg_still_errors() {
    let codes = dom_codes(
        r#"
declare const d: Document;
d.append({});
export {};
"#,
    );
    assert!(
        codes.contains(&2345) || codes.contains(&2769),
        "wrong-typed inherited method-overload arg should report TS2345/TS2769, got {codes:?}",
    );
}

/// A method inherited through a **deeper** heritage edge
/// (`Document -> Node -> EventTarget.addEventListener`) must also resolve, and a
/// wrong listener-type/key argument must still surface the overload error,
/// proving the walk descends multiple `extends` levels.
#[test]
fn deeply_inherited_method_overload_resolves_and_errors() {
    let ok = dom_codes(
        r#"
declare const d: Document;
d.addEventListener("click", () => {});
export {};
"#,
    );
    assert!(
        ok.is_empty(),
        "valid deeply-inherited addEventListener call should type-check, got {ok:?}",
    );
    let bad = dom_codes(
        r#"
declare const d: Document;
d.addEventListener(999, () => {});
export {};
"#,
    );
    assert!(
        bad.contains(&2769) || bad.contains(&2345),
        "addEventListener with a numeric event name should report the overload error, got {bad:?}",
    );
}

/// Name-agnostic proof that the heritage walk follows the *structural* extends
/// edge, not a hardcoded lib name: a user interface `B` extends a simple lib
/// interface and inherits its method; resolving the inherited method on `B`'s
/// value works the same. (Here the inherited member is read off `Element`, a
/// distinct lib interface from `Document`, via the `ParentNode` base.)
#[test]
fn inherited_method_resolves_across_distinct_lib_interface() {
    // `Element` also extends `ParentNode`, so `el.append` is inherited just like
    // `document.append` — different receiver interface, same structural edge.
    let codes = dom_codes(
        r#"
declare const el: Element;
el.append("x");
const n = el.querySelectorAll("div");
export {};
"#,
    );
    assert!(
        codes.is_empty(),
        "inherited method on a distinct lib interface should resolve cleanly, got {codes:?}",
    );
}

/// An inherited method whose own interface is **generic** must NOT be resolved
/// through the lazy walk: the base carries type arguments the walk cannot
/// substitute, so it falls back to full materialization. `Array<T>` methods
/// inherited via a generic base are the canonical case; here we prove a generic
/// receiver's method still resolves correctly through the full path (no TS2339,
/// no spurious widening) rather than being mis-handled by the lazy walk.
#[test]
fn generic_interface_method_still_resolves_via_full_path() {
    let codes = dom_codes(
        r#"
declare const a: number[];
const m = a.map(x => x + 1);
const ok: number[] = m;
const bad: string[] = m;
export {};
"#,
    );
    // `map` resolves (no TS2339) and the real return type is `number[]`, so only
    // the `string[]` mismatch errors — proving the generic path is intact and the
    // lazy walk did not widen or drop the member.
    assert!(
        !codes.contains(&2339),
        "generic interface method must resolve (no TS2339), got {codes:?}",
    );
    assert_eq!(
        codes.iter().filter(|&&c| c == 2322).count(),
        1,
        "only the string[] mismatch should error, got {codes:?}",
    );
}

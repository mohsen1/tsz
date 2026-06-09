//! Coverage for the lazy single-member lib-interface property-access fast path.
//!
//! Value-position property access on a simple lib-interface receiver resolves
//! only the accessed own member (see
//! `state::state_checking::lazy_lib_member`). These tests assert the fast path
//! is **behavior-preserving**: own-member reads type correctly, type mismatches
//! still error, missing members still report TS2339, and a user interface that
//! shadows a lib name falls back to the full path.
//!
//! The cases vary the lib interface and member spelling so the behavior follows
//! the structural shape (simple lib interface + own plain property), not any
//! particular identifier name.

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

/// Own plain property read on a **known-global lib value** receiver (e.g.
/// `document`, `location`, `navigator` directly, not via a typed local) resolves
/// to the correct member type with no diagnostics. This is the case the global
/// value-type override would otherwise eagerly materialize; preserving the bare
/// `Lazy` receiver keeps the single-member fast path engaged. Proven across three
/// distinct globals + members so the behavior follows the shape, not a spelling.
#[test]
fn global_value_receiver_own_member_resolves_without_diagnostics() {
    let codes = dom_codes(
        r#"
const a: string = document.title;
const b: string = location.href;
const c: string = navigator.userAgent;
export {};
"#,
    );
    assert!(
        codes.is_empty(),
        "global-value own-member reads should not produce diagnostics, got {codes:?}",
    );
}

/// A chained read through a member that is itself typed as a simple lib
/// interface (e.g. `document.body: HTMLElement`) resolves each link lazily and
/// types correctly. `document.body.innerHTML` is `string`. Before the
/// lib-interface-reference member stayed lazy, the intermediate `body` forced
/// full materialization of `HTMLElement`; this asserts the chain still
/// type-checks cleanly.
#[test]
fn chained_lib_interface_reference_member_resolves_without_diagnostics() {
    let codes = dom_codes(
        r#"
declare const d: Document;
const html: string = d.body.innerHTML;
const cls: string = d.body.className;
export {};
"#,
    );
    assert!(
        codes.is_empty(),
        "chained lib-interface-reference member reads should not produce diagnostics, got {codes:?}",
    );
}

/// A type mismatch on a chained lib-interface-reference member read must still
/// report TS2322 — keeping the intermediate member lazy does not widen the
/// leaf member type.
#[test]
fn chained_lib_interface_reference_member_type_mismatch_still_errors() {
    let codes = dom_codes(
        r#"
declare const d: Document;
const wrong: number = d.body.innerHTML;
export {};
"#,
    );
    assert!(
        codes.contains(&2322),
        "string leaf member assigned to number should report TS2322, got {codes:?}",
    );
}

/// A missing member reached through a chained lib-interface-reference member
/// (`d.body.__nope__`, where `body: HTMLElement`) must produce the exact same
/// diagnostic as a missing member on a direct `HTMLElement` receiver
/// (`h.__nope__`). Keeping the intermediate `body` lazy makes the chained
/// receiver resolve to the identical `HTMLElement` reference a type-position
/// annotation produces (PR #8638), so the absent-member error is unchanged.
#[test]
fn chained_lib_interface_reference_missing_leaf_matches_direct_receiver() {
    let chained = dom_codes(
        r#"
declare const d: Document;
const x = d.body.definitelyNotARealDomMember;
export {};
"#,
    );
    let direct = dom_codes(
        r#"
declare const h: HTMLElement;
const x = h.definitelyNotARealDomMember;
export {};
"#,
    );
    assert!(
        !chained.is_empty(),
        "an absent leaf member on a chained reference must still produce a diagnostic",
    );
    assert_eq!(
        chained, direct,
        "missing-member access through `d.body` should match a direct `HTMLElement` receiver, got chained={chained:?} direct={direct:?}",
    );
}

/// A type mismatch on an own member read through a global value receiver must
/// still report TS2322 — preserving the lazy receiver does not widen the member.
#[test]
fn global_value_receiver_type_mismatch_still_errors() {
    let codes = dom_codes(
        r#"
const wrong: number = document.title;
export {};
"#,
    );
    assert!(
        codes.contains(&2322),
        "string member assigned to number via global receiver should report TS2322, got {codes:?}",
    );
}

/// A missing member on a global value receiver must still report TS2339 — the
/// on-demand materialization fallback runs when the fast path misses, so the
/// diagnostic is unchanged from the eager-override path.
#[test]
fn global_value_receiver_missing_member_still_reports_ts2339() {
    let codes = dom_codes(
        r#"
const x = document.definitelyNotARealDomMember;
export {};
"#,
    );
    assert!(
        codes.contains(&2339),
        "absent member on global receiver should report TS2339, got {codes:?}",
    );
}

/// An inherited member read through a global value receiver must still resolve.
/// The own-member fast path misses (the member lives on a base interface) and the
/// full materialization path provides it — same result, just not eagerly forced.
#[test]
fn global_value_receiver_inherited_member_still_resolves() {
    let codes = dom_codes(
        r#"
const n: string = document.nodeName;
export {};
"#,
    );
    assert!(
        codes.is_empty(),
        "inherited member via global receiver should resolve cleanly, got {codes:?}",
    );
}

/// A `readonly` own plain property read resolves through the single-member fast
/// path (the `readonly` modifier is write-only and does not change the read
/// type). Proven across two distinct interfaces + member spellings
/// (`Element.tagName`, `Node.nodeName`) so the behavior follows the shape, not a
/// name. Before this, `readonly` members forced the receiver's full structural
/// materialization.
#[test]
fn readonly_own_member_read_resolves_without_diagnostics() {
    let codes = dom_codes(
        r#"
declare const el: Element;
declare const node: Node;
const t: string = el.tagName;
const n: string = node.nodeName;
export {};
"#,
    );
    assert!(
        codes.is_empty(),
        "readonly own-member reads should not produce diagnostics, got {codes:?}",
    );
}

/// A type mismatch on a `readonly` own member must still report TS2322 — the fast
/// path resolves the real member type, it does not widen it.
#[test]
fn readonly_own_member_type_mismatch_still_errors() {
    let codes = dom_codes(
        r#"
declare const el: Element;
const wrong: number = el.tagName;
export {};
"#,
    );
    assert!(
        codes.contains(&2322),
        "readonly string member assigned to number should report TS2322, got {codes:?}",
    );
}

/// Writing to a `readonly` member must still report TS2540. The readonly *write*
/// diagnostic is decided by `check_readonly_assignment`, which re-materializes the
/// receiver independently of the read fast path, so keeping readonly reads on the
/// fast path does not suppress it. Proven across two interfaces so it follows the
/// shape, not a spelling.
#[test]
fn readonly_member_write_still_reports_ts2540() {
    let codes = dom_codes(
        r#"
declare const el: Element;
declare const node: Node;
el.tagName = "x";
node.nodeName = "y";
export {};
"#,
    );
    assert_eq!(
        codes,
        vec![2540, 2540],
        "writes to readonly members should each report TS2540, got {codes:?}",
    );
}

/// A chained read through a `readonly` member that is itself typed as a simple
/// lib interface (`Document.documentElement: HTMLElement`) resolves each link
/// lazily and types correctly. This exercises both levers together: the readonly
/// member resolves through the fast path AND, being a bare lib-interface
/// reference, stays `Lazy` so the leaf `tagName` resolves without materializing
/// `HTMLElement`.
#[test]
fn readonly_lib_interface_reference_member_chains_cleanly() {
    let codes = dom_codes(
        r#"
declare const d: Document;
const t: string = d.documentElement.tagName;
export {};
"#,
    );
    assert!(
        codes.is_empty(),
        "chained read through a readonly lib-interface-reference member should resolve cleanly, got {codes:?}",
    );
}

/// A type mismatch on the leaf of a chained `readonly` lib-interface-reference
/// member read must still report TS2322 — keeping the intermediate readonly
/// member lazy does not widen the leaf member type.
#[test]
fn readonly_lib_interface_reference_chain_leaf_mismatch_still_errors() {
    let codes = dom_codes(
        r#"
declare const d: Document;
const wrong: number = d.documentElement.tagName;
export {};
"#,
    );
    assert!(
        codes.contains(&2322),
        "string leaf assigned to number through a readonly chain should report TS2322, got {codes:?}",
    );
}

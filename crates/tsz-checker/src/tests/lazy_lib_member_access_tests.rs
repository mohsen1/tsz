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
